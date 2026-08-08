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
    } = args;

    use colored::Colorize;
    use linter::error::{LintError, LintReport};
    use linter::Linter;

    // --unwired: report artifacts that exist on disk but nothing wires.
    if unwired {
        if interactive {
            interactive_unwired(&path, label_stubs.as_deref(), only.as_deref())?;
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
