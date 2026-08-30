//! `flint check` — lint YAML file(s) with Fleet-specific validation.

use crate::args::CheckArgs;
use crate::ci::github::{post_pr_comment, resolve_format_for_git};
use crate::commands::apply_fixes;
use crate::output::json::lint_report_to_json;
use crate::output::markdown::render_markdown_report;
use crate::overlay::{resolve_overlay_path, run_overlay_merge, OverlayTempFile};
use flint_lint as linter;
use std::path::PathBuf;

/// Tell the reader that some of these findings resolve themselves.
///
/// A finding's `help:` explains the problem; nothing in the per-finding output
/// says `--fix` would apply a remedy, so the fixable ones are invisible unless
/// we say so. Counts come from [`linter::LintReport::fixable_counts`], which
/// asks the applier rather than reading the `Fix` variant, so this line can
/// never promise more than `--fix` delivers.
fn print_fix_hint(results: &[(PathBuf, linter::error::LintReport)]) {
    use colored::Colorize;

    let (mut safe, mut unsafe_only) = (0usize, 0usize);
    for (_, report) in results {
        let (s, u) = report.fixable_counts();
        safe += s;
        unsafe_only += u;
    }
    if safe == 0 && unsafe_only == 0 {
        return;
    }

    let mut hint = String::new();
    if safe > 0 {
        hint.push_str(&format!("{safe} auto-fixable — run `flint check --fix`"));
    }
    if unsafe_only > 0 {
        if safe > 0 {
            hint.push_str("; ");
        }
        hint.push_str(&format!(
            "{unsafe_only} more with `flint check --fix --unsafe-fixes`"
        ));
    }
    println!("  {} {}", "↳".cyan(), hint);
}

/// Suggest the commands that follow from THIS run.
///
/// A summary that ends in a count leaves the reader to work out what to do
/// next, and the answer depends on which rules fired — orphans want a
/// different command from broken paths, and a snapshot-derived block wants a
/// refresh rather than an edit. Each line below is emitted only when a finding
/// of that kind is actually present, so the list stays short and never
/// suggests a command that would print nothing.
fn print_next_steps(results: &[(PathBuf, linter::error::LintReport)], errors: usize) {
    use colored::Colorize;
    use linter::codes;

    let has = |code: &str| {
        results.iter().any(|(_, r)| {
            r.errors
                .iter()
                .chain(&r.warnings)
                .chain(&r.infos)
                .any(|e| e.rule_code == Some(code))
        })
    };

    let mut steps: Vec<String> = Vec::new();
    if has(codes::ORPHANED_FILE) {
        steps.push(
            "flint paths --unwired --oneline    files nothing references (add | grep <name>)"
                .to_string(),
        );
    }
    if has(codes::PATH_EXISTS) {
        steps.push("flint paths                        where broken references moved to".to_string());
    }
    // Only reachable with a snapshot, and the remedy is server state rather
    // than an edit, so it must not be lumped in with --fix.
    if results.iter().any(|(_, r)| {
        r.errors
            .iter()
            .any(|e| e.message == linter::snapshot::HASH_NOT_UPLOADED)
    }) {
        steps.push(
            "flint dry-run --refresh-snapshot   re-read the server before judging uploads"
                .to_string(),
        );
    }
    if errors == 0 {
        steps.push("flint dry-run .                    would `fleetctl gitops` accept this?".to_string());
    }

    if steps.is_empty() {
        return;
    }
    println!("\n{}", "Next:".bold());
    for s in steps {
        println!("  {s}");
    }
}

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
    } else if single_file && results.is_empty() {
        // The one file asked for was filtered out by `[files]`. Indexing
        // `results[0]` here used to panic with "index out of bounds", which
        // read as a crash rather than the scoping decision it actually is.
        println!(
            "{} {} is not in scope for this repo's `[files]` configuration — nothing to check.",
            "•".dimmed(),
            lint_paths[0].display()
        );
    } else if single_file {
        let (file_path, report) = &results[0];
        println!("{} Linting {}...\n", "🔍".blue(), file_path.display());
        let source = std::fs::read_to_string(file_path).ok();
        print!("{}", report.render(source.as_deref()));
        if !fix {
            print_fix_hint(&results);
            print_next_steps(&results, total_errors);
        }
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
        // Verdict first: the counts answer "how much?", but the reader's
        // actual question is "am I clear?". Zero rows are omitted — three
        // lines of "0" push the one number that matters out of the eye's
        // path, and their absence already says zero.
        let headline = if total_errors > 0 {
            format!("{} — {} error(s)", "BLOCKED".red().bold(), total_errors)
        } else if total_warnings > 0 {
            format!("{} — no errors", "OK".green().bold())
        } else {
            format!("{} — clean", "OK".green().bold())
        };
        println!("{headline}   ({files_linted} file(s) linted)");
        if total_warnings > 0 {
            println!("  {} warning(s)", total_warnings.to_string().yellow());
        }
        if total_infos > 0 {
            println!("  {} info", total_infos.to_string().blue());
        }
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
        print_repeats(&results);
        if !fix {
            print_fix_hint(&results);
            print_next_steps(&results, total_errors);
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

/// How many files must carry the identical finding before it is indexed.
const REPEAT_THRESHOLD: usize = 5;

/// Findings whose exact text recurs across many files.
///
/// One dead glob copied into 32 fleets renders as 32 warnings and is one
/// problem; so is a hash the server lacks, referenced from 29 software files.
/// The per-file rendering above stays complete — this is the index a
/// 139-line wall was missing, so a reader can see that most of it is a few
/// problems repeated rather than many problems.
fn print_repeats(results: &[(PathBuf, linter::error::LintReport)]) {
    use colored::Colorize;
    use std::collections::BTreeMap;

    // (severity rank, message) -> files carrying it
    let mut groups: BTreeMap<(u8, String), Vec<&PathBuf>> = BTreeMap::new();
    for (path, report) in results {
        for (rank, list) in [(0u8, &report.errors), (1, &report.warnings), (2, &report.infos)] {
            for e in list {
                groups.entry((rank, e.message.clone())).or_default().push(path);
            }
        }
    }
    let mut rows: Vec<_> = groups
        .into_iter()
        .map(|(k, mut files)| {
            files.sort();
            files.dedup();
            (k, files)
        })
        .filter(|(_, files)| files.len() >= REPEAT_THRESHOLD)
        .collect();
    if rows.is_empty() {
        return;
    }
    rows.sort_by_key(|((rank, _), files)| (std::cmp::Reverse(files.len()), *rank));

    println!("\n{}", "Repeated across files".bold());
    for ((rank, msg), files) in rows {
        let sev = match rank {
            0 => "error".red().to_string(),
            1 => "warning".yellow().to_string(),
            _ => "info".blue().to_string(),
        };
        let shown: Vec<&str> = files
            .iter()
            .take(3)
            .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or_default())
            .collect();
        let more = files.len().saturating_sub(shown.len());
        let tail = if more > 0 { format!(", +{more} more") } else { String::new() };
        let msg: String = if msg.chars().count() > 84 {
            format!("{}…", msg.chars().take(83).collect::<String>())
        } else {
            msg
        };
        println!("  {}  {sev}  {msg}", format!("×{}", files.len()).bold());
        println!("        {}", format!("{}{tail}", shown.join(", ")).dimmed());
    }
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
