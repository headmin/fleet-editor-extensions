//! Code action generation for quick-fixes.
//!
//! Diagnostics carry their whole [`Fix`] (serialized by `diagnostics.rs` into
//! `Diagnostic.data`), so editor quick-fixes and `flint check --fix` share one
//! representation:
//!
//! - `Replace` — one action; preferred when the fix is `Safe`, offered with a
//!   "(may change semantics)" caveat when `Unsafe`.
//! - `Candidates` — one non-preferred action per option.
//! - `ReplaceLines` — a multi-line structural rewrite (e.g. expanding a
//!   directory `path:` entry into per-file entries) as a line-based edit.

use std::collections::HashMap;

use flint_lint::error::{Fix, FixSafety};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Diagnostic, Position,
    Range, TextEdit, Url, WorkspaceEdit,
};

/// Generate code actions for the diagnostics in the request range.
pub fn generate_code_actions(params: &CodeActionParams) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for diagnostic in &params.context.diagnostics {
        // Only process diagnostics produced by flint's linter.
        if diagnostic.source.as_deref() != Some("fleet-lint") {
            continue;
        }

        for action in create_fixes_from_diagnostic(diagnostic, &params.text_document.uri) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
    }

    actions
}

/// Rebuild the diagnostic's [`Fix`] from `data` and turn it into actions.
fn create_fixes_from_diagnostic(diagnostic: &Diagnostic, uri: &Url) -> Vec<CodeAction> {
    let fix: Fix = match diagnostic
        .data
        .as_ref()
        .and_then(|d| d.get("fix"))
        .and_then(|f| serde_json::from_value(f.clone()).ok())
    {
        Some(f) => f,
        None => return Vec::new(),
    };

    match fix {
        Fix::Replace { new, safety, .. } => {
            let edit = TextEdit {
                range: diagnostic.range,
                new_text: new.clone(),
            };
            vec![action_for(diagnostic, uri, edit, replace_title(&new, safety), safety)]
        }
        Fix::Candidates { options, .. } => options
            .iter()
            .map(|c| {
                let edit = TextEdit {
                    range: diagnostic.range,
                    new_text: c.clone(),
                };
                // Ambiguous fixes must never be auto-preferred.
                action_for(
                    diagnostic,
                    uri,
                    edit,
                    replace_title(c, FixSafety::Display),
                    FixSafety::Display,
                )
            })
            .collect(),
        Fix::ReplaceLines {
            start_line,
            end_line,
            replacement,
            safety,
        } => {
            // Line-based edit: replace [start_line, end_line] (1-indexed,
            // inclusive) by covering up to the start of the following line —
            // needs no document text, so this stays pure.
            let edit = TextEdit {
                range: Range {
                    start: Position {
                        line: start_line.saturating_sub(1) as u32,
                        character: 0,
                    },
                    end: Position {
                        line: end_line as u32,
                        character: 0,
                    },
                },
                new_text: format!("{replacement}\n"),
            };
            let lines = end_line - start_line + 1;
            let mut title = format!("Rewrite {lines} line(s)");
            if let Some(help) = diagnostic
                .data
                .as_ref()
                .and_then(|d| d.get("help"))
                .and_then(|h| h.as_str())
            {
                // The help text explains the structural rewrite better than a
                // generic label (e.g. the directory-expansion offer).
                if help.contains("expand") {
                    title = "Expand directory into per-file entries".to_string();
                }
            }
            if safety == FixSafety::Unsafe {
                title.push_str(" (may change semantics)");
            }
            vec![action_for(diagnostic, uri, edit, title, safety)]
        }
    }
}

/// Title for a plain replacement action.
fn replace_title(new_text: &str, safety: FixSafety) -> String {
    let mut title = format!("Replace with '{}'", truncate_suggestion(new_text, 40));
    if safety == FixSafety::Unsafe {
        title.push_str(" (may change semantics)");
    }
    title
}

/// Wrap a `TextEdit` in a quick-fix `CodeAction`. Only `Safe` fixes are
/// marked preferred — user-invoked quick fixes are consented, so `Unsafe` and
/// `Display` fixes are still offered, just not highlighted.
fn action_for(
    diagnostic: &Diagnostic,
    uri: &Url,
    edit: TextEdit,
    title: String,
    safety: FixSafety,
) -> CodeAction {
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    CodeAction {
        title,
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }),
        command: None,
        is_preferred: Some(safety == FixSafety::Safe),
        disabled: None,
        data: None,
    }
}

/// Truncate a suggestion string for display in the action title.
fn truncate_suggestion(s: &str, max_len: usize) -> String {
    // Take only the first line for display
    let first_line = s.lines().next().unwrap_or(s);

    if first_line.len() <= max_len {
        first_line.to_string()
    } else {
        format!("{}...", &first_line[..max_len - 3])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range};

    fn diag(data: Option<serde_json::Value>) -> Diagnostic {
        Diagnostic {
            range: Range {
                start: Position { line: 0, character: 12 },
                end: Position { line: 0, character: 40 },
            },
            severity: None,
            code: None,
            code_description: None,
            source: Some("fleet-lint".to_string()),
            message: "msg".to_string(),
            related_information: None,
            tags: None,
            data,
        }
    }

    fn fix_data(fix: &Fix) -> serde_json::Value {
        serde_json::json!({ "fix": fix })
    }

    #[test]
    fn test_truncate_suggestion() {
        assert_eq!(truncate_suggestion("short", 40), "short");
        assert_eq!(
            truncate_suggestion("this is a very long suggestion that should be truncated", 20),
            "this is a very lo..."
        );
        assert_eq!(truncate_suggestion("multi\nline\nsuggestion", 40), "multi");
    }

    #[test]
    fn test_safe_replace_is_preferred() {
        let d = diag(Some(fix_data(&Fix::Replace {
            old: Some("../old/path.yml".into()),
            new: "../new/path.yml".into(),
            safety: FixSafety::Safe,
        })));
        let uri = Url::parse("file:///test.yml").unwrap();
        let actions = create_fixes_from_diagnostic(&d, &uri);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].title, "Replace with '../new/path.yml'");
        assert_eq!(actions[0].kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(actions[0].is_preferred, Some(true));
    }

    #[test]
    fn test_unsafe_replace_is_caveated_not_preferred() {
        let d = diag(Some(fix_data(&Fix::Replace {
            old: Some("  ".into()),
            new: " ".into(),
            safety: FixSafety::Unsafe,
        })));
        let uri = Url::parse("file:///test.yml").unwrap();
        let actions = create_fixes_from_diagnostic(&d, &uri);

        assert_eq!(actions.len(), 1);
        assert!(actions[0].title.ends_with("(may change semantics)"));
        assert_eq!(actions[0].is_preferred, Some(false));
    }

    #[test]
    fn test_candidates_yield_one_action_each() {
        let d = diag(Some(fix_data(&Fix::Candidates {
            old: "../common.yml".into(),
            options: vec!["../a/common.yml".into(), "../b/common.yml".into()],
        })));
        let uri = Url::parse("file:///test.yml").unwrap();
        let actions = create_fixes_from_diagnostic(&d, &uri);

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0].title, "Replace with '../a/common.yml'");
        assert_eq!(actions[1].title, "Replace with '../b/common.yml'");
        // Ambiguous fixes must not be auto-preferred.
        assert_eq!(actions[0].is_preferred, Some(false));
        assert_eq!(actions[1].is_preferred, Some(false));
    }

    #[test]
    fn test_replace_lines_yields_line_based_edit() {
        // A multi-line structural rewrite (directory expansion) must surface
        // as a quick-fix with a line-based range — the historical gap where
        // block fixes worked in `--fix` but never appeared in the editor.
        let d = diag(Some(serde_json::json!({
            "fix": Fix::ReplaceLines {
                start_line: 4,
                end_line: 6,
                replacement: "      - path: a.mobileconfig\n      - path: b.mobileconfig".into(),
                safety: FixSafety::Unsafe,
            },
            "help": "Run `flint check --fix --unsafe-fixes` to expand it into 2 `- path:` entries",
        })));
        let uri = Url::parse("file:///team.yml").unwrap();
        let actions = create_fixes_from_diagnostic(&d, &uri);

        assert_eq!(actions.len(), 1);
        assert_eq!(
            actions[0].title,
            "Expand directory into per-file entries (may change semantics)"
        );
        assert_eq!(actions[0].is_preferred, Some(false));
        let edit = &actions[0].edit.as_ref().unwrap().changes.as_ref().unwrap()[&uri][0];
        // 1-indexed [4,6] inclusive → 0-indexed [3,6) exclusive-end covering
        // up to the start of line 7.
        assert_eq!(edit.range.start, Position { line: 3, character: 0 });
        assert_eq!(edit.range.end, Position { line: 6, character: 0 });
        assert!(edit.new_text.ends_with('\n'));
    }

    #[test]
    fn test_no_fix_data_yields_nothing() {
        let d = diag(None);
        let uri = Url::parse("file:///test.yml").unwrap();
        assert!(create_fixes_from_diagnostic(&d, &uri).is_empty());
    }

    #[test]
    fn test_wrong_source_is_skipped() {
        let d0 = diag(Some(fix_data(&Fix::Replace {
            old: None,
            new: "x".into(),
            safety: FixSafety::Safe,
        })));
        let mut d = d0;
        d.source = Some("other".to_string());
        let params = CodeActionParams {
            text_document: tower_lsp::lsp_types::TextDocumentIdentifier {
                uri: Url::parse("file:///test.yml").unwrap(),
            },
            range: d.range,
            context: tower_lsp::lsp_types::CodeActionContext {
                diagnostics: vec![d],
                only: None,
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        assert!(generate_code_actions(&params).is_empty());
    }
}
