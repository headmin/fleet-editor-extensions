//! Guards `codes::RuleMeta.fixable` against what the rules actually emit.
//!
//! `flint list-rules` prints this flag, and a `yes` that `--fix` never honours
//! is worse than a `no`: it sends the reader hunting for a fix that will never
//! be applied. The flag used to be a hand-maintained per-rule constant that
//! nothing checked, so it drifted.
//!
//! Fixability is per *finding*, not per rule: `hash-format` emits a real
//! replacement for uppercase hex but nothing for a wrong-length hash, and
//! `path-exists` can only offer a fix when the missing file is found elsewhere
//! in the repo. So a fixture that produces no fix proves nothing on its own —
//! only the two assertions below are sound.

use flint_lint::fix::{is_applicable, ApplyMode};
use flint_lint::{codes, Linter};
use std::path::Path;

/// Lint `source` and report, for `code`: did it fire, and did any of its
/// findings carry a fix `--fix` would actually apply?
///
/// `IncludeUnsafe`, because the flag claims "fixable at all", not "fixable
/// without `--unsafe-fixes`".
fn observe(source: &str, file: &str, code: &str) -> (bool, bool) {
    let linter = Linter::new();
    let report = linter
        .lint_content(source, Path::new(file))
        .unwrap_or_else(|e| panic!("linting {file} failed: {e:?}"));
    let mut fired = false;
    let mut fixable = false;
    for finding in report
        .errors
        .iter()
        .chain(&report.warnings)
        .chain(&report.infos)
    {
        if finding.rule_code == Some(code) {
            fired = true;
            if is_applicable(finding, ApplyMode::IncludeUnsafe) {
                fixable = true;
            }
        }
    }
    (fired, fixable)
}

/// Snippets that each trigger the named code. `Linter::new()` is config- and
/// snapshot-free, so version-gated codes (`deprecated-keys`) and cross-file
/// codes cannot be reached from here.
fn cases() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
        (
            codes::YAML_INDENTATION,
            "fleets/t.yml",
            "name: \"T\"\ncontrols:\n   macos_updates:\n      minimum_version: \"15.6\"\n",
        ),
        (
            codes::YAML_TRAILING_WHITESPACE,
            "fleets/t.yml",
            "name: \"T\" \n",
        ),
        (
            codes::YAML_EMPTY_VALUES,
            "fleets/t.yml",
            "name: \"T\"\nagent_options:\n",
        ),
        (
            codes::STRUCTURAL_VALIDATION,
            "fleets/t.yml",
            "name: \"T\"\norg_settings:\n  webhook_settings: {}\n",
        ),
        (
            codes::DATE_FORMAT,
            "fleets/t.yml",
            "name: \"T\"\ncontrols:\n  macos_updates:\n    deadline: \"10/08/2026\"\n    minimum_version: \"15.6\"\n",
        ),
        (
            codes::CATEGORIES,
            "fleets/t.yml",
            "name: \"T\"\nsoftware:\n  packages:\n    - path: ./x.yml\n      categories:\n        - \"productivity\"\n",
        ),
    ]
}

/// The sound direction: if a rule hands `--fix` something to apply, the flag
/// must not claim otherwise. Catches a rule that gains a fix without its
/// registry entry being updated.
#[test]
fn emitting_a_fix_implies_the_code_is_marked_fixable() {
    for (code, file, source) in cases() {
        let (fired, emits_fix) = observe(source, file, code);
        assert!(
            fired,
            "fixture for `{code}` no longer triggers it — update the snippet, \
             otherwise this code silently drops out of the guarantee"
        );
        if emits_fix {
            let declared = codes::meta(code)
                .unwrap_or_else(|| panic!("`{code}` is missing from codes::REGISTRY"))
                .fixable;
            assert!(
                declared,
                "`{code}` emits a fix that `--fix` applies, but codes::REGISTRY marks it \
                 fixable=false. Update the registry entry in codes.rs."
            );
        }
    }
}

/// Codes with no fix-emitting branch anywhere: their only remedy is
/// `with_suggestion`, which mints a display-only template the applier always
/// skips. Verified by reading the rule source, not inferred from the fixture —
/// `semantic.rs` contains exactly one `with_fix` call (the uppercase-hex
/// lowering behind `hash-format`), so nothing else it emits can be applied.
const NEVER_FIXABLE: &[&str] = &[
    // Only remedy is an example date; "10/08/2026" is genuinely ambiguous
    // between D/M and M/D, so there is nothing safe to substitute.
    codes::DATE_FORMAT,
    // Only remedy is naming the closest default category, offered as text.
    codes::CATEGORIES,
];

#[test]
fn never_fixable_codes_are_marked_false() {
    for code in NEVER_FIXABLE {
        let declared = codes::meta(code)
            .unwrap_or_else(|| panic!("`{code}` is missing from codes::REGISTRY"))
            .fixable;
        assert!(
            !declared,
            "`{code}` has no fix-emitting branch, but codes::REGISTRY marks it fixable=true — \
             `flint list-rules` would advertise a fix that `--fix` never applies. \
             If the rule gained a real fix, move it out of NEVER_FIXABLE."
        );

        // …and prove the claim still holds against a live run where we have a
        // fixture for it.
        if let Some((_, file, source)) = cases().into_iter().find(|(c, _, _)| c == code) {
            let (fired, emits_fix) = observe(source, file, code);
            assert!(fired, "fixture for `{code}` no longer triggers it");
            assert!(
                !emits_fix,
                "`{code}` is listed in NEVER_FIXABLE but emitted an applicable fix — \
                 remove it from that list and set fixable=true in codes.rs."
            );
        }
    }
}

#[test]
fn every_case_names_a_registered_code() {
    for (code, _, _) in cases() {
        assert!(
            codes::meta(code).is_some(),
            "`{code}` is not in codes::REGISTRY"
        );
    }
}
