//! `flint check` — lint YAML file(s) with Fleet-specific validation.

use crate::args::CheckArgs;
use crate::ci::github::{post_pr_comment, resolve_format_for_git};
use crate::commands::apply_fixes;
use crate::output::json::lint_report_to_json;
use crate::output::markdown::render_markdown_report;
use crate::overlay::{resolve_overlay_path, run_overlay_merge, OverlayTempFile};
use flint_lint as linter;
use std::path::PathBuf;

pub(crate) fn run(args: CheckArgs) -> anyhow::Result<()> {
    let CheckArgs {
        paths,
        fix,
        unsafe_fixes,
        mut format,
        hook_mode,
        staged,
        detailed_exitcodes,
        git,
        heading,
        base,
        env,
        exclude,
    } = args;

    use colored::Colorize;
    use linter::Linter;

    // --git implies --format markdown. Reject the contradictory case
    // (--git --format json) early so CI fails fast instead of
    // posting a comment in the wrong format or skipping silently.
    format = resolve_format_for_git(git, &format)?;

    // Overlay merge: when --base/--env are set, replace the normal
    // path-walk lint with a focused "merge two files and lint the
    // result" mode. clap's `requires` already enforces that both
    // flags appear together — we just need to do the merge.
    //
    // The merged file is written inside the base file's parent
    // directory so any relative `path:` refs in the YAML (e.g.
    // `./policies/foo.yml`) resolve correctly. Writing to /tmp
    // would silently break those refs.
    // `Option` so both match arms can definitely-initialize the
    // binding. The guard's Drop removes the synthetic merged file
    // on scope exit — but `std::process::exit` skips destructors,
    // so we also drop it explicitly before any exit() call below.
    let mut overlay_guard: Option<OverlayTempFile> = None;
    let lint_paths: Vec<PathBuf> = match (&base, &env) {
        (Some(base_path), Some(env_path)) => {
            if fix || unsafe_fixes {
                anyhow::bail!(
                    "--fix is not supported with --base/--env yet — \
                     merged-result fixes are ambiguous (which source file gets the edit?)"
                );
            }
            // The overlay merge produces a single synthetic file, so it
            // only makes sense against one target.
            if paths.len() != 1 {
                anyhow::bail!(
                    "--base/--env operate on a single target path (got {})",
                    paths.len()
                );
            }
            let base_resolved = resolve_overlay_path(base_path, &paths[0]);
            let env_resolved = resolve_overlay_path(env_path, &paths[0]);
            let merged_path = run_overlay_merge(&base_resolved, &env_resolved)?;
            overlay_guard = Some(OverlayTempFile::new(merged_path.clone()));
            vec![merged_path]
        }
        _ => paths,
    };

    // Load config from the first target so any `.fleetlint.toml`
    // discovered in or above it is applied — `Linter::new()` skips it
    // (issue #5). When a pre-commit hook passes many staged files they
    // share one repo, so the first file's ancestors find the config.
    let mut linter = Linter::from_path(&lint_paths[0]);

    // Merge any `--exclude` globs into the loaded config (creating a
    // default config if the repo has no .fleetlint.toml), so they apply
    // alongside `files.exclude`.
    if !exclude.is_empty() {
        let mut cfg = linter.config().clone();
        cfg.files.exclude.extend(exclude.iter().cloned());
        linter.set_config(cfg);
    }
    let json_mode = format == "json";
    let markdown_mode = format == "markdown";
    let github_mode = format == "github";
    let structured_mode = json_mode || markdown_mode || github_mode;

    // Collect (path, report) across every input into one list.
    // Directory inputs run the cross-file graph pass (a whole repo is
    // needed to resolve references); file inputs are linted
    // individually but still filtered through `should_lint_file`, so
    // the `flint-files` pre-commit hook — which passes every staged
    // file — honors `[files]` include/exclude in .fleetlint.toml.
    let mut results: Vec<(PathBuf, linter::error::LintReport)> = Vec::new();
    let mut skipped_by_config = 0usize;
    // Keep the nicer single-file text output only when the user pointed
    // flint at exactly one file.
    let single_file = lint_paths.len() == 1 && lint_paths[0].is_file();

    for p in &lint_paths {
        if p.is_file() {
            if !linter.config().should_lint_file(p) {
                skipped_by_config += 1;
                continue;
            }
            let report = linter.lint_file(p)?;
            results.push((p.clone(), report));
        } else if p.is_dir() {
            results.extend(linter.lint_directory(p, None)?);
        } else {
            anyhow::bail!("Path does not exist: {}", p.display());
        }
    }

    // Pre-commit scoping: keep only findings that touch a staged path
    // (directly or via `related`); everything else becomes a context count.
    let mut staged_context: Option<usize> = None;
    if staged {
        let staged_set = git_staged_paths(&lint_paths[0])?;
        let mut dropped = 0usize;
        for (path, report) in &mut results {
            let file_staged = staged_set.contains(&lexical_normalize(path));
            let keep = |e: &linter::LintError| {
                file_staged
                    || staged_set.contains(&lexical_normalize(&e.file))
                    || e.related
                        .iter()
                        .any(|r| staged_set.contains(&lexical_normalize(r)))
            };
            for list in [&mut report.errors, &mut report.warnings, &mut report.infos] {
                let before = list.len();
                list.retain(&keep);
                dropped += before - list.len();
            }
        }
        results.retain(|(_, r)| !(r.errors.is_empty() && r.warnings.is_empty() && r.infos.is_empty()));
        staged_context = Some(dropped);
    }

    // Apply fixes if requested.
    if fix {
        let mut total_fixed = 0;
        for (file_path, report) in &results {
            if let Ok(n) = apply_fixes(file_path, report, unsafe_fixes) {
                total_fixed += n;
            }
        }
        if total_fixed > 0 && !structured_mode {
            println!("{} Fixed {} issue(s)\n", "✓".green(), total_fixed);
        }
    }

    let files_linted = results.len();
    let mut total_errors = 0;
    let mut total_warnings = 0;
    let mut total_infos = 0;
    for (_, report) in &results {
        total_errors += report.errors.len();
        total_warnings += report.warnings.len();
        total_infos += report.infos.len();
    }
    let mut markdown_body: Option<String> = None;

    if json_mode {
        let file_outputs: Vec<_> = results
            .iter()
            .map(|(file_path, report)| {
                lint_report_to_json(&file_path.display().to_string(), report)
            })
            .collect();
        let wrapper = serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "files": file_outputs,
            "summary": {
                "files_linted": files_linted,
                "errors": total_errors,
                "warnings": total_warnings,
                "infos": total_infos,
            }
        });
        println!("{}", serde_json::to_string_pretty(&wrapper)?);
    } else if github_mode {
        // Workflow commands go to stdout; GitHub scrapes the runner log.
        // Paths must be workspace-relative for the annotation to bind to the
        // diff, so strip the directory flint was pointed at.
        let root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        print!("{}", crate::output::github::render(&results, &root));
    } else if markdown_mode {
        let pairs: Vec<(String, &linter::error::LintReport)> = results
            .iter()
            .map(|(p, r)| (p.display().to_string(), r))
            .collect();
        let body = render_markdown_report(
            &pairs,
            files_linted,
            total_errors,
            total_warnings,
            total_infos,
            heading.as_deref(),
        );
        print!("{}", body);
        markdown_body = Some(body);
    } else if single_file {
        let (file_path, report) = &results[0];
        println!("{} Linting {}...\n", "🔍".blue(), file_path.display());
        let source = std::fs::read_to_string(file_path).ok();
        print!("{}", report.render(source.as_deref()));
    } else {
        println!("{} Linting {} path(s)...\n", "🔍".blue(), lint_paths.len());

        for (file_path, report) in &results {
            if report.total_issues() > 0 {
                println!("\n{} {}", "File:".bold(), file_path.display());

                if let Ok(source) = std::fs::read_to_string(file_path) {
                    print!("{}", report.render(Some(&source)));
                } else {
                    print!("{}", report.render(None));
                }
            }
        }

        println!("\n{}", "=".repeat(60));
        println!("{} Linted {} file(s)", "Summary:".bold(), files_linted);
        println!("  {} error(s)", total_errors.to_string().red());
        println!("  {} warning(s)", total_warnings.to_string().yellow());
        println!("  {} info", total_infos.to_string().blue());
        if skipped_by_config > 0 {
            println!(
                "  {}",
                format!("{skipped_by_config} file(s) skipped by [files] config").dimmed()
            );
        }
        if let Some(dropped) = staged_context {
            if dropped > 0 {
                println!(
                    "  {}",
                    format!(
                        "{dropped} pre-existing finding(s) elsewhere (not blocking — \
                         run `flint check .` to see all)"
                    )
                    .dimmed()
                );
            }
        }
    }

    // Post BEFORE applying the exit code — a non-zero exit
    // shouldn't skip surfacing findings to the PR.
    if git {
        if let Some(body) = &markdown_body {
            if let Err(e) = post_pr_comment(body) {
                eprintln!("flint: --git post skipped: {}", e);
            }
        }
    }

    // Drop the overlay guard BEFORE any std::process::exit call.
    // exit() skips destructors, which would leak the synthetic
    // .flint-overlay-merge-<pid>.yml file into the user's repo.
    drop(overlay_guard);

    if !hook_mode {
        let total_findings = total_errors + total_warnings + total_infos;
        let code = resolve_exit_code(detailed_exitcodes, total_errors, total_findings);
        if code != 0 {
            std::process::exit(code);
        }
    }

    Ok(())
}

/// The staged file set, absolute + lexically normalized. `--no-renames` is
/// load-bearing: with rename detection on, only the NEW path is reported,
/// and the OLD path — the one every reference still points at — escapes
/// staged scope (ADR-010).
fn git_staged_paths(start: &std::path::Path) -> anyhow::Result<std::collections::HashSet<PathBuf>> {
    use std::process::Command;
    let dir = if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(std::path::Path::new("."))
    };
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()?;
    if !top.status.success() {
        anyhow::bail!("--staged requires a git repository");
    }
    let root = PathBuf::from(String::from_utf8_lossy(&top.stdout).trim());
    let out = Command::new("git")
        .args(["diff", "--cached", "--name-only", "--no-renames"])
        .current_dir(&root)
        .output()?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| lexical_normalize(&root.join(l)))
        .collect())
}

/// Collapse `.`/`..` without touching the filesystem, so paths of deleted
/// (staged-for-removal) files still compare equal.
fn lexical_normalize(p: &std::path::Path) -> PathBuf {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    let mut out = PathBuf::new();
    for c in abs.components() {
        match c {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

/// Map a lint run's counts to a process exit code.
///
/// Default (no `--detailed-exitcodes`): `0` unless errors were found, in which
/// case `1`. Warnings and infos do not affect the exit code — preserves the
/// pre-flag behavior so existing CI doesn't break.
///
/// With `--detailed-exitcodes` (terraform/fleet-plan convention):
/// `0` = no findings of any severity, `2` = any finding. Engine errors
/// propagate via `anyhow::Result` from `main` and surface as `1` regardless
/// of this flag.
pub(crate) fn resolve_exit_code(
    detailed_exitcodes: bool,
    errors: usize,
    total_findings: usize,
) -> i32 {
    if detailed_exitcodes {
        if total_findings > 0 {
            2
        } else {
            0
        }
    } else if errors > 0 {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Exit code matrix — preserves the legacy 0/1 contract by default and
    // promotes warnings/infos to exit-2 only when --detailed-exitcodes is set.
    #[test]
    fn exit_code_default_zero_when_clean() {
        assert_eq!(resolve_exit_code(false, 0, 0), 0);
    }

    #[test]
    fn exit_code_default_one_on_errors_only() {
        // Legacy contract: warnings/infos must NOT trigger non-zero exit.
        assert_eq!(resolve_exit_code(false, 0, 5), 0);
        assert_eq!(resolve_exit_code(false, 3, 8), 1);
    }

    #[test]
    fn exit_code_detailed_zero_when_no_findings() {
        assert_eq!(resolve_exit_code(true, 0, 0), 0);
    }

    #[test]
    fn exit_code_detailed_two_on_any_finding() {
        // Warnings alone, infos alone, and errors alone all surface as 2.
        assert_eq!(resolve_exit_code(true, 0, 1), 2);
        assert_eq!(resolve_exit_code(true, 1, 1), 2);
        assert_eq!(resolve_exit_code(true, 0, 7), 2);
    }
}
