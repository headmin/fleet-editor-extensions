//! `flint dry-run` — local, server-free gate: would this repo likely pass
//! `fleetctl gitops`?

use crate::args::DryRunArgs;
use flint_lint as linter;
use std::path::PathBuf;

pub(crate) fn run(args: DryRunArgs) -> anyhow::Result<()> {
    let DryRunArgs {
        path,
        strict,
        exclude,
        json,
        refresh_snapshot,
    } = args;

    use colored::Colorize;
    use linter::Linter;

    if !path.is_dir() {
        anyhow::bail!(
            "dry-run expects a repo directory (got {}). Use `flint check <file>` for a single file.",
            path.display()
        );
    }

    if refresh_snapshot {
        refresh_snapshot_before_lint(&path, json);
    }

    let mut linter = Linter::from_path(&path);
    if !exclude.is_empty() {
        let mut cfg = linter.config().clone();
        cfg.files.exclude.extend(exclude);
        linter.set_config(cfg);
    }

    let results = linter.lint_directory(&path, None)?;

    // Collect blocking issues (errors always; warnings too under --strict)
    // and advisory warnings. Each entry: (file, &LintError).
    let mut blocking: Vec<(&PathBuf, &linter::error::LintError)> = Vec::new();
    let mut advisory = 0usize;
    for (f, report) in &results {
        for e in &report.errors {
            blocking.push((f, e));
        }
        for w in &report.warnings {
            if strict {
                blocking.push((f, w));
            } else {
                advisory += 1;
            }
        }
    }
    let files = results.len();

    if json {
        let items: Vec<_> = blocking
            .iter()
            .map(|(f, e)| {
                serde_json::json!({
                    "file": f.display().to_string(),
                    "line": e.line(),
                    "rule": e.rule_code,
                    "severity": format!("{:?}", e.severity),
                    "message": e.message,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "verdict": if blocking.is_empty() { "pass" } else { "fail" },
                "files": files,
                "blocking": items.len(),
                "advisory_warnings": advisory,
                "issues": items,
            }))?
        );
    } else if blocking.is_empty() {
        println!(
            "{} Local dry-run: {} — {} file(s), 0 blocking{}",
            "✓".green(),
            "PASS".green().bold(),
            files,
            if advisory > 0 {
                format!(" ({advisory} advisory warning(s) — run `flint check .` for detail, or --strict to gate)")
            } else {
                String::new()
            }
        );
    } else {
        println!(
            "{} Local dry-run: {} — {} issue(s) would block `fleetctl gitops`:\n",
            "✗".red(),
            "FAIL".red().bold(),
            blocking.len()
        );
        for (f, e) in &blocking {
            let loc = e.line().map(|l| format!(":{l}")).unwrap_or_default();
            let rule = e.rule_code.unwrap_or("-");
            println!(
                "  {}{}  [{}] {}",
                f.display().to_string().bold(),
                loc.dimmed(),
                rule.yellow(),
                e.message
            );
        }
        if !strict && advisory > 0 {
            println!(
                "\n  {}",
                format!("+ {advisory} advisory warning(s) (run with --strict to gate on these too)").dimmed()
            );
        }
        std::process::exit(2);
    }

    Ok(())
}

/// Re-read server state into `.fleet-snapshot.json` before the lint runs.
///
/// A snapshot is evidence of presence, not of absence: it can prove a hash was
/// on the server when captured, but "absent from the snapshot" only ever means
/// "absent at capture time". Upload a package and re-run, and the finding is a
/// confident block on a condition that is no longer true. Refreshing first
/// makes "not uploaded" a statement about now.
///
/// Deliberately fail-soft. Dry-run's contract is an offline, deterministic
/// answer; adding a network call must not turn an unreachable server into a
/// failed run. On any error the previous snapshot stands and the reason is
/// printed, so the result is visibly based on older evidence rather than
/// silently so.
fn refresh_snapshot_before_lint(path: &std::path::Path, quiet: bool) {
    use colored::Colorize;

    let out = path.join(linter::snapshot::SNAPSHOT_FILE_NAME);
    let result = crate::commands::fleet::FleetClient::from_environment()
        .and_then(|client| crate::commands::fleet::snapshot(&client, Some(out), false));

    match result {
        Ok(()) => {
            if !quiet {
                println!(
                    "{} refreshed {} from the Fleet server\n",
                    "✓".green(),
                    linter::snapshot::SNAPSHOT_FILE_NAME
                );
            }
        }
        Err(e) => {
            eprintln!(
                "{} could not refresh {}: {e}\n  Continuing with the existing snapshot — \
                 findings about server state reflect its capture time, not now.",
                "!".yellow(),
                linter::snapshot::SNAPSHOT_FILE_NAME
            );
        }
    }
}
