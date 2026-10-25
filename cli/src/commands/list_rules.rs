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

/// One row per code in the registry: every code flint can emit. A per-file
/// rule describes itself; every other code carries its description in the
/// registry (exactly one source each — `every_code_has_one_description`).
fn rows() -> Vec<(&'static str, &'static str, bool, &'static str)> {
    let ruleset = linter::RuleSet::default_rules();
    linter::codes::REGISTRY
        .iter()
        .map(|m| {
            let rule = ruleset.rules().iter().find(|r| r.name() == m.code);
            let category = rule.map_or(m.category, |r| r.category());
            let description = rule.map_or(m.description.unwrap_or_default(), |r| r.description());
            (m.code, category, is_fixable(m.code), description)
        })
        .collect()
}

pub(crate) fn run(args: ListRulesArgs) -> anyhow::Result<()> {
    let rows = rows();
    if args.format == "json" {
        let rules: Vec<serde_json::Value> = rows
            .iter()
            .map(|(name, category, fixable, description)| {
                serde_json::json!({
                    "name": name,
                    "description": description,
                    "category": category,
                    "fixable": fixable,
                    "docs_url": linter::codes::doc_url(name),
                })
            })
            .collect();
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
            "{:<32} {:<14} {:<8} {}",
            "Rule".bold(),
            "Category".bold(),
            "Fixable".bold(),
            "Description".bold()
        );
        println!("{}", "-".repeat(90));
        for (name, category, fixable, description) in &rows {
            let fixable = if *fixable { "yes".green() } else { "no".dimmed() };
            println!("{:<32} {:<14} {:<8} {}", name, category, fixable, description);
        }
        println!("{}", "-".repeat(90));
        println!("{} rule(s) total", rows.len());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn lists_every_registry_code_with_a_description() {
        let rows = super::rows();
        assert_eq!(rows.len(), flint_lint::codes::REGISTRY.len());
        for (name, _, _, description) in rows {
            assert!(!description.is_empty(), "{name} has no description");
        }
    }
}
