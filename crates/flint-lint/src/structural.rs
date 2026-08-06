//! Structural YAML validation rule.
//!
//! Walks the raw YAML tree alongside the schema tree to detect:
//! - **Unknown keys** (with Levenshtein-distance typo suggestions)
//! - **Misplaced keys** (valid key, wrong nesting level)
//! - **Missing wrappers** (key belongs under a child that was omitted)

use super::deprecations::DEPRECATION_REGISTRY;
use super::error::LintError;
use super::fleet_config::FleetConfig;
use super::rules::Rule;
use super::structure::{schema_for_path, SchemaNode, KEY_REGISTRY};
use std::path::Path;

pub struct StructuralValidationRule;

impl Rule for StructuralValidationRule {
    fn name(&self) -> &'static str {
        "structural-validation"
    }

    fn description(&self) -> &'static str {
        "Validates YAML structure: catches unknown keys, misplaced keys, and missing wrappers"
    }
    fn category(&self) -> &'static str {
        "structural"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://fleetdm.com/docs/configuration/yaml-files")
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml_value: serde_yaml::Value = match serde_yaml::from_str(source) {
            Ok(v) => v,
            Err(_) => return Vec::new(), // parse errors are reported elsewhere
        };

        let schema = schema_for_path(file);
        let mut errors = Vec::new();

        validate_node(&yaml_value, schema, "", source, file, &mut errors);

        errors
    }
}

/// Recursively validate a YAML value against a schema node.
fn validate_node(
    value: &serde_yaml::Value,
    schema: &SchemaNode,
    path: &str,
    source: &str,
    file: &Path,
    errors: &mut Vec<LintError>,
) {
    match schema {
        SchemaNode::Mapping(children) => {
            if let serde_yaml::Value::Mapping(map) = value {
                for (key, child_value) in map {
                    let key_str = match key.as_str() {
                        Some(s) => s,
                        None => continue,
                    };

                    let child_path = if path.is_empty() {
                        key_str.to_string()
                    } else {
                        format!("{}.{}", path, key_str)
                    };

                    if let Some(child_schema) = children.get(key_str) {
                        // Valid key — recurse
                        validate_node(child_value, child_schema, &child_path, source, file, errors);
                    } else {
                        // Key not valid here — classify the error
                        let (line, col) = find_key_position(source, key_str, path);

                        if let Some(error) =
                            classify_unknown_key(key_str, path, children, file, line, col)
                        {
                            errors.push(error);
                        }
                    }
                }
            }
        }
        SchemaNode::Array(item_schema) => {
            if let serde_yaml::Value::Sequence(items) = value {
                for (idx, item) in items.iter().enumerate() {
                    let item_path = format!("{}[{}]", path, idx);
                    validate_node(item, item_schema, &item_path, source, file, errors);
                }
            }
        }
        SchemaNode::BooleanLeaf => {
            // Validate that the value is a boolean.
            // serde_yaml (YAML 1.2) only parses true/false as Bool.
            // Fleet uses Go's YAML 1.1 parser which also accepts yes/no/on/off.
            let is_bool = value.is_bool()
                || matches!(
                    value.as_str().map(|s| s.to_lowercase()).as_deref(),
                    Some("yes" | "no" | "on" | "off")
                );
            if !is_bool {
                let key_name = path.rsplit('.').next().unwrap_or(path);
                let (line, col) = find_value_position(source, key_name, path);
                let value_str = match value {
                    serde_yaml::Value::String(s) => format!("\"{}\"", s),
                    serde_yaml::Value::Number(n) => n.to_string(),
                    serde_yaml::Value::Null => "null".to_string(),
                    _ => format!("{:?}", value),
                };
                let mut err = LintError::warning(
                    format!("'{}' expects a boolean value, got {}", key_name, value_str),
                    file,
                )
                .with_help("Use 'true' or 'false'".to_string());
                if let Some(l) = line {
                    err = err.with_location(l, col.unwrap_or(1));
                }
                errors.push(err);
            }
        }
        SchemaNode::Leaf | SchemaNode::OpenMapping => {
            // No structural validation needed
        }
    }
}

/// Classify an unknown key into one of three error types.
fn classify_unknown_key(
    key: &str,
    current_path: &str,
    current_children: &std::collections::HashMap<&str, SchemaNode>,
    file: &Path,
    line: Option<usize>,
    col: Option<usize>,
) -> Option<LintError> {
    // 0. If this key is in the deprecation table, don't report it as "unknown".
    //    The DeprecationRule handles it with proper version-gated severity.
    if DEPRECATION_REGISTRY
        .find_deprecated_key(key, current_path)
        .is_some()
    {
        return None;
    }

    let registry = &*KEY_REGISTRY;

    // 1. Check if the key is valid somewhere else (misplaced key)
    if let Some(valid_paths) = registry.lookup(key) {
        // Filter to paths that don't match current location
        let other_paths: Vec<&&str> = valid_paths
            .iter()
            .filter(|p| {
                // The key is registered at `p`, meaning it's valid as a child of `p`.
                // current_path is where we currently are. If current_path != p, it's misplaced.
                **p != current_path
            })
            .collect();

        if !other_paths.is_empty() {
            // Check if the key is a grandchild (missing wrapper)
            // e.g., we're at "controls.macos_settings" and the key is "path" which belongs
            // under "controls.macos_settings.custom_settings[]"
            for sibling_key in current_children.keys() {
                let sibling_path = if current_path.is_empty() {
                    sibling_key.to_string()
                } else {
                    format!("{}.{}", current_path, sibling_key)
                };

                // Check if the key is valid under this sibling
                for vp in valid_paths {
                    if vp.starts_with(&sibling_path) {
                        let display_path = if current_path.is_empty() {
                            key.to_string()
                        } else {
                            format!("{}.{}", current_path, key)
                        };

                        let mut err = LintError::error(
                            format!(
                                "Key '{}' is not valid at '{}'. It requires wrapper '{}'",
                                key, display_path, sibling_key
                            ),
                            file,
                        )
                        .with_help(format!("Place '{}' inside '{}' instead", key, sibling_path));

                        if let Some(l) = line {
                            err = err.with_location(l, col.unwrap_or(1));
                        }
                        return Some(err);
                    }
                }
            }

            // Not a grandchild — plain misplaced key
            // Pick the most relevant suggestion path
            let suggestion_path = pick_best_path(key, current_path, &other_paths);

            let display_location = if current_path.is_empty() {
                "top level".to_string()
            } else {
                format!("'{}'", current_path)
            };

            let mut err = LintError::error(
                format!(
                    "Key '{}' is not valid under {}. It belongs under '{}'",
                    key, display_location, suggestion_path
                ),
                file,
            )
            .with_help(format!(
                "Move '{}' to be a child of '{}'",
                key, suggestion_path
            ));

            if let Some(l) = line {
                err = err.with_location(l, col.unwrap_or(1));
            }
            return Some(err);
        }
    }

    // 2. Truly unknown key — suggest closest match via Levenshtein distance
    let valid_keys: Vec<&str> = current_children.keys().copied().collect();
    let suggestion = find_closest_key(key, &valid_keys);

    let display_location = if current_path.is_empty() {
        "top level".to_string()
    } else {
        format!("'{}'", current_path)
    };

    let mut err = LintError::error(
        format!("Unknown key '{}' at {}", key, display_location),
        file,
    );

    if let Some(closest) = suggestion {
        err = err
            .with_help(format!("Did you mean '{}'?", closest))
            .with_suggestion(closest.to_string())
            .with_fix_safety(super::error::FixSafety::Safe);
    } else {
        let valid_list: Vec<&str> = current_children.keys().copied().collect();
        if !valid_list.is_empty() {
            let mut sorted = valid_list;
            sorted.sort();
            err = err.with_help(format!("Valid keys at this level: {}", sorted.join(", ")));
        }
    }

    if let Some(l) = line {
        err = err.with_location(l, col.unwrap_or(1));
    }

    Some(err)
}

/// Pick the most contextually relevant path from candidate paths.
fn pick_best_path(_key: &str, current_path: &str, candidates: &[&&str]) -> String {
    // Prefer paths that share a common prefix with current_path
    if !current_path.is_empty() {
        let current_parts: Vec<&str> = current_path.split('.').collect();
        let mut best_score = 0;
        let mut best = candidates[0];

        for candidate in candidates {
            let cand_parts: Vec<&str> = candidate.split('.').collect();
            let common = current_parts
                .iter()
                .zip(cand_parts.iter())
                .take_while(|(a, b)| a == b)
                .count();
            if common > best_score {
                best_score = common;
                best = candidate;
            }
        }
        return (*best).to_string();
    }

    // Fall back to first candidate
    (*candidates[0]).to_string()
}

// ---------------------------------------------------------------------------
// Levenshtein distance
// ---------------------------------------------------------------------------

fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr = vec![0; b_len + 1];

    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_len]
}

/// Find the closest matching key using Levenshtein distance.
/// Returns `None` if no key is close enough (distance > max(3, key_len/2)).
fn find_closest_key<'a>(key: &str, candidates: &[&'a str]) -> Option<&'a str> {
    let max_dist = 3.max(key.len() / 2);
    let mut best: Option<(&str, usize)> = None;

    for candidate in candidates {
        let dist = levenshtein_distance(key, candidate);
        if dist <= max_dist {
            match best {
                None => best = Some((candidate, dist)),
                Some((_, best_dist)) if dist < best_dist => best = Some((candidate, dist)),
                _ => {}
            }
        }
    }

    best.map(|(s, _)| s)
}

// ---------------------------------------------------------------------------
// Position finding
// ---------------------------------------------------------------------------

/// Find the line/column of a YAML key in the source text.
/// Uses a simple approach: search for the key followed by `:` in the source,
/// scoped to the approximate region based on the path context.
fn find_key_position(source: &str, key: &str, _path: &str) -> (Option<usize>, Option<usize>) {
    // Build a pattern: key followed by optional spaces then colon
    let pattern = format!("{}:", key);

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&pattern)
            || trimmed.starts_with(&format!("\"{}\":", key))
            || trimmed.starts_with(&format!("'{}':", key))
        {
            let col = line.find(key).unwrap_or(0) + 1; // 1-based
            return (Some(line_idx + 1), Some(col));
        }
    }

    (None, None)
}

/// Find the source position of a value (after the colon) for a given key.
fn find_value_position(source: &str, key: &str, _path: &str) -> (Option<usize>, Option<usize>) {
    let pattern = format!("{}:", key);

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&pattern)
            || trimmed.starts_with(&format!("\"{}\":", key))
            || trimmed.starts_with(&format!("'{}':", key))
        {
            // Point to the value (after the colon + space)
            if let Some(colon_pos) = line.find(':') {
                let val_col = colon_pos + 2; // after ": "
                return (Some(line_idx + 1), Some(val_col.max(1)));
            }
            let col = line.find(key).unwrap_or(0) + 1;
            return (Some(line_idx + 1), Some(col));
        }
    }

    (None, None)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn check(yaml: &str, file_name: &str) -> Vec<LintError> {
        let config = FleetConfig::default();
        let path = PathBuf::from(file_name);
        StructuralValidationRule.check(&config, &path, yaml)
    }

    // Issue #13: a policy path reference may carry inline fields (Fleet's
    // Policy struct embeds BaseItem alongside the full spec), and
    // agent_options.path must not be blamed on policies[0].
    #[test]
    fn test_policy_path_ref_with_labels_and_agent_options_path() {
        let yaml = r#"
name: Servers
policies:
  - path: ../platforms/linux/policies/baseline.policies.yml
    labels_include_any:
      - Ubuntu
reports:
  - path: ../platforms/all/reports/usb.reports.yml
agent_options:
  path: ../platforms/agent-options.yml
"#;
        let errors = check(yaml, "fleets/servers.yml");
        assert!(
            errors.is_empty(),
            "path refs with inline fields should be valid: {:?}",
            errors.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }

    // Issue #12: features.historical_data is valid at org and team level.
    #[test]
    fn test_features_historical_data() {
        let yaml = r#"
org_settings:
  features:
    enable_host_users: true
    historical_data:
      uptime: true
      vulnerabilities: true
"#;
        let errors = check(yaml, "default.yml");
        assert!(errors.is_empty(), "{:?}", errors);

        let team_yaml = r#"
name: Workstations
settings:
  features:
    historical_data:
      vulnerabilities: false
"#;
        let errors = check(team_yaml, "fleets/workstations.yml");
        assert!(errors.is_empty(), "{:?}", errors);

        // Unknown sub-key still flagged
        let bad = r#"
org_settings:
  features:
    historical_data:
      cpu_usage: true
"#;
        let errors = check(bad, "default.yml");
        assert_eq!(errors.len(), 1, "{:?}", errors);
        assert!(errors[0].message.contains("cpu_usage"));
    }

    // Issue #14: certificate SAN attribute (Fleet 4.86).
    #[test]
    fn test_android_certificate_subject_alternative_name() {
        let yaml = r#"
name: Android
controls:
  android_settings:
    certificates:
      - name: WiFi cert
        certificate_authority_name: MyCA
        subject_name: CN=%EmailAddress%
        subject_alternative_name: '%EmailAddress%'
"#;
        let errors = check(yaml, "fleets/android.yml");
        assert!(errors.is_empty(), "{:?}", errors);
    }

    #[test]
    fn test_valid_default_config() {
        let yaml = r#"
policies:
  - name: "Test"
    query: "SELECT 1;"
queries:
  - name: "Test"
    query: "SELECT 1;"
agent_options:
  config: {}
controls:
  scripts:
    - path: foo.sh
  macos_settings:
    custom_settings:
      - path: foo.mobileconfig
software: {}
org_settings:
  server_settings:
    server_url: https://example.com
"#;
        let errors = check(yaml, "default.yml");
        assert!(
            errors.is_empty(),
            "Expected no errors but got: {:?}",
            errors
        );
    }

    #[test]
    fn test_unknown_top_level_key() {
        let yaml = r#"
policis:
  - name: "Test"
    query: "SELECT 1;"
"#;
        let errors = check(yaml, "default.yml");
        assert!(!errors.is_empty(), "Expected errors for typo 'policis'");
        let err = &errors[0];
        assert!(
            err.message.contains("Unknown key 'policis'") || err.message.contains("policis"),
            "Error should mention 'policis': {}",
            err.message
        );
        assert!(
            err.help.as_ref().is_some_and(|h| h.contains("policies")),
            "Should suggest 'policies': {:?}",
            err.help
        );
    }

    #[test]
    fn test_misplaced_key_scripts_under_macos_settings() {
        let yaml = r#"
controls:
  macos_settings:
    custom_settings:
      - path: foo.mobileconfig
    scripts:
      - path: bar.sh
"#;
        let errors = check(yaml, "default.yml");
        assert!(!errors.is_empty(), "Expected error for misplaced 'scripts'");
        let err = &errors[0];
        assert!(
            err.message.contains("scripts") && err.message.contains("controls"),
            "Error should mention scripts belongs under controls: {}",
            err.message
        );
    }

    #[test]
    fn test_missing_wrapper_custom_settings() {
        // User puts path directly under macos_settings instead of under custom_settings
        let yaml = r#"
controls:
  macos_settings:
    path: foo.mobileconfig
"#;
        let errors = check(yaml, "default.yml");
        assert!(!errors.is_empty(), "Expected error for missing wrapper");
    }

    #[test]
    fn test_valid_team_config() {
        let yaml = r#"
name: Engineering
policies:
  - name: "Test"
    query: "SELECT 1;"
controls:
  scripts:
    - path: foo.sh
"#;
        let errors = check(yaml, "teams/engineering.yml");
        assert!(
            errors.is_empty(),
            "Expected no errors for valid team config but got: {:?}",
            errors
        );
    }

    #[test]
    fn test_valid_policy_lib_file() {
        let yaml = r#"
- name: "Test Policy"
  query: "SELECT 1;"
  platform: darwin
"#;
        let errors = check(yaml, "lib/policies/security.yml");
        assert!(
            errors.is_empty(),
            "Expected no errors for valid policy lib but got: {:?}",
            errors
        );
    }

    #[test]
    fn test_unknown_key_in_nested_context() {
        let yaml = r#"
org_settings:
  server_settings:
    server_url: https://example.com
    unknown_setting: true
"#;
        let errors = check(yaml, "default.yml");
        assert!(
            !errors.is_empty(),
            "Expected error for unknown key in server_settings"
        );
        assert!(
            errors[0].message.contains("unknown_setting")
                || errors[0].message.contains("Unknown key")
        );
    }

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("policis", "policies"), 1);
    }

    #[test]
    fn test_find_closest_key() {
        let candidates = &["policies", "queries", "labels", "controls"];
        assert_eq!(find_closest_key("policis", candidates), Some("policies"));
        assert_eq!(find_closest_key("queri", candidates), Some("queries"));
        assert_eq!(find_closest_key("zzzzzzzzzzz", candidates), None);
    }

    #[test]
    fn test_software_valid_keys() {
        let yaml = r#"
software:
  packages:
    - path: ../lib/software/firefox.yml
      self_service: true
  app_store_apps:
    - app_store_id: "12345"
  fleet_maintained_apps:
    - slug: slack/darwin
      self_service: true
"#;
        let errors = check(yaml, "default.yml");
        let software_errors: Vec<_> = errors
            .iter()
            .filter(|e| {
                e.message.contains("software")
                    || e.message.contains("self_service")
                    || e.message.contains("slug")
            })
            .collect();
        assert!(
            software_errors.is_empty(),
            "Valid software keys should not produce errors: {:?}",
            software_errors
        );
    }

    #[test]
    fn test_software_typo_detected() {
        let yaml = r#"
software:
  packages:
    - path: ../lib/software/firefox.yml
      self_servicae: true
      setupaaa_experience: true
"#;
        let errors = check(yaml, "default.yml");
        assert!(
            errors.iter().any(|e| e.message.contains("self_servicae")),
            "Should flag typo 'self_servicae': {:?}",
            errors
        );
        assert!(
            errors
                .iter()
                .any(|e| e.message.contains("setupaaa_experience")),
            "Should flag typo 'setupaaa_experience': {:?}",
            errors
        );
    }

    #[test]
    fn test_boolean_value_validation() {
        // Valid booleans should pass
        let yaml = r#"
software:
  packages:
    - path: ../lib/software/firefox.yml
      self_service: true
      setup_experience: false
"#;
        let errors = check(yaml, "default.yml");
        let bool_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.message.contains("expects a boolean"))
            .collect();
        assert!(
            bool_errors.is_empty(),
            "Valid booleans should not produce errors: {:?}",
            bool_errors
        );

        // YAML 1.1 booleans (yes/no) should also pass
        let yaml_yn = r#"
software:
  packages:
    - path: ../lib/software/firefox.yml
      self_service: yes
      setup_experience: no
"#;
        let errors_yn = check(yaml_yn, "default.yml");
        let bool_errors_yn: Vec<_> = errors_yn
            .iter()
            .filter(|e| e.message.contains("expects a boolean"))
            .collect();
        assert!(
            bool_errors_yn.is_empty(),
            "YAML 1.1 yes/no should be valid booleans: {:?}",
            bool_errors_yn
        );

        // Invalid value should be flagged
        let yaml_bad = r#"
software:
  packages:
    - path: ../lib/software/firefox.yml
      self_service: banana
"#;
        let errors_bad = check(yaml_bad, "default.yml");
        assert!(
            errors_bad
                .iter()
                .any(|e| e.message.contains("self_service")
                    && e.message.contains("expects a boolean")),
            "String 'banana' should be flagged as non-boolean: {:?}",
            errors_bad
        );
    }

    #[test]
    fn test_wrong_indentation_org_settings() {
        // org_info and org_name are siblings of server_url (wrong indent)
        // They should be flagged as misplaced
        let yaml = r#"
org_settings:
  server_settings:
    server_url: https://example.com
    org_info:
    org_name: CNG Fleet
"#;
        let errors = check(yaml, "default.yml");
        eprintln!("Errors: {:#?}", errors);
        assert!(
            errors.iter().any(|e| e.message.contains("org_info")),
            "Should flag 'org_info' as misplaced under server_settings: {:?}",
            errors
        );
        assert!(
            errors.iter().any(|e| e.message.contains("org_name")),
            "Should flag 'org_name' as misplaced under server_settings: {:?}",
            errors
        );
    }

    #[test]
    fn test_different_file_types_get_correct_schemas() {
        // Policy lib file should accept array items
        let policy_yaml = r#"
- name: "Test"
  query: "SELECT 1;"
"#;
        assert!(check(policy_yaml, "lib/policies/test.yml").is_empty());

        // Query lib file should accept array items
        let query_yaml = r#"
- name: "Test"
  query: "SELECT 1;"
  interval: 300
"#;
        assert!(check(query_yaml, "lib/queries/test.yml").is_empty());

        // Label lib file should accept array items
        let label_yaml = r#"
- name: "Test Label"
  query: "SELECT 1;"
  label_membership_type: dynamic
"#;
        assert!(check(label_yaml, "lib/labels/test.yml").is_empty());
    }

    // ---- Webhook settings per-fleet vs org-level ----

    #[test]
    fn test_per_fleet_failing_policies_webhook_accepted() {
        // Per Fleet docs (yaml-files.md:1102), failing_policies_webhook can be
        // configured per-fleet under `settings.webhook_settings`.
        let yaml = r#"
name: Workstations
settings:
  webhook_settings:
    failing_policies_webhook:
      enable_failing_policies_webhook: true
      destination_url: https://example.org/hook
      host_batch_size: 0
"#;
        let errors = check(yaml, "fleets/workstations.yml");
        assert!(
            errors.is_empty(),
            "Per-fleet failing_policies_webhook should be accepted: {:?}",
            errors
        );
    }

    #[test]
    fn test_per_fleet_activities_and_host_status_webhook_accepted() {
        let yaml = r#"
name: Workstations
settings:
  webhook_settings:
    activities_webhook:
      enable_activities_webhook: true
      destination_url: https://example.org/a
    host_status_webhook:
      enable_host_status_webhook: true
      destination_url: https://example.org/h
      days_count: 7
      host_percentage: 25
"#;
        let errors = check(yaml, "fleets/workstations.yml");
        assert!(
            errors.is_empty(),
            "Per-fleet activities_webhook and host_status_webhook should be accepted: {:?}",
            errors
        );
    }

    #[test]
    fn test_per_fleet_vulnerabilities_webhook_rejected() {
        // Per Fleet docs (yaml-files.md:1151): vulnerabilities_webhook is org-only.
        let yaml = r#"
name: Workstations
settings:
  webhook_settings:
    vulnerabilities_webhook:
      enable_vulnerabilities_webhook: true
      destination_url: https://example.org/v
"#;
        let errors = check(yaml, "fleets/workstations.yml");
        assert!(
            errors.iter().any(|e| e.message.contains("vulnerabilities_webhook")),
            "Expected vulnerabilities_webhook to be flagged as unknown under per-fleet settings, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_policy_webhooks_and_tickets_enabled_accepted() {
        // Per Fleet CHANGELOG: "Implemented `webhooks_and_tickets_enabled`
        // flag for policies in GitOps." Cross-validated against
        // testdata/generateGitops/expectedTeamPolicies.yaml:13.
        let yaml = r#"
- name: macOS - All available software updates installed
  query: SELECT 1
  platform: darwin
  webhooks_and_tickets_enabled: true
"#;
        let errors = check(yaml, "platforms/macos/policies/all-software-updates-installed.yml");
        assert!(
            errors.is_empty(),
            "webhooks_and_tickets_enabled is a valid policy field: {:?}",
            errors
        );
    }

    #[test]
    fn test_policy_install_software_app_store_id_accepted() {
        // Per gitops.go:231-236, install_software supports app_store_id.
        // Cross-validated against expectedTeamPolicies.yaml:30-31.
        let yaml = r#"
- name: VPP install
  query: SELECT 1
  install_software:
    app_store_id: "1234567890"
"#;
        let errors = check(yaml, "policies/test.yml");
        assert!(
            errors.is_empty(),
            "install_software.app_store_id is valid: {:?}",
            errors
        );
    }

    #[test]
    fn test_policy_team_field_accepted() {
        let yaml = "policies:\n  - name: Test\n    query: SELECT 1\n    team: Engineering\n";
        let errors = check(yaml, "default.yml");
        assert!(errors.is_empty(), "team is a valid policy field: {:?}", errors);
    }

    #[test]
    fn test_org_vulnerabilities_webhook_accepted() {
        let yaml = r#"
org_settings:
  webhook_settings:
    vulnerabilities_webhook:
      enable_vulnerabilities_webhook: true
      destination_url: https://example.org/v
      host_batch_size: 0
"#;
        let errors = check(yaml, "default.yml");
        assert!(
            errors.is_empty(),
            "Org-level vulnerabilities_webhook should be accepted: {:?}",
            errors
        );
    }

    // -- Issue #3: agent_options.script_execution_timeout + extensions --

    #[test]
    fn test_agent_options_script_execution_timeout_accepted() {
        // Regression for issue #3. Source: server/fleet/agent_options.go
        let yaml = r#"
agent_options:
  script_execution_timeout: 18000
  config:
    options:
      logger_plugin: filesystem
"#;
        let errors = check(yaml, "default.yml");
        assert!(
            errors.is_empty(),
            "script_execution_timeout is a valid agent_options key: {:?}",
            errors
        );
    }

    #[test]
    fn test_agent_options_extensions_accepted() {
        let yaml = "agent_options:\n  extensions:\n    plat: example\n";
        let errors = check(yaml, "default.yml");
        assert!(
            errors.is_empty(),
            "extensions is a valid agent_options key: {:?}",
            errors
        );
    }
}
