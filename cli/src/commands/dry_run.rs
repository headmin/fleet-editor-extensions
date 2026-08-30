//! `flint dry-run` — local, server-free gate: would this repo likely pass
//! `fleetctl gitops`?

use crate::args::DryRunArgs;
use anyhow::Context;
use flint_lint as linter;
use std::path::PathBuf;

pub(crate) fn run(args: DryRunArgs) -> anyhow::Result<()> {
    let DryRunArgs {
        path,
        strict,
        exclude,
        json,
        refresh_snapshot,
        assume_uploaded,
        against,
        oracle,
    } = args;

    use colored::Colorize;
    use linter::Linter;

    if !path.is_dir() {
        anyhow::bail!(
            "dry-run expects a repo directory (got {}). Use `flint check <file>` for a single file.",
            path.display()
        );
    }

    // `--refresh-snapshot`, or `[fleet] refresh_snapshot = true` for a repo
    // that always wants it. The flag can only turn the behaviour ON: a repo
    // opting in has decided its dry-run depends on the server, and a silent
    // per-run opt-out would make an offline answer look like a live one.
    let config_opt_in = linter::FleetLintConfig::find_and_load(&path)
        .map(|(_, c)| c.fleet.refresh_snapshot)
        .unwrap_or(false);
    if refresh_snapshot || config_opt_in {
        refresh_snapshot_before_lint(&path, json);
    }

    // --against REF: lint the merge git WOULD produce. The scratch dir must
    // outlive the lint, so it is held here rather than inside the branch.
    let mut _scratch: Option<tempfile::TempDir> = None;
    let lint_root: PathBuf = match against.as_deref() {
        None => path.clone(),
        Some(r) => {
            let repo = git_toplevel(&path)?;
            let (tree, conflicts) = merge_tree(&repo, r)?;
            if !conflicts.is_empty() {
                eprintln!(
                    "{} HEAD and {r} conflict textually in {} file(s); resolve the merge \
                     first — flint cannot lint a tree that does not exist yet:",
                    "✗".red(),
                    conflicts.len()
                );
                for c in &conflicts {
                    eprintln!("    {c}");
                }
                std::process::exit(2);
            }
            let dir = tempfile::Builder::new()
                .prefix("flint-against-")
                .tempdir()
                .context("failed to create a scratch directory")?;
            crate::commands::materialize_tree(&repo, &tree, dir.path())?;
            // The snapshot is local state, not part of any tree: carry it
            // over so server-side findings behave as they would on HEAD.
            let snap = path.join(linter::snapshot::SNAPSHOT_FILE_NAME);
            if snap.is_file() {
                let _ = std::fs::copy(&snap, dir.path().join(linter::snapshot::SNAPSHOT_FILE_NAME));
            }
            if !json {
                println!(
                    "{} Linting the merge of HEAD and {} (tree {})\n",
                    "→".dimmed(),
                    r.bold(),
                    &tree[..tree.len().min(12)]
                );
            }
            let root = dir.path().to_path_buf();
            _scratch = Some(dir);
            root
        }
    };

    let mut linter = Linter::from_path(&lint_root);
    if !exclude.is_empty() {
        let mut cfg = linter.config().clone();
        cfg.files.exclude.extend(exclude);
        linter.set_config(cfg);
    }

    let mut results = linter.lint_directory(&lint_root, None)?;
    // Report merge-preview paths the way a normal run does, not under a
    // scratch directory nobody will look in.
    if against.is_some() {
        for (p, _) in &mut results {
            if let Ok(rel) = p.strip_prefix(&lint_root) {
                *p = PathBuf::from(".").join(rel);
            }
        }
    }

    // Collect blocking issues (errors always; warnings too under --strict)
    // and advisory warnings. Each entry: (file, &LintError).
    let mut blocking: Vec<(&PathBuf, &linter::error::LintError)> = Vec::new();
    let mut advisory = 0usize;
    let mut assumed = 0usize;
    for (f, report) in &results {
        for e in &report.errors {
            // --assume-uploaded drops exactly the finding that exists only
            // because a snapshot was consulted. Counted, not silently
            // dropped: a PASS that rests on an assumption has to say so.
            if assume_uploaded && e.message == linter::snapshot::HASH_NOT_UPLOADED {
                assumed += 1;
                continue;
            }
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
        if assumed > 0 {
            // The verdict is conditional, so say what it rests on. Without
            // this the run is indistinguishable from one where the packages
            // really were on the server.
            println!(
                "  {} assuming {assumed} package(s) are uploaded (--assume-uploaded). \
                 `fleetctl gitops` will still fail if they are not — \
                 `flint dry-run --refresh-snapshot` checks for real.",
                "!".yellow()
            );
        }
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
    }

    // --oracle: is anything we just blocked on something Fleet's own parser
    // would accept? That is the false positive that stalls automation, and
    // this catches it on the tree being gated. Advisory — it never changes
    // the verdict or the exit code, which is why it prints after both.
    if let Some(bin) = oracle.as_deref() {
        if !json {
            audit_against_oracle(bin, &lint_root, &blocking);
        }
    }

    if !blocking.is_empty() {
        std::process::exit(2);
    }
    Ok(())
}

/// Compare this run's blocking findings with Fleet's parser on the same tree.
fn audit_against_oracle(
    bin: &std::path::Path,
    lint_root: &std::path::Path,
    blocking: &[(&PathBuf, &linter::error::LintError)],
) {
    use crate::commands::history::{print_diff_human, rel, run_oracle, CommitRef, Diff, EXPECTED_FLINT_ONLY};
    use colored::Colorize;
    use std::collections::{BTreeMap, BTreeSet};

    let root = lint_root.canonicalize().unwrap_or_else(|_| lint_root.to_path_buf());
    let verdicts = match run_oracle(bin, &root) {
        Ok(Some(v)) => v,
        Ok(None) => {
            println!("\n  {} the oracle found no YAML to judge in this tree", "·".dimmed());
            return;
        }
        Err(e) => {
            println!("\n  {} oracle audit skipped: {e}", "!".yellow());
            return;
        }
    };

    let mut ours: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (f, e) in blocking {
        if let Some(code) = e.rule_code {
            ours.entry(rel(&root, f)).or_default().insert(code.to_string());
        }
    }
    let here = CommitRef {
        sha: "WORKTREE".into(),
        short: "worktree".into(),
        subject: "the tree being gated".into(),
    };
    let mut diff = Diff::default();
    diff.absorb(&here, &ours, &verdicts.blocking, &verdicts.no_opinion);
    print_diff_human(&diff);

    // Say what was NOT judged. Fleet's parser reads a software or policy
    // fragment only through the fleet that references it, so a blocking
    // finding on such a file is outside this comparison — and a "no false
    // positive" verdict that quietly skipped 29 of 29 findings would be the
    // very over-claim this audit exists to catch.
    let unaudited: usize = blocking
        .iter()
        .filter(|(f, _)| verdicts.no_opinion.contains(&rel(&root, f)))
        .count();
    let unaudited_files = ours.keys().filter(|f| verdicts.no_opinion.contains(*f)).count();
    if unaudited > 0 {
        println!(
            "\n  {} {unaudited} blocking finding(s) on {unaudited_files} file(s) were not \
             audited: Fleet's parser judges a software or policy fragment only through the \
             fleet that references it.",
            "·".dimmed()
        );
    }

    let review: Vec<&String> = diff
        .flint_only
        .keys()
        .filter(|code| !EXPECTED_FLINT_ONLY.iter().any(|(c, _)| c == code))
        .collect();
    println!();
    let audited = blocking.len() - unaudited;
    if review.is_empty() {
        println!(
            "  {} of {audited} audited blocking finding(s), none blocks where Fleet would \
             accept — no false positive is gating this tree.",
            "✓".green()
        );
    } else {
        println!(
            "  {} {} rule(s) block where Fleet's parser accepts: {} — review before letting \
             them gate automation.",
            "!".yellow(),
            review.len(),
            review.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        );
    }
}

/// The git repository containing `start`.
fn git_toplevel(start: &std::path::Path) -> anyhow::Result<PathBuf> {
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(start)
        .output()
        .context("failed to run git")?;
    if !out.status.success() {
        anyhow::bail!("--against requires a git repository");
    }
    Ok(PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
}

/// The tree a merge of HEAD and `r` would produce, plus any files that
/// conflict textually. `git merge-tree --write-tree` builds the result in the
/// object store without touching the index or the working copy: exit 0 is a
/// clean merge (first line is the tree oid), exit 1 is a merge with
/// conflicts (tree oid, then the conflicted paths), anything else is a real
/// failure.
fn merge_tree(repo: &std::path::Path, r: &str) -> anyhow::Result<(String, Vec<String>)> {
    let out = std::process::Command::new("git")
        .args(["merge-tree", "--write-tree", "--name-only", "HEAD", r])
        .current_dir(repo)
        .output()
        .context("failed to run git merge-tree")?;
    match out.status.code() {
        Some(code @ (0 | 1)) => parse_merge_tree(code, &String::from_utf8_lossy(&out.stdout)),
        _ => anyhow::bail!(
            "git merge-tree HEAD {r} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
    }
}

/// `git merge-tree --write-tree --name-only` output: the tree oid on the
/// first line; on exit 1, the conflicted paths follow **until the first blank
/// line**, after which come informational messages ("Auto-merging …") that
/// are not paths. A first cut read to the end and reported three conflicts
/// where there was one.
fn parse_merge_tree(code: i32, stdout: &str) -> anyhow::Result<(String, Vec<String>)> {
    let mut lines = stdout.lines();
    let tree = lines
        .next()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .context("git merge-tree printed no tree")?
        .to_string();
    if code == 0 {
        return Ok((tree, Vec::new()));
    }
    let conflicts = lines
        .take_while(|l| !l.trim().is_empty())
        .map(|l| l.trim().to_string())
        .collect();
    Ok((tree, conflicts))
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

#[cfg(test)]
mod merge_tree_tests {
    use super::parse_merge_tree;

    #[test]
    fn a_clean_merge_is_a_tree_and_nothing_else() {
        let (tree, conflicts) = parse_merge_tree(0, "8a1594588d45abc\n").unwrap();
        assert_eq!(tree, "8a1594588d45abc");
        assert!(conflicts.is_empty());
    }

    /// The shape that produced "3 file(s)" for one conflict.
    #[test]
    fn conflicts_stop_at_the_blank_line_before_the_messages() {
        let out = "deadbeef\nfleets/ABC-TEST.yml\n\nAuto-merging fleets/ABC-TEST.yml\nCONFLICT (content): …\n";
        let (tree, conflicts) = parse_merge_tree(1, out).unwrap();
        assert_eq!(tree, "deadbeef");
        assert_eq!(conflicts, vec!["fleets/ABC-TEST.yml".to_string()]);
    }

    #[test]
    fn empty_output_is_an_error_not_an_empty_tree() {
        assert!(parse_merge_tree(0, "").is_err());
    }
}
