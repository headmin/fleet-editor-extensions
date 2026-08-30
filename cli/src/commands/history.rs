//! `flint history` — what the rules would have caught, and what they still cannot.
//!
//! Two capabilities with deliberately different epistemics, kept apart because
//! conflating them is how this idea usually goes wrong:
//!
//! * **Replay** (default) runs *today's* rules against *yesterday's* trees.
//!   Deterministic: flint's rules are pure functions of the tree, so the only
//!   question it answers is the measurable one — *how often would this rule
//!   have helped?* Its output is prioritisation evidence, not configuration.
//!
//! * **Archaeology** (`--suggest-patterns`) mines remediation commits for
//!   conventions no rule encodes yet. Interpretive: it answers *what rule
//!   should exist?*, so everything it emits is labelled a suggestion and is
//!   commented out. It is never written into `.fleetlint.toml` directly.

use crate::args::HistoryArgs;
use anyhow::{bail, Context, Result};
use colored::Colorize;
use flint_lint as linter;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

/// Codes that cannot be replayed, with the reason shown to the user.
///
/// A snapshot-derived finding depends on the Fleet server's state at that
/// commit, which is gone. Replaying it would compare an old tree against
/// today's server and invent history.
const NON_REPLAYABLE: &[(&str, &str)] = &[(
    linter::codes::SOFTWARE_SOURCE,
    "depends on server state at that commit, which no longer exists",
)];

pub(crate) fn run(args: HistoryArgs) -> Result<()> {
    let HistoryArgs {
        path,
        since,
        max,
        suggest_patterns,
        min_occurrences,
        oracle,
        scope_as_committed,
        gate,
        update_baseline,
        json,
    } = args;

    let repo = path
        .canonicalize()
        .with_context(|| format!("no such directory: {}", path.display()))?;
    let root = git(&repo, &["rev-parse", "--show-toplevel"])
        .context("flint history requires a git repository")?;
    let root = std::path::PathBuf::from(root.trim());

    // The scope flint and the oracle both honour. Held fixed across the replay
    // unless the caller asks for each tree's own.
    let scope = if scope_as_committed {
        None
    } else {
        ["fleetlint.toml", ".fleetlint.toml"]
            .iter()
            .map(|n| root.join(n))
            .find(|p| p.is_file())
    };

    let commits = first_parent_commits(&root, since.as_deref(), max)?;
    if commits.is_empty() {
        bail!("no commits to examine in the requested range");
    }

    if suggest_patterns {
        suggest(&root, &commits, min_occurrences, json)
    } else {
        replay(
            &root,
            &commits,
            oracle.as_deref(),
            scope.as_deref(),
            gate.as_deref(),
            update_baseline,
            json,
        )
    }
}

// ---------------------------------------------------------------------------
// git plumbing
// ---------------------------------------------------------------------------

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .context("failed to run git")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// One commit's identity, oldest first.
#[derive(Clone)]
struct CommitRef {
    sha: String,
    short: String,
    subject: String,
}

/// First-parent history, oldest → newest. First-parent because a merge's
/// second parent is another branch's development, not this repo's timeline —
/// and it is the merge result that ships.
fn first_parent_commits(root: &Path, since: Option<&str>, max: usize) -> Result<Vec<CommitRef>> {
    let range = since.map(|r| format!("{r}..HEAD"));
    let mut args = vec!["log", "--first-parent", "--reverse", "--format=%H%x1f%h%x1f%s"];
    let max_arg;
    if let Some(r) = &range {
        args.push(r);
    } else {
        max_arg = format!("-n{max}");
        args.push(&max_arg);
    }
    let out = git(root, &args)?;
    let mut commits: Vec<CommitRef> = out
        .lines()
        .filter_map(|l| {
            let mut parts = l.split('\u{1f}');
            Some(CommitRef {
                sha: parts.next()?.to_string(),
                short: parts.next()?.to_string(),
                subject: parts.next().unwrap_or_default().to_string(),
            })
        })
        .collect();
    // `--reverse` applies after `-n`, so a capped run is already the newest
    // `max`; a `--since` range may exceed the cap and is trimmed to the most
    // recent window so the answer stays bounded.
    if commits.len() > max {
        commits = commits.split_off(commits.len() - max);
    }
    Ok(commits)
}

/// Materialise one commit's tree into `dest` without touching the working
/// copy — `git archive` writes the tree as it was, so a replay never disturbs
/// the checkout the user is sitting in.
fn materialize(root: &Path, sha: &str, dest: &Path) -> Result<()> {
    let tar = dest.join(".flint-history.tar");
    let out = Command::new("git")
        .args(["archive", "--format=tar", "-o"])
        .arg(&tar)
        .arg(sha)
        .current_dir(root)
        .output()
        .context("failed to run git archive")?;
    if !out.status.success() {
        bail!(
            "git archive {sha} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let status = Command::new("tar")
        .arg("-xf")
        .arg(&tar)
        .arg("-C")
        .arg(dest)
        .status()
        .context("failed to run tar")?;
    if !status.success() {
        bail!("tar failed to extract {sha}");
    }
    let _ = std::fs::remove_file(&tar);
    Ok(())
}

// ---------------------------------------------------------------------------
// Mode 1 — replay
// ---------------------------------------------------------------------------

/// The set of rule codes that fired at one commit.
struct Replayed {
    commit: CommitRef,
    codes: BTreeSet<String>,
}

/// A contiguous run of commits in which one code fired.
struct RedWindow {
    opened: CommitRef,
    /// The first commit at which the code stopped firing. `None` = still
    /// firing at HEAD, which is current state rather than history.
    closed_by: Option<CommitRef>,
    commits: usize,
}

fn replay(
    root: &Path,
    commits: &[CommitRef],
    oracle: Option<&Path>,
    scope: Option<&Path>,
    gate: Option<&Path>,
    update_baseline: bool,
    json: bool,
) -> Result<()> {
    let skip: BTreeSet<&str> = NON_REPLAYABLE.iter().map(|(c, _)| *c).collect();
    let mut replayed = Vec::with_capacity(commits.len());
    let mut diff = Diff::default();

    for (i, c) in commits.iter().enumerate() {
        if !json {
            eprint!("\r  replaying {}/{} {}", i + 1, commits.len(), c.short);
        }
        // Explicitly NOT a dot-prefixed name. `TempDir::new()` produces
        // `.tmpXXXX`, and the oracle's file walk skips dot-directories —
        // including the root it is pointed at — so a hidden scratch dir makes
        // it report "no input files" for a tree that is perfectly fine.
        let dir = tempfile::Builder::new()
            .prefix("flint-history-")
            .tempdir()
            .context("failed to create a scratch directory")?;
        materialize(root, &c.sha, dir.path())?;
        // Overwrite whatever scope config the tree carried, so every commit is
        // judged against the same one. Both flint and the oracle read it.
        if let Some(cfg) = scope {
            let name = cfg.file_name().unwrap_or_default();
            std::fs::copy(cfg, dir.path().join(name))
                .with_context(|| format!("failed to apply {}", cfg.display()))?;
        }

        let lint = linter::Linter::from_path(dir.path());
        let results = lint.lint_directory(dir.path(), None).unwrap_or_default();
        let mut codes = BTreeSet::new();
        // Blocking claims only, keyed by file: the diff against Fleet is about
        // what flint says will FAIL an apply, not what it advises.
        let mut flint_blocking: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (file, report) in &results {
            for e in report.errors.iter().chain(&report.warnings) {
                if let Some(code) = e.rule_code {
                    if !skip.contains(code) {
                        codes.insert(code.to_string());
                    }
                }
            }
            for e in &report.errors {
                let Some(code) = e.rule_code else { continue };
                if !skip.contains(code) {
                    flint_blocking
                        .entry(rel(dir.path(), file))
                        .or_default()
                        .insert(code.to_string());
                }
            }
        }

        if let Some(bin) = oracle {
            // One unreadable tree must not abort a 358-commit run — but it is
            // recorded and reported, never silently dropped.
            match run_oracle(bin, dir.path()) {
                Ok(Some(v)) => diff.absorb(c, &flint_blocking, &v.blocking, &v.no_opinion),
                Ok(None) => diff.no_input += 1,
                Err(e) => diff.errors.push((c.short.clone(), e.to_string())),
            }
        }

        replayed.push(Replayed {
            commit: c.clone(),
            codes,
        });
    }
    if !json {
        eprint!("\r{}\r", " ".repeat(60));
    }

    if !json {
        match scope {
            Some(cfg) => println!(
                "  {} every tree scoped by {} ({})",
                "scope:".dimmed(),
                cfg.display(),
                describe_scope(cfg).dimmed()
            ),
            None => println!(
                "  {} each tree uses its own committed config (--scope-as-committed)",
                "scope:".dimmed()
            ),
        }
    }

    let windows = red_windows(&replayed);
    if json {
        print_replay_json(commits, &replayed, &windows, oracle.map(|_| &diff), scope);
    } else {
        print_replay_human(commits, &replayed, &windows);
        if oracle.is_some() {
            print_diff_human(&diff);
        }
    }

    if let Some(baseline) = gate {
        let card = Scorecard::build(commits.len(), &windows, &diff, oracle.is_some());
        run_gate(baseline, &card, update_baseline, json)?;
    }
    Ok(())
}

/// Group each code's firing history into windows.
fn red_windows(replayed: &[Replayed]) -> BTreeMap<String, Vec<RedWindow>> {
    let mut out: BTreeMap<String, Vec<RedWindow>> = BTreeMap::new();
    let mut open: BTreeMap<String, (CommitRef, usize)> = BTreeMap::new();

    for r in replayed {
        // Close any window whose code stopped firing at this commit.
        let closing: Vec<String> = open
            .keys()
            .filter(|c| !r.codes.contains(*c))
            .cloned()
            .collect();
        for code in closing {
            let (opened, commits) = open.remove(&code).expect("key came from this map");
            out.entry(code).or_default().push(RedWindow {
                opened,
                closed_by: Some(r.commit.clone()),
                commits,
            });
        }
        for code in &r.codes {
            match open.get_mut(code) {
                Some((_, n)) => *n += 1,
                None => {
                    open.insert(code.clone(), (r.commit.clone(), 1));
                }
            }
        }
    }
    // Whatever is still firing at HEAD is current state, not a closed window.
    for (code, (opened, commits)) in open {
        out.entry(code).or_default().push(RedWindow {
            opened,
            closed_by: None,
            commits,
        });
    }
    out
}

fn print_replay_human(
    commits: &[CommitRef],
    replayed: &[Replayed],
    windows: &BTreeMap<String, Vec<RedWindow>>,
) {
    let dirty = replayed.iter().filter(|r| !r.codes.is_empty()).count();
    println!(
        "\n{} replayed {} first-parent commit(s) — {} had findings\n",
        "flint history".bold(),
        commits.len(),
        dirty
    );

    // Rank by closed windows: recurrence is the whole thesis, and an open
    // window describes the present rather than a repeated mistake.
    let mut ranked: Vec<(&String, &Vec<RedWindow>)> = windows.iter().collect();
    ranked.sort_by(|a, b| {
        closed(b.1)
            .cmp(&closed(a.1))
            .then_with(|| span(b.1).cmp(&span(a.1)))
            .then_with(|| a.0.cmp(b.0))
    });

    if ranked.is_empty() {
        println!("  {} no rule fired anywhere in this range.", "✓".green());
        return;
    }

    for (code, ws) in ranked {
        let c = closed(ws);
        let label = if c >= 2 {
            format!("×{c} closed windows").red().bold().to_string()
        } else if c == 1 {
            "×1 closed window".yellow().to_string()
        } else {
            "open only".dimmed().to_string()
        };
        println!("  {}  {label}, {} commit(s)", code.bold(), span(ws));
        for w in ws {
            match &w.closed_by {
                Some(cl) => println!(
                    "      {} {}  →  fixed in {} {}",
                    w.opened.short.dimmed(),
                    truncate(&w.opened.subject, 44),
                    cl.short.green(),
                    truncate(&cl.subject, 44).dimmed()
                ),
                None => println!(
                    "      {} {}  →  {}",
                    w.opened.short.dimmed(),
                    truncate(&w.opened.subject, 44),
                    "still firing at HEAD".yellow()
                ),
            }
        }
        println!();
    }

    println!("{}", "Not replayed".dimmed());
    for (code, why) in NON_REPLAYABLE {
        println!("  {} — {}", code.dimmed(), why.dimmed());
    }
    println!(
        "\n{}",
        "A code with two or more closed windows is a repeat failure, not a one-off — \
         that is the ranking that matters."
            .dimmed()
    );
}

fn print_replay_json(
    commits: &[CommitRef],
    replayed: &[Replayed],
    windows: &BTreeMap<String, Vec<RedWindow>>,
    diff: Option<&Diff>,
    scope: Option<&Path>,
) {
    let codes: Vec<serde_json::Value> = windows
        .iter()
        .map(|(code, ws)| {
            serde_json::json!({
                "code": code,
                "closed_windows": closed(ws),
                "still_firing_at_head": ws.iter().any(|w| w.closed_by.is_none()),
                "commits_affected": span(ws),
                "windows": ws.iter().map(|w| serde_json::json!({
                    "opened_at": w.opened.sha,
                    "opened_short": w.opened.short,
                    "opened_subject": w.opened.subject,
                    "closed_by": w.closed_by.as_ref().map(|c| c.sha.clone()),
                    "closed_short": w.closed_by.as_ref().map(|c| c.short.clone()),
                    "closed_subject": w.closed_by.as_ref().map(|c| c.subject.clone()),
                    "commits": w.commits,
                })).collect::<Vec<_>>(),
            })
        })
        .collect();
    let doc = serde_json::json!({
        "mode": "replay",
        "scope": scope.map(|c| serde_json::json!({
            "config": c.display().to_string(),
            "applied_to_every_tree": true,
            "summary": describe_scope(c),
        })),
        "commits_replayed": commits.len(),
        "commits_with_findings": replayed.iter().filter(|r| !r.codes.is_empty()).count(),
        "not_replayed": NON_REPLAYABLE.iter()
            .map(|(c, w)| serde_json::json!({"code": c, "reason": w}))
            .collect::<Vec<_>>(),
        "codes": codes,
        "oracle_diff": diff.map(Diff::to_json),
    });
    println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
}

/// A one-line summary of what a scope config excludes, so an exempted path is
/// visible in the report rather than silently absent from it.
fn describe_scope(cfg: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(cfg) else {
        return "unreadable".to_string();
    };
    let globs = |key: &str| -> Vec<String> {
        let Some(rest) = text.split_once(&format!("{key} =")) else {
            return Vec::new();
        };
        let Some(list) = rest.1.split_once(']') else {
            return Vec::new();
        };
        list.0
            .split(',')
            .filter_map(|s| {
                let t = s.trim().trim_matches(['[', '"', '\n', ' ']).trim();
                (!t.is_empty() && !t.starts_with('#')).then(|| t.to_string())
            })
            .collect()
    };
    let inc = globs("include");
    let exc = globs("exclude");
    match (inc.is_empty(), exc.is_empty()) {
        (true, true) => "no [files] scoping".to_string(),
        (false, true) => format!("include: {}", inc.join(", ")),
        (true, false) => format!("exclude: {}", exc.join(", ")),
        (false, false) => format!("include: {} · exclude: {}", inc.join(", "), exc.join(", ")),
    }
}

fn closed(ws: &[RedWindow]) -> usize {
    ws.iter().filter(|w| w.closed_by.is_some()).count()
}

fn span(ws: &[RedWindow]) -> usize {
    ws.iter().map(|w| w.commits).sum()
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_string();
    }
    let head: String = s.chars().take(n.saturating_sub(1)).collect();
    format!("{head}…")
}

// ---------------------------------------------------------------------------
// Oracle diff — flint's verdict against Fleet's own parser
// ---------------------------------------------------------------------------
//
// Replay on its own measures *consistency*: its ground truth is flint. The
// oracle calls `spec.GitOpsFromFile`, the function `fleetctl gitops` itself
// calls, so diffing the two measures *correctness* instead.
//
// The comparison is deliberately restricted to blocking claims on both sides.
// flint reports plenty Fleet does not — conventions, hygiene, advisory
// warnings — and that is by design. The claim worth checking is the strong
// one: "this will fail the apply."

/// One file's verdict from the oracle.
#[derive(serde::Deserialize)]
struct OracleFile {
    path: String,
    /// Whether Fleet's parser could read the file. An entry file it could not
    /// parse may carry NO error-severity finding, so reading only `findings`
    /// would make an unreadable file look accepted.
    #[serde(default = "yes")]
    parsed: bool,
    /// Fleet's parser is not the right judge of this file at all.
    ///
    /// `GitOpsFromFile` accepts only a fleet file (`name:`) or the global
    /// config; a profile, a standalone policy list, or anything pulled in via
    /// `path:` is validated as part of its PARENT. flint lints those
    /// individually, so counting them either way invents a verdict Fleet never
    /// gave — the oracle says so itself and excludes them.
    #[serde(default)]
    not_entry_file: bool,
    #[serde(default)]
    findings: Vec<OracleFinding>,
}

/// Absent `parsed` means the oracle predates the field; assume it read the file
/// rather than inventing a blocking verdict.
fn yes() -> bool {
    true
}

#[derive(serde::Deserialize)]
struct OracleFinding {
    #[serde(default)]
    severity: String,
    #[serde(default)]
    message: String,
}

#[derive(serde::Deserialize)]
struct OracleOut {
    #[serde(default)]
    files: Vec<OracleFile>,
}

/// Run the oracle over a materialised tree and return each file's blocking
/// messages, keyed by the same repo-relative path flint findings use.
/// Fleet's verdicts on one tree: what it blocked, and what it declined to judge.
struct OracleVerdicts {
    blocking: BTreeMap<String, Vec<String>>,
    /// Paths Fleet's parser has no opinion on. Excluded from the diff on BOTH
    /// sides — a fragment is not an acceptance.
    no_opinion: BTreeSet<String>,
}

fn run_oracle(bin: &Path, dir: &Path) -> Result<Option<OracleVerdicts>> {
    let out = Command::new(bin)
        .arg("--repo")
        .arg(dir)
        .arg("--pretty=false")
        .output()
        .with_context(|| format!("failed to run the oracle at {}", bin.display()))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        // A tree with no YAML at all is a legitimate state, not a failure —
        // the early history of a repo has nothing for Fleet to parse.
        if stderr.contains("no input files") {
            return Ok(None);
        }
        bail!(
            "oracle failed ({} --repo {} --pretty=false): {}",
            bin.display(),
            dir.display(),
            stderr.trim()
        );
    }
    let parsed: OracleOut =
        serde_json::from_slice(&out.stdout).context("oracle did not emit the expected JSON")?;

    let mut blocking_by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut no_opinion: BTreeSet<String> = BTreeSet::new();
    for f in parsed.files {
        if f.not_entry_file {
            no_opinion.insert(rel(dir, Path::new(&f.path)));
            continue;
        }
        let mut blocking: Vec<String> = f
            .findings
            .into_iter()
            .filter(|x| x.severity == "error")
            .map(|x| x.message)
            .collect();
        if !f.parsed && blocking.is_empty() {
            blocking.push("Fleet's parser could not read this file".to_string());
        }
        if !blocking.is_empty() {
            // Fleet embeds absolute paths in some messages. Left alone, the
            // scratch directory makes every commit's copy of one complaint
            // unique, so it never clusters and the count is lost.
            let base = dir.to_string_lossy().to_string();
            let blocking = blocking
                .into_iter()
                .map(|m| m.replace(&base, "<tree>"))
                .collect();
            blocking_by_file.insert(rel(dir, Path::new(&f.path)), blocking);
        }
    }
    Ok(Some(OracleVerdicts { blocking: blocking_by_file, no_opinion }))
}

/// A path relative to the materialised tree, so flint's and the oracle's
/// answers are keyed the same way regardless of scratch-directory naming.
fn rel(base: &Path, file: &Path) -> String {
    file.strip_prefix(base)
        .unwrap_or(file)
        .to_string_lossy()
        .trim_start_matches("./")
        .replace('\\', "/")
}

/// One (commit, file) where the two tools disagreed.
struct Occurrence {
    commit: String,
    file: String,
    detail: String,
}

/// How many examples to retain per disagreement. Enough to act on, few enough
/// that a 358-commit run stays readable.
const EXAMPLES: usize = 3;

#[derive(Default)]
struct Diff {
    /// (commit, file) pairs where both called it blocking.
    agreed: usize,
    /// flint said blocking, Fleet did not — keyed by flint's rule code. The
    /// expensive direction: a rule claiming Fleet rejects what Fleet accepts
    /// sends someone to edit working config.
    flint_only: BTreeMap<String, (usize, Vec<Occurrence>)>,
    /// Fleet said blocking, flint was silent — keyed by a normalised message.
    /// This is the gap list, measured rather than argued.
    fleet_only: BTreeMap<String, (usize, Vec<Occurrence>)>,
    /// Trees with no YAML for Fleet to parse — the early history of a repo.
    no_input: usize,
    /// Commits where the oracle itself failed. Reported, never swallowed: a
    /// comparison that quietly skipped part of the range is not a comparison.
    errors: Vec<(String, String)>,
}

impl Diff {
    fn absorb(
        &mut self,
        commit: &CommitRef,
        flint: &BTreeMap<String, BTreeSet<String>>,
        fleet: &BTreeMap<String, Vec<String>>,
        no_opinion: &BTreeSet<String>,
    ) {
        for (file, codes) in flint.iter().filter(|(f, _)| !no_opinion.contains(*f)) {
            match fleet.get(file) {
                Some(_) => self.agreed += 1,
                None => {
                    for code in codes {
                        record(
                            &mut self.flint_only,
                            code.clone(),
                            commit,
                            file,
                            "Fleet's parser accepted this file",
                        );
                    }
                }
            }
        }
        for (file, messages) in fleet.iter().filter(|(f, _)| !no_opinion.contains(*f)) {
            if flint.contains_key(file) {
                continue;
            }
            for m in messages {
                record(&mut self.fleet_only, normalise(m), commit, file, m);
            }
        }
    }

    fn to_json(&self) -> serde_json::Value {
        let bucket = |b: &BTreeMap<String, (usize, Vec<Occurrence>)>| {
            b.iter()
                .map(|(k, (n, ex))| {
                    serde_json::json!({
                        "key": k,
                        "occurrences": n,
                        "examples": ex.iter().map(|o| serde_json::json!({
                            "commit": o.commit, "file": o.file, "detail": o.detail,
                        })).collect::<Vec<_>>(),
                    })
                })
                .collect::<Vec<_>>()
        };
        serde_json::json!({
            "contract": "blocking claims only: flint errors vs oracle findings with severity=error",
            "agreed_file_verdicts": self.agreed,
            "trees_with_no_yaml": self.no_input,
            "oracle_errors": self.errors.iter()
                .map(|(c, e)| serde_json::json!({"commit": c, "error": e}))
                .collect::<Vec<_>>(),
            "flint_blocking_fleet_accepted": bucket(&self.flint_only),
            "fleet_blocking_flint_silent": bucket(&self.fleet_only),
        })
    }
}

fn record(
    bucket: &mut BTreeMap<String, (usize, Vec<Occurrence>)>,
    key: String,
    commit: &CommitRef,
    file: &str,
    detail: &str,
) {
    let slot = bucket.entry(key).or_insert_with(|| (0, Vec::new()));
    slot.0 += 1;
    if slot.1.len() < EXAMPLES {
        slot.1.push(Occurrence {
            commit: commit.short.clone(),
            file: file.to_string(),
            detail: detail.to_string(),
        });
    }
}

/// Collapse a Fleet message to its shape so the same complaint about different
/// files clusters together: quoted specifics become `…`.
fn normalise(message: &str) -> String {
    let mut out = String::with_capacity(message.len());
    let mut in_quote = false;
    for ch in message.chars() {
        if ch == '"' {
            if !in_quote {
                out.push_str("\"…\"");
            }
            in_quote = !in_quote;
            continue;
        }
        if !in_quote {
            out.push(ch);
        }
    }
    let trimmed = out.trim();
    truncate(trimmed, 90)
}

fn print_diff_human(diff: &Diff) {
    println!("\n{}", "Against Fleet's own parser".bold());
    println!(
        "{}\n",
        "Blocking claims only — flint errors vs the oracle's severity=error. \
         flint reports much that Fleet does not by design; the claim worth checking \
         is \"this will fail the apply\"."
            .dimmed()
    );
    println!("  {} file verdicts agreed", diff.agreed);
    if diff.no_input > 0 {
        println!(
            "  {}",
            format!(
                "{} tree(s) had no YAML for Fleet to parse — early history",
                diff.no_input
            )
            .dimmed()
        );
    }
    if !diff.errors.is_empty() {
        println!(
            "  {} the oracle failed on {} commit(s); those trees are NOT in the comparison",
            "!".yellow(),
            diff.errors.len()
        );
        for (c, e) in diff.errors.iter().take(3) {
            println!("      {} {}", c.dimmed(), truncate(e, 90).dimmed());
        }
    }
    println!();

    if diff.fleet_only.is_empty() {
        println!("  {} Fleet never rejected a file flint passed.", "✓".green());
    } else {
        println!(
            "  {}  {}",
            "Fleet blocked, flint silent".red().bold(),
            "— the gap list, measured".dimmed()
        );
        let mut rows: Vec<_> = diff.fleet_only.iter().collect();
        rows.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
        for (msg, (n, ex)) in rows {
            println!("      {} {}", format!("×{n}").red(), msg);
            for o in ex.iter().take(1) {
                println!("          {} {}", o.commit.dimmed(), o.file.dimmed());
            }
        }
    }
    println!();

    if diff.flint_only.is_empty() {
        println!(
            "  {} flint never claimed blocking where Fleet accepted.",
            "✓".green()
        );
    } else {
        println!(
            "  {}  {}",
            "flint blocked, Fleet accepted".yellow().bold(),
            "— review each: a false claim sends someone to edit working config".dimmed()
        );
        let mut rows: Vec<_> = diff.flint_only.iter().collect();
        rows.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
        for (code, (n, ex)) in rows {
            println!("      {} {}", format!("×{n}").yellow(), code.bold());
            for o in ex.iter().take(1) {
                println!("          {} {}", o.commit.dimmed(), o.file.dimmed());
            }
        }
        println!(
            "\n  {}",
            "Not automatically wrong: the oracle covers the OFFLINE half of validation \
             only, so anything needing live server state legitimately appears here."
                .dimmed()
        );
    }
}

// ---------------------------------------------------------------------------
// The gate — rule quality as something CI can fail on
// ---------------------------------------------------------------------------
//
// Every other check in this repo asks whether the CONFIG is right. This one
// asks whether the RULES are, by holding a replay against a stored baseline.
//
// Only new KEYS gate. Occurrence counts move with the range replayed — add a
// commit and they grow on their own — so failing on them would produce alarms
// that say nothing. A rule that newly over-claims, or a Fleet complaint flint
// has newly gone silent on, is a regression regardless of range.

/// A compact, comparable summary of one replay.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Scorecard {
    /// Context only; never compared.
    commits: usize,
    /// Rule code → closed red windows.
    closed_windows: BTreeMap<String, usize>,
    /// Whether the oracle ran. A run without it cannot gate the halves below,
    /// and must not be mistaken for one that found nothing.
    #[serde(default)]
    oracle: bool,
    /// Rule code → verdicts where flint blocked and Fleet accepted.
    #[serde(default)]
    flint_only: BTreeMap<String, usize>,
    /// Fleet complaint shape → occurrences where flint said nothing.
    #[serde(default)]
    gap_shapes: BTreeMap<String, usize>,
}

impl Scorecard {
    fn build(
        commits: usize,
        windows: &BTreeMap<String, Vec<RedWindow>>,
        diff: &Diff,
        oracle: bool,
    ) -> Self {
        Self {
            commits,
            closed_windows: windows.iter().map(|(k, v)| (k.clone(), closed(v))).collect(),
            oracle,
            flint_only: diff.flint_only.iter().map(|(k, (n, _))| (k.clone(), *n)).collect(),
            gap_shapes: diff.fleet_only.iter().map(|(k, (n, _))| (k.clone(), *n)).collect(),
        }
    }
}

#[derive(Default)]
struct GateVerdict {
    regressions: Vec<String>,
    improvements: Vec<String>,
    notes: Vec<String>,
}

impl GateVerdict {
    fn compare(base: &Scorecard, now: &Scorecard) -> Self {
        let mut v = Self::default();

        // A baseline recorded WITH the oracle, compared against a run without
        // it, has no oracle halves to compare — every gap would otherwise
        // vanish and be reported as closed. Skip those buckets entirely and
        // say so; a silent "all gaps closed" is the worst thing this gate
        // could print.
        let oracle_comparable = now.oracle || !base.oracle;
        if !oracle_comparable {
            v.notes.push(
                "baseline was recorded WITH --oracle and this run was not, so the \
                 correctness halves (over-claims and gaps) were NOT compared"
                    .to_string(),
            );
        }

        // A rule that did not over-claim before and does now.
        for (code, n) in now.flint_only.iter().filter(|_| oracle_comparable) {
            match base.flint_only.get(code) {
                None => v.regressions.push(format!(
                    "'{code}' now claims blocking where Fleet accepts (×{n}) — it did not before"
                )),
                Some(was) if n > was => v.notes.push(format!(
                    "'{code}' over-claims more than the baseline ({was} → {n}); counts move with \
                     the range, so this does not fail the gate"
                )),
                _ => {}
            }
        }
        for code in base.flint_only.keys().filter(|_| oracle_comparable) {
            if !now.flint_only.contains_key(code) {
                v.improvements
                    .push(format!("'{code}' no longer claims blocking where Fleet accepts"));
            }
        }

        // A complaint Fleet makes that flint has newly gone silent on.
        for (shape, n) in now.gap_shapes.iter().filter(|_| oracle_comparable) {
            if !base.gap_shapes.contains_key(shape) {
                v.regressions.push(format!(
                    "Fleet blocks and flint is silent on something new (×{n}): {}",
                    truncate(shape, 78)
                ));
            }
        }
        for shape in base.gap_shapes.keys().filter(|_| oracle_comparable) {
            if !now.gap_shapes.contains_key(shape) {
                v.improvements
                    .push(format!("gap closed: {}", truncate(shape, 78)));
            }
        }

        // A rule that used to catch something in this history and no longer
        // does. Replaying a fixed range can only ADD closed windows as commits
        // accrue, so a fall is never explained by the range — the rule changed.
        // This is the regression a flint version bump introduces, and the only
        // one detectable without the oracle.
        for (code, was) in &base.closed_windows {
            let now_n = now.closed_windows.get(code).copied().unwrap_or(0);
            if *was > 0 && now_n < *was {
                v.regressions.push(format!(
                    "'{code}' no longer catches what it used to: {was} closed window(s) → {now_n}                      over the same history"
                ));
            }
        }

        // A rule with no historical evidence is not a failure — but it is the
        // question worth asking of any rule being added.
        for (code, n) in &now.closed_windows {
            if !base.closed_windows.contains_key(code) {
                v.notes.push(if *n == 0 {
                    format!("new rule '{code}' scores 0 closed windows — it would never have fired \
                             in this range")
                } else {
                    format!("new rule '{code}' scores ×{n} closed windows")
                });
            }
        }

        v
    }
}

fn run_gate(baseline: &Path, card: &Scorecard, update: bool, json: bool) -> Result<()> {
    let write = |why: &str| -> Result<()> {
        if let Some(parent) = baseline.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).ok();
            }
        }
        std::fs::write(baseline, serde_json::to_string_pretty(card)?)
            .with_context(|| format!("failed to write {}", baseline.display()))?;
        if !json {
            println!("\n{} {} — {why}", "baseline written:".bold(), baseline.display());
        }
        Ok(())
    };

    if update {
        return write("--update-baseline");
    }
    if !baseline.exists() {
        return write("no baseline existed; this run is now the reference");
    }

    let text = std::fs::read_to_string(baseline)
        .with_context(|| format!("failed to read {}", baseline.display()))?;
    let base: Scorecard = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a flint history scorecard", baseline.display()))?;
    let verdict = GateVerdict::compare(&base, card);

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "gate": if verdict.regressions.is_empty() { "pass" } else { "fail" },
                "baseline": baseline.display().to_string(),
                "baseline_commits": base.commits,
                "commits": card.commits,
                "regressions": verdict.regressions,
                "improvements": verdict.improvements,
                "notes": verdict.notes,
            }))
            .unwrap_or_default()
        );
    } else {
        println!("\n{} against {}", "Gate".bold(), baseline.display().to_string().dimmed());
        for i in &verdict.improvements {
            println!("  {} {i}", "✓".green());
        }
        for n in &verdict.notes {
            println!("  {} {n}", "·".dimmed());
        }
        for r in &verdict.regressions {
            println!("  {} {r}", "✗".red());
        }
        if verdict.regressions.is_empty() {
            println!("\n  {} rule quality held.", "PASS".green().bold());
        } else {
            println!(
                "\n  {} {} regression(s). Re-run with --update-baseline once each is understood.",
                "FAIL".red().bold(),
                verdict.regressions.len()
            );
        }
    }

    if !verdict.regressions.is_empty() {
        std::process::exit(2);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mode 2 — archaeology
// ---------------------------------------------------------------------------

/// Words that mark a commit as remediation rather than feature work.
///
/// Tuned to be inclusive: a false positive costs one weak candidate, which the
/// recurrence threshold and the add/edit dominance test then discard. The
/// normative words earn their place — `d777926` ("policy run_script targets
/// **must** also be in controls.scripts") is one of the most instructive
/// remediation commits in the history this was built against, and it never
/// says "fix".
const FIX_WORDS: &[&str] = &[
    "fix", "correct", "repair", "restore", "unbreak", "revert", "missing", "broken", "register",
    "re-add", "readd", "resolve", "failure", "red", "regression", "must", "should", "instead",
];

/// One remediation commit that supports a finding.
struct Evidence {
    short: String,
    subject: String,
}

/// Where remediation concentrated: one YAML key in one area, with how the fix
/// commits touched it.
struct Churn {
    key: String,
    files: String,
    /// Commits whose diff ADDED this key — an omission that had to be repaired.
    added: Vec<Evidence>,
    /// Commits whose diff REMOVED or rewrote it — editing, not omission.
    edited: Vec<Evidence>,
}

impl Churn {
    fn total(&self) -> usize {
        self.added.len() + self.edited.len()
    }

    /// A key is guardrail-shaped only when the repairs were **additions**.
    ///
    /// This is the distinction that decides signal from noise. `path:` churns
    /// constantly in a GitOps repo because paths get edited — asserting that
    /// every fleet file must contain `path:` encodes nothing anyone learned.
    /// A key that fix commits only ever *add* is the opposite: each occurrence
    /// is somebody discovering it was missing.
    fn is_guardrail_shaped(&self, min_occurrences: usize) -> bool {
        // Additions must dominate, not merely lead: a key edited nearly as
        // often as it is added is being maintained, not forgotten.
        self.added.len() >= min_occurrences && self.edited.len() * 3 <= self.added.len()
    }
}

fn suggest(root: &Path, commits: &[CommitRef], min_occurrences: usize, json: bool) -> Result<()> {
    let mut by_key: BTreeMap<(String, String), (Vec<Evidence>, Vec<Evidence>)> = BTreeMap::new();

    for c in commits {
        if !is_fix_shaped(&c.subject) {
            continue;
        }
        let diff = git(
            root,
            &["show", "--format=", "--unified=0", "--no-color", &c.sha],
        )
        .unwrap_or_default();
        for (key, area, added) in keys_touched(&diff) {
            let ev = Evidence {
                short: c.short.clone(),
                subject: c.subject.clone(),
            };
            let slot = by_key.entry((key, area)).or_default();
            if added {
                slot.0.push(ev);
            } else {
                slot.1.push(ev);
            }
        }
    }

    let mut churn: Vec<Churn> = by_key
        .into_iter()
        .map(|((key, files), (added, edited))| Churn {
            key,
            files,
            added,
            edited,
        })
        // Recurrence is the entire thesis: one repair is an anecdote.
        .filter(|c| c.total() >= min_occurrences)
        .collect();
    churn.sort_by(|a, b| b.total().cmp(&a.total()).then_with(|| a.key.cmp(&b.key)));

    let (candidates, hotspots): (Vec<&Churn>, Vec<&Churn>) = churn
        .iter()
        .partition(|c| c.is_guardrail_shaped(min_occurrences));

    if json {
        print_suggest_json(&candidates, &hotspots, min_occurrences);
    } else {
        print_suggest_human(&candidates, &hotspots, min_occurrences);
    }
    Ok(())
}

fn is_fix_shaped(subject: &str) -> bool {
    let lower = subject.to_lowercase();
    FIX_WORDS.iter().any(|w| lower.contains(w))
}

/// YAML keys added or removed by a diff, each with the area it changed in and
/// whether the line was an addition. Only `+`/`-` lines count: a key sitting in
/// context was not part of the fix.
fn keys_touched(diff: &str) -> BTreeSet<(String, String, bool)> {
    let mut out = BTreeSet::new();
    let mut file = String::new();
    for line in diff.lines() {
        if let Some(rest) = line.strip_prefix("+++ b/") {
            file = rest.to_string();
            continue;
        }
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
            continue;
        }
        let added = line.starts_with('+');
        let Some(body) = line.strip_prefix('+').or_else(|| line.strip_prefix('-')) else {
            continue;
        };
        if !file.ends_with(".yml") && !file.ends_with(".yaml") {
            continue;
        }
        let trimmed = body.trim_start().trim_start_matches("- ").trim_start();
        if trimmed.starts_with('#') {
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let key = &trimmed[..colon];
        if key.is_empty()
            || key.len() > 40
            || !key
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            continue;
        }
        out.insert((key.to_string(), dir_glob(&file), added));
    }
    out
}

/// The directory a file sits in, as a glob — the narrowest honest scope for a
/// suggestion mined from that file.
fn dir_glob(file: &str) -> String {
    match file.rfind('/') {
        Some(i) => format!("{}/*.yml", &file[..i]),
        None => "*.yml".to_string(),
    }
}

/// The `[[patterns]]` block a candidate suggests, commented out.
///
/// `why` is required by the schema and carries the evidence, so a reviewer —
/// or an agent — sees the commits that argued for the rule before adopting it.
fn pattern_toml(c: &Churn) -> String {
    let cites: Vec<&str> = c.added.iter().map(|e| e.short.as_str()).collect();
    format!(
        "[[patterns]]\n\
         files = \"{}\"\n\
         assert = \"content-must-match\"\n\
         regex = \"(?m)^\\\\s*{}:\"\n\
         severity = \"warn\"\n\
         why = \"'{}' was missing and had to be added by hand in {} separate fix commits \
         ({}). Encode it so the next omission is caught before apply.\"\n",
        c.files,
        c.key,
        c.key,
        c.added.len(),
        cites.join(", ")
    )
}

fn print_suggest_human(candidates: &[&Churn], hotspots: &[&Churn], min_occurrences: usize) {
    println!(
        "\n{} mined from remediation commits\n",
        "flint history --suggest-patterns".bold()
    );
    println!(
        "{}\n",
        "SUGGESTIONS — heuristic, not verified. Review each before adopting; \
         nothing here is written to your config."
            .yellow()
    );

    if candidates.is_empty() {
        println!(
            "  {} no key was repeatedly *added* by fix commits at least {min_occurrences} times.",
            "·".dimmed()
        );
        println!(
            "  {}",
            "A guardrail is only proposed for a key the repairs kept ADDING — an omission \
             somebody rediscovered. Keys that churn through edits are listed below instead."
                .dimmed()
        );
    }
    for c in candidates {
        println!(
            "  {} in {}  {}",
            c.key.bold(),
            c.files.dimmed(),
            format!("×{} added, never removed", c.added.len()).red()
        );
        for e in &c.added {
            println!("      {} {}", e.short.dimmed(), truncate(&e.subject, 60).dimmed());
        }
        println!();
        for line in pattern_toml(c).lines() {
            println!("      {}", format!("# {line}").dimmed());
        }
        println!();
    }

    if !hotspots.is_empty() {
        println!("\n{}", "Remediation hotspots — evidence, not guardrails".bold());
        println!(
            "{}\n",
            "These keys were repaired repeatedly but through edits, so no assertion follows \
             from them. They say where a real rule would pay off, not what it should assert."
                .dimmed()
        );
        for c in hotspots.iter().take(10) {
            println!(
                "  {:<24} {:<44} {}",
                c.key,
                c.files.dimmed(),
                format!("×{} ({} added, {} edited)", c.total(), c.added.len(), c.edited.len())
                    .dimmed()
            );
        }
    }

    println!("\n{}", "What this cannot express".dimmed());
    for limit in [
        "\"this script is registered in THIS fleet\" — relational",
        "\"this hash is on THIS team's software list\" — relational",
        "\"this path resolves after the merge\" — temporal",
    ] {
        println!("  {} {}", "·".dimmed(), limit.dimmed());
    }
    println!(
        "{}",
        "\n[[patterns]] asserts are textual and structural, so mining captures the \
         convention-shaped tail, never the correctness-shaped head. Those need real rules."
            .dimmed()
    );
}

fn print_suggest_json(candidates: &[&Churn], hotspots: &[&Churn], min_occurrences: usize) {
    let ev = |list: &[Evidence]| {
        list.iter()
            .map(|e| serde_json::json!({"commit": e.short, "subject": e.subject}))
            .collect::<Vec<_>>()
    };
    let items: Vec<serde_json::Value> = candidates
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": format!("missing-key/{}", c.key),
                "status": "suggestion",
                "requires_review": true,
                "rationale": "fix commits only ever ADDED this key, so each occurrence is an \
                              omission somebody rediscovered",
                "assert": "content-must-match",
                "key": c.key,
                "files": c.files,
                "occurrences": c.added.len(),
                "evidence": ev(&c.added),
                "pattern_toml": pattern_toml(c),
            })
        })
        .collect();
    let spots: Vec<serde_json::Value> = hotspots
        .iter()
        .map(|c| {
            serde_json::json!({
                "id": format!("hotspot/{}", c.key),
                "status": "evidence_only",
                "reason_no_pattern": "the key churns through edits, not omissions — no assertion \
                                      follows from it",
                "key": c.key,
                "files": c.files,
                "repairs_total": c.total(),
                "repairs_adding": c.added.len(),
                "repairs_editing": c.edited.len(),
                "evidence": ev(&c.added).into_iter().chain(ev(&c.edited)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let doc = serde_json::json!({
        "mode": "suggest-patterns",
        "disclaimer": "Heuristic suggestions mined from remediation commits. Each requires \
                       human review before adoption; none has been written to any config.",
        "min_occurrences": min_occurrences,
        "cannot_express": [
            "relational constraints (script registered in THIS fleet, hash on THIS team)",
            "temporal constraints (path resolves after the merge)"
        ],
        "candidates": items,
        "hotspots": spots,
    });
    println!("{}", serde_json::to_string_pretty(&doc).unwrap_or_default());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(short: &str) -> CommitRef {
        CommitRef {
            sha: format!("{short}0000000000000000000000000000000000"),
            short: short.to_string(),
            subject: format!("subject {short}"),
        }
    }

    fn replayed(short: &str, codes: &[&str]) -> Replayed {
        Replayed {
            commit: commit(short),
            codes: codes.iter().map(|c| c.to_string()).collect(),
        }
    }

    #[test]
    fn a_window_closes_at_the_commit_that_stops_the_finding() {
        let history = [
            replayed("aaa", &[]),
            replayed("bbb", &["path-exists"]),
            replayed("ccc", &["path-exists"]),
            replayed("ddd", &[]),
        ];
        let windows = red_windows(&history);
        let ws = &windows["path-exists"];
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].opened.short, "bbb", "opens where it first fired");
        assert_eq!(ws[0].closed_by.as_ref().unwrap().short, "ddd", "closed by the fix");
        assert_eq!(ws[0].commits, 2);
        assert_eq!(closed(ws), 1);
    }

    /// Recurrence is the thesis: two windows for one code is the signal that
    /// separates a repeat failure from a one-off.
    #[test]
    fn separate_windows_are_counted_separately() {
        let history = [
            replayed("aaa", &["unregistered-script"]),
            replayed("bbb", &[]),
            replayed("ccc", &["unregistered-script"]),
            replayed("ddd", &[]),
        ];
        let ws = &red_windows(&history)["unregistered-script"];
        assert_eq!(closed(ws), 2, "two distinct red windows");
        assert_eq!(span(ws), 2);
    }

    /// A finding still present at HEAD is current state, not history — it must
    /// not inflate the recurrence count.
    #[test]
    fn a_window_open_at_head_is_not_counted_as_closed() {
        let history = [replayed("aaa", &[]), replayed("bbb", &["orphaned-file"])];
        let ws = &red_windows(&history)["orphaned-file"];
        assert_eq!(ws.len(), 1);
        assert!(ws[0].closed_by.is_none());
        assert_eq!(closed(ws), 0, "open windows score zero");
    }

    // -----------------------------------------------------------------------
    // the gate
    // -----------------------------------------------------------------------

    fn card(flint_only: &[(&str, usize)], gaps: &[(&str, usize)], codes: &[(&str, usize)]) -> Scorecard {
        Scorecard {
            commits: 100,
            closed_windows: codes.iter().map(|(k, n)| (k.to_string(), *n)).collect(),
            oracle: true,
            flint_only: flint_only.iter().map(|(k, n)| (k.to_string(), *n)).collect(),
            gap_shapes: gaps.iter().map(|(k, n)| (k.to_string(), *n)).collect(),
        }
    }

    #[test]
    fn an_unchanged_run_passes() {
        let base = card(&[("categories", 2)], &[("unknown key", 5)], &[("path-exists", 3)]);
        let now = card(&[("categories", 2)], &[("unknown key", 5)], &[("path-exists", 3)]);
        assert!(GateVerdict::compare(&base, &now).regressions.is_empty());
    }

    /// The expensive direction, newly appearing.
    #[test]
    fn a_rule_that_newly_over_claims_is_a_regression() {
        let base = card(&[], &[], &[]);
        let now = card(&[("categories", 3)], &[], &[]);
        let v = GateVerdict::compare(&base, &now);
        assert_eq!(v.regressions.len(), 1, "{:?}", v.regressions);
        assert!(v.regressions[0].contains("categories"));
    }

    #[test]
    fn a_new_gap_shape_is_a_regression() {
        let base = card(&[], &[("unknown key", 1)], &[]);
        let now = card(&[], &[("unknown key", 1), ("duplicate report names", 7)], &[]);
        let v = GateVerdict::compare(&base, &now);
        assert_eq!(v.regressions.len(), 1, "{:?}", v.regressions);
        assert!(v.regressions[0].contains("duplicate report names"));
    }

    /// Counts move with the range replayed — add a commit and they grow on
    /// their own. Failing on them would produce alarms that say nothing.
    #[test]
    fn a_higher_count_on_a_known_key_is_a_note_not_a_failure() {
        let base = card(&[("categories", 2)], &[("unknown key", 3)], &[]);
        let now = card(&[("categories", 9)], &[("unknown key", 40)], &[]);
        let v = GateVerdict::compare(&base, &now);
        assert!(v.regressions.is_empty(), "{:?}", v.regressions);
        assert_eq!(v.notes.len(), 1, "the flint-only rise is still reported");
        assert!(v.notes[0].contains("does not fail the gate"));
    }

    #[test]
    fn closing_a_gap_or_retiring_an_over_claim_is_an_improvement() {
        let base = card(&[("categories", 2)], &[("unknown key", 3)], &[]);
        let now = card(&[], &[], &[]);
        let v = GateVerdict::compare(&base, &now);
        assert!(v.regressions.is_empty());
        assert_eq!(v.improvements.len(), 2, "{:?}", v.improvements);
    }

    /// The question worth asking of any rule being added: would it ever have
    /// fired? Not a failure — the range may simply not contain the defect.
    /// The regression a flint version bump introduces, and the only one the
    /// gate can see without the oracle.
    #[test]
    fn a_rule_that_stops_catching_what_it_used_to_is_a_regression() {
        let base = card(&[], &[], &[("unregistered-script", 3)]);
        let now = card(&[], &[], &[("unregistered-script", 1)]);
        let v = GateVerdict::compare(&base, &now);
        assert_eq!(v.regressions.len(), 1, "{:?}", v.regressions);
        assert!(v.regressions[0].contains("no longer catches"));
    }

    #[test]
    fn a_rule_removed_entirely_is_also_caught() {
        let base = card(&[], &[], &[("unregistered-script", 3)]);
        let now = card(&[], &[], &[]);
        assert_eq!(GateVerdict::compare(&base, &now).regressions.len(), 1);
    }

    /// Replaying a longer range can only add closed windows, so a rise is
    /// never a regression.
    #[test]
    fn more_closed_windows_than_the_baseline_is_fine() {
        let base = card(&[], &[], &[("path-exists", 2)]);
        let now = card(&[], &[], &[("path-exists", 5)]);
        assert!(GateVerdict::compare(&base, &now).regressions.is_empty());
    }

    #[test]
    fn a_new_rule_with_no_history_is_noted_not_failed() {
        let base = card(&[], &[], &[("path-exists", 3)]);
        let now = card(&[], &[], &[("path-exists", 3), ("brand-new-rule", 0)]);
        let v = GateVerdict::compare(&base, &now);
        assert!(v.regressions.is_empty());
        assert!(
            v.notes.iter().any(|n| n.contains("brand-new-rule") && n.contains("0 closed windows")),
            "{:?}",
            v.notes
        );
    }

    /// A baseline recorded with the oracle, compared against a run without it,
    /// would silently look like every gap had closed.
    /// The worst thing this gate could print is "all gaps closed" because the
    /// oracle simply was not run. CI without the oracle must compare neither
    /// half, not compare them against nothing.
    #[test]
    fn dropping_the_oracle_compares_neither_half_and_says_so() {
        let base = card(&[("categories", 1)], &[("unknown key", 1)], &[("path-exists", 2)]);
        let mut now = card(&[], &[], &[("path-exists", 2)]);
        now.oracle = false;
        let v = GateVerdict::compare(&base, &now);
        assert!(v.regressions.is_empty(), "{:?}", v.regressions);
        assert!(
            v.improvements.is_empty(),
            "vanished gaps must NOT read as closed: {:?}",
            v.improvements
        );
        assert!(v.notes.iter().any(|n| n.contains("NOT compared")), "{:?}", v.notes);
    }

    /// The closed-window half still gates without the oracle — that is what
    /// makes a replay-only CI run worth running.
    #[test]
    fn without_the_oracle_the_closed_window_half_still_gates() {
        let base = card(&[], &[], &[("unregistered-script", 3)]);
        let mut now = card(&[], &[], &[("unregistered-script", 0)]);
        now.oracle = false;
        assert_eq!(GateVerdict::compare(&base, &now).regressions.len(), 1);
    }

    #[test]
    fn fix_shaped_subjects_are_recognised() {
        assert!(is_fix_shaped("Fix CI failure: register enable-autofill.sh"));
        assert!(is_fix_shaped("policy run_script targets must also be in controls.scripts"));
        assert!(is_fix_shaped("Restore missing profile"));
        assert!(!is_fix_shaped("Add ABC fleets for ZULU, KILO, GHI"));
        assert!(!is_fix_shaped("Bump dock package to 1.0.2"));
    }

    #[test]
    fn only_changed_yaml_keys_are_collected() {
        let diff = "\
+++ b/fleets/a.yml
@@ -1,2 +1,3 @@
+name: ABC
-query: SELECT 1;
 description: untouched context line
+# comment: ignored
";
        let keys = keys_touched(diff);
        assert!(keys.contains(&("name".into(), "fleets/*.yml".into(), true)));
        assert!(keys.contains(&("query".into(), "fleets/*.yml".into(), false)));
        assert!(
            !keys.iter().any(|(k, _, _)| k == "description"),
            "context lines were not part of the fix"
        );
        assert!(!keys.iter().any(|(k, _, _)| k == "comment"), "comments are not keys");
    }

    #[test]
    fn non_yaml_files_contribute_no_keys() {
        let diff = "+++ b/scripts/thing.sh\n@@ -1 +1 @@\n+echo hello: world\n";
        assert!(keys_touched(diff).is_empty());
    }

    fn churn(key: &str, added: usize, edited: usize) -> Churn {
        let ev = |n: usize| {
            (0..n)
                .map(|i| Evidence {
                    short: format!("c{i}"),
                    subject: "s".into(),
                })
                .collect()
        };
        Churn {
            key: key.to_string(),
            files: "fleets/*.yml".into(),
            added: ev(added),
            edited: ev(edited),
        }
    }

    /// The distinction that decides signal from noise. `path:` churns because
    /// paths get edited; asserting every fleet must contain `path:` encodes
    /// nothing anyone learned.
    #[test]
    fn only_omission_shaped_churn_becomes_a_guardrail() {
        assert!(churn("bootstrap", 3, 0).is_guardrail_shaped(2));
        assert!(churn("bootstrap", 4, 1).is_guardrail_shaped(2));
        assert!(!churn("path", 8, 8).is_guardrail_shaped(2), "edit churn is not a convention");
        assert!(!churn("post_install_script", 2, 3).is_guardrail_shaped(2));
        assert!(!churn("rare", 1, 0).is_guardrail_shaped(2), "one repair is an anecdote");
    }

    /// The `why` field is required by the pattern schema and is what lets a
    /// reviewer — or an agent — judge the suggestion.
    #[test]
    fn suggested_pattern_carries_its_evidence() {
        let toml = pattern_toml(&churn("bootstrap", 2, 0));
        assert!(toml.contains("assert = \"content-must-match\""));
        assert!(toml.contains("files = \"fleets/*.yml\""));
        assert!(toml.contains("why = "), "the schema requires a reason");
        assert!(toml.contains("c0, c1"), "cites the commits: {toml}");
    }

    // -----------------------------------------------------------------------
    // oracle diff
    // -----------------------------------------------------------------------

    fn blocking(pairs: &[(&str, &[&str])]) -> BTreeMap<String, BTreeSet<String>> {
        pairs
            .iter()
            .map(|(f, codes)| {
                (
                    f.to_string(),
                    codes.iter().map(|c| c.to_string()).collect::<BTreeSet<_>>(),
                )
            })
            .collect()
    }

    fn fleet(pairs: &[(&str, &str)]) -> BTreeMap<String, Vec<String>> {
        pairs
            .iter()
            .map(|(f, m)| (f.to_string(), vec![m.to_string()]))
            .collect()
    }

    /// Fleet's parser judges only entry files. A profile or standalone policy
    /// list is validated as part of its PARENT, so counting it either way
    /// invents a verdict Fleet never gave.
    #[test]
    fn files_fleet_has_no_opinion_on_are_excluded_from_both_sides() {
        let mut d = Diff::default();
        let ignored: BTreeSet<String> = ["profiles/a.mobileconfig".to_string()].into();
        d.absorb(
            &commit("aaa"),
            &blocking(&[("profiles/a.mobileconfig", &["profile-well-formed"])]),
            &fleet(&[("profiles/a.mobileconfig", "not an entry file")]),
            &ignored,
        );
        assert_eq!(d.agreed, 0, "a fragment is not an agreement");
        assert!(d.flint_only.is_empty(), "nor an over-claim");
        assert!(d.fleet_only.is_empty(), "nor a gap");
    }

    #[test]
    fn agreement_is_counted_not_reported() {
        let mut d = Diff::default();
        d.absorb(
            &commit("aaa"),
            &blocking(&[("fleets/a.yml", &["path-exists"])]),
            &fleet(&[("fleets/a.yml", "no such file")]),
            &BTreeSet::new(),
        );
        assert_eq!(d.agreed, 1);
        assert!(d.flint_only.is_empty());
        assert!(d.fleet_only.is_empty());
    }

    /// The expensive direction: flint claiming an apply will fail where Fleet's
    /// own parser accepts the file.
    #[test]
    fn flint_blocking_where_fleet_accepts_is_bucketed_by_code() {
        let mut d = Diff::default();
        d.absorb(
            &commit("aaa"),
            &blocking(&[("fleets/a.yml", &["categories", "fma-slug"])]),
            &BTreeMap::new(),
            &BTreeSet::new(),
        );
        assert_eq!(d.agreed, 0);
        assert_eq!(d.flint_only["categories"].0, 1);
        assert_eq!(d.flint_only["fma-slug"].0, 1);
        assert_eq!(d.flint_only["categories"].1[0].file, "fleets/a.yml");
    }

    /// The gap list: Fleet rejects, flint says nothing.
    #[test]
    fn fleet_blocking_where_flint_is_silent_clusters_by_message_shape() {
        let mut d = Diff::default();
        for (c, f) in [("aaa", "fleets/a.yml"), ("bbb", "fleets/b.yml")] {
            d.absorb(
                &commit(c),
                &BTreeMap::new(),
                &fleet(&[(f, &format!("environment variable \"{f}\" not set"))]),
                &BTreeSet::new(),
            );
        }
        // Two different files, one shape — they must cluster into one entry.
        assert_eq!(d.fleet_only.len(), 1, "got: {:?}", d.fleet_only.keys().collect::<Vec<_>>());
        let (n, examples) = d.fleet_only.values().next().unwrap();
        assert_eq!(*n, 2);
        assert_eq!(examples.len(), 2);
    }

    #[test]
    fn examples_are_capped_but_the_count_is_not() {
        let mut d = Diff::default();
        for i in 0..10 {
            d.absorb(
                &commit(&format!("c{i}")),
                &blocking(&[("fleets/a.yml", &["categories"])]),
                &BTreeMap::new(),
                &BTreeSet::new(),
            );
        }
        let (n, examples) = &d.flint_only["categories"];
        assert_eq!(*n, 10, "every occurrence counts");
        assert_eq!(examples.len(), EXAMPLES, "only a few are retained");
    }

    #[test]
    fn normalise_collapses_quoted_specifics() {
        assert_eq!(
            normalise("environment variable \"FLEET_URL\" not set"),
            normalise("environment variable \"FLEET_BOOTSTRAP_TOKEN\" not set"),
            "same complaint, different variable — must cluster"
        );
        assert_ne!(
            normalise("environment variable \"X\" not set"),
            normalise("glob pattern \"X\" matched no files"),
            "different complaints must not cluster"
        );
    }

    #[test]
    fn rel_keys_both_tools_the_same_way() {
        let base = Path::new("/tmp/flint-history-abc");
        assert_eq!(rel(base, Path::new("/tmp/flint-history-abc/fleets/a.yml")), "fleets/a.yml");
        // A path already relative, or outside the base, is passed through.
        assert_eq!(rel(base, Path::new("fleets/a.yml")), "fleets/a.yml");
    }

    #[test]
    fn dir_glob_scopes_to_the_files_directory() {
        assert_eq!(dir_glob("fleets/a.yml"), "fleets/*.yml");
        assert_eq!(dir_glob("platforms/macos/policies/x.yml"), "platforms/macos/policies/*.yml");
        assert_eq!(dir_glob("default.yml"), "*.yml");
    }

    #[test]
    fn truncate_keeps_short_strings_intact() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
    }
}
