//! `flint paths` — broken `path:` reference report (+ --fix) and the
//! unwired-artifact report.

use crate::args::PathsArgs;
use crate::commands::apply_fixes;
use crate::interactive::wire::interactive_unwired;
use flint_lint as linter;
use std::path::PathBuf;

pub(crate) fn run(args: PathsArgs) -> anyhow::Result<()> {
    let PathsArgs {
        path,
        fix,
        unwired,
        interactive,
        label_stubs,
        only,
        oneline,
        prompt,
    } = args;

    use colored::Colorize;
    use linter::error::{LintError, LintReport};
    use linter::Linter;

    // --unwired: report artifacts that exist on disk but nothing wires.
    if unwired {
        if interactive {
            interactive_unwired(&path, label_stubs.as_deref(), only.as_deref())?;
        } else if oneline {
            report_unwired_oneline(&path);
        } else if prompt {
            report_unwired_prompt(&path, only.as_deref());
        } else {
            report_unwired(&path);
        }
        return Ok(());
    }

    let linter = Linter::from_path(&path);
    let results: Vec<(PathBuf, LintReport)> = if path.is_dir() {
        linter.lint_directory(&path, None)?
    } else if path.is_file() {
        vec![(path.clone(), linter.lint_file(&path)?)]
    } else {
        anyhow::bail!("path not found: {}", path.display());
    };

    let is_path_finding = |e: &&LintError| {
        matches!(
            e.rule_code,
            Some("path-exists") | Some("path-case") | Some("path-is-file") | Some("path-empty")
        )
    };

    let mut total = 0usize;
    let mut fixable = 0usize;
    let mut fixed_total = 0usize;
    let mut blocks: Vec<String> = Vec::new();

    // Resolved once: every archaeology lookup is relative to the same root.
    let repo_root = git_toplevel(&path);

    for (file_path, report) in &results {
        let findings: Vec<&LintError> = report
            .errors
            .iter()
            .chain(report.warnings.iter())
            .filter(is_path_finding)
            .collect();
        if findings.is_empty() {
            continue;
        }

        // Apply only the path findings (so unrelated safe fixes are
        // never touched by `flint paths --fix`).
        if fix {
            let mut subset = LintReport::new();
            for e in &findings {
                subset.add((*e).clone());
            }
            if let Ok(n) = apply_fixes(file_path, &subset, false) {
                fixed_total += n;
            }
        }

        let mut block = format!("{}\n", file_path.display().to_string().bold());
        for e in &findings {
            total += 1;
            let loc = e.line().map(|l| format!("L{l}")).unwrap_or_else(|| "—".into());
            let old = e.context.as_deref().unwrap_or("");
            match e.suggestion() {
                Some(new) => {
                    fixable += 1;
                    block.push_str(&format!("  {}  {} {}\n", loc.dimmed(), "-".red(), old.red()));
                    block.push_str(&format!("      {} {}\n", "+".green(), new.green()));
                }
                None => {
                    let why = e.help.as_deref().unwrap_or("no unique match found");
                    block.push_str(&format!(
                        "  {}  {} {}  ({})\n",
                        loc.dimmed(),
                        "?".yellow(),
                        old.yellow(),
                        why
                    ));
                    // No unique match on disk — but git usually knows what
                    // happened to the file.
                    if let Some(note) = repo_root
                        .as_deref()
                        .and_then(|root| archaeology(root, file_path, old))
                    {
                        block.push_str(&format!("      {} {}\n", "↳".dimmed(), note.dimmed()));
                    }
                }
            }
        }
        blocks.push(block);
    }

    if total == 0 {
        println!("{} No broken path references found.", "✓".green());
        return Ok(());
    }

    println!("{} {}\n", "Path references —".bold(), path.display());
    for b in &blocks {
        println!("{b}");
    }

    if fix {
        println!("{} Fixed {} reference(s).", "✓".green(), fixed_total);
        let remaining = total - fixed_total;
        if remaining > 0 {
            println!(
                "{} {} reference(s) need manual attention.",
                "!".yellow(),
                remaining
            );
        }
    } else {
        println!(
            "{} broken reference(s), {} auto-fixable. Run `flint paths {} --fix` to apply.",
            total,
            fixable,
            path.display()
        );
    }

    Ok(())
}

/// Report artifacts that exist on disk but no fleet config references, grouped
/// The git repository containing `start`, if any.
fn git_toplevel(start: &std::path::Path) -> Option<PathBuf> {
    let dir = if start.is_dir() { start } else { start.parent()? };
    let out = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()))
        .and_then(|p| p.canonicalize().ok())
}

/// What git knows about a missing target: the commit that deleted it, or the
/// path it was renamed to.
///
/// `flint paths --help` promised "where the files moved to", yet the incident's
/// 48 broken references all reported "no unique match found" — every one had
/// been deleted or renamed in a commit git could name. `git log` follows a
/// path that no longer exists, and `--name-status -M` distinguishes the two
/// cases, so the answer costs one process per finding.
fn archaeology(repo_root: &std::path::Path, referrer: &std::path::Path, target: &str) -> Option<String> {
    // Resolve the reference the way Fleet does — relative to the file that
    // holds it — then make it repo-relative for git.
    let base = referrer.parent()?.canonicalize().ok()?;
    let mut abs = base;
    for comp in std::path::Path::new(target).components() {
        match comp {
            std::path::Component::ParentDir => {
                abs.pop();
            }
            std::path::Component::CurDir => {}
            other => abs.push(other),
        }
    }
    let rel = abs.strip_prefix(repo_root).ok()?.to_string_lossy().replace('\\', "/");

    let out = std::process::Command::new("git")
        .args(["log", "-1", "-M", "--name-status", "--format=%h%x1f%s", "--", &rel])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    describe_name_status(&String::from_utf8_lossy(&out.stdout))
}

/// One `git log -1 -M --name-status --format=%h%x1f%s` record: the commit
/// line, then `D\tpath` for a deletion or `R<score>\told\tnew` for a rename.
fn describe_name_status(text: &str) -> Option<String> {
    let mut lines = text.lines().filter(|l| !l.is_empty());
    let (sha, subject) = lines.next()?.split_once('\u{1f}')?;
    for line in lines {
        let mut cols = line.split('\t');
        let status = cols.next()?;
        if status.starts_with('D') {
            return Some(format!("deleted in {sha} \"{subject}\""));
        }
        if status.starts_with('R') {
            let _old = cols.next()?;
            let new = cols.next()?;
            return Some(format!("renamed to {new} in {sha} \"{subject}\""));
        }
    }
    None
}
/// Resolve the scan root the same way every unwired reporter does.
fn unwired_root(path: &std::path::Path) -> PathBuf {
    if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    }
}

/// One tab-separated record per artifact: `rel <TAB> section <TAB> wire`.
///
/// The grouped report is built for a reader — 270 lines of YAML for a repo
/// this size — which makes "is THIS file wired?" hard to answer. One line per
/// artifact makes the report a filter target:
///
/// ```text
/// flint paths --unwired --oneline | grep pilot
/// ```
///
/// Tabs, not spaces, so `cut -f2` works regardless of path length. No colour
/// and no header: every line is data, so a non-empty exit means "still
/// orphaned" and an empty one means "wired".
fn report_unwired_oneline(path: &std::path::Path) {
    use linter::unwired::find_unwired;

    for a in find_unwired(&unwired_root(path)).artifacts {
        println!("{}\t{}\t{}", a.rel, a.section, a.wire);
    }
}

/// Emit a self-contained instruction per artifact, sized for a small model.
///
/// The grouped report tells a human where an artifact *could* go and leaves
/// the judgement to them. A weak agent cannot make that judgement, so this
/// removes every decision from the task: the key path, the exact line to
/// insert, and two commands that decide whether the job is done. The negative
/// check matters most — `flint check` passing only proves nothing broke, a
/// bar the agent clears by doing nothing at all, whereas the grep going quiet
/// can only happen if THIS artifact got wired.
fn report_unwired_prompt(path: &std::path::Path, only: Option<&str>) {
    use linter::unwired::{find_unwired, glob_match};

    let root = unwired_root(path);
    let report = find_unwired(&root);
    if report.artifacts.is_empty() {
        println!("No unwired artifacts. Nothing to do.");
        return;
    }

    // Which fleet file to wire INTO is the one thing flint cannot decide — a
    // repo with 25 fleets has 25 right answers. Naming the first one found
    // would be worse than naming none: a weak agent complies confidently and
    // edits the wrong fleet. So the target is only stated when `--only`
    // narrows it to exactly one; otherwise the choice is spelled out as the
    // single decision the caller has to make.
    let candidates: Vec<String> = linter::unwired::config_files(&root)
        .iter()
        .filter_map(|p| {
            p.strip_prefix(&root)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .filter(|r| match only {
            Some(g) => glob_match(g, r) || r.ends_with(g),
            None => true,
        })
        .collect();

    let target = match candidates.as_slice() {
        [one] => one.clone(),
        [] => {
            eprintln!(
                "No fleet file matches {:?}. Run without --only to see the candidates.",
                only.unwrap_or("")
            );
            return;
        }
        many => {
            let shown: Vec<&str> = many.iter().take(6).map(String::as_str).collect();
            let more = many.len().saturating_sub(shown.len());
            format!(
                "<CHOOSE ONE of {} fleet files: {}{}>",
                many.len(),
                shown.join(", "),
                if more > 0 {
                    format!(", +{more} more")
                } else {
                    String::new()
                }
            )
        }
    };
    let ambiguous = candidates.len() > 1;
    if ambiguous {
        println!(
            "NOTE  {} fleet files exist, so EDIT below is a choice, not an instruction.\n\
                   Re-run with `--only <file>` to pin it and remove the decision.\n",
            candidates.len()
        );
    }

    for (i, a) in report.artifacts.iter().enumerate() {
        let key = a.section;
        let line = if a.single_only {
            // No list form: the section takes one file, written as `key: value`.
            format!("{}: {}", key.rsplit('.').next().unwrap_or(key), a.wire)
        } else {
            format!("- path: {}", a.wire)
        };
        let name = a
            .rel
            .rsplit('/')
            .next()
            .and_then(|f| f.split('.').next())
            .unwrap_or(&a.rel);

        println!("TASK {}/{}  Wire 1 orphaned artifact into a Fleet GitOps fleet file.", i + 1, report.artifacts.len());
        println!();
        println!("EDIT    {target}");
        println!("UNDER   {key}");
        println!("INSERT  {line}");
        println!();
        println!("RULES");
        println!("1. Add the INSERT text under the key path UNDER.");
        println!("2. Create that nesting only if it is absent. Two-space indent.");
        println!("3. Change nothing else in the file.");
        println!();
        println!("DONE WHEN");
        println!("  flint check {target}   prints no error");
        println!("  flint paths --unwired --oneline | grep {name}   prints nothing");
        println!();
    }
}

/// by directory, with copy-paste `paths:` glob and `path:` constructs suggested
/// per type. Paths are written as a fleet file (e.g. `fleets/x.yml`) would.
fn report_unwired(path: &std::path::Path) {
    use colored::Colorize;
    use linter::unwired::{find_unwired, UnwiredArtifact};
    use std::collections::BTreeMap;

    let root = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    };

    let report = find_unwired(&root);
    let orphans = report.artifacts;
    if orphans.is_empty() {
        println!("{} No unwired artifacts found.", "✓".green());
        return;
    }

    // Group by (section, wire-dir, ext) so one `paths:` glob can cover a dir.
    // wire-dir is the artifact's directory expressed relative to where the
    // repo's fleet files live (so the glob matches the repo's structure).
    let mut groups: BTreeMap<(&'static str, String, String), Vec<&UnwiredArtifact>> =
        BTreeMap::new();
    for o in &orphans {
        let p = std::path::Path::new(&o.wire);
        let dir = p
            .parent()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_default();
        let ext = p
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        groups.entry((o.section, dir, ext)).or_default().push(o);
    }

    // Describe where the paths are written from, accounting for repo structure.
    let from_desc = if report.wire_base_rel.is_empty() {
        "the repo root (default.yml)".to_string()
    } else {
        format!("{}/ (where this repo's fleet files live)", report.wire_base_rel)
    };

    println!("{} {}", "Unwired artifacts —".bold(), root.display());
    println!(
        "{}",
        "These files exist but no fleet/team config references them.".dimmed()
    );
    println!(
        "{}",
        format!(
            "Add each block to a fleet/team file; paths shown are relative to {from_desc}. \
             If you wire from a file at a different depth, adjust the leading ../."
        )
        .dimmed()
    );
    println!();

    for ((section, dir, ext), files) in &groups {
        let single = files[0].single_only;
        println!(
            "{} {} unwired · {}/  → scope: a fleet/team file, under {}",
            "▸".blue(),
            files.len(),
            dir,
            section.bold()
        );

        // Render the dotted section as a nested YAML block so the exact
        // placement is unambiguous.
        let keys: Vec<&str> = section.split('.').collect();
        let mut indent = String::from("    ");
        for k in &keys[..keys.len() - 1] {
            println!("{indent}{k}:");
            indent.push_str("  ");
        }
        let leaf = keys[keys.len() - 1];

        if single {
            // Scalar field, one file per team — show each as an option.
            println!(
                "{indent}{}{}",
                format!("{leaf}:").green(),
                "  # one per team — pick the file for this team:".dimmed()
            );
            for o in files.iter() {
                println!("{indent}{}", format!("  # {leaf}: {}", o.wire).green());
            }
        } else {
            // List field — one glob covers the dir, or list files individually.
            println!("{indent}{leaf}:");
            println!("{indent}  {}", format!("- paths: {dir}/*.{ext}").green());
            println!("{indent}  {}", "# …or individually:".dimmed());
            for o in files.iter() {
                println!("{indent}  {}", format!("# - path: {}", o.wire).dimmed());
            }
        }
        println!();
    }

    println!(
        "{} {} unwired artifact(s) across {} group(s).",
        "✗".yellow(),
        orphans.len(),
        groups.len()
    );
}

/// One-line summary of an artifact for the interactive picker: a profile's
/// display name + scope + payload type, a declaration's identifier + type, or
/// just the filename for other artifact kinds.
pub(crate) fn describe_artifact(path: &std::path::Path) -> String {
    use colored::Colorize;
    use linter::profile::{parse_declaration, parse_mobileconfig};

    let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let content = std::fs::read_to_string(path).unwrap_or_default();
    match path.extension().and_then(|e| e.to_str()) {
        Some("mobileconfig") => {
            let i = parse_mobileconfig(&content);
            let name = i
                .display_name
                .as_deref()
                .or(i.identifier.as_deref())
                .unwrap_or(fname);
            let scope = i.scope.as_deref().unwrap_or("System");
            let types = if i.payload_types.is_empty() {
                String::new()
            } else {
                format!(" · {}", i.payload_types.join(", "))
            };
            format!("{} [{scope}]{types}  {}", name.bold(), fname.dimmed())
        }
        Some("json") => {
            let d = parse_declaration(&content);
            let id = d.identifier.as_deref().unwrap_or(fname);
            let t = d.decl_type.as_deref().unwrap_or("");
            format!("{}  {}  {}", id.bold(), t, fname.dimmed())
        }
        _ => fname.to_string(),
    }
}

#[cfg(test)]
mod archaeology_tests {
    use super::describe_name_status;

    #[test]
    fn a_deletion_names_the_commit() {
        let out = "cffe9ac\u{1f}Delete app-notifications-teams.mobileconfig\nD\tplatforms/x/app-notifications-teams.mobileconfig\n";
        assert_eq!(
            describe_name_status(out).as_deref(),
            Some("deleted in cffe9ac \"Delete app-notifications-teams.mobileconfig\"")
        );
    }

    #[test]
    fn a_rename_names_the_new_path() {
        let out = "e012ae0\u{1f}Profile naming pass\nR100\told/Efficient Elements.yml\tnew/efficient-elements.yml\n";
        assert_eq!(
            describe_name_status(out).as_deref(),
            Some("renamed to new/efficient-elements.yml in e012ae0 \"Profile naming pass\"")
        );
    }

    /// A modification is not an explanation for a missing file.
    #[test]
    fn an_edit_is_not_reported() {
        assert!(describe_name_status("abc1234\u{1f}tweak\nM\tpath.yml\n").is_none());
        assert!(describe_name_status("").is_none());
    }
}
