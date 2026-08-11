//! Shared YAML walking utilities for lint rules.
//!
//! Provides helpers for parsing and navigating raw `serde_yaml::Value` trees,
//! used by rules that need to inspect fields across both typed and untyped sections.

use super::error::Span;
use serde_yaml::{Mapping, Value};

/// Parse YAML source, returning None on failure (rules skip unparseable files).
pub(crate) fn parse_yaml(source: &str) -> Option<Value> {
    serde_yaml::from_str(source).ok()
}

/// Keys whose string value Fleet resolves as a repo-relative file path.
///
/// Derived from fleet v4.89.2 `pkg/spec/gitops.go` — every field passed through
/// `absPathFrom()` at apply time, in `ResolveFilePathsAbs` (the
/// `controls.macos_setup` / `setup_experience` fields) and
/// `reanchorOrgSettingsPaths` (the `org_settings` ones). Both spellings of the
/// renamed keys are listed, because Fleet still accepts the old ones.
///
/// `bootstrap_package` / `macos_bootstrap_package` are deliberately absent:
/// Fleet never anchors them, because they carry a URL. Treating them as paths
/// would invent an error for every correct config.
pub(crate) const PATH_BEARING_KEYS: &[&str] = &[
    // Section items and package references — the original two cases.
    "path",
    "package_path",
    // controls.setup_experience (a.k.a. controls.macos_setup)
    "apple_setup_assistant",
    "macos_setup_assistant",
    "macos_script",
    // org_settings
    "end_user_license_agreement",
    "org_logo_path_light_mode",
    "org_logo_path_dark_mode",
];

/// Path-bearing only inside the setup-experience section.
///
/// `script` is the pre-rename spelling of `macos_script`. Unlike the names in
/// [`PATH_BEARING_KEYS`] it is far too generic to match document-wide, so it is
/// honoured only where Fleet defines it.
const SETUP_SCOPED_PATH_KEYS: &[&str] = &["script"];

/// True for the mapping that holds the setup-experience fields, under either
/// the current name or the pre-rename one.
fn is_setup_section(dotted: &str) -> bool {
    dotted.ends_with("setup_experience") || dotted.ends_with("macos_setup")
}

/// True for any key that names a file. Used when locating a value in the raw
/// source, where the enclosing section is not known — so the setup-scoped
/// `script` is accepted here too; the value has already been collected by
/// [`collect_path_values`], which did apply the scope.
fn is_path_key(key: &str) -> bool {
    PATH_BEARING_KEYS.contains(&key) || SETUP_SCOPED_PATH_KEYS.contains(&key)
}

/// Every value in `doc` that Fleet will resolve as a repo-relative file path.
///
/// THE shared answer to "which strings in this document name a file?", used by
/// `path-exists` (does the target exist?) and by `workspace::referenced_by`
/// (which files are wired?). Keeping one list means a key cannot be visible to
/// one and invisible to the other — the split that made live ADE enrollment
/// profiles report as orphaned while a typo in the same key went unreported.
pub(crate) fn collect_path_values(doc: &Value) -> Vec<String> {
    let mut out = Vec::new();
    walk_mappings_with_path(doc, &mut |dotted, map| {
        let scoped = is_setup_section(dotted);
        let keys = PATH_BEARING_KEYS
            .iter()
            .chain(SETUP_SCOPED_PATH_KEYS.iter().filter(|_| scoped));
        for key in keys {
            if let Some(Value::String(s)) = map.get(Value::String((*key).to_string())) {
                out.push(s.clone());
            }
        }
    });
    out
}

/// Depth-first visit of every mapping in the tree (mappings inside sequences
/// included). THE shared recursive walker — rules that only need to inspect
/// each mapping (e.g. "does it have a `path:` key?") use this instead of
/// rolling their own recursion.
pub(crate) fn walk_mappings(value: &Value, visit: &mut impl FnMut(&Mapping)) {
    match value {
        Value::Mapping(map) => {
            visit(map);
            for (_, v) in map {
                walk_mappings(v, visit);
            }
        }
        Value::Sequence(seq) => {
            for item in seq {
                walk_mappings(item, visit);
            }
        }
        _ => {}
    }
}

/// Like [`walk_mappings`], but each visit also receives the dotted context
/// path of the mapping (sequence hops appear as `[idx]`, e.g.
/// `policies[0].install_software`). Used by rules that key their checks on
/// where a mapping sits (deprecations).
pub(crate) fn walk_mappings_with_path(value: &Value, visit: &mut impl FnMut(&str, &Mapping)) {
    fn inner(value: &Value, path: &str, visit: &mut impl FnMut(&str, &Mapping)) {
        match value {
            Value::Mapping(map) => {
                visit(path, map);
                for (key, v) in map {
                    let Some(key_str) = key.as_str() else { continue };
                    let child_path = if path.is_empty() {
                        key_str.to_string()
                    } else {
                        format!("{}.{}", path, key_str)
                    };
                    inner(v, &child_path, visit);
                }
            }
            Value::Sequence(seq) => {
                for (idx, item) in seq.iter().enumerate() {
                    let item_path = format!("{}[{}]", path, idx);
                    inner(item, &item_path, visit);
                }
            }
            _ => {}
        }
    }
    inner(value, "", visit);
}

/// Locate a specific `path:` value in the source text, spanning the value.
pub(crate) fn find_path_value_line(source: &str, path_value: &str) -> Option<Span> {
    for (line_idx, line) in source.lines().enumerate() {
        // Match `<key>: <value>` or `- <key>: <value>` for any key that names a
        // file. Keyed on the whole key, not a prefix, so `package_path:` is not
        // mistaken for `path:`.
        let trimmed = line.trim_start();
        let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed).trim_start();
        if let Some((key, rest)) = trimmed.split_once(':') {
            if !is_path_key(key.trim()) {
                continue;
            }
            let rest = rest.trim();
            // Strip optional quotes
            let unquoted = rest
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('\'');
            if unquoted == path_value {
                // Point to the start of the value
                if let Some(val_offset) = line.find(path_value) {
                    return Some(Span::token(
                        line_idx + 1,
                        val_offset + 1,
                        path_value.len(),
                    ));
                }
                // Fall back to after the colon
                if let Some(colon_pos) = line.find(':') {
                    return Some(Span::token(line_idx + 1, colon_pos + 3, path_value.len()));
                }
                return Some(Span::line(line_idx + 1));
            }
        }
    }
    None
}

/// Walk a `serde_yaml::Value` tree along a path of keys (e.g., `["software", "packages"]`)
/// and return all array items found at that path.
pub(crate) fn collect_items_at_path<'a>(root: &'a Value, path: &[&str]) -> Vec<&'a Value> {
    let mut current = root;

    for &key in path {
        match current {
            Value::Mapping(map) => match map.get(Value::String(key.to_string())) {
                Some(v) => current = v,
                None => return Vec::new(),
            },
            _ => return Vec::new(),
        }
    }

    // The final node should be a sequence
    match current {
        Value::Sequence(seq) => seq.iter().collect(),
        _ => Vec::new(),
    }
}

/// Check if a `serde_yaml::Value::Mapping` contains a given key.
pub(crate) fn mapping_has_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Mapping(map) => map.contains_key(Value::String(key.to_string())),
        _ => false,
    }
}

/// Get a string value from a mapping by key.
pub(crate) fn mapping_get_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    match value {
        Value::Mapping(map) => map
            .get(Value::String(key.to_string()))
            .and_then(|v| v.as_str()),
        _ => None,
    }
}

/// Get a display name for an item (tries name, slug, app_store_id, path in order).
pub(crate) fn item_display_name(value: &Value) -> String {
    for key in &["name", "slug", "app_store_id", "path"] {
        if let Some(s) = mapping_get_str(value, key) {
            return s.to_string();
        }
    }
    "unnamed".to_string()
}

/// Find the 1-indexed line number of a YAML key in source text.
/// Searches for the key at the appropriate indentation level.
/// Returns the first match after `after_line` (0 = search from start).
pub(crate) fn find_key_line(source: &str, key: &str, after_line: usize) -> Option<usize> {
    let pattern = format!("{}:", key);
    for (idx, line) in source.lines().enumerate() {
        if idx < after_line {
            continue;
        }
        let trimmed = line.trim().trim_start_matches('-').trim();
        if trimmed.starts_with(&pattern) {
            return Some(idx + 1); // 1-indexed
        }
    }
    None
}

/// Span covering the KEY token of `key:`, for diagnostics about the key
/// itself (unknown key, misplaced key, deprecated spelling).
///
/// Prefer this over [`find_key_line`] plus a hardcoded column. A column of 1
/// puts the caret under the indentation, which reads as "something is wrong
/// with this whole line" when the point is a single token.
pub(crate) fn find_key_span(source: &str, key: &str, after_line: usize) -> Option<Span> {
    let pattern = format!("{key}:");
    for (idx, line) in source.lines().enumerate() {
        if idx < after_line {
            continue;
        }
        let trimmed = line.trim().trim_start_matches('-').trim();
        if trimmed.starts_with(&pattern) {
            // `find` is safe here: `trimmed` came from `line`, so the key is
            // present. Fall back to a whole-line span rather than guessing.
            return match line.find(key) {
                Some(col) => Some(Span::token(idx + 1, col + 1, key.chars().count())),
                None => Some(Span::line(idx + 1)),
            };
        }
    }
    None
}

/// Span covering the VALUE of `key: value`, for diagnostics about the value
/// (invalid platform, unknown label, bad interval).
///
/// Matches the value exactly after unquoting, so `platform: "darwin"` and
/// `platform: darwin` both resolve, and a different value on another line
/// does not.
pub(crate) fn find_value_span(source: &str, key: &str, value: &str) -> Option<Span> {
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim().trim_start_matches('-').trim_start();
        let Some(rest) = trimmed.strip_prefix(key).and_then(|r| r.trim_start().strip_prefix(':'))
        else {
            continue;
        };
        let raw = rest.trim();
        if raw.trim_matches('"').trim_matches('\'') != value {
            continue;
        }
        // Point at the value as written, skipping any quote so the caret
        // covers the text the user would edit.
        return match line.rfind(value) {
            Some(col) => Some(Span::token(idx + 1, col + 1, value.chars().count())),
            None => Some(Span::line(idx + 1)),
        };
    }
    None
}

/// True if `content` carries no usable data: only blank lines, and — when
/// `yaml_comments` is set — `#` comment lines. A YAML file that is empty,
/// whitespace-only, or comment-only parses to null, which Fleet rejects where
/// it expects a document (a software/policy/profile referenced by `path:`).
pub(crate) fn is_effectively_empty(content: &str, yaml_comments: bool) -> bool {
    for line in content.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if yaml_comments && t.starts_with('#') {
            continue;
        }
        return false; // found real content
    }
    true
}

/// Get all string values from an array field within a mapping.
pub(crate) fn mapping_get_string_array<'a>(value: &'a Value, key: &str) -> Vec<&'a str> {
    match value {
        Value::Mapping(map) => map
            .get(Value::String(key.to_string()))
            .and_then(|v| v.as_sequence())
            .map(|seq| seq.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_items_at_path() {
        let yaml: Value = serde_yaml::from_str(
            "software:\n  packages:\n    - path: foo.yml\n    - path: bar.yml\n",
        )
        .unwrap();
        let items = collect_items_at_path(&yaml, &["software", "packages"]);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_collect_items_missing_path() {
        let yaml: Value = serde_yaml::from_str("policies:\n  - name: test\n").unwrap();
        let items = collect_items_at_path(&yaml, &["software", "packages"]);
        assert!(items.is_empty());
    }

    #[test]
    fn test_mapping_helpers() {
        let yaml: Value =
            serde_yaml::from_str("name: test\nplatform: darwin\ncritical: true\n").unwrap();
        assert!(mapping_has_key(&yaml, "name"));
        assert!(!mapping_has_key(&yaml, "missing"));
        assert_eq!(mapping_get_str(&yaml, "name"), Some("test"));
        assert_eq!(mapping_get_str(&yaml, "missing"), None);
    }

    #[test]
    fn test_item_display_name() {
        let yaml: Value = serde_yaml::from_str("name: My Policy\nquery: SELECT 1\n").unwrap();
        assert_eq!(item_display_name(&yaml), "My Policy");

        let yaml: Value = serde_yaml::from_str("slug: firefox/darwin\n").unwrap();
        assert_eq!(item_display_name(&yaml), "firefox/darwin");

        let yaml: Value = serde_yaml::from_str("critical: true\n").unwrap();
        assert_eq!(item_display_name(&yaml), "unnamed");
    }

    #[test]
    fn test_find_key_line() {
        let source = "policies:\n  - name: test\n    platform: darwin\n";
        assert_eq!(find_key_line(source, "name", 0), Some(2));
        assert_eq!(find_key_line(source, "platform", 0), Some(3));
        assert_eq!(find_key_line(source, "missing", 0), None);
    }
}

#[cfg(test)]
mod span_tests {
    use super::*;

    #[test]
    fn value_span_points_at_the_value() {
        let src = "policies:\n  - name: Test\n    platform: darwn\n";
        let span = find_value_span(src, "platform", "darwn").expect("found");
        assert_eq!((span.line, span.column, span.len), (3, 15, 5));
        // Column 15 is exactly where `darwn` starts on that line.
        assert_eq!(&src.lines().nth(2).unwrap()[span.column - 1..], "darwn");
    }

    #[test]
    fn value_span_matches_through_quotes_and_skips_other_values() {
        let src = "a:\n  platform: linux\n  platform: \"darwin\"\n";
        let span = find_value_span(src, "platform", "darwin").expect("found");
        assert_eq!(span.line, 3, "must skip the line whose value differs");
        assert_eq!(span.len, 6);
    }

    #[test]
    fn key_span_covers_the_key_token() {
        let src = "policies:\n  - name: Test\n    polcies: x\n";
        let span = find_key_span(src, "polcies", 0).expect("found");
        assert_eq!((span.line, span.column, span.len), (3, 5, 7));
    }

    /// The control: an absent key or value must not produce a confident span
    /// pointing at something arbitrary.
    #[test]
    fn absent_key_or_value_yields_no_span() {
        let src = "policies:\n  - name: Test\n";
        assert!(find_key_span(src, "platform", 0).is_none());
        assert!(find_value_span(src, "platform", "darwin").is_none());
        assert!(find_value_span(src, "name", "Other").is_none());
    }

    /// `apple_setup_assistant` was invisible to both `path-exists` and
    /// `workspace::referenced_by`, which collected only `path`/`paths`. Live
    /// ADE enrollment profiles were reported as orphaned, and a typo in the
    /// key was never reported at all — it surfaced at `fleetctl gitops` time,
    /// on the one profile that cannot be re-applied after enrollment.
    #[test]
    fn collects_every_key_fleet_resolves_as_a_path() {
        let src = r#"
controls:
  setup_experience:
    apple_setup_assistant: ./enroll/default.dep.json
    macos_script: ./scripts/setup.sh
    script: ./scripts/old-spelling.sh
    bootstrap_package: https://example.com/pkg.pkg
  apple_settings:
    configuration_profiles:
      - path: ./profiles/wifi.mobileconfig
software:
  packages:
    - package_path: ./software/app.yml
"#;
        let found = collect_path_values(&parse_yaml(src).unwrap());

        for expected in [
            "./enroll/default.dep.json",
            "./scripts/setup.sh",
            "./scripts/old-spelling.sh",
            "./profiles/wifi.mobileconfig",
            "./software/app.yml",
        ] {
            assert!(found.iter().any(|v| v == expected), "missed {expected}");
        }

        // Fleet never anchors bootstrap_package — it is a URL. Collecting it
        // would make `path-exists` invent an error for a correct config.
        assert!(
            !found.iter().any(|v| v.contains("example.com")),
            "bootstrap_package is a URL and must not be treated as a path"
        );
    }

    /// `script` is honoured only where Fleet defines it. The bare name is too
    /// generic to match document-wide.
    #[test]
    fn bare_script_key_is_scoped_to_the_setup_section() {
        let outside = r#"
policies:
  - name: P
    script: not-a-setup-path
"#;
        let found = collect_path_values(&parse_yaml(outside).unwrap());
        assert!(
            found.is_empty(),
            "`script` outside the setup section must be ignored, got {found:?}"
        );
    }

    /// The span helper keys on the whole key, so `package_path:` is not
    /// mistaken for `path:`.
    #[test]
    fn finds_the_value_span_for_a_non_path_key() {
        let src = "controls:\n  setup_experience:\n    apple_setup_assistant: ./a.dep.json\n";
        let span = find_path_value_line(src, "./a.dep.json").expect("span");
        assert_eq!(span.line, 3);
        assert_eq!(span.len, "./a.dep.json".len());
    }
}
