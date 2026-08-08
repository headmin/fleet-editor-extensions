//! GitHub-flavored-markdown rendering of a lint run (for `--format markdown`
//! and the `--git` PR-comment flow).

use flint_lint as linter;

/// Hidden marker emitted at the top of every markdown body. Lets a
/// future dedup pass identify flint-authored PR comments (e.g. for
/// edit-in-place updates) without trying to fingerprint the body.
pub(crate) const MARKDOWN_MARKER: &str = "<!-- flint-check-report -->";

/// Render a lint run as a GitHub-flavored-markdown comment body.
///
/// Output is a `## flint check` heading, a summary line, and one
/// `<details>` block per file that has at least one finding. The
/// per-file body is a table of `severity | line | rule | message` rows,
/// so a CI step can pipe this straight into `gh pr comment --body-file -`.
pub(crate) fn render_markdown_report(
    files: &[(String, &linter::error::LintReport)],
    files_linted: usize,
    errors: usize,
    warnings: usize,
    infos: usize,
    heading: Option<&str>,
) -> String {
    let mut out = String::new();
    // HTML comment is invisible in rendered markdown. Future dedup logic
    // (edit-in-place rather than stacking comments) can grep PR comments
    // for this marker to identify flint-authored bodies.
    out.push_str(MARKDOWN_MARKER);
    out.push('\n');
    out.push_str("## ");
    out.push_str(heading.unwrap_or("flint check"));
    out.push_str("\n\n");

    let total = errors + warnings + infos;
    if total == 0 {
        out.push_str(&format!(
            "✓ No issues found across {} file(s).\n",
            files_linted
        ));
        return out;
    }

    out.push_str(&format!(
        "**Summary:** {} error(s), {} warning(s), {} info across {} file(s).\n\n",
        errors, warnings, infos, files_linted
    ));

    for (path, report) in files {
        if report.total_issues() == 0 {
            continue;
        }
        out.push_str(&format!(
            "<details><summary><code>{}</code> — {} error(s), {} warning(s), {} info</summary>\n\n",
            md_escape(path),
            report.errors.len(),
            report.warnings.len(),
            report.infos.len()
        ));
        out.push_str("| Severity | Line | Rule | Message |\n");
        out.push_str("| --- | --- | --- | --- |\n");

        let rows = report
            .errors
            .iter()
            .chain(report.warnings.iter())
            .chain(report.infos.iter());
        for e in rows {
            let icon = match e.severity {
                linter::Severity::Error => "❌ error",
                linter::Severity::Warning => "⚠️ warning",
                linter::Severity::Info => "ℹ️ info",
            };
            let loc = match (e.line(), e.column()) {
                (Some(l), Some(c)) => format!("{}:{}", l, c),
                (Some(l), None) => l.to_string(),
                _ => "—".to_string(),
            };
            let rule = e
                .rule_code
                .map(|c| format!("`{}`", md_escape(c)))
                .unwrap_or_else(|| "—".to_string());
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                icon,
                loc,
                rule,
                md_escape(&e.message)
            ));
        }
        out.push_str("\n</details>\n\n");
    }
    out
}

/// Escape characters that would break a GitHub-markdown table cell.
///
/// Pipes terminate cells, backticks toggle inline code, and backslashes
/// need doubling so they don't escape the *next* character we emit.
pub(crate) fn md_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('`', "\\`")
        .replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_report() -> linter::error::LintReport {
        use linter::error::{FixSafety, LintError, LintReport, Severity};
        let mut r = LintReport::new();
        let mut err = LintError::error("missing required field `platforms`", "teams/ws.yml");
        err.severity = Severity::Error;
        err.span = Some(linter::error::Span { line: 12, column: 5, len: 0 });
        err.rule_code = Some("required-fields");
        r.add(err);

        let mut warn = LintError::warning("interval `1s` is very short", "teams/ws.yml");
        warn.span = Some(linter::error::Span::line(47));
        warn.rule_code = Some("interval-validation");
        warn.fix = Some(linter::error::Fix::Replace {
            old: None,
            new: "interval: 3600".into(),
            safety: FixSafety::Display,
        });
        r.add(warn);
        r
    }

    #[test]
    fn markdown_zero_findings_emits_clean_summary() {
        let report = linter::error::LintReport::new();
        let out = render_markdown_report(&[("teams/ws.yml".into(), &report)], 1, 0, 0, 0, None);
        assert!(out.starts_with(MARKDOWN_MARKER), "marker must lead the body");
        assert!(out.contains("## flint check"));
        assert!(out.contains("✓ No issues found"));
        // No <details> blocks when clean.
        assert!(!out.contains("<details>"));
    }

    #[test]
    fn markdown_marker_is_present_on_every_render() {
        // Marker must appear regardless of findings — dedup/edit-in-place
        // logic relies on it for both clean and dirty runs.
        let empty = linter::error::LintReport::new();
        let busy = make_report();
        let clean = render_markdown_report(&[("a.yml".into(), &empty)], 1, 0, 0, 0, None);
        let dirty = render_markdown_report(&[("b.yml".into(), &busy)], 1, 1, 1, 0, None);
        assert!(clean.contains(MARKDOWN_MARKER));
        assert!(dirty.contains(MARKDOWN_MARKER));
    }

    #[test]
    fn markdown_custom_heading_replaces_default() {
        // Monorepo case: two flint reports in one PR comment need
        // distinguishable headings so reviewers can tell them apart.
        let empty = linter::error::LintReport::new();
        let out = render_markdown_report(
            &[("a.yml".into(), &empty)],
            1,
            0,
            0,
            0,
            Some("Staging diff"),
        );
        assert!(out.contains("## Staging diff"));
        assert!(
            !out.contains("## flint check"),
            "custom heading must REPLACE the default, not append"
        );
        // Marker still leads regardless of heading.
        assert!(out.starts_with(MARKDOWN_MARKER));
    }

    #[test]
    fn markdown_renders_per_file_details_with_table() {
        let report = make_report();
        let pairs = vec![("teams/ws.yml".to_string(), &report)];
        let out = render_markdown_report(&pairs, 1, 1, 1, 0, None);

        assert!(out.contains("**Summary:** 1 error(s), 1 warning(s)"));
        assert!(out.contains("<details><summary><code>teams/ws.yml</code>"));
        assert!(out.contains("| Severity | Line | Rule | Message |"));
        assert!(out.contains("❌ error"));
        assert!(out.contains("⚠️ warning"));
        assert!(out.contains("`required-fields`"));
        assert!(out.contains("12:5"));
        // Files with zero issues must be skipped, but our single file has issues.
        assert_eq!(out.matches("<details>").count(), 1);
    }

    #[test]
    fn markdown_skips_files_with_no_findings() {
        let empty = linter::error::LintReport::new();
        let busy = make_report();
        let pairs = vec![
            ("a.yml".to_string(), &empty),
            ("b.yml".to_string(), &busy),
        ];
        let out = render_markdown_report(&pairs, 2, 1, 1, 0, None);
        assert!(!out.contains("a.yml"), "empty files must not appear");
        assert!(out.contains("b.yml"));
    }

    #[test]
    fn markdown_escapes_pipes_and_backticks_in_messages() {
        // Diagnostic messages can contain `|` (e.g. enum lists) and backticks
        // (quoting field names). Both would break a markdown table row if
        // emitted verbatim.
        use linter::error::{LintError, LintReport};
        let mut r = LintReport::new();
        let mut err = LintError::error(
            "value must be one of `a` | `b` | `c`",
            "teams/ws.yml",
        );
        err.span = Some(linter::error::Span::line(1));
        err.rule_code = Some("enum");
        r.add(err);

        let out = render_markdown_report(&[("teams/ws.yml".into(), &r)], 1, 1, 0, 0, None);
        assert!(out.contains("\\|"), "pipes must be escaped in cells");
        assert!(out.contains("\\`"), "backticks must be escaped in cells");
    }
}
