//! JSON rendering of a lint run (for `--format json`).

use flint_lint as linter;

/// Convert a LintReport to a JSON value for structured output.
pub(crate) fn lint_report_to_json(
    file_path: &str,
    report: &linter::error::LintReport,
) -> serde_json::Value {
    let to_json = |errors: &[linter::LintError]| -> Vec<serde_json::Value> {
        errors
            .iter()
            .map(|e| {
                let mut obj = serde_json::json!({
                    "message": e.message,
                    "severity": match e.severity {
                        linter::Severity::Error => "error",
                        linter::Severity::Warning => "warning",
                        linter::Severity::Info => "info",
                    },
                });
                let m = obj.as_object_mut().unwrap();
                if let Some(line) = e.line() {
                    m.insert("line".into(), serde_json::json!(line));
                }
                if let Some(col) = e.column() {
                    m.insert("column".into(), serde_json::json!(col));
                }
                if let Some(ref code) = e.rule_code {
                    m.insert("rule".into(), serde_json::json!(code));
                }
                if let Some(ref help) = e.help {
                    m.insert("help".into(), serde_json::json!(help));
                }
                if let Some(suggestion) = e.suggestion() {
                    m.insert("suggestion".into(), serde_json::json!(suggestion));
                }
                if let Some(ref ctx) = e.context {
                    m.insert("context".into(), serde_json::json!(ctx));
                }
                if !e.related.is_empty() {
                    let related: Vec<String> = e
                        .related
                        .iter()
                        .map(|p| p.display().to_string())
                        .collect();
                    m.insert("related".into(), serde_json::json!(related));
                }
                obj
            })
            .collect()
    };

    let mut diagnostics = to_json(&report.errors);
    diagnostics.extend(to_json(&report.warnings));
    diagnostics.extend(to_json(&report.infos));

    serde_json::json!({
        "path": file_path,
        "diagnostics": diagnostics,
        "counts": {
            "errors": report.errors.len(),
            "warnings": report.warnings.len(),
            "infos": report.infos.len(),
        }
    })
}
