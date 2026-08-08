//! GitHub Actions integration for `flint check --git`: PR-context detection
//! from CI env vars and posting the markdown report via `gh pr comment`.

/// Reconcile the user's `--format` choice against `--git`.
///
/// `--git` only makes sense for markdown output, so:
/// - default (`text`) silently upgrades to `markdown` (simplest CI invocation).
/// - explicit `markdown` stays as-is.
/// - any other explicit format (e.g. `json`) is a user error and bails.
pub(crate) fn resolve_format_for_git(git: bool, format: &str) -> anyhow::Result<String> {
    if !git {
        return Ok(format.to_string());
    }
    match format {
        "markdown" | "text" => Ok("markdown".to_string()),
        other => anyhow::bail!(
            "--git implies --format markdown; got --format {other}. \
             Drop --format to use the default, or set it to markdown explicitly."
        ),
    }
}

/// PR context detected from CI environment variables.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PrContext {
    /// GitHub Actions running on a `pull_request` (or `pull_request_target`)
    /// event. PR number parsed from `GITHUB_REF` (`refs/pull/<n>/merge`).
    GithubActions { pr_number: String },
}

/// Extract the PR number from a GitHub Actions `GITHUB_REF` value.
///
/// On `pull_request` events the ref looks like `refs/pull/123/merge`.
/// Returns `None` for push/tag refs and other formats so the caller
/// can fall through to a clear "not a PR build" error.
pub(crate) fn parse_github_pr_ref(github_ref: &str) -> Option<&str> {
    let rest = github_ref.strip_prefix("refs/pull/")?;
    let num = rest.split('/').next()?;
    if num.is_empty() || !num.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(num)
}

/// Detect the current PR context from CI env vars.
///
/// Reads `GITHUB_ACTIONS` and `GITHUB_REF` from the process environment.
/// Returns an error explaining what's missing so CI logs surface the
/// concrete reason (wrong event, missing env var, not in CI).
pub(crate) fn detect_pr_context() -> anyhow::Result<PrContext> {
    if std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
        let github_ref = std::env::var("GITHUB_REF").map_err(|_| {
            anyhow::anyhow!("GITHUB_ACTIONS is set but GITHUB_REF is not — cannot find PR number")
        })?;
        let pr = parse_github_pr_ref(&github_ref).ok_or_else(|| {
            anyhow::anyhow!(
                "GITHUB_REF={github_ref} is not a pull_request ref (expected refs/pull/<n>/merge)"
            )
        })?;
        return Ok(PrContext::GithubActions {
            pr_number: pr.to_string(),
        });
    }
    if std::env::var("GITLAB_CI").as_deref() == Ok("true") {
        anyhow::bail!("GitLab CI is not yet supported in --git mode (GitHub Actions only for now)");
    }
    anyhow::bail!("no supported CI environment detected (expected GITHUB_ACTIONS=true)");
}

/// Post the markdown body as a PR comment via `gh pr comment`.
///
/// Shells out to `gh` to inherit the user's existing auth (the standard
/// `GITHUB_TOKEN` on GitHub-hosted runners). Stdout from `gh` propagates
/// to the user; non-zero exit becomes an error so the caller can log it.
/// Note: every invocation creates a new comment — dedup/edit-in-place is
/// a follow-up (see `MARKDOWN_MARKER`).
pub(crate) fn post_pr_comment(body: &str) -> anyhow::Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let ctx = detect_pr_context()?;
    let PrContext::GithubActions { pr_number } = ctx;

    let mut child = Command::new("gh")
        .args(["pr", "comment", &pr_number, "--body-file", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn `gh` (is it installed and on PATH?): {e}"))?;

    if let Some(stdin) = child.stdin.as_mut() {
        stdin.write_all(body.as_bytes())?;
    }
    let status = child.wait()?;
    if !status.success() {
        anyhow::bail!("`gh pr comment` exited with status {status}");
    }
    eprintln!("flint: posted check report to PR #{pr_number}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Parser for $GITHUB_REF on pull_request events. Critical that bogus
    // values (push refs, tags, malformed strings) return None so the
    // caller surfaces a clear error rather than posting to PR #0 or
    // truncating non-numeric noise.
    #[test]
    fn parse_github_pr_ref_accepts_pull_request_refs() {
        assert_eq!(parse_github_pr_ref("refs/pull/1/merge"), Some("1"));
        assert_eq!(parse_github_pr_ref("refs/pull/12345/merge"), Some("12345"));
        // pull_request_target also uses /merge but `/head` shows up too.
        assert_eq!(parse_github_pr_ref("refs/pull/42/head"), Some("42"));
    }

    #[test]
    fn parse_github_pr_ref_rejects_non_pr_refs() {
        // Push to main.
        assert_eq!(parse_github_pr_ref("refs/heads/main"), None);
        // Tag push.
        assert_eq!(parse_github_pr_ref("refs/tags/v1.0.0"), None);
        // Empty.
        assert_eq!(parse_github_pr_ref(""), None);
        // Right prefix, but non-numeric PR id (would silently post to a
        // nonsense PR if we didn't validate).
        assert_eq!(parse_github_pr_ref("refs/pull/abc/merge"), None);
        // Right prefix, empty number segment.
        assert_eq!(parse_github_pr_ref("refs/pull//merge"), None);
    }

    // `--git` validation. The pure resolver is testable; the dispatcher
    // just delegates to it.
    #[test]
    fn git_flag_off_passes_format_through_unchanged() {
        assert_eq!(resolve_format_for_git(false, "text").unwrap(), "text");
        assert_eq!(resolve_format_for_git(false, "json").unwrap(), "json");
        assert_eq!(
            resolve_format_for_git(false, "markdown").unwrap(),
            "markdown"
        );
    }

    #[test]
    fn git_flag_upgrades_default_text_to_markdown() {
        // The 90%-case CI invocation is `flint check --git` — no explicit
        // format. We silently pick markdown rather than erroring on the
        // default value the user didn't set.
        assert_eq!(resolve_format_for_git(true, "text").unwrap(), "markdown");
    }

    #[test]
    fn git_flag_keeps_explicit_markdown() {
        assert_eq!(
            resolve_format_for_git(true, "markdown").unwrap(),
            "markdown"
        );
    }

    #[test]
    fn git_flag_rejects_incompatible_format() {
        let err = resolve_format_for_git(true, "json").unwrap_err().to_string();
        assert!(
            err.contains("--git implies --format markdown"),
            "error must explain the conflict, got: {err}"
        );
        assert!(err.contains("json"), "error must echo the offending format");
    }
}
