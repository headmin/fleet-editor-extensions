//! Code action generation for quick-fixes.
//!
//! This module generates LSP code actions based on diagnostics that have
//! suggestion data attached to them.

use std::collections::HashMap;

use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, CodeActionParams, Diagnostic, TextEdit, Url,
    WorkspaceEdit,
};

/// Generate code actions for diagnostics in the given range.
///
/// This function looks at all diagnostics from fleet-lsp that have suggestion
/// data attached, and generates quick-fix code actions for them.
pub fn generate_code_actions(params: &CodeActionParams) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();

    for diagnostic in &params.context.diagnostics {
        // Only process diagnostics from fleet-lsp
        if diagnostic.source.as_deref() != Some("fleet-lsp") {
            continue;
        }

        // Check if diagnostic has suggestion data
        if let Some(action) = create_fix_from_diagnostic(diagnostic, &params.text_document.uri) {
            actions.push(CodeActionOrCommand::CodeAction(action));
        }
    }

    actions
}

/// Create a quick-fix code action from a diagnostic with suggestion data.
fn create_fix_from_diagnostic(diagnostic: &Diagnostic, uri: &Url) -> Option<CodeAction> {
    // Get suggestion from diagnostic data
    let data = diagnostic.data.as_ref()?;
    let suggestion = data.get("suggestion")?.as_str()?;

    // Create the text edit that replaces the diagnostic range with the suggestion
    let edit = TextEdit {
        range: diagnostic.range,
        new_text: suggestion.to_string(),
    };

    // Build workspace edit with changes to this document
    let mut changes = HashMap::new();
    changes.insert(uri.clone(), vec![edit]);

    let workspace_edit = WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    };

    // Create the code action
    Some(CodeAction {
        title: format!("Replace with '{}'", truncate_suggestion(suggestion, 40)),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diagnostic.clone()]),
        edit: Some(workspace_edit),
        command: None,
        is_preferred: Some(true),
        disabled: None,
        data: None,
    })
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

    #[test]
    fn test_truncate_suggestion() {
        assert_eq!(truncate_suggestion("short", 40), "short");
        assert_eq!(
            truncate_suggestion("this is a very long suggestion that should be truncated", 20),
            "this is a very lo..."
        );
        assert_eq!(
            truncate_suggestion("multi\nline\nsuggestion", 40),
            "multi"
        );
    }

    #[test]
    fn test_create_fix_from_diagnostic_with_suggestion() {
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
            severity: None,
            code: None,
            code_description: None,
            source: Some("fleet-lsp".to_string()),
            message: "Invalid value".to_string(),
            related_information: None,
            tags: None,
            data: Some(serde_json::json!({
                "suggestion": "darwin"
            })),
        };

        let uri = Url::parse("file:///test.yml").unwrap();
        let action = create_fix_from_diagnostic(&diagnostic, &uri);

        assert!(action.is_some());
        let action = action.unwrap();
        assert_eq!(action.title, "Replace with 'darwin'");
        assert_eq!(action.kind, Some(CodeActionKind::QUICKFIX));
        assert_eq!(action.is_preferred, Some(true));
    }

    #[test]
    fn test_create_fix_from_diagnostic_without_suggestion() {
        let diagnostic = Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 5,
                },
            },
            severity: None,
            code: None,
            code_description: None,
            source: Some("fleet-lsp".to_string()),
            message: "Invalid value".to_string(),
            related_information: None,
            tags: None,
            data: None, // No suggestion data
        };

        let uri = Url::parse("file:///test.yml").unwrap();
        let action = create_fix_from_diagnostic(&diagnostic, &uri);

        assert!(action.is_none());
    }
}
