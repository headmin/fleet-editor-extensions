//! YAML overlay merge — combine a base manifest with an environment overlay.
//!
//! Mirrors the algorithm fleet-plan uses (see `internal/merge/merge.go`):
//! parse both documents as YAML mappings, then deep-merge — overlay values
//! win for everything except nested mappings, which recurse. Equivalent to:
//!
//!   yq eval-all '. as $item ireduce ({}; . *+ $item)' base.yml overlay.yml
//!
//! Scope is intentionally narrow: this is for `default.yml`-style global
//! config that Fleet GitOps repos typically split into `base.yml` plus
//! `environments/{prod,staging}.yml`. Per-team overlay support is out of
//! scope (Fleet GitOps doesn't recommend it and fleet-plan doesn't do it).

use serde_yaml::{Mapping, Value};
use std::fmt;

/// Reasons a merge can fail. Surfaced via CLI errors with enough context
/// that users can fix the offending file without re-reading the linter.
#[derive(Debug)]
pub enum OverlayError {
    /// The base file failed to parse as YAML.
    ParseBase(String),
    /// The overlay file failed to parse as YAML.
    ParseOverlay(String),
    /// The base file parsed but isn't a mapping (e.g. it's a list or scalar).
    /// Fleet GitOps configs are always top-level mappings.
    BaseNotMap,
    /// The overlay parsed to non-map content. Silently producing only the
    /// base (which is what naive merges do) is worse than erroring loudly.
    OverlayNotMap,
    /// Serializing the merged result back to YAML failed (unlikely; surfaced
    /// for completeness so callers don't need to map a separate error).
    Serialize(String),
}

impl fmt::Display for OverlayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OverlayError::ParseBase(m) => write!(f, "parsing base YAML: {m}"),
            OverlayError::ParseOverlay(m) => write!(f, "parsing overlay YAML: {m}"),
            OverlayError::BaseNotMap => {
                write!(f, "base YAML is not a mapping (top-level must be key: value pairs)")
            }
            OverlayError::OverlayNotMap => {
                write!(f, "overlay YAML is not a mapping (top-level must be key: value pairs)")
            }
            OverlayError::Serialize(m) => write!(f, "serializing merged YAML: {m}"),
        }
    }
}

impl std::error::Error for OverlayError {}

/// Deep-merge two YAML documents. Returns the merged result as a YAML string.
///
/// Algorithm (matches fleet-plan):
/// - Both inputs must be mappings (or empty/null — treated as empty mapping).
/// - Nested mappings are merged recursively.
/// - All other values (scalars, sequences, mixed types) are overwritten by
///   the overlay. Lists are *replaced*, not concatenated — surprising the
///   first time you hit it, but it matches Fleet's apparent semantics and
///   fleet-plan's behavior.
pub fn merge_yaml(base: &str, overlay: &str) -> Result<String, OverlayError> {
    let base_val = parse_or_empty(base, OverlayError::ParseBase)?;
    let overlay_val = parse_or_empty(overlay, OverlayError::ParseOverlay)?;

    let mut base_map = match base_val {
        Value::Mapping(m) => m,
        Value::Null => Mapping::new(),
        _ => return Err(OverlayError::BaseNotMap),
    };

    let overlay_map = match overlay_val {
        Value::Mapping(m) => m,
        Value::Null => {
            // Empty overlay is a no-op — just re-serialize base.
            return serde_yaml::to_string(&Value::Mapping(base_map))
                .map_err(|e| OverlayError::Serialize(e.to_string()));
        }
        _ => return Err(OverlayError::OverlayNotMap),
    };

    deep_merge(&mut base_map, overlay_map);

    serde_yaml::to_string(&Value::Mapping(base_map))
        .map_err(|e| OverlayError::Serialize(e.to_string()))
}

/// Parse a YAML string. Empty/whitespace-only input yields Null so the
/// caller can treat it as "no document" without branching on `is_empty`.
fn parse_or_empty(
    s: &str,
    err_ctor: fn(String) -> OverlayError,
) -> Result<Value, OverlayError> {
    if s.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_yaml::from_str(s).map_err(|e| err_ctor(e.to_string()))
}

/// Merge `src` into `dst` in place. Recurses into nested mappings; all
/// other value types are overwritten by `src`. Mirrors fleet-plan's
/// `deepMerge` exactly — keep the algorithm bit-for-bit so users moving
/// between the two tools don't see different merge outputs.
fn deep_merge(dst: &mut Mapping, src: Mapping) {
    for (k, sv) in src {
        // `remove` + re-insert is the idiomatic way to take ownership of
        // a value out of a Mapping without fighting the borrow checker on
        // simultaneous mutable + immutable references during recursion.
        let merged = match (dst.remove(&k), sv) {
            (Some(Value::Mapping(mut dm)), Value::Mapping(sm)) => {
                deep_merge(&mut dm, sm);
                Value::Mapping(dm)
            }
            // Non-map (or missing) destination — overlay value wins.
            (_, sv) => sv,
        };
        dst.insert(k, merged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny helper — every test asserts on the merged YAML string, but
    /// parsing it back to a Value gives stable structural comparison
    /// regardless of key ordering or whitespace.
    fn parse(s: &str) -> Value {
        serde_yaml::from_str(s).expect("merged output must be valid YAML")
    }

    #[test]
    fn overlay_wins_on_scalar_conflict() {
        let base = "name: production\nreplicas: 1\n";
        let overlay = "replicas: 3\n";
        let out = merge_yaml(base, overlay).unwrap();
        let v = parse(&out);
        assert_eq!(v["name"], Value::from("production"));
        assert_eq!(v["replicas"], Value::from(3));
    }

    #[test]
    fn overlay_adds_new_top_level_keys() {
        let base = "a: 1\n";
        let overlay = "b: 2\n";
        let out = merge_yaml(base, overlay).unwrap();
        let v = parse(&out);
        assert_eq!(v["a"], Value::from(1));
        assert_eq!(v["b"], Value::from(2));
    }

    #[test]
    fn nested_maps_merge_recursively() {
        // Critical: the base's `agent_options.config.options` keys must
        // survive when the overlay only sets `agent_options.config.decorators`.
        // Naive (shallow) merge would lose them.
        let base = r#"
agent_options:
  config:
    options:
      schedule_splay_percent: 10
"#;
        let overlay = r#"
agent_options:
  config:
    decorators:
      load:
        - SELECT version FROM osquery_info
"#;
        let out = merge_yaml(base, overlay).unwrap();
        let v = parse(&out);
        assert_eq!(
            v["agent_options"]["config"]["options"]["schedule_splay_percent"],
            Value::from(10),
            "base nested key must survive merge"
        );
        assert!(
            v["agent_options"]["config"]["decorators"].is_mapping(),
            "overlay nested key must be present"
        );
    }

    #[test]
    fn lists_are_replaced_not_concatenated() {
        // This is the subtle one. Users may expect list concat; fleet-plan
        // and yq's deep-merge replace. Documented here as a regression test.
        let base = "fleets:\n  - alpha\n  - beta\n";
        let overlay = "fleets:\n  - gamma\n";
        let out = merge_yaml(base, overlay).unwrap();
        let v = parse(&out);
        let fleets = v["fleets"].as_sequence().unwrap();
        assert_eq!(fleets.len(), 1, "list must be REPLACED by overlay, not appended");
        assert_eq!(fleets[0], Value::from("gamma"));
    }

    #[test]
    fn overlay_overrides_type_change_scalar_to_map() {
        // Base has a scalar where overlay has a map — overlay wins,
        // base value is discarded entirely (no attempt to coerce).
        let base = "controls: disabled\n";
        let overlay = "controls:\n  scripts:\n    - id: 1\n";
        let out = merge_yaml(base, overlay).unwrap();
        let v = parse(&out);
        assert!(v["controls"].is_mapping());
    }

    #[test]
    fn empty_base_returns_overlay_unchanged() {
        let out = merge_yaml("", "a: 1\n").unwrap();
        let v = parse(&out);
        assert_eq!(v["a"], Value::from(1));
    }

    #[test]
    fn empty_overlay_returns_base_unchanged() {
        let out = merge_yaml("a: 1\n", "").unwrap();
        let v = parse(&out);
        assert_eq!(v["a"], Value::from(1));
    }

    #[test]
    fn both_empty_yields_empty_mapping() {
        let out = merge_yaml("", "").unwrap();
        // serde_yaml emits empty mappings as "{}" or similar; we just need
        // it to round-trip without error.
        let v: Value = serde_yaml::from_str(&out).unwrap();
        assert!(
            v.is_null() || v.is_mapping(),
            "empty merge should produce empty mapping or null, got {v:?}"
        );
    }

    #[test]
    fn base_not_a_mapping_is_an_error() {
        // A list at top level is invalid Fleet GitOps; surface a clear
        // error rather than silently producing the overlay.
        let result = merge_yaml("- one\n- two\n", "a: 1\n");
        assert!(matches!(result, Err(OverlayError::BaseNotMap)));
    }

    #[test]
    fn overlay_not_a_mapping_is_an_error() {
        // The fleet-plan team specifically calls this out — silently
        // producing only the base when the overlay is malformed (e.g. a
        // bare list or scalar) hides real user errors.
        let result = merge_yaml("a: 1\n", "- one\n");
        assert!(matches!(result, Err(OverlayError::OverlayNotMap)));
    }

    #[test]
    fn base_parse_error_surfaces_clear_message() {
        let result = merge_yaml("a: [unclosed\n", "b: 2\n");
        match result {
            Err(OverlayError::ParseBase(msg)) => assert!(!msg.is_empty()),
            other => panic!("expected ParseBase, got {other:?}"),
        }
    }

    #[test]
    fn overlay_parse_error_surfaces_clear_message() {
        let result = merge_yaml("a: 1\n", "b: [unclosed\n");
        match result {
            Err(OverlayError::ParseOverlay(msg)) => assert!(!msg.is_empty()),
            other => panic!("expected ParseOverlay, got {other:?}"),
        }
    }

    #[test]
    fn deep_merge_preserves_keys_at_multiple_levels() {
        // Three-level nesting — common in Fleet `agent_options`.
        let base = r#"
agent_options:
  config:
    options:
      a: 1
      b: 2
  overrides:
    platforms:
      darwin:
        x: 1
"#;
        let overlay = r#"
agent_options:
  config:
    options:
      b: 20
      c: 3
  overrides:
    platforms:
      darwin:
        y: 2
"#;
        let out = merge_yaml(base, overlay).unwrap();
        let v = parse(&out);
        let opts = &v["agent_options"]["config"]["options"];
        assert_eq!(opts["a"], Value::from(1), "base-only key survives");
        assert_eq!(opts["b"], Value::from(20), "conflict resolved by overlay");
        assert_eq!(opts["c"], Value::from(3), "overlay-only key present");
        let darwin = &v["agent_options"]["overrides"]["platforms"]["darwin"];
        assert_eq!(darwin["x"], Value::from(1));
        assert_eq!(darwin["y"], Value::from(2));
    }
}
