//! GitHub Actions workflow-command output.
//!
//! Emits one [workflow command] per finding, which GitHub renders as an
//! **inline annotation on the pull-request diff**, on the offending line.
//!
//! This is the plainest possible CI integration: it needs no token, no `gh`
//! binary, and no API call — GitHub parses the runner's stdout. Compare
//! `--git`, which posts one markdown comment and requires `gh` on PATH, a
//! token with PR-comment scope, and a `pull_request` event (it errors on a
//! push build). `--format github` works on every event, including pushes and
//! merge queues.
//!
//! Annotation placement depends on the diagnostic's span, so this shares the
//! precision of the terminal renderer's caret: a rule pointing at column 1
//! annotates the indentation.
//!
//! [workflow command]: https://docs.github.com/en/actions/reference/workflow-commands-for-github-actions

use flint_lint::error::{LintError, LintReport, Severity};
use std::path::Path;

/// Render every finding as a workflow command, one per line.
///
/// `root` is stripped from paths when possible: GitHub matches annotations to
/// the diff by repository-relative path, so an absolute path from the runner's
/// workspace silently produces an annotation attached to no file.
pub(crate) fn render(results: &[(std::path::PathBuf, LintReport)], root: &Path) -> String {
    let mut out = String::new();
    for (path, report) in results {
        // Two shapes reach here: absolute paths (flint pointed at an
        // absolute directory) and `./`-prefixed ones (pointed at `.`, the
        // usual CI invocation). `strip_prefix` only handles the first, and a
        // surviving `./` is not a no-op to GitHub — it fails to bind the
        // annotation to the diff, so the finding renders attached to nothing.
        let rel = path.strip_prefix(root).unwrap_or(path);
        let rel = rel.to_string_lossy().replace('\\', "/");
        let rel = rel.strip_prefix("./").unwrap_or(&rel).to_string();
        for finding in report
            .errors
            .iter()
            .chain(&report.warnings)
            .chain(&report.infos)
        {
            out.push_str(&command_for(finding, &rel));
            out.push('\n');
        }
    }
    out
}

/// One `::level file=…,line=…,col=…,title=…::message` line.
fn command_for(err: &LintError, rel_path: &str) -> String {
    // `notice` is GitHub's spelling; there is no `info` level.
    let level = match err.severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "notice",
    };

    let mut props = format!("file={}", escape_property(rel_path));
    if let Some(span) = err.span {
        props.push_str(&format!(",line={},col={}", span.line, span.column));
        // endColumn makes the annotation underline the exact token rather
        // than the whole line, matching the terminal caret.
        if span.len > 0 {
            props.push_str(&format!(",endColumn={}", span.column + span.len));
        }
    }
    let title = match err.rule_code {
        Some(code) => format!("flint ({code})"),
        None => "flint".to_string(),
    };
    props.push_str(&format!(",title={}", escape_property(&title)));

    // The help text is worth carrying — it is usually the actionable half —
    // and a multi-line annotation body is legal once newlines are escaped.
    let mut body = err.message.clone();
    if let Some(help) = &err.help {
        body.push_str("\nhelp: ");
        body.push_str(help);
    }

    format!("::{level} {props}::{}", escape_data(&body))
}

/// Escape a property value. Commas and colons would otherwise terminate the
/// property list, so a Windows-style path or a message containing `::` would
/// truncate the command and produce a mangled annotation.
fn escape_property(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace(':', "%3A")
        .replace(',', "%2C")
}

/// Escape a command's message body. Only `%`, CR and LF are special here —
/// commas and colons are fine after the `::` separator.
fn escape_data(s: &str) -> String {
    s.replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

#[cfg(test)]
mod tests {
    use super::*;
    use flint_lint::error::Span;
    use std::path::PathBuf;

    fn report_with(err: LintError) -> Vec<(PathBuf, LintReport)> {
        let mut r = LintReport::new();
        r.add(err);
        vec![(PathBuf::from("/repo/fleets/a.yml"), r)]
    }

    #[test]
    fn renders_a_workflow_command_with_span_and_code() {
        let err = LintError::error("invalid platform 'darwn'", "/repo/fleets/a.yml")
            .with_span(Span::token(3, 15, 5))
            .with_rule_code("platform-compatibility")
            .with_help("Valid platforms: darwin, windows");

        let out = render(&report_with(err), Path::new("/repo"));
        assert_eq!(
            out.trim_end(),
            "::error file=fleets/a.yml,line=3,col=15,endColumn=20,\
             title=flint (platform-compatibility)::invalid platform 'darwn'%0Ahelp: \
             Valid platforms%3A darwin, windows"
                .replace("%3A", ":")
                .as_str(),
            "got: {out}"
        );
    }

    /// Paths must be repository-relative. An absolute runner path produces an
    /// annotation GitHub cannot attach to the diff — it renders, but against
    /// no file, which looks like the tool found nothing.
    #[test]
    fn strips_the_workspace_root_from_paths() {
        let err = LintError::warning("m", "/repo/fleets/a.yml").with_span(Span::at(1, 1));
        let out = render(&report_with(err), Path::new("/repo"));
        assert!(out.contains("file=fleets/a.yml"), "got: {out}");
        assert!(!out.contains("/repo/"), "absolute path leaked: {out}");
    }

    /// `flint check .` — the usual CI invocation — yields `./fleets/a.yml`,
    /// which `strip_prefix` cannot touch because it is already relative. The
    /// `./` must still go: GitHub will not bind such a path to the diff, so
    /// the annotation renders against no file and the run looks clean.
    /// Found by running against a real repo, not by this test.
    #[test]
    fn strips_a_leading_dot_slash_from_relative_paths() {
        let mut r = LintReport::new();
        r.add(LintError::warning("m", "./fleets/a.yml").with_span(Span::at(1, 1)));
        let results = vec![(PathBuf::from("./fleets/a.yml"), r)];

        let out = render(&results, Path::new("/somewhere/else"));
        assert!(out.contains("file=fleets/a.yml"), "got: {out}");
        assert!(!out.contains("./fleets"), "'./' prefix survived: {out}");
    }

    /// Severity names are GitHub's, not flint's: there is no `info` level.
    #[test]
    fn info_maps_to_notice() {
        let err = LintError::info("m", "/repo/fleets/a.yml");
        let out = render(&report_with(err), Path::new("/repo"));
        assert!(out.starts_with("::notice "), "got: {out}");
    }

    /// Escaping matches GitHub's own toolkit, which treats the two halves of
    /// a command differently: PROPERTY values escape `:` and `,` because those
    /// terminate the property list, while the DATA body escapes only `%`, CR
    /// and LF. `::` inside a message is therefore legal and left alone — the
    /// parser takes everything after the first separator as data.
    ///
    /// Worth pinning, because over-escaping is as wrong as under-escaping:
    /// turning `:` into `%3A` in a message would show users mangled text.
    #[test]
    fn escapes_per_github_rules_for_properties_and_data() {
        let err = LintError::error("bad ::marker, 100% done", "/repo/a.yml")
            .with_span(Span::at(1, 1))
            .with_rule_code("x");
        let out = render(&report_with(err), Path::new("/repo"));

        // Data: percent escaped, colon and comma left readable.
        assert!(
            out.trim_end().ends_with("::bad ::marker, 100%25 done"),
            "got: {out}"
        );

        // Property: a title containing a colon would otherwise end the list.
        assert_eq!(
            escape_property("flint (a:b,c)"),
            "flint (a%3Ab%2Cc)",
            "property values must escape : and ,"
        );
        // And a newline in a message must never split the command in two.
        assert_eq!(escape_data("a\nb"), "a%0Ab");
    }

    /// No span means no line/col — GitHub then attaches the annotation to the
    /// file rather than guessing a line.
    #[test]
    fn omits_position_when_there_is_no_span() {
        let err = LintError::error("m", "/repo/a.yml");
        let out = render(&report_with(err), Path::new("/repo"));
        assert!(!out.contains("line="), "got: {out}");
    }
}
