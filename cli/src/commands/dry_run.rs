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
    } = args;

    use colored::Colorize;
    use linter::Linter;

    if !path.is_dir() {
        anyhow::bail!(
            "dry-run expects a repo directory (got {}). Use `flint check <file>` for a single file.",
            path.display()
        );
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
