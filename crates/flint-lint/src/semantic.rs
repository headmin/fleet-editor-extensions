//! Semantic validation rules for Fleet GitOps YAML.
//!
//! These rules validate domain-specific constraints that go beyond structural
//! schema validation — mutual exclusivity, format rules, file extensions, etc.

use std::path::Path;

use super::engine::{detect_file_type, FileType};
use super::error::LintError;
use super::fleet_config::FleetConfig;
use super::rules::Rule;
use super::yaml_utils::*;

// ============================================================================
// Rule 1: Label Targeting Mutual Exclusivity
// ============================================================================

/// Validates label-targeting combinations against Fleet's PER-CONTEXT
/// contracts (verified in Fleet source, main @ 3c8df41762):
///
/// - Software items (`pkg/spec/gitops.go`): only ONE of
///   `labels_include_all` / `labels_include_any` / `labels_exclude_any`.
/// - Profiles & scripts (`fleet/mdm.go` ValidateMDMProfileSpecs): the two
///   include forms are mutually exclusive; `labels_exclude_any` MAY
///   coexist with either (289 live combos in the reference repo).
/// - Policies (`fleet/policies.go` verifyPolicyLabelScopes): include pair
///   exclusive, exclude pair (`labels_exclude_any`/`labels_exclude_all`)
///   exclusive, and the SAME label may not appear in both an include and
///   an exclude list (`fleet.LabelOverlap`, policies.go:230).
///
/// The docs sentence "only one of these fields can be set" describes the
/// software context only — a blanket rule false-positived on hundreds of
/// legal profile combos.
///
/// Presence is measured by VALUE, not by key: every Fleet check counts
/// `len(slice) > 0` (policies.go:207, gitops.go:2334), and policies.go:203
/// spells it out — `{labels_include_any: [], labels_include_all: [A]}` is
/// valid. Keying off mere presence would flag that legal shape.
pub struct LabelTargetingRule;

impl Rule for LabelTargetingRule {
    fn name(&self) -> &'static str {
        "label-targeting"
    }
    fn description(&self) -> &'static str {
        "Checks that only one labels_* targeting key is set per item"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        // Software items: Fleet rejects ANY two of the three (gitops.go).
        let strict_paths: &[&[&str]] = &[
            &["software", "packages"],
            &["software", "app_store_apps"],
            &["software", "fleet_maintained_apps"],
        ];
        // Everything else: only the include pair is mutually exclusive;
        // exclude coexists (mdm.go, policies.go).
        let include_pair_paths: &[&[&str]] = &[
            &["policies"],
            &["queries"],
            &["reports"],
            &["controls", "scripts"],
            &["controls", "apple_settings", "configuration_profiles"],
            &["controls", "apple_settings", "custom_settings"],
            &["controls", "macos_settings", "configuration_profiles"],
            &["controls", "macos_settings", "custom_settings"],
            &["controls", "windows_settings", "configuration_profiles"],
            &["controls", "windows_settings", "custom_settings"],
            &["controls", "android_settings", "configuration_profiles"],
            &["controls", "android_settings", "custom_settings"],
        ];

        // Mirrors Fleet's `len(slice) > 0`: an empty list is "no value".
        let has_labels = |item: &serde_yaml::Value, key: &str| {
            !mapping_get_string_array(item, key).is_empty()
        };

        let flag_pair = |errors: &mut Vec<LintError>, item, a: &str, b: &str, why: &str| {
            if has_labels(item, a) && has_labels(item, b) {
                errors.push(
                    LintError::error(
                        format!(
                            "'{}' sets both {} and {} — {}",
                            item_display_name(item),
                            a,
                            b,
                            why
                        ),
                        file,
                    )
                    .with_help(
                        "labels_include_any = hosts with ANY listed label; \
                         labels_include_all = hosts with ALL; labels_exclude_* = \
                         hosts WITHOUT"
                            .to_string(),
                    ),
                );
            }
        };

        for path in strict_paths {
            for item in collect_items_at_path(&yaml, path) {
                let present: Vec<&str> = [
                    "labels_include_any",
                    "labels_include_all",
                    "labels_exclude_any",
                ]
                .iter()
                .copied()
                .filter(|k| has_labels(item, k))
                .collect();
                if present.len() > 1 {
                    errors.push(
                        LintError::error(
                            format!(
                                "'{}' sets {} — software items allow only ONE labels_* key",
                                item_display_name(item),
                                present.join(" and "),
                            ),
                            file,
                        )
                        .with_help(
                            "Fleet rejects this at apply time: pick one of \
                             labels_include_any, labels_include_all, or labels_exclude_any"
                                .to_string(),
                        ),
                    );
                }
            }
        }

        for path in include_pair_paths {
            for item in collect_items_at_path(&yaml, path) {
                flag_pair(
                    &mut errors,
                    item,
                    "labels_include_any",
                    "labels_include_all",
                    "only one include form is allowed",
                );
                // Policies additionally have an exclude pair (policies.go).
                if *path == ["policies"] {
                    flag_pair(
                        &mut errors,
                        item,
                        "labels_exclude_any",
                        "labels_exclude_all",
                        "only one exclude form is allowed",
                    );

                    // Value-level check: policies are the only context where
                    // an include form and an exclude form may be combined, so
                    // they are the only context where one label can land on
                    // both sides. Fleet rejects that at apply time
                    // (policies.go:230 → LabelOverlap). Matching is exact —
                    // Fleet compares raw names, no trimming or case folding.
                    let include: Vec<&str> = ["labels_include_any", "labels_include_all"]
                        .iter()
                        .flat_map(|k| mapping_get_string_array(item, k))
                        .collect();
                    let exclude: Vec<&str> = ["labels_exclude_any", "labels_exclude_all"]
                        .iter()
                        .flat_map(|k| mapping_get_string_array(item, k))
                        .collect();

                    // Report every overlap, in include-list order, so the
                    // message is deterministic and fixing is one pass. Fleet
                    // only names the first — it stops at the first error.
                    let mut overlaps: Vec<&str> = Vec::new();
                    for name in &include {
                        if exclude.contains(name) && !overlaps.contains(name) {
                            overlaps.push(name);
                        }
                    }

                    if !overlaps.is_empty() {
                        let quoted: Vec<String> =
                            overlaps.iter().map(|n| format!("'{}'", n)).collect();
                        errors.push(
                            LintError::error(
                                format!(
                                    "'{}' lists {} in both an include and an exclude list",
                                    item_display_name(item),
                                    quoted.join(", "),
                                ),
                                file,
                            )
                            .with_help(
                                "Fleet rejects this at apply time: a label cannot both \
                                 select and deselect the same hosts — drop it from one \
                                 side"
                                    .to_string(),
                            ),
                        );
                    }
                }
            }
        }

        errors
    }
}

// ============================================================================
// Rule 2: Label Membership Type Consistency
// ============================================================================

/// Validates label membership type constraints:
/// - `dynamic`: requires `query`, forbids `hosts`/`criteria`
/// - `manual`: requires `hosts`, forbids `query`/`criteria`
/// - `host_vitals`: requires `criteria`, forbids `query`/`hosts`
pub struct LabelMembershipRule;

impl Rule for LabelMembershipRule {
    fn name(&self) -> &'static str {
        "label-membership"
    }
    fn description(&self) -> &'static str {
        "Checks label membership type consistency (dynamic→query, manual→hosts, host_vitals→criteria)"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        // Standalone label files (e.g. ./labels/my-label.yml) are top-level
        // sequences; fleet/team configs wrap labels under a `labels:` key.
        // Gate the root-sequence walk on file type so we don't misread a
        // top-level sequence in policies/queries files as labels.
        let is_label_file = matches!(detect_file_type(file), FileType::Labels);
        let items: Vec<&serde_yaml::Value> = match (&yaml, is_label_file) {
            (serde_yaml::Value::Sequence(seq), true) => seq.iter().collect(),
            _ => collect_items_at_path(&yaml, &["labels"]),
        };

        for item in items {
            // Skip path/glob references
            if (mapping_has_key(item, "path") || mapping_has_key(item, "paths"))
                && !mapping_has_key(item, "name")
            {
                continue;
            }

            let name = item_display_name(item);
            let membership_type_raw = mapping_get_str(item, "label_membership_type");
            let has_membership_key = mapping_has_key(item, "label_membership_type");

            let has_query = mapping_has_key(item, "query");
            let has_hosts = mapping_has_key(item, "hosts");
            let has_criteria = mapping_has_key(item, "criteria");

            // `label_membership_type:` with no value is parsed as null by YAML.
            // Silently defaulting to "dynamic" hides a clear user error — flag it
            // directly and suggest a type based on which membership field is set.
            if has_membership_key && membership_type_raw.is_none() {
                let suggestion = if has_criteria {
                    "host_vitals"
                } else if has_hosts {
                    "manual"
                } else {
                    "dynamic"
                };
                errors.push(
                    LintError::error(
                        format!("Label '{}' has empty 'label_membership_type'", name),
                        file,
                    )
                    .with_help(
                        "Provide a value: 'dynamic' (query), 'manual' (hosts), or 'host_vitals' (criteria)",
                    )
                    .with_suggestion(format!("label_membership_type: {}", suggestion)),
                );
                continue;
            }

            // Fleet server defaults to "dynamic" when the key is absent entirely.
            let membership_type = membership_type_raw.unwrap_or("dynamic");

            match membership_type {
                "dynamic" => {
                    if !has_query {
                        errors.push(
                            LintError::error(
                                format!("Label '{}' is dynamic but missing 'query' field", name),
                                file,
                            )
                            .with_help("Dynamic labels require a SQL query to determine membership")
                            .with_suggestion("query: \"SELECT 1 FROM ...;\""),
                        );
                    }
                    if has_hosts {
                        errors.push(
                            LintError::error(
                                format!("Label '{}' is dynamic but has 'hosts' field", name),
                                file,
                            )
                            .with_help("Dynamic labels use 'query' for membership, not 'hosts'. Use label_membership_type: manual for host lists"),
                        );
                    }
                    if has_criteria {
                        errors.push(
                            LintError::error(
                                format!("Label '{}' is dynamic but has 'criteria' field", name),
                                file,
                            )
                            .with_help("Dynamic labels use 'query' for membership, not 'criteria'. Use label_membership_type: host_vitals for vital-based criteria"),
                        );
                    }
                }
                "manual" => {
                    if has_query {
                        errors.push(
                            LintError::error(
                                format!("Label '{}' is manual but has 'query' field", name),
                                file,
                            )
                            .with_help("Manual labels use 'hosts' for membership, not 'query'. Use label_membership_type: dynamic for SQL queries"),
                        );
                    }
                    if has_criteria {
                        errors.push(
                            LintError::error(
                                format!("Label '{}' is manual but has 'criteria' field", name),
                                file,
                            )
                            .with_help("Manual labels use 'hosts' for membership, not 'criteria'. Use label_membership_type: host_vitals for vital-based criteria"),
                        );
                    }
                }
                "host_vitals" => {
                    if !has_criteria {
                        errors.push(
                            LintError::error(
                                format!("Label '{}' is host_vitals but missing 'criteria' field", name),
                                file,
                            )
                            .with_help("host_vitals labels require 'criteria' with 'vital' and 'value' fields"),
                        );
                    }
                    if has_query {
                        errors.push(
                            LintError::error(
                                format!("Label '{}' is host_vitals but has 'query' field", name),
                                file,
                            )
                            .with_help("host_vitals labels use 'criteria', not 'query'"),
                        );
                    }
                    if has_hosts {
                        errors.push(
                            LintError::error(
                                format!("Label '{}' is host_vitals but has 'hosts' field", name),
                                file,
                            )
                            .with_help("host_vitals labels use 'criteria', not 'hosts'. Use label_membership_type: manual for explicit host lists"),
                        );
                    }
                    if has_criteria {
                        if let serde_yaml::Value::Mapping(map) = item {
                            if let Some(criteria) =
                                map.get(serde_yaml::Value::String("criteria".to_string()))
                            {
                                validate_criteria(criteria, file, &name, &mut errors);
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        errors
    }
}

/// Vitals that Fleet's `parseHostVitalCriteria()` currently registers.
/// Anything else fails server-side with `unknown vital <name>`.
/// Keep this in sync with the `hostVitals` map in Fleet's Go source.
const KNOWN_HOST_VITALS: &[&str] = &["end_user_idp_group", "end_user_idp_department"];

/// Validate a host_vital_criteria node.
///
/// Tracks Fleet's `parseHostVitalCriteria()` behavior:
/// - Requires a leaf `{vital, value, operator?}` with both `vital` and `value`.
/// - Rejects `and`/`or` composites outright (not supported yet).
/// - `vital` must be one of the registered vitals in `KNOWN_HOST_VITALS`.
fn validate_criteria(
    node: &serde_yaml::Value,
    file: &Path,
    label_name: &str,
    errors: &mut Vec<LintError>,
) {
    let map = match node {
        serde_yaml::Value::Mapping(m) => m,
        _ => {
            errors.push(
                LintError::error(
                    format!("Label '{}' criteria must be a mapping", label_name),
                    file,
                )
                .with_help("Use {vital, value} with an optional operator"),
            );
            return;
        }
    };

    let has = |k: &str| map.contains_key(serde_yaml::Value::String(k.to_string()));
    let get = |k: &str| map.get(serde_yaml::Value::String(k.to_string()));

    // Fleet's current parser rejects and/or entirely. Flag them with Fleet's
    // exact error message so users see the same diagnostic client-side as
    // they would server-side.
    let mut flagged_composite = false;
    for key in ["and", "or"] {
        if has(key) {
            flagged_composite = true;
            errors.push(
                LintError::error(
                    format!(
                        "Label '{}' uses '{}' criteria — And/Or criteria not supported in host vitals labels yet",
                        label_name, key
                    ),
                    file,
                )
                .with_help("Fleet's parseHostVitalCriteria currently accepts only a single {vital, value} leaf. Remove 'and'/'or' until support lands."),
            );
        }
    }

    // If and/or was flagged, skip downstream leaf checks — they'd produce
    // misleading cascades ("empty criteria", "missing vital") for input
    // that is already unambiguously rejected.
    if flagged_composite {
        return;
    }

    let has_vital = has("vital");
    let has_value = has("value");

    if !has_vital && !has_value {
        errors.push(
            LintError::error(
                format!("Label '{}' has an empty criteria node", label_name),
                file,
            )
            .with_help("Provide {vital, value}"),
        );
        return;
    }

    if !has_vital {
        errors.push(
            LintError::error(
                format!("Label '{}' criteria missing 'vital' field", label_name),
                file,
            )
            .with_help("Leaf criteria require both 'vital' and 'value'"),
        );
    }
    if !has_value {
        errors.push(
            LintError::error(
                format!("Label '{}' criteria missing 'value' field", label_name),
                file,
            )
            .with_help("Leaf criteria require both 'vital' and 'value'"),
        );
    }

    if let Some(serde_yaml::Value::String(vital_name)) = get("vital") {
        if !KNOWN_HOST_VITALS.contains(&vital_name.as_str()) {
            errors.push(
                LintError::error(
                    format!(
                        "Label '{}' uses unknown vital '{}'",
                        label_name, vital_name
                    ),
                    file,
                )
                .with_help(format!(
                    "Fleet's parseHostVitalCriteria currently registers: {}. Anything else fails with 'unknown vital'.",
                    KNOWN_HOST_VITALS.join(", ")
                )),
            );
        }
    }
}

// ============================================================================
// Rule 3: Date Format Validation
// ============================================================================

/// Validates that `deadline` fields match YYYY-MM-DD format.
pub struct DateFormatRule;

impl Rule for DateFormatRule {
    fn name(&self) -> &'static str {
        "date-format"
    }
    fn description(&self) -> &'static str {
        "Checks that deadline fields use YYYY-MM-DD format"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }
    fn is_fixable(&self) -> bool {
        true
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        let update_paths: &[&[&str]] = &[
            &["controls", "macos_updates"],
            &["controls", "ios_updates"],
            &["controls", "ipados_updates"],
        ];

        for path in update_paths {
            // Walk to the updates mapping
            let mut current = &yaml;
            let mut found = true;
            for &key in *path {
                match current {
                    serde_yaml::Value::Mapping(map) => {
                        match map.get(serde_yaml::Value::String(key.to_string())) {
                            Some(v) => current = v,
                            None => {
                                found = false;
                                break;
                            }
                        }
                    }
                    _ => {
                        found = false;
                        break;
                    }
                }
            }

            if !found {
                continue;
            }

            if let Some(deadline) = mapping_get_str(current, "deadline") {
                if !is_valid_date(deadline) {
                    let section = path.last().unwrap_or(&"updates");
                    errors.push(
                        LintError::error(
                            format!(
                                "{}: deadline '{}' is not a valid YYYY-MM-DD date",
                                section, deadline
                            ),
                            file,
                        )
                        .with_help("Deadline must be in YYYY-MM-DD format (e.g., 2025-06-15)")
                        .with_suggestion("deadline: \"2025-06-15\""),
                    );
                }
            }
        }

        errors
    }
}

/// Validate a date string matches YYYY-MM-DD and is a real date.
fn is_valid_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return false;
    }

    let year: u32 = match parts[0].parse() {
        Ok(y) if (2000..=2100).contains(&y) => y,
        _ => return false,
    };
    let month: u32 = match parts[1].parse() {
        Ok(m) if (1..=12).contains(&m) => m,
        _ => return false,
    };
    let day: u32 = match parts[2].parse() {
        Ok(d) if d >= 1 => d,
        _ => return false,
    };

    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
                29
            } else {
                28
            }
        }
        _ => return false,
    };

    day <= max_day
}

// ============================================================================
// Rule: Patch Policy Coupling
// ============================================================================

/// Validates that patch-policy fields are used consistently.
///
/// Per Fleet docs (yaml-files.md:141-149):
/// - A patch policy requires `type: patch` AND `fleet_maintained_app_slug`.
/// - `install_software: true` is only meaningful on a patch policy; on a
///   regular policy `install_software` must be a mapping (`package_path`
///   or `hash_sha256`).
pub struct PatchPolicyRule;

impl Rule for PatchPolicyRule {
    fn name(&self) -> &'static str {
        "patch-policy"
    }
    fn description(&self) -> &'static str {
        "Checks patch policy fields: type:patch requires fleet_maintained_app_slug; install_software:true requires type:patch"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        // Like LabelMembershipRule, cover both wrapped (fleets/teams files) and
        // standalone (lib/policies/*.yml) layouts. Gate the root-sequence walk
        // on file type so queries/labels files don't get misread as policies.
        let is_policy_file = matches!(detect_file_type(file), FileType::Policies);
        let items: Vec<&serde_yaml::Value> = match (&yaml, is_policy_file) {
            (serde_yaml::Value::Sequence(seq), true) => seq.iter().collect(),
            _ => collect_items_at_path(&yaml, &["policies"]),
        };

        for item in items {
            if (mapping_has_key(item, "path") || mapping_has_key(item, "paths"))
                && !mapping_has_key(item, "name")
            {
                continue;
            }

            let name = item_display_name(item);
            let policy_type = mapping_get_str(item, "type");
            let has_slug = mapping_has_key(item, "fleet_maintained_app_slug");

            // install_software value — boolean true vs mapping form
            let install_bool = item
                .as_mapping()
                .and_then(|m| m.get(serde_yaml::Value::String("install_software".to_string())))
                .and_then(|v| v.as_bool());

            if policy_type == Some("patch") && !has_slug {
                errors.push(
                    LintError::error(
                        format!(
                            "Patch policy '{}' is missing 'fleet_maintained_app_slug'",
                            name
                        ),
                        file,
                    )
                    .with_help("Patch policies track a Fleet-Maintained App — specify which one")
                    .with_suggestion("fleet_maintained_app_slug: <slug>"),
                );
            }

            // `fleet_maintained_app_slug` is only meaningful for patch policies.
            // Using it without `type: patch` likely means the user forgot the type.
            if has_slug && policy_type != Some("patch") {
                errors.push(
                    LintError::error(
                        format!(
                            "Policy '{}' has 'fleet_maintained_app_slug' but is not a patch policy",
                            name
                        ),
                        file,
                    )
                    .with_help("`fleet_maintained_app_slug` is only used on patch policies — add `type: patch`, or remove the slug")
                    .with_suggestion("type: patch"),
                );
            }

            // install_software: true makes sense only with type: patch.
            if install_bool == Some(true) && policy_type != Some("patch") {
                errors.push(
                    LintError::error(
                        format!(
                            "Policy '{}' uses 'install_software: true' but is not a patch policy",
                            name
                        ),
                        file,
                    )
                    .with_help("`install_software: true` installs the Fleet-Maintained App on failure — only valid with `type: patch`. For regular install-on-fail, use an install_software mapping with 'package_path' or 'hash_sha256'.")
                    .with_suggestion("type: patch"),
                );
            }
        }

        errors
    }
}

// ============================================================================
// Rule: Policy Automation Location
// ============================================================================

/// Flags policy automations (`run_script`, `install_software`, `calendar_events_enabled`)
/// when configured in `default.yml`.
///
/// Per Fleet docs (yaml-files.md:245):
/// > Currently, the `run_script` and `install_software` policy automations can
/// > only be configured for a fleet (`fleets/fleet-name.yml`) or "Unassigned"
/// > (`fleets/unassigned.yml`) … `calendar_events_enabled` can only be
/// > configured for policies on a fleet.
///
/// Policies in `default.yml` are global and don't belong to a fleet, so these
/// fields are a silent misconfiguration — Fleet server will ignore them.
pub struct PolicyAutomationLocationRule;

impl Rule for PolicyAutomationLocationRule {
    fn name(&self) -> &'static str {
        "policy-automation-location"
    }
    fn description(&self) -> &'static str {
        "Flags run_script / install_software / calendar_events_enabled on policies in default.yml (fleet-only per Fleet docs)"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        // Only applies to default.yml. Other file types (fleet files, lib
        // files, standalone) either allow these automations or can't be
        // reliably classified without cross-file analysis.
        let is_default_yml = file
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|name| name == "default.yml");
        if !is_default_yml {
            return Vec::new();
        }

        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        for item in collect_items_at_path(&yaml, &["policies"]) {
            // Skip path/glob references — the referenced file is linted separately.
            if (mapping_has_key(item, "path") || mapping_has_key(item, "paths"))
                && !mapping_has_key(item, "name")
            {
                continue;
            }

            let name = item_display_name(item);

            for field in ["run_script", "install_software", "calendar_events_enabled"] {
                if mapping_has_key(item, field) {
                    errors.push(
                        LintError::error(
                            format!(
                                "Policy '{}' sets '{}' in default.yml, but this automation is fleet-only",
                                name, field
                            ),
                            file,
                        )
                        .with_help(format!(
                            "Move the policy to a fleet file (fleets/<name>.yml or fleets/unassigned.yml), or remove '{}'. See https://fleetdm.com/docs/configuration/yaml-files#policies",
                            field
                        )),
                    );
                }
            }
        }

        errors
    }
}

// ============================================================================
// Rule 4: Hash Format Validation
// ============================================================================

/// Validates that `hash_sha256` values are 64 lowercase hex characters.
pub struct HashFormatRule;

impl Rule for HashFormatRule {
    fn name(&self) -> &'static str {
        "hash-format"
    }
    fn description(&self) -> &'static str {
        "Checks that hash_sha256 values are valid 64-character lowercase hex strings"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }
    fn is_fixable(&self) -> bool {
        true
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        // Check software.packages[].hash_sha256
        for item in collect_items_at_path(&yaml, &["software", "packages"]) {
            if let Some(hash) = mapping_get_str(item, "hash_sha256") {
                check_hash(hash, &item_display_name(item), file, &mut errors);
            }
        }

        // Check policies[].install_software.hash_sha256
        for item in collect_items_at_path(&yaml, &["policies"]) {
            if let Some(install) = item
                .as_mapping()
                .and_then(|m| m.get(serde_yaml::Value::String("install_software".to_string())))
            {
                if let Some(hash) = mapping_get_str(install, "hash_sha256") {
                    check_hash(hash, &item_display_name(item), file, &mut errors);
                }
            }
        }

        errors
    }
}

fn check_hash(hash: &str, item_name: &str, file: &Path, errors: &mut Vec<LintError>) {
    if hash.len() != 64 {
        errors.push(
            LintError::error(
                format!(
                    "'{}': hash_sha256 must be exactly 64 characters (got {})",
                    item_name,
                    hash.len()
                ),
                file,
            )
            .with_help("SHA256 hashes are 64 lowercase hexadecimal characters"),
        );
        return;
    }

    if !hash
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        if hash.chars().all(|c| c.is_ascii_hexdigit()) {
            // Uppercase hex — suggest lowercase
            errors.push(
                LintError::error(
                    format!("'{}': hash_sha256 must be lowercase hex", item_name),
                    file,
                )
                .with_fix(super::error::Fix::Replace {
                    old: Some(hash.to_string()),
                    new: hash.to_lowercase(),
                    safety: super::error::FixSafety::Safe,
                }),
            );
        } else {
            errors.push(
                LintError::error(
                    format!("'{}': hash_sha256 contains invalid characters", item_name),
                    file,
                )
                .with_help("SHA256 hashes must contain only characters 0-9 and a-f"),
            );
        }
    }
}

// ============================================================================
// Rule 5: Categories Validation
// ============================================================================

const VALID_CATEGORIES: &[&str] = &[
    "Browsers",
    "Communication",
    "Developer tools",
    "Productivity",
    "Security",
    "Utilities",
];

/// Validates `categories` values against Fleet's CURRENT contract: custom
/// category names are legal (fleet source: DefaultSelfServiceCategoryNames
/// plus user-created categories; unknown names are accepted, emoji
/// supported, ≤255 chars). Enforcing the six defaults as the only valid
/// set produced 487 false positives on a real repo using emoji-prefixed
/// custom categories. What still warns: empty names, >255 chars, and
/// case-variants of a default name (likely an unintended near-duplicate).
pub struct CategoriesRule;

impl Rule for CategoriesRule {
    fn name(&self) -> &'static str {
        "categories"
    }
    fn description(&self) -> &'static str {
        "Checks software category values (empty, over-long, or case-variant of a default)"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }
    fn is_fixable(&self) -> bool {
        true
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        let paths: &[&[&str]] = &[
            &["software", "packages"],
            &["software", "app_store_apps"],
            &["software", "fleet_maintained_apps"],
        ];

        for path in paths {
            for item in collect_items_at_path(&yaml, path) {
                let name = item_display_name(item);
                for cat in mapping_get_string_array(item, "categories") {
                    if cat.trim().is_empty() {
                        errors.push(LintError::warning(
                            format!("'{}': empty category name", name),
                            file,
                        ));
                        continue;
                    }
                    if cat.chars().count() > 255 {
                        errors.push(LintError::warning(
                            format!(
                                "'{}': category '{}…' exceeds Fleet's 255-character limit",
                                name,
                                cat.chars().take(30).collect::<String>()
                            ),
                            file,
                        ));
                        continue;
                    }
                    // Custom names are legal; only flag a CASE-variant of a
                    // default (e.g. 'browsers') — almost certainly meant the
                    // default and would create a near-duplicate category.
                    if !VALID_CATEGORIES.contains(&cat) {
                        if let Some(default) = VALID_CATEGORIES
                            .iter()
                            .find(|d| d.eq_ignore_ascii_case(cat))
                        {
                            errors.push(
                                LintError::warning(
                                    format!(
                                        "'{}': category '{}' is a case-variant of the default '{}'",
                                        name, cat, default
                                    ),
                                    file,
                                )
                                .with_help(
                                    "Fleet treats these as different categories — use the \
                                     default's casing unless a separate category is intended"
                                        .to_string(),
                                )
                                .with_suggestion(default.to_string()),
                            );
                        }
                    }
                }
            }
        }

        errors
    }
}
// ============================================================================
// Rule 6: File Extension Validation
// ============================================================================

/// Validates that profile/script paths have correct file extensions.
pub struct FileExtensionRule;

impl Rule for FileExtensionRule {
    fn name(&self) -> &'static str {
        "file-extension"
    }
    fn description(&self) -> &'static str {
        "Checks that MDM profile and script paths have valid file extensions"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        let checks: &[(&[&str], &[&str], &str)] = &[
            (
                &["controls", "macos_settings", "custom_settings"],
                &[".mobileconfig", ".json"],
                "macOS profiles",
            ),
            (
                &["controls", "windows_settings", "custom_settings"],
                &[".xml"],
                "Windows profiles",
            ),
            (
                &["controls", "android_settings", "custom_settings"],
                &[".json"],
                "Android profiles",
            ),
            (
                &["controls", "scripts"],
                &[".sh", ".ps1", ".zsh"],
                "scripts",
            ),
        ];

        for (path, valid_exts, context) in checks {
            for item in collect_items_at_path(&yaml, path) {
                if let Some(path_val) = mapping_get_str(item, "path") {
                    if !valid_exts.iter().any(|ext| path_val.ends_with(ext)) {
                        errors.push(
                            LintError::warning(
                                format!("{}: '{}' has unexpected extension", context, path_val),
                                file,
                            )
                            .with_help(format!(
                                "Expected extensions for {}: {}",
                                context,
                                valid_exts.join(", ")
                            )),
                        );
                    }
                }
            }
        }

        errors
    }
}

// ============================================================================
// Rule 7: Secret Hygiene
// ============================================================================

/// Checks that integration credential fields use environment variable references.
pub struct SecretHygieneRule;

impl Rule for SecretHygieneRule {
    fn name(&self) -> &'static str {
        "secret-hygiene"
    }
    fn description(&self) -> &'static str {
        "Checks that API tokens and secrets use environment variable references ($VAR)"
    }
    fn category(&self) -> &'static str {
        "security"
    }
    fn is_fixable(&self) -> bool {
        true
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        // integrations.jira[].api_token
        check_secret_field(
            &yaml,
            &["integrations", "jira"],
            "api_token",
            file,
            &mut errors,
        );
        check_secret_field(
            &yaml,
            &["org_settings", "integrations", "jira"],
            "api_token",
            file,
            &mut errors,
        );

        // integrations.zendesk[].api_token
        check_secret_field(
            &yaml,
            &["integrations", "zendesk"],
            "api_token",
            file,
            &mut errors,
        );
        check_secret_field(
            &yaml,
            &["org_settings", "integrations", "zendesk"],
            "api_token",
            file,
            &mut errors,
        );

        // integrations.google_calendar[].api_key_json
        check_secret_field(
            &yaml,
            &["integrations", "google_calendar"],
            "api_key_json",
            file,
            &mut errors,
        );
        check_secret_field(
            &yaml,
            &["org_settings", "integrations", "google_calendar"],
            "api_key_json",
            file,
            &mut errors,
        );

        errors
    }
}

// ============================================================================
// Rule 8: Path / Paths Reference Validation
// ============================================================================

/// Validates path/paths fields on entities (policies, reports, labels, scripts, etc.):
/// - `path` must NOT contain glob characters (`*?[{`)
/// - `paths` MUST contain glob characters
/// - Cannot have both `path` and `paths` on the same entry
/// - Scripts require `path` or `paths` (no inline allowed)
pub struct PathReferenceRule;

impl Rule for PathReferenceRule {
    fn name(&self) -> &'static str {
        "path-reference"
    }
    fn description(&self) -> &'static str {
        "Validates path/paths fields: glob usage, mutual exclusivity, and script requirements"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        // Check all sections that support path/paths references
        let sections = &["policies", "reports", "queries", "labels"];
        for section in sections {
            for item in collect_items_at_path(&yaml, &[section]) {
                check_path_fields(item, section, false, file, source, &mut errors);
            }
        }

        // Scripts require path or paths (no inline)
        for item in collect_items_at_path(&yaml, &["controls", "scripts"]) {
            check_path_fields(item, "script", true, file, source, &mut errors);
        }

        // Custom settings (profiles) also use path/paths
        for section in &["macos_settings", "windows_settings", "android_settings"] {
            let paths = &["controls", section, "custom_settings"];
            for item in collect_items_at_path(&yaml, paths) {
                check_path_fields(item, "profile", false, file, source, &mut errors);
            }
        }

        errors
    }
}

/// Returns true if the string contains glob metacharacters.
fn contains_glob_meta(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

fn check_path_fields(
    item: &serde_yaml::Value,
    entity_type: &str,
    require_file_ref: bool,
    file: &Path,
    source: &str,
    errors: &mut Vec<LintError>,
) {
    let has_path = mapping_get_str(item, "path").is_some();
    let has_paths = mapping_get_str(item, "paths").is_some();

    // Can't have both path and paths
    if has_path && has_paths {
        let name = item_display_name(item);
        let mut err = LintError::error(
            format!("{entity_type} '{name}' has both 'path' and 'paths' — use one or the other"),
            file,
        )
        .with_help("'path' is for a single file, 'paths' is for glob patterns");

        if let Some(line) = find_key_line(source, "paths", 0) {
            err = err.with_location(line, 0);
        }
        errors.push(err);
        return;
    }

    // path must NOT contain glob characters
    if let Some(path_val) = mapping_get_str(item, "path") {
        if contains_glob_meta(path_val) {
            let name = item_display_name(item);
            let mut err = LintError::error(
                format!("{entity_type} '{name}' 'path' contains glob characters — use 'paths' for glob patterns"),
                file,
            )
            .with_help(format!("Change 'path: {path_val}' to 'paths: {path_val}'"))
            .with_context(path_val.to_string())
            .with_suggestion(format!("paths: {path_val}"));

            if let Some(line) = find_key_line(source, "path", 0) {
                err = err.with_location(line, 0);
            }
            errors.push(err);
        }
    }

    // paths MUST contain glob characters
    if let Some(paths_val) = mapping_get_str(item, "paths") {
        if !contains_glob_meta(paths_val) {
            let name = item_display_name(item);
            let mut err = LintError::error(
                format!("{entity_type} '{name}' 'paths' does not contain glob characters — use 'path' for a specific file"),
                file,
            )
            .with_help(format!("Change 'paths: {paths_val}' to 'path: {paths_val}'"))
            .with_context(paths_val.to_string())
            .with_suggestion(format!("path: {paths_val}"));

            if let Some(line) = find_key_line(source, "paths", 0) {
                err = err.with_location(line, 0);
            }
            errors.push(err);
        }
    }

    // Scripts require path or paths (no inline)
    if require_file_ref && !has_path && !has_paths {
        let name = item_display_name(item);
        let mut err = LintError::error(
            format!("{entity_type} '{name}' has no 'path' or 'paths' field — scripts must reference a file"),
            file,
        ).with_help("Add 'path: ./path/to/script.sh' or 'paths: ./scripts/*.sh'");

        // Try to find the line of this item
        if let Some(name_str) = mapping_get_str(item, "name") {
            if let Some(line) = find_key_line(source, name_str, 0) {
                err = err.with_location(line, 0);
            }
        }
        errors.push(err);
    }
}

// ============================================================================
// Rule 11: Shebang Syntax
// ============================================================================

/// Warns when POSIX shell scripts referenced via `path:` lack a shebang.
///
/// macOS/Linux scripts deployed via Fleet MDM are invoked by the orbit
/// agent, which respects shebangs to pick the interpreter. A `.sh` file
/// without `#!/bin/sh` may be invoked with `sh` on Linux but `bash` on
/// macOS, producing different runtime behavior — a class of "works on
/// my machine" bug. Surfacing this at lint time avoids the surprise.
///
/// Scope: only checks files ending in `.sh`, `.zsh`, `.bash` — the
/// platforms where shebang is the convention. PowerShell (`.ps1`) and
/// batch scripts have their own header conventions and aren't checked.
pub struct ShebangSyntaxRule;

impl Rule for ShebangSyntaxRule {
    fn name(&self) -> &'static str {
        "shebang-syntax"
    }
    fn description(&self) -> &'static str {
        "Checks that referenced .sh/.zsh/.bash script files start with a shebang (#!)"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();
        let mut script_paths: Vec<String> = Vec::new();
        collect_path_refs_recursive(&yaml, &mut script_paths);

        // Resolve script paths relative to the YAML file's parent.
        // Fleet GitOps convention: `path:` refs are relative to the file
        // that contains them, not to the workspace root.
        let yaml_dir = file.parent().unwrap_or_else(|| Path::new("."));

        for rel_path in script_paths {
            if !is_posix_shell_script(&rel_path) {
                continue;
            }
            let abs_path = yaml_dir.join(&rel_path);
            // Missing file is a different rule's job (path-reference);
            // we only check shebangs in files that actually exist.
            let content = match std::fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(_) => continue,
            };
            let first_line = content.lines().next().unwrap_or("");
            if !first_line.starts_with("#!") {
                let mut err = LintError::warning(
                    format!("Script '{}' is missing a shebang (#!) on first line", rel_path),
                    file,
                )
                .with_rule_code(crate::codes::SHEBANG_SYNTAX)
                .with_help("Add `#!/bin/sh` (or `#!/usr/bin/env bash`) as the first line so the interpreter is unambiguous across platforms");
                // Try to point at the path: line where this script was referenced.
                if let Some(line) = find_key_line(source, "path", 0) {
                    err = err.with_location(line, 0);
                }
                errors.push(err);
            }
        }
        errors
    }
}

/// Recursively walk a YAML value and collect every `path:` string field.
///
/// Catches script references regardless of where they live in the
/// schema (controls.scripts, software.packages[].install_script.path,
/// platforms.*.scripts, etc.) without encoding each location.
fn collect_path_refs_recursive(v: &serde_yaml::Value, out: &mut Vec<String>) {
    match v {
        serde_yaml::Value::Mapping(map) => {
            for (k, val) in map {
                if k.as_str() == Some("path") {
                    if let Some(s) = val.as_str() {
                        out.push(s.to_string());
                    }
                }
                collect_path_refs_recursive(val, out);
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                collect_path_refs_recursive(item, out);
            }
        }
        _ => {}
    }
}

fn is_posix_shell_script(path: &str) -> bool {
    path.ends_with(".sh") || path.ends_with(".zsh") || path.ends_with(".bash")
}

// ============================================================================
// Rule 12: Webhook Endpoint Validity
// ============================================================================

/// Validates that webhook destination URLs and integration URLs are
/// well-formed HTTPS URLs.
///
/// Fleet sends webhooks server-to-server, so the receiver must be
/// reachable from Fleet's network. A misconfigured URL (missing
/// scheme, plain HTTP, spaces, empty host) silently fails when the
/// webhook fires — and webhook delivery isn't retried, so a single
/// typo can drop alerts forever. Catch it before the manifest applies.
///
/// Skips env-var refs (`$WEBHOOK_URL`) and 1Password refs (`op://...`)
/// since those resolve server-side and can't be validated locally.
pub struct WebhookEndpointRule;

impl Rule for WebhookEndpointRule {
    fn name(&self) -> &'static str {
        "webhook-endpoint-valid"
    }
    fn description(&self) -> &'static str {
        "Checks that webhook destination URLs and integration URLs are well-formed HTTPS URLs"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let mut errors = Vec::new();

        // Known webhook URL locations. Both top-level (legacy) and
        // org_settings-nested (current) paths are checked because real
        // repos have both, depending on how old the config is.
        let webhook_names = &[
            "host_status_webhook",
            "failing_policies_webhook",
            "vulnerabilities_webhook",
            "activities_webhook",
        ];
        for name in webhook_names {
            check_webhook_url_field(&yaml, &["webhook_settings", name], "destination_url", file, &mut errors);
            check_webhook_url_field(
                &yaml,
                &["org_settings", "webhook_settings", name],
                "destination_url",
                file,
                &mut errors,
            );
        }

        // Integration `url` fields. Same dual-location pattern.
        for integration in &["jira", "zendesk"] {
            check_webhook_url_field(&yaml, &["integrations", integration], "url", file, &mut errors);
            check_webhook_url_field(
                &yaml,
                &["org_settings", "integrations", integration],
                "url",
                file,
                &mut errors,
            );
        }

        errors
    }
}

fn check_webhook_url_field(
    yaml: &serde_yaml::Value,
    path: &[&str],
    field: &str,
    file: &Path,
    errors: &mut Vec<LintError>,
) {
    // Walk the path manually so we handle both terminal shapes:
    // - mapping terminal (`webhook_settings.host_status_webhook.destination_url`)
    // - sequence terminal (`integrations.jira[].url`)
    // `collect_items_at_path` only handles the sequence case, which is
    // why webhook checks need this custom walk.
    let mut current = yaml;
    for &key in path {
        match current.get(key) {
            Some(v) => current = v,
            None => return,
        }
    }

    let items: Vec<&serde_yaml::Value> = match current {
        serde_yaml::Value::Sequence(seq) => seq.iter().collect(),
        serde_yaml::Value::Mapping(_) => vec![current],
        _ => return,
    };

    for item in items {
        if let Some(url) = mapping_get_str(item, field) {
            if let Some(reason) = webhook_url_problem(url) {
                errors.push(
                    LintError::warning(
                        format!("'{}' is not a valid webhook URL ({}): {}", field, reason, url),
                        file,
                    )
                    .with_rule_code(crate::codes::WEBHOOK_ENDPOINT_VALID)
                    .with_help(
                        "Webhook URLs must use https:// and include a host (e.g. https://hooks.example.com/path)",
                    ),
                );
            }
        }
    }
}

/// Inspect a URL string and return a short description of what's wrong,
/// or `None` if the value is acceptable (including env-var/op:// refs
/// that can't be validated locally).
///
/// Conservative on purpose: rejects only clearly-broken values. A full
/// RFC-3986 parser would require pulling in the `url` crate; the simple
/// checks here catch the realistic typos without that dep weight.
fn webhook_url_problem(url: &str) -> Option<&'static str> {
    if url.is_empty() {
        return Some("empty");
    }
    // Env var / 1Password refs resolve server-side; not validatable here.
    if url.starts_with('$') || url.starts_with("op://") {
        return None;
    }
    if url.contains(char::is_whitespace) {
        return Some("contains whitespace");
    }
    if !url.starts_with("https://") {
        if url.starts_with("http://") {
            return Some("plain HTTP — use https://");
        }
        return Some("missing https:// scheme");
    }
    let after_scheme = &url["https://".len()..];
    let host = after_scheme.split('/').next().unwrap_or("");
    if host.is_empty() {
        return Some("missing host");
    }
    None
}

// ============================================================================
// Rule: Software Package URL validation (software-url)
// ============================================================================

/// Validates `software.packages[].url` (and a standalone software file's
/// top-level `url:`) is a well-formed https URL — mirroring the URL validation
/// `fleetctl gitops --dry-run` performs server-side (added in Fleet 4.66), so a
/// malformed URL is caught locally before it reaches the pipeline. Also flags
/// the unfilled `https://REPLACE-ME.example.com/…` placeholder that
/// `flint pkg --yml` emits, which parses as a URL but fails at apply.
pub struct SoftwareUrlRule;

impl Rule for SoftwareUrlRule {
    fn name(&self) -> &'static str {
        "software-url"
    }
    fn description(&self) -> &'static str {
        "Checks software package URLs are well-formed https URLs and not unfilled placeholders"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };
        let mut errors = Vec::new();
        for entry in software_package_entries(file, &yaml) {
            if let Some(url) = mapping_get_str(entry, "url") {
                check_software_url(url, source, file, &mut errors);
            }
        }
        errors
    }
}

/// Collect every software package entry in a file, across all three shapes:
///   - inline `software.packages[]` in a fleet/team/default config;
///   - a standalone software file that is a top-level **sequence** of entries
///     (`- hash_sha256: …` / `- url: …`, what `flint pkg` and `--yml` emit);
///   - a standalone software file that is a single top-level **mapping**.
///
/// A bare top-level sequence is ambiguous (it could be policies or labels), so
/// it is only treated as software when the path is a software file. A top-level
/// mapping is treated as a package only when it carries `hash_sha256` — a strong
/// software signal — so an unrelated top-level `url:` isn't misread.
fn software_package_entries<'a>(file: &Path, yaml: &'a serde_yaml::Value) -> Vec<&'a serde_yaml::Value> {
    use serde_yaml::Value;
    let mut entries: Vec<&Value> = collect_items_at_path(yaml, &["software", "packages"]);
    match yaml {
        Value::Sequence(seq) if matches!(detect_file_type(file), FileType::Software) => {
            entries.extend(seq.iter());
        }
        Value::Mapping(_) if mapping_has_key(yaml, "hash_sha256") => {
            entries.push(yaml);
        }
        _ => {}
    }
    entries
}

/// Validate one software `url:` value, pushing a finding when it is malformed
/// (fails dry-run) or still the `flint pkg --yml` placeholder (fails at apply).
/// Env-var / `op://` refs resolve server-side and are skipped.
fn check_software_url(url: &str, source: &str, file: &Path, errors: &mut Vec<LintError>) {
    if url.starts_with('$') || url.starts_with("op://") {
        return;
    }
    let (line, col) = find_url_value_line(source, url);

    // The pkg --yml scaffold ships a REPLACE-ME host: it parses as a valid URL
    // (so the malformed check below won't catch it) but will fail when Fleet
    // tries to download it. Surface it as its own actionable warning.
    if url.contains("REPLACE-ME") || url.contains("REPLACE_ME") {
        let mut err = LintError::warning(format!("software url is still a placeholder: {url}"), file)
            .with_rule_code(crate::codes::SOFTWARE_URL)
            .with_help("Replace the `flint pkg --yml` placeholder with the real installer URL Fleet should download from.");
        if let (Some(l), Some(c)) = (line, col) {
            err = err.with_location(l, c);
        }
        errors.push(err);
        return;
    }

    if let Some(reason) = webhook_url_problem(url) {
        let mut err = LintError::error(format!("software url is not a valid URL ({reason}): {url}"), file)
            .with_rule_code(crate::codes::SOFTWARE_URL)
            .with_help("Fleet downloads the package from this URL; it must be a well-formed https:// URL with a host.");
        if let (Some(l), Some(c)) = (line, col) {
            err = err.with_location(l, c);
        }
        errors.push(err);
    }
}

/// Find the 1-indexed (line, column) of a `url:` value in source. The column
/// points at the start of the value so an editor range covers the URL itself.
fn find_url_value_line(source: &str, url: &str) -> (Option<usize>, Option<usize>) {
    for (idx, line) in source.lines().enumerate() {
        if let Some(vpos) = line.find(url) {
            // Only treat it as a url: value when `url:` precedes it on the line
            // (avoids matching the same string inside a comment elsewhere).
            if line[..vpos].contains("url:") {
                return (Some(idx + 1), Some(vpos + 1));
            }
        }
    }
    (None, None)
}

// ============================================================================
// Rule: Software package installer source (software-source)
// ============================================================================

/// Flags a software package that carries `hash_sha256` but no `url`. Such an
/// entry has no installer Fleet can download — it can only be installed if a
/// package with that exact hash is *already cached* in Fleet. Against a fresh
/// or different server, `fleetctl gitops` fails with
/// `package not found with hash <…>`. This is the exact failure produced when
/// `flint pkg`'s minimal metadata block (`- hash_sha256: …`, no url) is used as
/// a standalone software file instead of `flint pkg --yml` (which scaffolds the
/// `url:`). A warning, not an error, since the cached-by-hash pattern is a
/// legitimate — if server-dependent — Fleet feature.
pub struct SoftwareSourceRule {
    /// Optional server snapshot. With a FRESH one carrying installer hashes,
    /// "is this package uploaded?" stops being a guess.
    pub snapshot: Option<std::sync::Arc<crate::snapshot::LoadedSnapshot>>,
    /// Values this repo declares as intentionally unresolved.
    pub placeholders: crate::config::PlaceholdersConfig,
    /// Paths some config file references, filled in per directory lint.
    pub referenced: crate::rules::ReferencedPaths,
}

impl Rule for SoftwareSourceRule {
    fn name(&self) -> &'static str {
        "software-source"
    }
    fn description(&self) -> &'static str {
        "Flags software packages with a hash but no url installer source (gitops fails 'package not found with hash' unless the package is already cached in Fleet)"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        // A software lib file that is empty or comment-only (e.g. just a
        // `# … version …` header, no `hash_sha256:`) has no package at all —
        // flag it even when linted directly, where no `path:` referrer exists
        // to trigger the reference-side `path-empty` check.
        if matches!(detect_file_type(file), FileType::Software)
            && is_effectively_empty(source, true)
        {
            return vec![LintError::error(
                "software file has no package definition (only a comment / empty)".to_string(),
                file,
            )
            .with_rule_code(crate::codes::SOFTWARE_SOURCE)
            .with_help(
                "A software file needs at least `hash_sha256:` (and a `url:`, or the package uploaded by that hash). Regenerate it with `flint pkg --yml`.",
            )];
        }

        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };
        let mut errors = Vec::new();
        for entry in software_package_entries(file, &yaml) {
            let hash = match mapping_get_str(entry, "hash_sha256") {
                Some(h) => h,
                None => continue, // no hash → app-store/FMA/path entry, not affected
            };
            let has_url = mapping_get_str(entry, "url").is_some_and(|u| !u.trim().is_empty());
            if has_url {
                continue;
            }

            // A value that is not a 64-hex digest is not a hash at all — it is
            // a SCAFFOLD MARKER, e.g.
            // `hash_sha256: PLACEHOLDER_REPLACE_WITH_ACTUAL_PKG_HASH`,
            // written deliberately by `flint gen` or by hand when a package is
            // planned but not yet built.
            //
            // That is a different state from "this package is not uploaded",
            // and must never gate: the author already knows it is incomplete —
            // that is what the marker SAYS — and these files are typically left
            // unreferenced until the real hash lands. Escalating it to an error
            // blocks commits on work the author has explicitly parked.
            // Fleet's own interpolation: nothing to check, so say nothing.
            if crate::config::is_fleet_variable(hash) {
                continue;
            }

            let is_hex_digest =
                hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
            if !is_hex_digest {
                let (line, col) = find_line_containing(source, hash);
                let declared = self.placeholders.is_placeholder(hash);
                let msg = if declared {
                    format!("'{hash}' is a declared placeholder — package not built yet")
                } else {
                    format!("'{hash}' is not a valid sha256 (64 hex chars)")
                };
                let mut err = LintError::warning(
                    msg,
                    file,
                )
                .with_rule_code(crate::codes::SOFTWARE_SOURCE)
                .with_help(
                    "Intentional scaffolding is fine: this stays a warning and never blocks a commit. Fill in the real hash (or add a `url:`) before referencing this file from a fleet — an unresolved hash only reaches Fleet once something points at it.",
                );
                if let (Some(l), Some(c)) = (line, col) {
                    err = err.with_location(l, c);
                }
                errors.push(err);
                continue;
            }

            // `hash_sha256:` with no `url:` is VALID when that exact installer
            // is already uploaded, and unresolvable otherwise. Without a
            // snapshot flint cannot tell those apart and warns on both — 55
            // such findings on the reference repo, at least one of which a
            // production CI log later proved to be a false alarm.
            //
            // A fresh snapshot settles it BOTH ways: present -> silent (the
            // upload-and-reference-by-hash workflow is working as intended),
            // absent -> this WILL fail the apply with "package not found with
            // hash", so it gates.
            // Escalation additionally requires the file to be WIRED. Fleet
            // reads a software file only via a `path:`/`paths:` reference, so
            // an unreferenced one with an unresolved hash cannot fail an
            // apply — reporting it as an error would assert a failure that
            // cannot occur. Unknown wiring (single-file lint) counts as not
            // referenced, so escalation stays conservative.
            let is_referenced = self
                .referenced
                .get()
                .is_some_and(|set| set.contains(&crate::util::normalize_path(file)));

            let authoritative = self
                .snapshot
                .as_deref()
                .filter(|_| is_referenced)
                .filter(|s| s.freshness.may_gate() && s.has_software());
            if let Some(snap) = authoritative {
                if snap.knows_hash(hash) {
                    continue;
                }
                let (line, col) = find_line_containing(source, hash);
                let mut err = LintError::error(
                    "software package hash is not uploaded to the Fleet server".to_string(),
                    file,
                )
                .with_rule_code(crate::codes::SOFTWARE_SOURCE)
                .with_help(
                    "No installer with this hash exists on the server, and there is no `url:` to download one, so `fleetctl gitops` fails with 'package not found with hash'. Add a `url:`, or upload the package.",
                );
                if let (Some(l), Some(c)) = (line, col) {
                    err = err.with_location(l, c);
                }
                errors.push(err);
                continue;
            }
            let (line, col) = find_line_containing(source, hash);
            let mut err = LintError::warning(
                "software package has hash_sha256 but no url installer source".to_string(),
                file,
            )
            .with_rule_code(crate::codes::SOFTWARE_SOURCE)
            .with_help(
                "Fleet has no installer to download for this package. Either add a `url:` (e.g. regenerate with `flint pkg --yml`), OR ensure a package with this exact hash is uploaded to the target Fleet server (the upload-and-reference-by-hash workflow). Otherwise `fleetctl gitops` fails with 'package not found with hash'.",
            );
            if let (Some(l), Some(c)) = (line, col) {
                err = err.with_location(l, c);
            }
            errors.push(err);
        }
        errors
    }
}

/// Find the 1-indexed (line, column) of the first line containing `needle`.
fn find_line_containing(source: &str, needle: &str) -> (Option<usize>, Option<usize>) {
    for (idx, line) in source.lines().enumerate() {
        if let Some(pos) = line.find(needle) {
            return (Some(idx + 1), Some(pos + 1));
        }
    }
    (None, None)
}

// ============================================================================
// Rule 13: Calendar-Event Integration Coercion
// ============================================================================

/// Warns when policies enable `calendar_events_enabled: true` but the
/// required `google_calendar` integration is not configured.
///
/// `calendar_events_enabled` is a per-policy opt-in; the actual calendar
/// integration lives in `org_settings.integrations.google_calendar[]`.
/// Forgetting the integration is silent — Fleet accepts the policy YAML
/// and only fails to schedule calendar events at runtime. This rule
/// surfaces the mismatch before deploy.
///
/// Scoped to `FleetConfig` files (default.yml, fleets/*, teams/*) where
/// both policies and the integrations block can co-exist. Team-level
/// policy files without integrations are normal — they inherit from
/// the global default.yml — and don't trigger this rule.
pub struct CalendarEventCoercionRule;

impl Rule for CalendarEventCoercionRule {
    fn name(&self) -> &'static str {
        "calendar-event-coercion"
    }
    fn description(&self) -> &'static str {
        "Checks that policies enabling calendar events have a configured google_calendar integration"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        if !matches!(detect_file_type(file), FileType::FleetConfig) {
            return Vec::new();
        }

        let yaml = match parse_yaml(source) {
            Some(v) => v,
            None => return Vec::new(),
        };

        let policies = collect_items_at_path(&yaml, &["policies"]);
        let mut offenders: Vec<String> = Vec::new();
        for policy in &policies {
            let enabled = policy
                .get("calendar_events_enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if enabled {
                let name = mapping_get_str(policy, "name").unwrap_or("(unnamed)");
                offenders.push(name.to_string());
            }
        }

        if offenders.is_empty() {
            return Vec::new();
        }

        // Calendar integration can live at either of two paths. We
        // consider it "configured" if the sequence exists and is non-empty.
        let calendar_configured = !collect_items_at_path(&yaml, &["integrations", "google_calendar"]).is_empty()
            || !collect_items_at_path(&yaml, &["org_settings", "integrations", "google_calendar"]).is_empty();

        if calendar_configured {
            return Vec::new();
        }

        offenders
            .into_iter()
            .map(|name| {
                LintError::warning(
                    format!(
                        "Policy '{}' has calendar_events_enabled: true but no google_calendar integration is configured",
                        name
                    ),
                    file,
                )
                .with_rule_code(crate::codes::CALENDAR_EVENT_COERCION)
                .with_help(
                    "Add `integrations.google_calendar` (or `org_settings.integrations.google_calendar`) with at least one entry. Without the integration, the calendar feature silently no-ops at runtime.",
                )
            })
            .collect()
    }
}

fn check_secret_field(
    yaml: &serde_yaml::Value,
    path: &[&str],
    field: &str,
    file: &Path,
    errors: &mut Vec<LintError>,
) {
    for item in collect_items_at_path(yaml, path) {
        if let Some(value) = mapping_get_str(item, field) {
            // Skip empty, env var refs, and 1Password refs
            if value.is_empty() || value.starts_with('$') || value.starts_with("op://") {
                continue;
            }
            errors.push(
                LintError::warning(
                    format!(
                        "Integration '{}' field contains a plain-text value",
                        field
                    ),
                    file,
                )
                .with_help("Use an environment variable ($VAR) or 1Password reference (op://...) for secrets")
                .with_suggestion(format!("${}", field.to_uppercase())),
            );
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Severity;
    use std::path::PathBuf;

    fn lint(rule: &dyn Rule, source: &str) -> Vec<LintError> {
        let config: FleetConfig = serde_yaml::from_str(source).unwrap_or_default();
        rule.check(&config, &PathBuf::from("test.yml"), source)
    }

    /// Lint with a path that makes `detect_file_type` return the right FileType.
    /// Use this for rules that gate root-sequence walking on file type.
    fn lint_at(rule: &dyn Rule, source: &str, path: &str) -> Vec<LintError> {
        let config: FleetConfig = serde_yaml::from_str(source).unwrap_or_default();
        rule.check(&config, &PathBuf::from(path), source)
    }

    // -- LabelTargetingRule --

    #[test]
    fn test_label_targeting_valid() {
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any:\n      - Engineering\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_label_targeting_per_context_contracts() {
        // Verified against Fleet source (policies.go, mdm.go, gitops.go):

        // Policies: include+exclude MAY combine…
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any:\n      - Eng\n    labels_exclude_any:\n      - QA\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);

        // …but the exclude PAIR is exclusive (policies.go).
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_exclude_any:\n      - Eng\n    labels_exclude_all:\n      - QA\n",
        );
        assert_eq!(errors.len(), 1);

        // Profiles: include+exclude legal (289 live combos in the
        // reference repo; mdm.go only bans the include pair)…
        let errors = lint(
            &LabelTargetingRule,
            "controls:\n  apple_settings:\n    configuration_profiles:\n      - path: ../p/wifi.mobileconfig\n        labels_include_any:\n          - Eng\n        labels_exclude_any:\n          - QA\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);

        // …include pair on a profile item is not.
        let errors = lint(
            &LabelTargetingRule,
            "controls:\n  apple_settings:\n    configuration_profiles:\n      - path: ../p/wifi.mobileconfig\n        labels_include_any:\n          - Eng\n        labels_include_all:\n          - QA\n",
        );
        assert_eq!(errors.len(), 1);

        // Software: ANY two of the three is rejected (gitops.go).
        let errors = lint(
            &LabelTargetingRule,
            "software:\n  packages:\n    - path: ../s/a.package.yml\n      labels_include_any:\n        - Eng\n      labels_exclude_any:\n        - QA\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("software items allow only ONE"));
    }

    #[test]
    fn test_label_targeting_include_exclude_overlap() {
        // policies.go:230 → LabelOverlap: the same label may not appear in
        // both an include and an exclude list.
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any:\n      - Eng\n    labels_exclude_any:\n      - Eng\n",
        );
        assert_eq!(errors.len(), 1, "{:?}", errors);
        assert!(errors[0].message.contains("'Eng'"), "{:?}", errors[0]);
        assert!(errors[0]
            .message
            .contains("both an include and an exclude list"));

        // Overlap is computed across the concatenated pairs, so
        // include_all ∩ exclude_all counts too.
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_all:\n      - A\n      - B\n    labels_exclude_all:\n      - B\n",
        );
        assert_eq!(errors.len(), 1, "{:?}", errors);
        assert!(errors[0].message.contains("'B'"));

        // Every overlapping name is named, in include-list order.
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any:\n      - A\n      - B\n    labels_exclude_any:\n      - B\n      - A\n",
        );
        assert_eq!(errors.len(), 1, "{:?}", errors);
        assert!(errors[0].message.contains("'A', 'B'"), "{:?}", errors[0]);

        // Disjoint include/exclude stays legal.
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any:\n      - Eng\n    labels_exclude_any:\n      - QA\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);

        // Fleet compares raw names (labels.go:385 — no trim, no case fold),
        // so a case difference is NOT an overlap. Flagging it would be a
        // false positive against real server behavior.
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any:\n      - Eng\n    labels_exclude_any:\n      - eng\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);
    }

    #[test]
    fn test_label_targeting_overlap_is_policies_only() {
        // Profiles legally combine include+exclude, but mdm.go's
        // ValidateMDMProfileSpecs never calls LabelOverlap — so the same
        // label on both sides is accepted for a profile.
        let errors = lint(
            &LabelTargetingRule,
            "controls:\n  apple_settings:\n    configuration_profiles:\n      - path: ../p/wifi.mobileconfig\n        labels_include_any:\n          - Eng\n        labels_exclude_any:\n          - Eng\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);

        // Scripts likewise.
        let errors = lint(
            &LabelTargetingRule,
            "controls:\n  scripts:\n    - path: ../s/x.sh\n      labels_include_any:\n        - Eng\n      labels_exclude_any:\n        - Eng\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);
    }

    #[test]
    fn test_label_targeting_empty_lists_are_no_value() {
        // policies.go:203 states this shape explicitly as VALID:
        // {LabelsIncludeAny: [], LabelsIncludeAll: [A]}.
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any: []\n    labels_include_all:\n      - Eng\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);

        // Same for the software strict 3-way (gitops.go counts len > 0).
        let errors = lint(
            &LabelTargetingRule,
            "software:\n  packages:\n    - path: ../s/a.package.yml\n      labels_include_any: []\n      labels_exclude_any:\n        - QA\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);

        // An empty exclude list cannot overlap with anything.
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any:\n      - Eng\n    labels_exclude_any: []\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);
    }

    #[test]
    fn test_label_targeting_mutual_exclusion() {
        let errors = lint(
            &LabelTargetingRule,
            "policies:\n  - name: test\n    labels_include_any:\n      - Eng\n    labels_include_all:\n      - QA\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .message
            .contains("labels_include_any and labels_include_all"));
    }

    // -- LabelMembershipRule --

    #[test]
    fn test_label_membership_dynamic_valid() {
        let errors = lint(
            &LabelMembershipRule,
            "labels:\n  - name: test\n    label_membership_type: dynamic\n    query: \"SELECT 1\"\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_label_membership_manual_with_query() {
        let errors = lint(
            &LabelMembershipRule,
            "labels:\n  - name: test\n    label_membership_type: manual\n    query: \"SELECT 1\"\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("manual but has 'query'"));
    }

    #[test]
    fn test_label_membership_dynamic_missing_query() {
        let errors = lint(
            &LabelMembershipRule,
            "labels:\n  - name: test\n    label_membership_type: dynamic\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("missing 'query'"));
    }

    #[test]
    fn test_label_membership_dynamic_with_criteria() {
        let errors = lint(
            &LabelMembershipRule,
            "labels:\n  - name: test\n    label_membership_type: dynamic\n    query: \"SELECT 1\"\n    criteria:\n      vital: os_version\n      value: \"15.0\"\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("dynamic but has 'criteria'"));
    }

    #[test]
    fn test_label_membership_manual_with_criteria() {
        let errors = lint(
            &LabelMembershipRule,
            "labels:\n  - name: test\n    label_membership_type: manual\n    hosts:\n      - host1\n    criteria:\n      vital: os_version\n      value: \"15.0\"\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("manual but has 'criteria'"));
    }

    #[test]
    fn test_label_membership_host_vitals_valid() {
        let errors = lint(
            &LabelMembershipRule,
            "labels:\n  - name: test\n    label_membership_type: host_vitals\n    criteria:\n      vital: end_user_idp_department\n      value: Engineering\n",
        );
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_label_membership_host_vitals_missing_criteria() {
        let errors = lint(
            &LabelMembershipRule,
            "labels:\n  - name: test\n    label_membership_type: host_vitals\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("missing 'criteria'"));
    }

    #[test]
    fn test_label_membership_host_vitals_with_hosts() {
        let errors = lint(
            &LabelMembershipRule,
            "labels:\n  - name: test\n    label_membership_type: host_vitals\n    criteria:\n      vital: end_user_idp_group\n      value: Eng\n    hosts:\n      - host1\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("host_vitals but has 'hosts'"));
    }

    #[test]
    fn test_criteria_and_rejected() {
        // Fleet's parseHostVitalCriteria rejects And/Or outright.
        let yaml = "labels:\n  - name: t\n    label_membership_type: host_vitals\n    criteria:\n      and:\n        - vital: end_user_idp_group\n          value: A\n        - vital: end_user_idp_group\n          value: B\n";
        let errors = lint(&LabelMembershipRule, yaml);
        assert!(
            errors.iter().any(|e| e.message.contains("And/Or criteria not supported")),
            "expected And/Or rejection, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_criteria_or_rejected() {
        let yaml = "labels:\n  - name: t\n    label_membership_type: host_vitals\n    criteria:\n      or:\n        - vital: end_user_idp_group\n          value: A\n        - vital: end_user_idp_group\n          value: B\n";
        let errors = lint(&LabelMembershipRule, yaml);
        assert!(
            errors.iter().any(|e| e.message.contains("And/Or criteria not supported")),
            "expected And/Or rejection, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_criteria_unknown_vital_rejected() {
        // os_version is not in the hostVitals registry — Fleet rejects it.
        let yaml = "labels:\n  - name: t\n    label_membership_type: host_vitals\n    criteria:\n      vital: os_version\n      value: \"15.0\"\n";
        let errors = lint(&LabelMembershipRule, yaml);
        assert!(
            errors.iter().any(|e| e.message.contains("unknown vital 'os_version'")),
            "expected unknown-vital error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_criteria_leaf_missing_value() {
        let yaml = "labels:\n  - name: t\n    label_membership_type: host_vitals\n    criteria:\n      vital: end_user_idp_group\n";
        let errors = lint(&LabelMembershipRule, yaml);
        assert!(errors.iter().any(|e| e.message.contains("missing 'value'")));
    }

    #[test]
    fn test_standalone_label_file_dynamic_missing_query() {
        let yaml = "- name: standalone test\n  label_membership_type: dynamic\n";
        let errors = lint_at(&LabelMembershipRule, yaml, "labels/test.yml");
        assert!(
            errors.iter().any(|e| e.message.contains("missing 'query'")),
            "standalone label file should be scanned: {:?}",
            errors
        );
    }

    #[test]
    fn test_standalone_label_file_host_vitals_valid() {
        let yaml = "- name: Engineering\n  label_membership_type: host_vitals\n  criteria:\n    vital: end_user_idp_department\n    value: Engineering\n";
        let errors = lint_at(&LabelMembershipRule, yaml, "labels/test.yml");
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_null_membership_type_suggests_host_vitals() {
        let yaml = "- name: Engineering\n  description: Eng label\n  label_membership_type:\n  criteria:\n    vital: end_user_idp_department\n    value: Engineering\n";
        let errors = lint_at(&LabelMembershipRule, yaml, "labels/test.yml");
        let err = errors
            .iter()
            .find(|e| e.message.contains("empty 'label_membership_type'"))
            .expect("expected empty-membership error");
        assert_eq!(err.suggestion(), Some("label_membership_type: host_vitals"));
    }

    #[test]
    fn test_null_membership_type_suggests_manual() {
        let yaml = "- name: VIPs\n  label_membership_type:\n  hosts:\n    - host1\n";
        let errors = lint_at(&LabelMembershipRule, yaml, "labels/test.yml");
        let err = errors
            .iter()
            .find(|e| e.message.contains("empty 'label_membership_type'"))
            .expect("expected empty-membership error");
        assert_eq!(err.suggestion(), Some("label_membership_type: manual"));
    }

    #[test]
    fn test_null_membership_type_defaults_to_dynamic_suggestion() {
        let yaml = "- name: L\n  label_membership_type:\n  query: SELECT 1\n";
        let errors = lint_at(&LabelMembershipRule, yaml, "labels/test.yml");
        let err = errors
            .iter()
            .find(|e| e.message.contains("empty 'label_membership_type'"))
            .expect("expected empty-membership error");
        assert_eq!(err.suggestion(), Some("label_membership_type: dynamic"));
    }

    #[test]
    fn test_label_rule_skips_policy_file() {
        // Regression: a top-level sequence in a policies/*.yml file must NOT
        // be iterated as labels (that was the original bug in standalone-file
        // support).
        let yaml = "- name: some policy\n  type: patch\n  fleet_maintained_app_slug: firefox\n";
        let errors = lint_at(&LabelMembershipRule, yaml, "policies/test.yml");
        assert!(
            errors.is_empty(),
            "LabelMembershipRule should skip policy files: {:?}",
            errors
        );
    }

    // -- PatchPolicyRule --

    #[test]
    fn test_patch_policy_valid() {
        let errors = lint(
            &PatchPolicyRule,
            "policies:\n  - name: Firefox patch\n    type: patch\n    fleet_maintained_app_slug: firefox\n    install_software: true\n",
        );
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_patch_policy_missing_slug() {
        let errors = lint(
            &PatchPolicyRule,
            "policies:\n  - name: Firefox patch\n    type: patch\n    install_software: true\n",
        );
        assert!(
            errors.iter().any(|e| e.message.contains("missing 'fleet_maintained_app_slug'")),
            "expected missing-slug error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_install_software_true_without_patch_type() {
        let errors = lint(
            &PatchPolicyRule,
            "policies:\n  - name: Random\n    query: SELECT 1\n    install_software: true\n",
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("'install_software: true' but is not a patch policy")),
            "expected install_software/patch-type error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_install_software_mapping_ok_on_regular_policy() {
        // Regular policy with install_software mapping (no patch type required).
        let errors = lint(
            &PatchPolicyRule,
            "policies:\n  - name: Install Firefox\n    query: SELECT 1\n    install_software:\n      package_path: ./firefox.package.yml\n",
        );
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    #[test]
    fn test_patch_policy_standalone_file() {
        // Standalone lib/policies/*.yml file (top-level sequence).
        let errors = lint_at(
            &PatchPolicyRule,
            "- name: Firefox patch\n  type: patch\n",
            "policies/test.yml",
        );
        assert!(
            errors.iter().any(|e| e.message.contains("missing 'fleet_maintained_app_slug'")),
            "standalone patch policy should be scanned: {:?}",
            errors
        );
    }

    #[test]
    fn test_slug_without_patch_type_flagged() {
        let errors = lint(
            &PatchPolicyRule,
            "policies:\n  - name: stray slug\n    query: SELECT 1\n    fleet_maintained_app_slug: zoom/darwin\n",
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("has 'fleet_maintained_app_slug' but is not a patch policy")),
            "expected stray-slug error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_explicit_type_dynamic_treated_as_default() {
        // `type: dynamic` is a valid explicit form of the default — should
        // behave the same as no type: no patch-policy errors.
        let errors = lint(
            &PatchPolicyRule,
            "policies:\n  - name: classic\n    type: dynamic\n    query: SELECT 1\n",
        );
        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
    }

    // -- PolicyAutomationLocationRule --

    #[test]
    fn test_install_software_in_default_yml_flagged() {
        let yaml = "policies:\n  - name: Install Zoom\n    query: SELECT 1\n    install_software:\n      package_path: ./zoom.package.yml\n";
        let errors = lint_at(
            &PolicyAutomationLocationRule,
            yaml,
            "default.yml",
        );
        assert!(
            errors.iter().any(|e| e.message.contains("install_software")
                && e.message.contains("fleet-only")),
            "expected install_software location error, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_run_script_in_default_yml_flagged() {
        let yaml = "policies:\n  - name: Test\n    query: SELECT 1\n    run_script:\n      path: ./fix.sh\n";
        let errors = lint_at(&PolicyAutomationLocationRule, yaml, "default.yml");
        assert!(errors.iter().any(|e| e.message.contains("run_script")));
    }

    #[test]
    fn test_calendar_events_enabled_in_default_yml_flagged() {
        let yaml = "policies:\n  - name: Test\n    query: SELECT 1\n    calendar_events_enabled: true\n";
        let errors = lint_at(&PolicyAutomationLocationRule, yaml, "default.yml");
        assert!(errors.iter().any(|e| e.message.contains("calendar_events_enabled")));
    }

    #[test]
    fn test_automations_in_fleet_file_not_flagged() {
        // Same content, but in a fleet file — these automations are allowed.
        let yaml = "policies:\n  - name: Install Zoom\n    query: SELECT 1\n    install_software:\n      package_path: ./zoom.package.yml\n    run_script:\n      path: ./fix.sh\n    calendar_events_enabled: true\n";
        let errors = lint_at(
            &PolicyAutomationLocationRule,
            yaml,
            "fleets/workstations.yml",
        );
        assert!(
            errors.is_empty(),
            "fleet file automations should not be flagged: {:?}",
            errors
        );
    }

    #[test]
    fn test_automations_in_unassigned_not_flagged() {
        let yaml = "policies:\n  - name: Test\n    query: SELECT 1\n    install_software:\n      package_path: ./x.yml\n";
        let errors = lint_at(
            &PolicyAutomationLocationRule,
            yaml,
            "fleets/unassigned.yml",
        );
        assert!(
            errors.is_empty(),
            "unassigned.yml automations should not be flagged: {:?}",
            errors
        );
    }

    #[test]
    fn test_default_yml_path_references_skipped() {
        // A path reference in default.yml is fine — the referenced lib file
        // might be imported by a fleet file too. Only inline policies are flagged.
        let yaml = "policies:\n  - path: ../lib/pol.policies.yml\n";
        let errors = lint_at(&PolicyAutomationLocationRule, yaml, "default.yml");
        assert!(errors.is_empty(), "path refs should not be flagged: {:?}", errors);
    }

    #[test]
    fn test_default_yml_without_automations_clean() {
        let yaml = "policies:\n  - name: FileVault\n    query: SELECT 1\n    platform: darwin\n";
        let errors = lint_at(&PolicyAutomationLocationRule, yaml, "default.yml");
        assert!(errors.is_empty(), "clean default.yml should pass: {:?}", errors);
    }

    #[test]
    fn test_patch_rule_skips_label_file() {
        // Regression: PatchPolicyRule must not iterate a standalone label
        // file's top-level sequence as policies.
        let yaml = "- name: Engineering\n  label_membership_type: host_vitals\n  criteria:\n    vital: end_user_idp_department\n    value: Engineering\n";
        let errors = lint_at(&PatchPolicyRule, yaml, "labels/test.yml");
        assert!(
            errors.is_empty(),
            "PatchPolicyRule should skip label files: {:?}",
            errors
        );
    }

    // -- DateFormatRule --

    #[test]
    fn test_date_format_valid() {
        let errors = lint(
            &DateFormatRule,
            "controls:\n  macos_updates:\n    deadline: \"2025-06-15\"\n    minimum_version: \"15.1\"\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_date_format_invalid() {
        let errors = lint(
            &DateFormatRule,
            "controls:\n  macos_updates:\n    deadline: \"15-06-2025\"\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("not a valid YYYY-MM-DD"));
    }

    #[test]
    fn test_date_format_invalid_month() {
        let errors = lint(
            &DateFormatRule,
            "controls:\n  macos_updates:\n    deadline: \"2025-13-01\"\n",
        );
        assert_eq!(errors.len(), 1);
    }

    // -- HashFormatRule --

    #[test]
    fn test_hash_format_valid() {
        let errors = lint(
            &HashFormatRule,
            "software:\n  packages:\n    - path: foo.yml\n      hash_sha256: fd22528a87f3cfdb81aca981953aa5c8d7084581b9209bb69abf69c09a0afaaf\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_hash_format_uppercase() {
        let errors = lint(
            &HashFormatRule,
            "software:\n  packages:\n    - path: foo.yml\n      hash_sha256: FD22528A87F3CFDB81ACA981953AA5C8D7084581B9209BB69ABF69C09A0AFAAF\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("lowercase"));
        assert!(errors[0].suggestion().is_some());
    }

    #[test]
    fn test_hash_format_wrong_length() {
        let errors = lint(
            &HashFormatRule,
            "software:\n  packages:\n    - path: foo.yml\n      hash_sha256: abc123\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("64 characters"));
    }

    // -- CategoriesRule --

    #[test]
    fn test_categories_valid() {
        let errors = lint(
            &CategoriesRule,
            "software:\n  packages:\n    - path: foo.yml\n      categories:\n        - Browsers\n        - Security\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_categories_custom_names_are_legal() {
        // Fleet accepts user-created categories (emoji supported, ≤255
        // chars) — the old fixed-set check produced 487 false positives.
        let errors = lint(
            &CategoriesRule,
            "software:\n  packages:\n    - path: foo.yml\n      categories:\n        - Gaming\n        - \"🛟 Support\"\n",
        );
        assert!(errors.is_empty(), "{:?}", errors);
    }

    #[test]
    fn test_categories_case_suggestion() {
        let errors = lint(
            &CategoriesRule,
            "software:\n  packages:\n    - path: foo.yml\n      categories:\n        - browsers\n",
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].suggestion(), Some("Browsers"));
    }

    // -- FileExtensionRule --

    #[test]
    fn test_file_extension_valid() {
        let errors = lint(
            &FileExtensionRule,
            "controls:\n  macos_settings:\n    custom_settings:\n      - path: ../lib/profile.mobileconfig\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_file_extension_invalid_macos() {
        let errors = lint(
            &FileExtensionRule,
            "controls:\n  macos_settings:\n    custom_settings:\n      - path: ../lib/profile.xml\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("unexpected extension"));
    }

    #[test]
    fn test_file_extension_scripts() {
        let errors = lint(
            &FileExtensionRule,
            "controls:\n  scripts:\n    - path: ../lib/setup.sh\n",
        );
        assert!(errors.is_empty());
    }

    // -- SecretHygieneRule --

    #[test]
    fn test_secret_hygiene_env_var() {
        let errors = lint(
            &SecretHygieneRule,
            "integrations:\n  jira:\n    - url: https://jira.example.com\n      api_token: $JIRA_TOKEN\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_secret_hygiene_plaintext() {
        let errors = lint(
            &SecretHygieneRule,
            "integrations:\n  jira:\n    - url: https://jira.example.com\n      api_token: my-secret-token-123\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("plain-text"));
    }

    #[test]
    fn test_secret_hygiene_op_ref() {
        let errors = lint(
            &SecretHygieneRule,
            "integrations:\n  jira:\n    - url: https://jira.example.com\n      api_token: \"op://Vault/Jira/token\"\n",
        );
        assert!(errors.is_empty());
    }

    // -- PathReferenceRule --

    #[test]
    fn test_path_ref_valid_path() {
        let errors = lint(
            &PathReferenceRule,
            "policies:\n  - path: ../lib/policy.yml\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_path_ref_valid_paths_glob() {
        let errors = lint(
            &PathReferenceRule,
            "policies:\n  - paths: ../lib/policies/*.yml\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_path_ref_glob_in_path_field() {
        let errors = lint(
            &PathReferenceRule,
            "policies:\n  - path: ../lib/policies/*.yml\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("glob characters"));
        assert!(errors[0].message.contains("use 'paths'"));
    }

    #[test]
    fn test_path_ref_no_glob_in_paths_field() {
        let errors = lint(
            &PathReferenceRule,
            "policies:\n  - paths: ../lib/policies/specific.yml\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("does not contain glob"));
        assert!(errors[0].message.contains("use 'path'"));
    }

    #[test]
    fn test_path_ref_both_path_and_paths() {
        let errors = lint(
            &PathReferenceRule,
            "policies:\n  - path: foo.yml\n    paths: bar/*.yml\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("both 'path' and 'paths'"));
    }

    #[test]
    fn test_path_ref_inline_policy_ok() {
        // Inline policies (no path/paths) are fine
        let errors = lint(
            &PathReferenceRule,
            "policies:\n  - name: test\n    query: SELECT 1\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_path_ref_script_requires_path() {
        let errors = lint(
            &PathReferenceRule,
            "controls:\n  scripts:\n    - name: inline-script\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("must reference a file"));
    }

    #[test]
    fn test_path_ref_script_with_path_ok() {
        let errors = lint(
            &PathReferenceRule,
            "controls:\n  scripts:\n    - path: ./scripts/setup.sh\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn test_path_ref_script_with_glob_ok() {
        let errors = lint(
            &PathReferenceRule,
            "controls:\n  scripts:\n    - paths: ./scripts/*.sh\n",
        );
        assert!(errors.is_empty());
    }

    // -- is_valid_date --

    #[test]
    fn test_valid_dates() {
        assert!(is_valid_date("2025-06-15"));
        assert!(is_valid_date("2024-02-29")); // leap year
        assert!(is_valid_date("2025-12-31"));
    }

    #[test]
    fn test_invalid_dates() {
        assert!(!is_valid_date("2025-13-01")); // month > 12
        assert!(!is_valid_date("2025-02-29")); // not a leap year
        assert!(!is_valid_date("15-06-2025")); // wrong format
        assert!(!is_valid_date("2025/06/15")); // wrong separator
        assert!(!is_valid_date("not-a-date"));
    }

    // -- ShebangSyntaxRule --
    // These tests touch the filesystem because the rule reads the
    // referenced script files. tempfile gives us a per-test scratch
    // directory so concurrent test runs don't collide.

    #[test]
    fn shebang_rule_flags_sh_script_without_shebang() {
        let tmp = tempfile::tempdir().unwrap();
        let script_path = tmp.path().join("install.sh");
        std::fs::write(&script_path, "echo hello\n").unwrap();

        let yaml_path = tmp.path().join("default.yml");
        let source = "controls:\n  scripts:\n    - path: install.sh\n";
        std::fs::write(&yaml_path, source).unwrap();

        let errors = ShebangSyntaxRule.check(
            &FleetConfig::default(),
            &yaml_path,
            source,
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("missing a shebang"));
    }

    #[test]
    fn shebang_rule_accepts_sh_script_with_shebang() {
        let tmp = tempfile::tempdir().unwrap();
        let script_path = tmp.path().join("install.sh");
        std::fs::write(&script_path, "#!/bin/sh\necho hello\n").unwrap();

        let yaml_path = tmp.path().join("default.yml");
        let source = "controls:\n  scripts:\n    - path: install.sh\n";
        std::fs::write(&yaml_path, source).unwrap();

        let errors = ShebangSyntaxRule.check(
            &FleetConfig::default(),
            &yaml_path,
            source,
        );
        assert!(errors.is_empty(), "shebang present, should not warn: {errors:?}");
    }

    #[test]
    fn shebang_rule_ignores_ps1_and_other_non_posix_scripts() {
        // PowerShell, batch, and Python scripts have their own conventions
        // and aren't checked. .ps1 specifically must NOT trigger this rule.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("install.ps1"), "Write-Host hi\n").unwrap();

        let yaml_path = tmp.path().join("default.yml");
        let source = "controls:\n  scripts:\n    - path: install.ps1\n";
        std::fs::write(&yaml_path, source).unwrap();

        let errors = ShebangSyntaxRule.check(
            &FleetConfig::default(),
            &yaml_path,
            source,
        );
        assert!(errors.is_empty(), ".ps1 must not trigger shebang rule");
    }

    #[test]
    fn shebang_rule_silent_when_script_file_does_not_exist() {
        // Missing-file is path-reference rule's job, not ours. Stay quiet
        // so we don't double-report and so missing optional scripts don't
        // produce noise.
        let tmp = tempfile::tempdir().unwrap();
        let yaml_path = tmp.path().join("default.yml");
        let source = "controls:\n  scripts:\n    - path: nonexistent.sh\n";
        std::fs::write(&yaml_path, source).unwrap();

        let errors = ShebangSyntaxRule.check(
            &FleetConfig::default(),
            &yaml_path,
            source,
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn is_posix_shell_script_extension_matrix() {
        assert!(is_posix_shell_script("foo.sh"));
        assert!(is_posix_shell_script("nested/dir/foo.bash"));
        assert!(is_posix_shell_script("install.zsh"));
        assert!(!is_posix_shell_script("foo.ps1"));
        assert!(!is_posix_shell_script("foo.py"));
        assert!(!is_posix_shell_script("foo.txt"));
        assert!(!is_posix_shell_script("foo.shell")); // not a real extension
    }

    // -- WebhookEndpointRule --
    // Pure YAML, no fs needed.

    #[test]
    fn webhook_rule_accepts_https_url() {
        let errors = lint(
            &WebhookEndpointRule,
            "webhook_settings:\n  host_status_webhook:\n    destination_url: https://hooks.example.com/fleet\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn webhook_rule_flags_plain_http() {
        // Plain HTTP is rejected because tokens travel in the body or
        // headers — sending over an unencrypted channel would leak them.
        let errors = lint(
            &WebhookEndpointRule,
            "webhook_settings:\n  host_status_webhook:\n    destination_url: http://hooks.example.com/fleet\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("plain HTTP"));
    }

    #[test]
    fn webhook_rule_flags_missing_scheme() {
        let errors = lint(
            &WebhookEndpointRule,
            "webhook_settings:\n  host_status_webhook:\n    destination_url: hooks.example.com/fleet\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("missing https"));
    }

    #[test]
    fn webhook_rule_accepts_env_var_refs() {
        // Server-side resolution — we can't validate, so we don't warn.
        // Mirrors secret-hygiene's $VAR-skip behavior.
        let errors = lint(
            &WebhookEndpointRule,
            "webhook_settings:\n  host_status_webhook:\n    destination_url: $WEBHOOK_URL\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn webhook_rule_accepts_1password_refs() {
        let errors = lint(
            &WebhookEndpointRule,
            "webhook_settings:\n  host_status_webhook:\n    destination_url: op://vault/item/url\n",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn webhook_rule_flags_whitespace_in_url() {
        let errors = lint(
            &WebhookEndpointRule,
            "webhook_settings:\n  host_status_webhook:\n    destination_url: \"https://hooks example.com\"\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("whitespace"));
    }

    // -- SoftwareUrlRule --

    #[test]
    fn software_url_accepts_valid_inline_package() {
        let errors = lint(
            &SoftwareUrlRule,
            "software:\n  packages:\n    - url: https://cdn.example.com/app.pkg\n      hash_sha256: abc\n",
        );
        assert!(errors.is_empty(), "got: {errors:?}");
    }

    #[test]
    fn software_url_flags_plain_http_inline() {
        let errors = lint(
            &SoftwareUrlRule,
            "software:\n  packages:\n    - url: http://cdn.example.com/app.pkg\n",
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].severity, Severity::Error);
        assert!(errors[0].message.contains("plain HTTP"));
        assert!(errors[0].line().is_some());
    }

    #[test]
    fn software_url_flags_missing_scheme_standalone_file() {
        // Standalone software file shape (top-level hash_sha256 + url).
        let errors = lint(
            &SoftwareUrlRule,
            "hash_sha256: 8c30a711\nurl: cdn.example.com/app.pkg\nself_service: false\n",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("missing https"));
    }

    #[test]
    fn software_url_flags_pkg_yml_placeholder() {
        // The exact placeholder `flint pkg --yml` writes — parses as a URL but
        // would fail at apply, so it's surfaced as a warning.
        let errors = lint(
            &SoftwareUrlRule,
            "hash_sha256: 8c30a711\nurl: https://REPLACE-ME.example.com/Support.3.0.3.pkg\nself_service: false\n",
        );
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].severity, Severity::Warning);
        assert!(errors[0].message.contains("placeholder"));
    }

    #[test]
    fn software_url_skips_env_and_op_refs() {
        let errors = lint(
            &SoftwareUrlRule,
            "software:\n  packages:\n    - url: $INSTALLER_URL\n    - url: op://vault/item/url\n",
        );
        assert!(errors.is_empty(), "got: {errors:?}");
    }

    #[test]
    fn software_url_ignores_unrelated_top_level_url() {
        // A non-software file with a top-level `url:` but no hash_sha256 must
        // not be treated as a software package.
        let errors = lint(&SoftwareUrlRule, "url: not-a-software-file\n");
        assert!(errors.is_empty(), "got: {errors:?}");
    }

    #[test]
    fn software_url_handles_sequence_form_file() {
        // Standalone software file written as a top-level sequence (what
        // `flint pkg` emits) — must still validate each entry's url.
        let errs = lint_at(
            &SoftwareUrlRule,
            "- url: http://cdn.example.com/a.pkg\n  hash_sha256: abc\n",
            "platforms/macos/L0/software/a.yml",
        );
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("plain HTTP"));
    }

    // -- SoftwareSourceRule --

    #[test]
    fn software_source_flags_hash_without_url() {
        // The real dry-run failure: `flint pkg`'s minimal block used as a
        // standalone software file — a sequence entry with a hash but no url.
        let errs = lint_at(
            &SoftwareSourceRule { snapshot: None, placeholders: Default::default(), referenced: Default::default() },
            "# com.fleetdm.fonts.corp (Corp-Fonts-1.0.pkg) version 1.0\n- hash_sha256: 3a673c556d864348df3702a806be41bcdf44721976c7aacac41682aa159a3be2\n",
            "platforms/macos/L0/software/corp-fonts.yml",
        );
        assert_eq!(errs.len(), 1, "got: {errs:?}");
        assert_eq!(errs[0].severity, Severity::Warning);
        assert_eq!(errs[0].rule_code, Some("software-source"));
        assert!(errs[0].message.contains("no url"));
        assert!(errs[0].line().is_some());
    }

    #[test]
    fn software_source_ok_when_url_present() {
        let errs = lint_at(
            &SoftwareSourceRule { snapshot: None, placeholders: Default::default(), referenced: Default::default() },
            "- url: https://cdn.example.com/a.pkg\n  hash_sha256: abc\n",
            "platforms/macos/L0/software/a.yml",
        );
        assert!(errs.is_empty(), "got: {errs:?}");
    }

    #[test]
    fn software_source_flags_inline_hash_without_url() {
        // Same problem inline in a team file's software.packages.
        let errs = lint(
            &SoftwareSourceRule { snapshot: None, placeholders: Default::default(), referenced: Default::default() },
            "software:\n  packages:\n    - hash_sha256: deadbeef\n",
        );
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].rule_code, Some("software-source"));
    }

    #[test]
    fn software_source_flags_comment_only_file() {
        // A software lib file that is only a comment (no minimum key) → error.
        let errs = lint_at(
            &SoftwareSourceRule { snapshot: None, placeholders: Default::default(), referenced: Default::default() },
            "# com.example.fonts-corp (Corp-Fonts-1.0.1.pkg) version 1.0.1\n",
            "platforms/macos/L0/software/corp-fonts.yml",
        );
        assert_eq!(errs.len(), 1, "got: {errs:?}");
        assert_eq!(errs[0].severity, Severity::Error);
        assert!(errs[0].message.contains("no package definition"));
    }

    #[test]
    fn software_source_ignores_path_reference_entry() {
        // A `path:` reference (no hash) is not flagged — its source lives in
        // the referenced file.
        let errs = lint(
            &SoftwareSourceRule { snapshot: None, placeholders: Default::default(), referenced: Default::default() },
            "software:\n  packages:\n    - path: ../lib/software/a.yml\n",
        );
        assert!(errs.is_empty(), "got: {errs:?}");
    }

    #[test]
    fn webhook_rule_handles_org_settings_path() {
        // Same logic, different schema location. Both paths in the wild.
        let errors = lint(
            &WebhookEndpointRule,
            "org_settings:\n  webhook_settings:\n    failing_policies_webhook:\n      destination_url: not-a-url\n",
        );
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn webhook_url_problem_unit_matrix() {
        // Pure helper — exhaustive matrix for confidence.
        assert_eq!(webhook_url_problem("https://example.com"), None);
        assert_eq!(webhook_url_problem("https://example.com/path"), None);
        assert_eq!(webhook_url_problem("$VAR"), None);
        assert_eq!(webhook_url_problem("op://vault/x"), None);
        assert_eq!(webhook_url_problem(""), Some("empty"));
        assert_eq!(webhook_url_problem("http://example.com"), Some("plain HTTP — use https://"));
        assert_eq!(webhook_url_problem("example.com"), Some("missing https:// scheme"));
        assert_eq!(webhook_url_problem("https://"), Some("missing host"));
        assert_eq!(webhook_url_problem("https:// has spaces"), Some("contains whitespace"));
    }

    // -- CalendarEventCoercionRule --

    #[test]
    fn calendar_rule_flags_enabled_without_integration() {
        let errors = lint_at(
            &CalendarEventCoercionRule,
            "policies:\n  - name: Onboarding\n    calendar_events_enabled: true\n",
            "default.yml",
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("Onboarding"));
        assert!(errors[0].message.contains("calendar_events_enabled"));
    }

    #[test]
    fn calendar_rule_silent_when_integration_present() {
        // Same policy, but with the integration block — must NOT warn.
        let errors = lint_at(
            &CalendarEventCoercionRule,
            "policies:\n  - name: Onboarding\n    calendar_events_enabled: true\nintegrations:\n  google_calendar:\n    - domain: example.com\n",
            "default.yml",
        );
        assert!(errors.is_empty(), "integration present, should be silent: {errors:?}");
    }

    #[test]
    fn calendar_rule_accepts_integration_under_org_settings() {
        // The integration can live at either top-level or under
        // org_settings — both schemas exist in real repos.
        let errors = lint_at(
            &CalendarEventCoercionRule,
            "policies:\n  - name: P\n    calendar_events_enabled: true\norg_settings:\n  integrations:\n    google_calendar:\n      - domain: example.com\n",
            "default.yml",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn calendar_rule_silent_when_no_policy_opts_in() {
        let errors = lint_at(
            &CalendarEventCoercionRule,
            "policies:\n  - name: Normal\n",
            "default.yml",
        );
        assert!(errors.is_empty());
    }

    #[test]
    fn calendar_rule_skips_non_fleet_config_files() {
        // Team-level policy files commonly opt in to calendar events
        // without containing the integration block — the integration is
        // global. This rule must NOT fire on those files.
        let errors = lint_at(
            &CalendarEventCoercionRule,
            "- name: P\n  calendar_events_enabled: true\n",
            "policies/team-a/onboarding.yml",
        );
        assert!(errors.is_empty(), "non-FleetConfig files must be skipped");
    }

    #[test]
    fn calendar_rule_reports_each_offending_policy_separately() {
        // Two policies both opt in without the integration — both should
        // appear in the diagnostics, not just the first one. Users need
        // to know every offender so they can fix them in one pass.
        let errors = lint_at(
            &CalendarEventCoercionRule,
            "policies:\n  - name: A\n    calendar_events_enabled: true\n  - name: B\n    calendar_events_enabled: true\n",
            "default.yml",
        );
        assert_eq!(errors.len(), 2);
        let messages: Vec<&str> = errors.iter().map(|e| e.message.as_str()).collect();
        assert!(messages.iter().any(|m| m.contains("'A'")));
        assert!(messages.iter().any(|m| m.contains("'B'")));
    }
}
