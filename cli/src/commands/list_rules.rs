//! `flint list-rules` — inventory of per-file and cross-file lint rules.

use crate::args::ListRulesArgs;
use flint_lint as linter;

/// Whether `--fix` can ever apply a fix for this rule's code.
///
/// Read from the code registry rather than a per-rule constant so there is one
/// source of truth, guarded against the rules' actual behaviour by
/// `flint-lint/tests/fixable_metadata.rs`. An unregistered name answers `no`:
/// silence is the safe direction for a flag that promises a fix.
fn is_fixable(rule_name: &str) -> bool {
    linter::codes::meta(rule_name).is_some_and(|m| m.fixable)
}

pub(crate) fn run(args: ListRulesArgs) -> anyhow::Result<()> {
    let format = args.format;
    let ruleset = linter::RuleSet::default_rules();

    // Cross-file checks live in a directory-level graph pass, not the
    // per-file Rule registry, so they're listed here explicitly. They
    // run only on `flint check <dir>` (a repo is needed to resolve
    // references) and are configurable by these codes in .fleetlint.toml.
    let cross_file: &[(&str, &str, &str)] = &[
        (
            "label-reference",
            "cross-file",
            "Flags labels_include_*/exclude_any values not defined in the repo (nor a Fleet built-in)",
        ),
        (
            "install-software-hash",
            "cross-file",
            "Flags policy install_software.hash_sha256 with no matching software package in the repo",
        ),
        (
            "install-software-team",
            "cross-file",
            "Flags a fleet/team whose policy auto-installs a package not in that team's software list",
        ),
        (
            "install-software-id",
            "cross-file",
            "Flags a policy whose query checks a different package id than the package it install_software installs",
        ),
        (
            "app-store-vpp",
            "cross-file",
            "Flags software.app_store_apps when no volume_purchasing_program (VPP) is configured in org_settings",
        ),
    ];

    if format == "json" {
        // v0.2.0: the "preview" and "severity" keys were dropped — they were
        // static metadata the engine never enforced (see CHANGELOG). docs_url
        // now comes from the code registry.
        let mut rules: Vec<serde_json::Value> = ruleset
            .rules()
            .iter()
            .map(|r| {
                serde_json::json!({
                    "name": r.name(),
                    "description": r.description(),
                    "category": r.category(),
                    "fixable": is_fixable(r.name()),
                    "docs_url": linter::codes::doc_url(r.name()),
                })
            })
            .collect();
        for (name, category, desc) in cross_file {
            rules.push(serde_json::json!({
                "name": name,
                "description": desc,
                "category": category,
                "fixable": false,
                "docs_url": linter::codes::doc_url(name),
            }));
        }
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "rules": rules,
                "total": rules.len(),
            }))?
        );
    } else {
        use colored::Colorize;
        println!("{}", "Fleet GitOps Lint Rules".bold());
        println!("{}", "=".repeat(90));
        println!(
            "{:<28} {:<14} {:<8} {}",
            "Rule".bold(),
            "Category".bold(),
            "Fixable".bold(),
            "Description".bold()
        );
        println!("{}", "-".repeat(90));
        for rule in ruleset.rules() {
            let fixable = if is_fixable(rule.name()) {
                "yes".green()
            } else {
                "no".dimmed()
            };
            println!(
                "{:<28} {:<14} {:<8} {}",
                rule.name(),
                rule.category(),
                fixable,
                rule.description()
            );
        }
        for (name, category, desc) in cross_file {
            println!("{:<28} {:<14} {:<8} {}", name, category, "no".dimmed(), desc);
        }
        println!("{}", "-".repeat(90));
        println!(
            "{} rule(s) total ({} cross-file, directory-level)",
            ruleset.rules().len() + cross_file.len(),
            cross_file.len()
        );
    }

    Ok(())
}
