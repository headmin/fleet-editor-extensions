//! Detects a deprecated key and its replacement being present together.
//!
//! Fleet migrates deprecated GitOps keys to their new names before parsing.
//! When BOTH spellings are present in the same mapping it cannot choose, and
//! `GitOpsFromFile` fails outright:
//!
//! ```text
//! failed to process deprecated keys in file …/team.yml: cannot specify both
//! 'controls.macos_settings' (deprecated) and 'controls.apple_settings';
//! use only 'controls.apple_settings'
//! ```
//!
//! That is an apply failure, not a style issue, which is why this reports an
//! error. It was found by wiring a profile into a fleet file that still used
//! the deprecated spelling: `flint paths --unwired` writes the modern key, so
//! flint's own tooling could produce a file Fleet refuses to parse — and
//! `flint check` said nothing.
//!
//! # Why this mirrors Fleet's algorithm rather than approximating it
//!
//! The mappings are applied IN ORDER, and each one rewrites the document for
//! the next. `controls.macos_settings -> controls.apple_settings` runs before
//! `controls.apple_settings.custom_settings -> …configuration_profiles`, so a
//! file written entirely in the old spelling has its children examined under
//! the NEW parent name. Checking the original document against each path
//! independently would miss exactly that case. So the walk below applies the
//! renames to a working copy as it goes, the way Fleet does.

use super::codes;
use super::error::{LintError, Span};
use serde_yaml::{Mapping, Value};
use std::path::Path;

/// Fleet's `DeprecatedGitOpsKeyMappings`, transcribed from
/// `pkg/spec/gitops_deprecations.go` at v4.89.2 — the version the target
/// server runs.
///
/// ORDER IS SIGNIFICANT: parent renames precede their children, and Fleet
/// applies them top to bottom. Keep this list in the same order as the Go
/// source so the two stay comparable line by line.
///
/// The leaf-only entries at the end of Fleet's table (`team` -> `fleet`,
/// `query_id` -> `report_id`, and so on) are DELIBERATELY OMITTED. Fleet's own
/// comment says they exist only so `buildAliasRules` can derive renames for
/// serialized API output, and that they "are not gitops input keys". Treating
/// them as paths here would flag an ordinary `team:` field inside unrelated
/// config.
const MAPPINGS: &[(&str, &str)] = &[
    ("team_settings", "settings"),
    ("queries", "reports"),
    ("controls.macos_settings", "controls.apple_settings"),
    (
        "controls.apple_settings.custom_settings",
        "controls.apple_settings.configuration_profiles",
    ),
    (
        "controls.windows_settings.custom_settings",
        "controls.windows_settings.configuration_profiles",
    ),
    (
        "controls.android_settings.custom_settings",
        "controls.android_settings.configuration_profiles",
    ),
    ("controls.macos_setup", "controls.setup_experience"),
    (
        "controls.setup_experience.bootstrap_package",
        "controls.setup_experience.macos_bootstrap_package",
    ),
    (
        "controls.setup_experience.macos_setup_assistant",
        "controls.setup_experience.apple_setup_assistant",
    ),
    (
        "controls.setup_experience.enable_release_device_manually",
        "controls.setup_experience.apple_enable_release_device_manually",
    ),
    (
        "controls.setup_experience.script",
        "controls.setup_experience.macos_script",
    ),
    (
        "controls.setup_experience.manual_agent_install",
        "controls.setup_experience.macos_manual_agent_install",
    ),
    (
        "controls.setup_experience.enable_managed_local_account",
        "controls.setup_experience.enable_create_local_admin_account",
    ),
    (
        "org_settings.server_settings.live_query_disabled",
        "org_settings.server_settings.live_reporting_disabled",
    ),
    (
        "org_settings.server_settings.query_reports_disabled",
        "org_settings.server_settings.discard_reports_data",
    ),
    (
        "org_settings.server_settings.query_report_cap",
        "org_settings.server_settings.report_cap",
    ),
    (
        "org_settings.org_info.org_logo_url",
        "org_settings.org_info.org_logo_url_dark_mode",
    ),
    (
        "org_settings.org_info.org_logo_url_light_background",
        "org_settings.org_info.org_logo_url_light_mode",
    ),
    (
        "org_settings.mdm.apple_business_manager",
        "org_settings.mdm.apple_business",
    ),
    (
        "org_settings.mdm.apple_business[].macos_team",
        "org_settings.mdm.apple_business[].macos_fleet",
    ),
    (
        "org_settings.mdm.apple_business[].ios_team",
        "org_settings.mdm.apple_business[].ios_fleet",
    ),
    (
        "org_settings.mdm.apple_business[].ipados_team",
        "org_settings.mdm.apple_business[].ipados_fleet",
    ),
    (
        "org_settings.mdm.apple_business[].byod_team",
        "org_settings.mdm.apple_business[].byod_fleet",
    ),
    (
        "org_settings.mdm.volume_purchasing_program[].teams",
        "org_settings.mdm.volume_purchasing_program[].fleets",
    ),
];

/// One conflict: a deprecated path and its replacement both present.
pub struct Conflict {
    pub old_path: &'static str,
    pub new_path: &'static str,
    /// The leaf key name of the deprecated spelling, for locating it.
    pub old_leaf: &'static str,
}

/// Every deprecated/replacement pair present together in `doc`.
///
/// Mutates a clone as it walks — see the module docs on ordering.
pub fn conflicts(doc: &Value) -> Vec<Conflict> {
    let mut work = doc.clone();
    let mut out = Vec::new();
    for (old, new) in MAPPINGS {
        let old_parts: Vec<&str> = old.split('.').collect();
        let new_parts: Vec<&str> = new.split('.').collect();
        migrate(&mut work, &old_parts, &new_parts, old, new, &mut out);
    }
    out
}

/// Mirror of Fleet's `migrateKeyPathRecursive`.
fn migrate(
    node: &mut Value,
    old_parts: &[&str],
    new_parts: &[&str],
    full_old: &'static str,
    full_new: &'static str,
    out: &mut Vec<Conflict>,
) {
    let (Some(old_key), Some(new_key)) = (old_parts.first(), new_parts.first()) else {
        return;
    };
    let Value::Mapping(map) = node else { return };

    // `foo[]` — iterate array elements with the remaining path.
    if let Some(trimmed) = old_key.strip_suffix("[]") {
        let Some(Value::Sequence(items)) = map.get_mut(Value::from(trimmed)) else {
            return;
        };
        for item in items.iter_mut() {
            migrate(item, &old_parts[1..], &new_parts[1..], full_old, full_new, out);
        }
        return;
    }

    if old_parts.len() == 1 {
        migrate_leaf(map, old_key, new_key, full_old, full_new, out);
        return;
    }

    let Some(nested) = map.get_mut(Value::from(*old_key)) else {
        return;
    };
    migrate(nested, &old_parts[1..], &new_parts[1..], full_old, full_new, out);
}

/// Mirror of Fleet's `migrateLeafKey`: both present is the error; otherwise
/// the value moves to the new key so later mappings see the migrated shape.
fn migrate_leaf(
    map: &mut Mapping,
    old_key: &str,
    new_key: &str,
    full_old: &'static str,
    full_new: &'static str,
    out: &mut Vec<Conflict>,
) {
    let old_k = Value::from(old_key);
    let new_k = Value::from(new_key);
    if !map.contains_key(&old_k) {
        return;
    }
    if map.contains_key(&new_k) {
        out.push(Conflict {
            old_path: full_old,
            new_path: full_new,
            // `old_key` is a slice of `full_old`, which is 'static.
            old_leaf: full_old.rsplit('.').next().unwrap_or(full_old),
        });
        return;
    }
    if let Some(v) = map.remove(&old_k) {
        map.insert(new_k, v);
    }
}

/// The deprecated spelling of a leaf key, derived from [`MAPPINGS`] so the two
/// cannot drift. `configuration_profiles` -> `custom_settings`, and so on.
fn deprecated_leaf_of(modern_leaf: &str) -> Option<&'static str> {
    MAPPINGS.iter().find_map(|(old, new)| {
        let new_leaf = new.rsplit('.').next()?;
        (new_leaf == modern_leaf).then(|| old.rsplit('.').next())?
    })
}

/// Rewrite a modern section path to the spelling `doc` already uses.
///
/// Wiring an artifact into a file that has not been migrated must not write
/// the modern key alongside the deprecated one — that is precisely the
/// conflict this module reports, and `flint paths --unwired` used to create
/// it. Writing into the section the file already has keeps the file
/// parseable; the separate `deprecated-keys` warning still tells the author
/// to migrate.
///
/// Only substitutes a deprecated spelling when it is ACTUALLY PRESENT. A file
/// with neither key gets the modern name, so new files never start out
/// deprecated.
pub fn section_path_for(doc: &Value, modern_path: &str) -> String {
    let mut node = doc;
    let mut out: Vec<String> = Vec::new();

    for segment in modern_path.split('.') {
        let Value::Mapping(map) = node else {
            // Cannot inspect further; keep the rest as-is.
            out.push(segment.to_string());
            continue;
        };
        let chosen = if map.contains_key(Value::from(segment)) {
            segment.to_string()
        } else if let Some(dep) = deprecated_leaf_of(segment) {
            if map.contains_key(Value::from(dep)) {
                dep.to_string()
            } else {
                segment.to_string()
            }
        } else {
            segment.to_string()
        };
        node = map.get(Value::from(chosen.as_str())).unwrap_or(&Value::Null);
        out.push(chosen);
    }
    out.join(".")
}

/// Build diagnostics for a parsed document.
pub fn check(source: &str, file: &Path) -> Vec<LintError> {
    let Ok(doc) = serde_yaml::from_str::<Value>(source) else {
        return Vec::new();
    };
    conflicts(&doc)
        .into_iter()
        .map(|c| {
            let mut err = LintError::error(
                format!(
                    "'{}' (deprecated) and '{}' are both present — Fleet cannot migrate the \
                     deprecated key and rejects the file",
                    c.old_path, c.new_path
                ),
                file,
            )
            .with_rule_code(codes::DEPRECATED_KEYS)
            .with_help(format!(
                "Remove '{}' and keep only '{}'. `fleetctl gitops` fails with \
                 \"cannot specify both\" before applying anything.",
                c.old_path, c.new_path
            ));
            if let Some(span) = super::yaml_utils::find_key_span(source, c.old_leaf, 0) {
                err = err.with_span(span);
            } else {
                err = err.with_span(Span::line(1));
            }
            err
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(src: &str) -> Vec<String> {
        check(src, Path::new("t.yml"))
            .into_iter()
            .map(|e| e.message)
            .collect()
    }

    /// The case that started this: `paths --unwired` writes the modern key
    /// into a file still using the deprecated one. Verified against Fleet
    /// v4.89.2, which fails the parse with "cannot specify both".
    #[test]
    fn both_spellings_of_a_renamed_parent_is_an_error() {
        let src = "name: T\ncontrols:\n  macos_settings:\n    custom_settings:\n      - path: a\n  apple_settings:\n    configuration_profiles:\n      - path: b\n";
        let found = msgs(src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("controls.macos_settings"), "{found:?}");
        assert!(found[0].contains("controls.apple_settings"), "{found:?}");
    }

    /// ORDERING. A file written entirely in the old spelling has its children
    /// examined under the NEW parent, because Fleet renames the parent first.
    /// Checking the original document path-by-path would miss this entirely.
    #[test]
    fn conflict_is_found_after_the_parent_rename_is_applied() {
        let src = "controls:\n  macos_settings:\n    custom_settings:\n      - path: a\n    configuration_profiles:\n      - path: b\n";
        let found = msgs(src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(
            found[0].contains("controls.apple_settings.custom_settings"),
            "the path is reported under the migrated parent: {found:?}"
        );
    }

    /// THE CONTROL. Using only the deprecated spelling is legal — Fleet
    /// migrates it and continues. If this fired, every repo mid-migration
    /// would be blocked, which is the false-positive class that has bitten
    /// this project repeatedly.
    #[test]
    fn deprecated_spelling_alone_is_not_a_conflict() {
        let src = "team_settings:\n  a: 1\ncontrols:\n  macos_settings:\n    custom_settings:\n      - path: a\n";
        assert!(msgs(src).is_empty(), "{:?}", msgs(src));
    }

    /// And the modern spelling alone, obviously.
    #[test]
    fn modern_spelling_alone_is_not_a_conflict() {
        let src = "settings:\n  a: 1\ncontrols:\n  apple_settings:\n    configuration_profiles:\n      - path: a\n";
        assert!(msgs(src).is_empty(), "{:?}", msgs(src));
    }

    #[test]
    fn top_level_pairs_are_detected() {
        assert_eq!(msgs("team_settings: {}\nsettings: {}\n").len(), 1);
        assert_eq!(msgs("queries: []\nreports: []\n").len(), 1);
    }

    /// Array paths (`apple_business[]`) are walked per element.
    #[test]
    fn conflicts_inside_array_elements_are_detected() {
        let src = "org_settings:\n  mdm:\n    apple_business:\n      - macos_team: A\n        macos_fleet: B\n";
        let found = msgs(src);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].contains("macos_team"), "{found:?}");
    }

    /// The leaf-only API aliases in Fleet's table are not GitOps input keys.
    /// A `team:` field elsewhere must not be treated as a deprecated path, or
    /// ordinary config starts failing.
    #[test]
    fn api_only_aliases_are_not_treated_as_gitops_paths() {
        let src = "policies:\n  - name: p\n    team: Workstations\n    fleet: Workstations\n";
        assert!(msgs(src).is_empty(), "{:?}", msgs(src));
    }

    // --- section_path_for -------------------------------------------------

    #[test]
    fn wiring_uses_the_deprecated_spelling_a_file_already_has() {
        let doc: Value = serde_yaml::from_str(
            "name: T\ncontrols:\n  macos_settings:\n    custom_settings:\n      - path: a\n",
        )
        .unwrap();
        assert_eq!(
            section_path_for(&doc, "controls.apple_settings.configuration_profiles"),
            "controls.macos_settings.custom_settings",
            "must not add the modern key beside the deprecated one"
        );
    }

    #[test]
    fn wiring_uses_modern_names_for_a_migrated_file() {
        let doc: Value = serde_yaml::from_str(
            "controls:\n  apple_settings:\n    configuration_profiles:\n      - path: a\n",
        )
        .unwrap();
        assert_eq!(
            section_path_for(&doc, "controls.apple_settings.configuration_profiles"),
            "controls.apple_settings.configuration_profiles"
        );
    }

    /// A file with NEITHER spelling gets the modern one — new sections must
    /// not be created already-deprecated.
    #[test]
    fn wiring_a_fresh_file_uses_modern_names() {
        let doc: Value = serde_yaml::from_str("name: T\n").unwrap();
        assert_eq!(
            section_path_for(&doc, "controls.apple_settings.configuration_profiles"),
            "controls.apple_settings.configuration_profiles"
        );
    }

    /// The end-to-end property: whatever section is chosen, wiring into it
    /// must not produce a document Fleet rejects.
    #[test]
    fn chosen_section_never_creates_a_conflict() {
        for src in [
            "controls:\n  macos_settings:\n    custom_settings: []\n",
            "controls:\n  apple_settings:\n    configuration_profiles: []\n",
            "controls:\n  macos_settings: {}\n",
            "name: T\n",
        ] {
            let doc: Value = serde_yaml::from_str(src).unwrap();
            let path = section_path_for(&doc, "controls.apple_settings.configuration_profiles");
            // Simulate the insert by adding the resolved path to the document.
            let mut d = doc.clone();
            insert_empty(&mut d, &path);
            assert!(
                conflicts(&d).is_empty(),
                "wiring into {path} created a conflict for {src:?}"
            );
        }
    }

    /// Test helper: ensure `path` exists in `doc` as a nested empty sequence.
    fn insert_empty(doc: &mut Value, path: &str) {
        let mut node = doc;
        let parts: Vec<&str> = path.split('.').collect();
        for (i, seg) in parts.iter().enumerate() {
            if !matches!(node, Value::Mapping(_)) {
                *node = Value::Mapping(Mapping::new());
            }
            let Value::Mapping(map) = node else { unreachable!() };
            let k = Value::from(*seg);
            if !map.contains_key(&k) {
                map.insert(
                    k.clone(),
                    if i == parts.len() - 1 {
                        Value::Sequence(vec![])
                    } else {
                        Value::Mapping(Mapping::new())
                    },
                );
            }
            node = map.get_mut(&k).unwrap();
        }
    }

    #[test]
    fn diagnostic_points_at_the_deprecated_key() {
        let src = "controls:\n  macos_settings: {}\n  apple_settings: {}\n";
        let errs = check(src, Path::new("t.yml"));
        let span = errs[0].span.expect("span");
        assert_eq!(span.line, 2, "points at the deprecated spelling");
    }
}
