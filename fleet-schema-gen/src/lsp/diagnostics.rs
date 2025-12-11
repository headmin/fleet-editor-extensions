//! Conversion utilities from LintError to LSP Diagnostic.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, Position, Range};

use crate::linter::error::{LintError, Severity};
use super::position::to_lsp_position;

/// Convert a LintError to an LSP Diagnostic.
pub fn lint_error_to_diagnostic(error: &LintError, source: &str) -> Diagnostic {
    let range = error_to_range(error, source);
    let severity = match error.severity {
        Severity::Error => DiagnosticSeverity::ERROR,
        Severity::Warning => DiagnosticSeverity::WARNING,
        Severity::Info => DiagnosticSeverity::INFORMATION,
    };

    let message = format_message(error);

    // Include suggestion in data for code actions
    let data = error.suggestion.as_ref().map(|s| {
        serde_json::json!({
            "suggestion": s,
            "help": error.help
        })
    });

    Diagnostic {
        range,
        severity: Some(severity),
        code: None,
        code_description: None,
        source: Some("fleet-lsp".to_string()),
        message,
        related_information: None,
        tags: None,
        data,
    }
}

/// Convert error location to LSP Range.
fn error_to_range(error: &LintError, source: &str) -> Range {
    match (error.line, error.column) {
        (Some(line), Some(col)) => {
            let start = to_lsp_position(line, col, source);
            // Estimate end position - highlight the word/context if available
            let end_col = col + error.context.as_ref().map(|c| c.len()).unwrap_or(1);
            let end = to_lsp_position(line, end_col, source);
            Range { start, end }
        }
        (Some(line), None) => {
            // Highlight the entire line
            let start = Position {
                line: (line.saturating_sub(1)) as u32,
                character: 0,
            };
            let line_content = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
            let end = Position {
                line: (line.saturating_sub(1)) as u32,
                character: line_content.len() as u32,
            };
            Range { start, end }
        }
        _ => {
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
        let error = LintError {
            severity: Severity::Error,
            message: "Missing required field 'query'".to_string(),
            file: PathBuf::from("test.yml"),
            line: Some(5),
            column: Some(3),
            context: Some("name".to_string()),
            help: Some("Policies must have a query field".to_string()),
            suggestion: Some("query: \"SELECT 1;\"".to_string()),
        };

        let source = "policies:\n  - name: test\n    platform: darwin\n";
        let diagnostic = lint_error_to_diagnostic(&error, source);

        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::ERROR));
        assert_eq!(diagnostic.source, Some("fleet-lsp".to_string()));
        assert!(diagnostic.message.contains("Missing required field"));
        assert!(diagnostic.message.contains("Help:"));
        assert!(diagnostic.data.is_some());
    }
}
