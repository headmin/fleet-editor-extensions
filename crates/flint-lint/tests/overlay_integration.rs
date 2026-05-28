//! Integration tests for overlay merge against realistic Fleet GitOps content.
//!
//! Fixtures in `tests/fixtures/overlay/` model the structure of fleet-plan's
//! `testdata/default.yml` with hand-crafted environment overlays added.
//!
//! Scope split:
//! - Unit tests in `src/overlay.rs` exhaustively cover algorithm branches
//!   (scalar conflict, nested map merge, list replacement, error paths) on
//!   minimal synthetic inputs.
//! - This file covers the merge *contract* against realistic documents —
//!   that base-only keys survive, overlay-only keys appear, conflicting
//!   keys resolve correctly, `path:` refs are preserved, and applying
//!   different overlays to the same base yields different results.
//!
//! No external fixtures are required; everything is checked in.

use flint_lint::merge_yaml;
use std::fs;
use std::path::PathBuf;

/// Load a fixture YAML from `tests/fixtures/overlay/<name>`.
///
/// Uses `CARGO_MANIFEST_DIR` so the test passes regardless of where
/// `cargo test` is invoked from (workspace root vs. crate dir).
fn fixture(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("overlay")
        .join(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("reading {}: {}", path.display(), e))
}

/// Parse a merged YAML string back to a `serde_yaml::Value` so tests
/// can index into the structure rather than string-matching the output.
/// String comparison would be brittle against trivial formatting drift
/// (key ordering, quoting style) that doesn't change semantics.
fn parse(s: &str) -> serde_yaml::Value {
    serde_yaml::from_str(s).expect("merged output must be valid YAML")
}

#[test]
fn merge_base_with_prod_resolves_all_three_conflict_types() {
    let base = fixture("base.yml");
    let prod = fixture("prod.yml");
    let merged = merge_yaml(&base, &prod).expect("merge");
    let v = parse(&merged);

    // Type 1: overlay wins on direct scalar conflict.
    assert_eq!(
        v["org_settings"]["server_settings"]["server_url"].as_str(),
        Some("https://fleet.prod.example.com"),
        "overlay must override base server_url"
    );
    assert_eq!(
        v["org_settings"]["server_settings"]["enable_analytics"].as_bool(),
        Some(false),
        "overlay must override base enable_analytics"
    );

    // Type 2: base-only nested keys survive when overlay doesn't touch them.
    // org_info isn't mentioned in prod.yml — if naive merge replaced
    // org_settings wholesale, org_name would disappear.
    assert_eq!(
        v["org_settings"]["org_info"]["org_name"].as_str(),
        Some("Acme Corp"),
        "base nested key must survive when overlay omits it"
    );
    assert_eq!(
        v["org_settings"]["smtp_settings"]["enable_smtp"].as_bool(),
        Some(false),
        "base sibling key must survive when overlay touches a different sibling"
    );

    // Type 3: overlay-only nested keys are added to the merged result.
    // macos_updates only exists in prod.yml.
    assert!(
        v["controls"]["macos_updates"]["minimum_version"].is_string(),
        "overlay-only nested key must appear in merge"
    );
    assert_eq!(
        v["controls"]["macos_updates"]["minimum_version"].as_str(),
        Some("14.0.0")
    );
}

#[test]
fn nested_agent_options_merges_at_depth_three() {
    // agent_options.config.options is three levels deep and is the most
    // common merge collision in real Fleet GitOps repos. Verifies that
    // recursion doesn't bail out early on deeply nested mappings.
    let merged = merge_yaml(&fixture("base.yml"), &fixture("prod.yml")).expect("merge");
    let v = parse(&merged);
    let options = &v["agent_options"]["config"]["options"];

    assert_eq!(
        options["distributed_interval"].as_i64(),
        Some(30),
        "overlay wins at depth 3"
    );
    assert_eq!(
        options["logger_tls_period"].as_i64(),
        Some(30),
        "overlay wins on second conflicting key at depth 3"
    );
    assert_eq!(
        options["schedule_splay_percent"].as_i64(),
        Some(10),
        "base-only sibling at depth 3 must survive"
    );
    assert_eq!(
        options["pack_delimiter"].as_str(),
        Some("/"),
        "base-only sibling at depth 3 (different type) must survive"
    );
}

#[test]
fn path_references_in_base_survive_overlay_merge() {
    // The whole point of overlay merge for a linter is that the merged
    // result is what gets linted — including its `path:` refs to external
    // policy/query/label files. If those refs vanish, downstream lint
    // rules can't find their targets.
    let merged = merge_yaml(&fixture("base.yml"), &fixture("prod.yml")).expect("merge");
    let v = parse(&merged);

    let policies = v["policies"]
        .as_sequence()
        .expect("policies must remain a sequence after merge");
    assert_eq!(policies.len(), 2, "both base policy refs must survive");
    assert!(
        policies
            .iter()
            .any(|p| p["path"].as_str() == Some("policies/shared/global-policy.yml")),
        "global-policy path ref must survive"
    );
    assert!(
        policies
            .iter()
            .any(|p| p["path"].as_str() == Some("policies/mac/macos-filevault-enabled.yml")),
        "macos-filevault path ref must survive"
    );

    let labels = v["labels"].as_sequence().expect("labels must be a sequence");
    assert_eq!(labels.len(), 1);
    assert_eq!(
        labels[0]["path"].as_str(),
        Some("labels/shared/all-labels.yml")
    );
}

#[test]
fn different_overlays_against_same_base_produce_different_results() {
    // Same base, two environments. The two merged outputs should differ
    // exactly where the overlays differ — proves the overlay is actually
    // being consulted and not silently ignored.
    let base = fixture("base.yml");
    let v_prod = parse(&merge_yaml(&base, &fixture("prod.yml")).unwrap());
    let v_staging = parse(&merge_yaml(&base, &fixture("staging.yml")).unwrap());

    // Server URLs diverge per environment.
    assert_eq!(
        v_prod["org_settings"]["server_settings"]["server_url"].as_str(),
        Some("https://fleet.prod.example.com")
    );
    assert_eq!(
        v_staging["org_settings"]["server_settings"]["server_url"].as_str(),
        Some("https://fleet.staging.example.com")
    );

    // Disk encryption is on in prod, off in staging.
    assert_eq!(
        v_prod["controls"]["enable_disk_encryption"].as_bool(),
        Some(true)
    );
    assert_eq!(
        v_staging["controls"]["enable_disk_encryption"].as_bool(),
        Some(false)
    );

    // distributed_interval: 30 in prod, 5 in staging (overlay-specific
    // overrides of the same base value of 10).
    assert_eq!(
        v_prod["agent_options"]["config"]["options"]["distributed_interval"].as_i64(),
        Some(30)
    );
    assert_eq!(
        v_staging["agent_options"]["config"]["options"]["distributed_interval"].as_i64(),
        Some(5)
    );
}

#[test]
fn merged_result_has_expected_top_level_shape() {
    // Sanity check on the overall structure — a regression in deep_merge
    // that duplicated or dropped top-level keys would show up here loudly
    // even if individual key tests still passed.
    let merged = merge_yaml(&fixture("base.yml"), &fixture("prod.yml")).expect("merge");
    let v = parse(&merged);
    let map = v.as_mapping().expect("merge result must be a mapping");

    let mut keys: Vec<&str> = map.keys().filter_map(|k| k.as_str()).collect();
    keys.sort();

    // base contributes: org_settings, agent_options, controls, policies, queries, labels.
    // prod adds no new top-level keys (only deep modifications).
    let expected: Vec<&str> = vec![
        "agent_options",
        "controls",
        "labels",
        "org_settings",
        "policies",
        "queries",
    ];
    assert_eq!(
        keys, expected,
        "unexpected top-level keys after merge: got {keys:?}, want {expected:?}"
    );
}

#[test]
fn staging_overlay_does_not_introduce_macos_updates() {
    // Negative test for cross-overlay leakage: staging.yml has no
    // controls.macos_updates section. The merged staging result must
    // not contain it (otherwise we'd be accidentally leaking prod-only
    // overlay content into staging — would mean the merge function is
    // stateful, which it must not be).
    let merged = merge_yaml(&fixture("base.yml"), &fixture("staging.yml")).expect("merge");
    let v = parse(&merged);
    assert!(
        v["controls"]["macos_updates"].is_null(),
        "staging merge must not contain prod-only macos_updates section"
    );
}

#[test]
fn merge_round_trip_produces_valid_yaml() {
    // Belt-and-suspenders: the merged output is itself a valid YAML
    // document that can be re-parsed and re-merged. Catches accidental
    // emission of malformed YAML (e.g. unquoted special chars, broken
    // indentation) by the serializer.
    let merged_once = merge_yaml(&fixture("base.yml"), &fixture("prod.yml")).expect("first merge");
    let merged_twice = merge_yaml(&merged_once, "").expect("re-merge with empty overlay");

    // Re-merging with empty overlay should yield the same document.
    let v1 = parse(&merged_once);
    let v2 = parse(&merged_twice);
    assert_eq!(v1, v2, "merge with empty overlay must be a no-op");
}
