//! Conversion utilities from LintError to LSP Diagnostic.

use tower_lsp::lsp_types::{
    CodeDescription, Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, DiagnosticTag,
    Location, NumberOrString, Position, Range, Url,
};

use super::fleet::GitOpsError;
use super::position::to_lsp_position;
use flint_lint::error::{LintError, Severity};

/// Convert a LintError to an LSP Diagnostic.
pub fn lint_error_to_diagnostic(error: &LintError, source: &str) -> Diagnostic {
    let range = error_to_range(error, source);
    let severity = match error.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    };

    let message = format_message(error);

    // Serialize the error's whole `Fix` into `data` so code actions rebuild
    // it losslessly — one fix representation shared with `flint check --fix`.
    let data = error.fix.as_ref().map(|fix| {
        serde_json::json!({
            "fix": fix,
            "help": error.help,
        })
    });

    // DEPRECATED tag (strikethrough in editors) — driven by the code
    // registry instead of the old fragile message string-match. The
    // deprecated-keys rule covers both its "is deprecated" and "was removed"
    // phrasings.
    let tags = error
        .rule_code
        .and_then(flint_lint::codes::meta)
        .filter(|m| m.is_deprecation)
        .map(|_| vec![DiagnosticTag::DEPRECATED]);

    // Cross-file findings carry the OTHER path(s) involved (ADR-010
    // `related`) — surfaced as clickable related-information entries so a
    // broken reference links straight to the renamed/missing target.
    let related_information = if error.related.is_empty() {
        None
    } else {
        let items: Vec<DiagnosticRelatedInformation> = error
            .related
            .iter()
            .filter_map(|p| {
                let uri = Url::from_file_path(p).ok()?;
                Some(DiagnosticRelatedInformation {
                    location: Location {
                        uri,
                        range: Range::default(),
                    },
                    message: "related file involved in this finding".to_string(),
                })
            })
            .collect();
        (!items.is_empty()).then_some(items)
    };

    // Diagnostic code + documentation link (stamped by the engine from the
    // code registry — no parallel URL table here anymore).
    let code = error
        .rule_code
        .map(|c| NumberOrString::String(c.to_string()));
    let code_description = error
        .doc_url
        .and_then(|u| Url::parse(u).ok())
        .map(|href| CodeDescription { href });

    Diagnostic {
        range,
        severity: Some(severity),
        code,
        code_description,
        source: Some("fleet-lint".to_string()),
        message,
        related_information,
        tags,
        data,
    }
}

/// Convert error location to LSP Range.
///
/// The span's `len` drives the highlight width; a zero `len` falls back to
/// the context width (the matched token) or a single character — matching
/// the historical estimate exactly.
fn error_to_range(error: &LintError, source: &str) -> Range {
    match error.span {
        Some(span) => {
            let start = to_lsp_position(span.line, span.column, source);
            let width = if span.len > 0 {
                span.len
            } else {
                error.context.as_ref().map(|c| c.len()).unwrap_or(1)
            };
            let end = to_lsp_position(span.line, span.column + width, source);
            Range { start, end }
        }
        None => {
            // No location - highlight first line
            Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            }
        }
    }
}

/// Convert a GitOps dry-run error to an LSP Diagnostic.
///
/// GitOps errors are not line-specific (they come from the server),
/// so they always appear at line 0 with source "fleet-gitops".
pub fn gitops_error_to_diagnostic(error: &GitOpsError) -> Diagnostic {
    let message = if let Some(hint) = &error.hint {
        format!("{}\n\n→ {}", error.message, hint)
    } else {
        error.message.clone()
    };
    Diagnostic {
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 0,
            },
        },
        severity: Some(DiagnosticSeverity::ERROR),
        source: Some("fleet-gitops".to_string()),
        message,
        ..Default::default()
    }
}

/// Format the diagnostic message with help text.
fn format_message(error: &LintError) -> String {
    let mut msg = error.message.clone();

    if let Some(help) = &error.help {
        msg.push_str("\n\nHelp: ");
        msg.push_str(help);
    }

    msg
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_lint_error_to_diagnostic() {
        let mut error =
            LintError::error("Missing required field 'query'", PathBuf::from("test.yml"))
                .with_location(5, 3)
                .with_context("name")
                .with_help("Policies must have a query field")
                .with_rule_code(flint_lint::codes::REQUIRED_FIELDS)
                .with_suggestion("query: \"SELECT 1;\"");
        // The engine stamps doc_url from the code registry before diagnostics
        // reach the LSP; mirror that contract here.
        error.doc_url = flint_lint::codes::doc_url(flint_lint::codes::REQUIRED_FIELDS);

        let source = "policies:\n  - name: test\n    platform: darwin\n";
        let diagnostic = lint_error_to_diagnostic(&error, source);

        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diagnostic.source, Some("fleet-lint".to_string()));
        assert!(diagnostic.message.contains("Missing required field"));
        assert!(diagnostic.message.contains("Help:"));
        // The whole Fix is serialized into data for code actions.
        let data = diagnostic.data.as_ref().expect("fix data");
        assert_eq!(data["fix"]["kind"], "replace");
        assert_eq!(data["fix"]["new"], "query: \"SELECT 1;\"");

        // Verify diagnostic code and doc link
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("required-fields".to_string()))
        );
        assert!(diagnostic.code_description.is_some());
        let desc = diagnostic.code_description.unwrap();
        assert!(desc
            .href
            .as_str()
            .contains("fleetdm.com/docs/configuration/yaml-files"));
    }
}
