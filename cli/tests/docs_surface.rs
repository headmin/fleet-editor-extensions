//! GUARD (surface audit 2026-08-07): the docs site must never show a
//! deprecated key as current YAML. `fleet-variables.md` once demonstrated
//! `macos_settings:`/`custom_settings:` with no note. Pages that are ABOUT
//! the renames (migration guide, the deprecated-keys rule docs) are
//! allowlisted; everything else fails this test if a fenced ```yaml block
//! uses an old key — until the example is migrated to the current name.

use flint_lint::deprecations::{DeprecationKind, DEPRECATION_REGISTRY};

const ALLOWLIST: &[&str] = &["migration.md", "rules.md"];

#[test]
fn docs_pages_never_advertise_deprecated_keys() {
    let docs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/docs");
    if !docs_dir.is_dir() {
        return; // docs not present in this checkout — nothing to audit
    }

    let old_keys: Vec<&str> = DEPRECATION_REGISTRY
        .entries()
        .iter()
        .filter_map(|d| match &d.kind {
            DeprecationKind::KeyRename { old_key, .. } => Some(*old_key),
            _ => None,
        })
        // `labels`/`script` are context-scoped renames whose bare names are
        // legitimate top-level keys — line-based scanning can't tell the
        // contexts apart, so they're checked by the LSP hover guard instead.
        .filter(|k| !matches!(*k, "labels" | "script"))
        .collect();

    let mut violations = Vec::new();
    for entry in std::fs::read_dir(&docs_dir).expect("read docs dir") {
        let path = entry.expect("dir entry").path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if path.extension().is_none_or(|e| e != "md") || ALLOWLIST.contains(&name.as_str()) {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let mut in_yaml = false;
        for (i, line) in content.lines().enumerate() {
            if line.trim_start().starts_with("```") {
                in_yaml = line.contains("yaml");
                continue;
            }
            if !in_yaml || line.contains("flint: ignore") {
                continue;
            }
            for key in &old_keys {
                if line.trim_start().starts_with(&format!("{}:", key)) {
                    violations.push(format!("{name}:{}: uses '{key}:'", i + 1));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "docs pages advertise deprecated keys in yaml examples \
         (migrate the example or allowlist a page that is ABOUT the rename):\n{}",
        violations.join("\n")
    );
}
