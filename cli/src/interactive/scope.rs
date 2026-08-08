//! The `flint init` scope assistant: one in/out question per directory,
//! a delta before anything is written.
//!
//! All the measurement lives in `flint_lint::scope`. This file is only the
//! terminal half, on the same `ask`/`ask_yes` primitives the unwired-file
//! walk uses.

use super::ask;
use colored::Colorize;
use flint_lint::scope::{self, ScopePreview, ScopeScan, ScopeSelection, ScopeUnit};
use flint_lint::{DetectedConfig, InitPrompts, StrictnessLevel};

/// Drives `flint init`'s questions over stdin.
pub(crate) struct TerminalPrompts;

impl InitPrompts for TerminalPrompts {
    fn strictness(&self, _detected: &DetectedConfig) -> anyhow::Result<StrictnessLevel> {
        let stdin = std::io::stdin();
        println!("\n{}", "? What strictness level would you like?".bold());
        println!(
            "  {} - Enforce best practices (require platform, warn on SELECT *)",
            "1. Strict".cyan()
        );
        println!(
            "  {} - Balanced defaults (recommended)",
            "2. Moderate".green()
        );
        println!("  {} - Minimal warnings", "3. Relaxed".yellow());
        let answer = ask(&stdin, "\n  Enter choice [2]: ")?.unwrap_or_default();
        Ok(flint_lint::parse_strictness(&answer))
    }

    fn scope(&self, scan: &ScopeScan) -> anyhow::Result<ScopeSelection> {
        let stdin = std::io::stdin();

        if scan.top_level().is_empty() {
            return Ok(ScopeSelection::default());
        }

        println!("\n{}", "? Which directories should flint lint?".bold());
        println!(
            "  {}",
            "Answers become DIRECTORY globs. A narrowed scope also scopes the"
                .dimmed()
        );
        println!(
            "  {}",
            "cross-file rules (orphaned-file, duplicate-content, case-collision,".dimmed()
        );
        println!(
            "  {}",
            "unregistered-script), which report on scripts and profiles too.".dimmed()
        );

        // The loop exists so "no, let me redo that" costs one keystroke
        // rather than a re-run of the whole command.
        loop {
            let mut selection = ScopeSelection::default();
            println!();
            for unit in scan.top_level() {
                if !walk_unit(&stdin, scan, unit, &mut selection)? {
                    // EOF or quit — keep whatever was answered so far.
                    break;
                }
            }

            let preview = scope::preview(scan, &selection);
            print_preview(&preview);

            match ask(
                &stdin,
                "\n  Write this scope?  [y]es / [e]dit again / [s]kip (leave scope unset): ",
            )?
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
            {
                "e" | "edit" => continue,
                "s" | "skip" | "n" | "no" => return Ok(ScopeSelection::default()),
                _ => return Ok(selection),
            }
        }
    }
}

/// Ask about one unit; on a "drill in" answer, recurse into its
/// subdirectories. Returns false when the user quit or stdin closed.
fn walk_unit(
    stdin: &std::io::Stdin,
    scan: &ScopeScan,
    unit: &ScopeUnit,
    selection: &mut ScopeSelection,
) -> anyhow::Result<bool> {
    let indent = "  ".repeat(unit.depth + 1);
    let children = if unit.is_dir {
        scan.children(&unit.rel)
    } else {
        Vec::new()
    };

    println!("{}{} {}", indent, "▸".blue(), unit.label().bold());
    println!("{}  {}", indent, describe(unit).dimmed());

    let choices = if children.is_empty() {
        "[Y]es / [n]o: "
    } else {
        "[Y]es / [n]o / [d]rill in: "
    };
    let answer = match ask(stdin, &format!("{indent}  In scope? {choices}"))? {
        Some(a) => a.to_lowercase(),
        None => return Ok(false),
    };

    match answer.as_str() {
        "n" | "no" => {
            selection.decide(unit, false);
            // Surface the consequence at the moment of the decision, not
            // only in the summary — this is the answer most likely to be
            // given carelessly.
            if !unit.referenced.is_empty() {
                println!(
                    "{}  {} {}",
                    indent,
                    "⚠".yellow(),
                    format!(
                        "{} config file(s) reference {} file(s) in here",
                        unit.referencing.len(),
                        unit.referenced.len()
                    )
                    .yellow()
                );
            }
        }
        "d" | "drill" if !children.is_empty() => {
            selection.decide(unit, true);
            for child in children {
                if !walk_unit(stdin, scan, child, selection)? {
                    return Ok(false);
                }
            }
        }
        "q" | "quit" => return Ok(false),
        // Blank or anything else: keep it. Defaulting to "in scope" means a
        // hurried answer can never silence a rule.
        _ => selection.decide(unit, true),
    }
    Ok(true)
}

/// One-line summary of what a unit holds and who points at it.
fn describe(unit: &ScopeUnit) -> String {
    let mut s = format!("{} file(s)", unit.files);
    if unit.yaml_files > 0 && unit.yaml_files != unit.files {
        s.push_str(&format!(", {} YAML", unit.yaml_files));
    }
    if !unit.referencing.is_empty() {
        s.push_str(&format!(
            " · referenced by {} config(s)",
            unit.referencing.len()
        ));
    }
    s
}

/// Requirement 3: make the choice concrete before it is written.
fn print_preview(preview: &ScopePreview) {
    println!();
    for w in &preview.warnings {
        println!(
            "  {} {}",
            "⚠".yellow(),
            format!(
                "{} is out of scope, but {} config file(s) reference {} file(s) under it",
                w.rel, w.configs, w.files
            )
            .yellow()
        );
        if !w.examples.is_empty() {
            println!("      {}", w.examples.join(", ").dimmed());
        }
        // Not a mistake, and worth saying so plainly: scoping filters the
        // finding subject, not the workspace file set. The references still
        // resolve; what stops is flint reporting ON those files.
        println!(
            "      {}",
            "references still resolve — findings about those files are suppressed".dimmed()
        );
    }

    if preview.include.is_empty() {
        println!(
            "  {} nothing narrowed — all {} file(s) stay in scope.",
            "→".bold(),
            preview.total
        );
        if !preview.exclude.is_empty() && preview.skipped > 0 {
            println!(
                "      {}",
                format!("({} skipped by the default excludes)", preview.skipped).dimmed()
            );
        }
        return;
    }

    println!(
        "  {} this config puts {} of {} file(s) in scope; {} skipped.",
        "→".bold(),
        preview.in_scope.to_string().green(),
        preview.total,
        preview.skipped.to_string().yellow()
    );
    println!("      {} {}", "include:".dimmed(), preview.include.join(", "));
    println!("      {} {}", "exclude:".dimmed(), preview.exclude.join(", "));
}
