//! Deprecation table and registry for Fleet GitOps key/directory renames.
//!
//! Single source of truth for all deprecations. Each entry is gated by version:
//! - **Dormant**: version < `deprecated_in` — no diagnostics
//! - **Warning**: `deprecated_in` <= version < `error_in` — emit warning
//! - **Error**: `error_in` <= version < `removed_in` — emit error
//! - **Removed**: version >= `removed_in` — key is gone, falls through to unknown
//!
//! All entries start with `deprecated_in = v99.0.0` (dormant). When Fleet ships
//! a rename, change the constant to the actual version to activate.

use once_cell::sync::Lazy;

use super::version::Version;

/// The kind of deprecation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeprecationKind {
    /// A YAML key was renamed.
    KeyRename {
        old_key: &'static str,
        new_key: &'static str,
        /// Dot-separated parent paths where this key appears. Each entry is
        /// `""` (top level only), `"*"` (anywhere), or a canonical path in
        /// the KEY_REGISTRY grammar — array segments written `[]`, e.g.
        /// `controls.apple_settings.configuration_profiles[]`. Actual walk
        /// paths carry indices (`[3]`); `find_deprecated_key` canonicalizes
        /// them before matching.
        context_paths: &'static [&'static str],
    },
    /// A directory was renamed.
    DirectoryRename {
        old_dir: &'static str,
        new_dir: &'static str,
    },
    /// A file was renamed.
    FileRename {
        old_name: &'static str,
        new_name: &'static str,
    },
}

/// Which phase a deprecation is in for a given target version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeprecationPhase {
    /// Not yet active — no diagnostics emitted.
    Dormant,
    /// Active as a warning (grace period).
    Warning,
    /// Active as an error (grace period ended).
    Error,
    /// Fully removed — the old key/dir is no longer recognized at all.
    Removed,
}

/// A single deprecation entry.
#[derive(Debug, Clone)]
pub struct Deprecation {
    /// Unique identifier, e.g. `"team-settings-to-settings"`.
    pub id: &'static str,
    /// What changed.
    pub kind: DeprecationKind,
    /// Version when warnings start.
    pub deprecated_in: Version,
    /// Version when errors start (grace period end). `None` = no error phase.
    pub error_in: Option<Version>,
    /// Version when the old name is fully removed. `None` = never removed.
    pub removed_in: Option<Version>,
    /// Human-readable description of the change.
    pub description: &'static str,
    /// Glob patterns for files this deprecation applies to (empty = all files).
    pub file_patterns: &'static [&'static str],
}

impl Deprecation {
    /// Determine which phase this deprecation is in for a given target version.
    pub fn phase_for_version(&self, target: &Version) -> DeprecationPhase {
        if let Some(ref removed) = self.removed_in {
            if target >= removed {
                return DeprecationPhase::Removed;
            }
        }
        if let Some(ref error) = self.error_in {
            if target >= error {
                return DeprecationPhase::Error;
            }
        }
        if target >= &self.deprecated_in {
            return DeprecationPhase::Warning;
        }
        DeprecationPhase::Dormant
    }
}

/// Registry of all known deprecations.
pub struct DeprecationRegistry {
    entries: Vec<Deprecation>,
}

impl DeprecationRegistry {
    /// Find a deprecated key by name and context path.
    ///
    /// `context_path` is the dot-separated path to the parent mapping
    /// (empty string for top-level keys). Array indices are canonicalized
    /// to `[]` before matching, so walk paths like
    /// `controls.apple_settings.configuration_profiles[3]` match registered
    /// contexts written in the KEY_REGISTRY grammar.
    pub fn find_deprecated_key(&self, key: &str, context_path: &str) -> Option<&Deprecation> {
        let canonical = canonicalize_context(context_path);
        self.entries.iter().find(|d| match &d.kind {
            DeprecationKind::KeyRename {
                old_key,
                context_paths,
                ..
            } => {
                if *old_key != key {
                    return false;
                }
                context_paths.iter().any(|cp| {
                    if *cp == "*" {
                        // Wildcard: match in any context
                        true
                    } else if cp.is_empty() {
                        // Empty: match only at top level
                        canonical.is_empty()
                    } else {
                        // Specific context: match exact or as parent prefix
                        // (registry "controls" matches "controls" itself and
                        // "controls.macos_settings", incl. array items after
                        // canonicalization).
                        *cp == canonical || canonical.starts_with(&format!("{}.", cp))
                    }
                })
            }
            _ => false,
        })
    }

    /// Find a deprecated directory by name.
    pub fn find_deprecated_directory(&self, dir_name: &str) -> Option<&Deprecation> {
        self.entries.iter().find(|d| match &d.kind {
            DeprecationKind::DirectoryRename { old_dir, .. } => *old_dir == dir_name,
            _ => false,
        })
    }

    /// Return all deprecations that are non-dormant for a given version.
    pub fn active_deprecations(&self, version: &Version) -> Vec<&Deprecation> {
        self.entries
            .iter()
            .filter(|d| d.phase_for_version(version) != DeprecationPhase::Dormant)
            .collect()
    }

    /// Return all directory rename deprecations active for a given version.
    pub fn active_directory_renames(&self, version: &Version) -> Vec<&Deprecation> {
        self.entries
            .iter()
            .filter(|d| matches!(&d.kind, DeprecationKind::DirectoryRename { .. }))
            .filter(|d| d.phase_for_version(version) != DeprecationPhase::Dormant)
            .collect()
    }

    /// Return all file rename deprecations active for a given version.
    pub fn active_file_renames(&self, version: &Version) -> Vec<&Deprecation> {
        self.entries
            .iter()
            .filter(|d| matches!(&d.kind, DeprecationKind::FileRename { .. }))
            .filter(|d| d.phase_for_version(version) != DeprecationPhase::Dormant)
            .collect()
    }

    /// Get all entries in the registry.
    pub fn entries(&self) -> &[Deprecation] {
        &self.entries
    }
}

// Each `deprecated_in` is the Fleet release that introduced the new name
// (git tag --contains on the renameto / alias commit). From that version the
// new name is accepted and the old one warns, so it is also the earliest
// target a migration may rename to. No entry has an error phase: Fleet 4.93
// still accepts every old name with a warning.

/// Rewrite array indices to the canonical `[]` form used by registered
/// contexts (`controls.apple_settings.configuration_profiles[3]` →
/// `…configuration_profiles[]`), matching the KEY_REGISTRY path grammar.
fn canonicalize_context(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '[' {
            while matches!(chars.peek(), Some(d) if d.is_ascii_digit()) {
                chars.next();
            }
        }
    }
    out
}

/// Global deprecation registry.
pub static DEPRECATION_REGISTRY: Lazy<DeprecationRegistry> = Lazy::new(|| {
    DeprecationRegistry {
        entries: vec![
            // teams/ directory -> fleets/
            Deprecation {
                id: "teams-dir-to-fleets-dir",
                kind: DeprecationKind::DirectoryRename {
                    old_dir: "teams",
                    new_dir: "fleets",
                },
                deprecated_in: Version::new(4, 84, 0),
                error_in: None,
                removed_in: None,
                description: "The 'teams/' directory is being renamed to 'fleets/'",
                file_patterns: &[],
            },
            // team_settings -> settings
            Deprecation {
                id: "team-settings-to-settings",
                kind: DeprecationKind::KeyRename {
                    old_key: "team_settings",
                    new_key: "settings",
                    context_paths: &[""],
                },
                deprecated_in: Version::new(4, 82, 0),
                error_in: None,
                removed_in: None,
                description: "The 'team_settings' key is being renamed to 'settings'",
                file_patterns: &[],
            },
            // queries -> reports
            Deprecation {
                id: "queries-to-reports",
                kind: DeprecationKind::KeyRename {
                    old_key: "queries",
                    new_key: "reports",
                    context_paths: &[""],
                },
                deprecated_in: Version::new(4, 82, 0),
                error_in: None,
                removed_in: None,
                description: "The 'queries' key is being renamed to 'reports'",
                file_patterns: &[],
            },
            // no-team.yml -> unassigned.yml
            Deprecation {
                id: "no-team-to-unassigned",
                kind: DeprecationKind::FileRename {
                    old_name: "no-team.yml",
                    new_name: "unassigned.yml",
                },
                deprecated_in: Version::new(4, 83, 0),
                error_in: None,
                removed_in: None,
                description: "The 'no-team.yml' file is being renamed to 'unassigned.yml'",
                file_patterns: &[],
            },
            // ── Controls key renames (PR #42968) ────────────────────
            // macos_settings -> apple_settings
            Deprecation {
                id: "macos-settings-to-apple-settings",
                kind: DeprecationKind::KeyRename {
                    old_key: "macos_settings",
                    new_key: "apple_settings",
                    context_paths: &["controls"],
                },
                deprecated_in: Version::new(4, 83, 0),
                error_in: None,
                removed_in: None,
                description: "The 'macos_settings' key is being renamed to 'apple_settings'",
                file_patterns: &[],
            },
            // custom_settings -> configuration_profiles (under any *_settings)
            Deprecation {
                id: "custom-settings-to-configuration-profiles",
                kind: DeprecationKind::KeyRename {
                    old_key: "custom_settings",
                    new_key: "configuration_profiles",
                    context_paths: &["*"],
                },
                deprecated_in: Version::new(4, 83, 0),
                error_in: None,
                removed_in: None,
                description: "The 'custom_settings' key is being renamed to 'configuration_profiles'",
                file_patterns: &[],
            },
            // macos_setup -> setup_experience
            Deprecation {
                id: "macos-setup-to-setup-experience",
                kind: DeprecationKind::KeyRename {
                    old_key: "macos_setup",
                    new_key: "setup_experience",
                    context_paths: &["controls"],
                },
                deprecated_in: Version::new(4, 83, 0),
                error_in: None,
                removed_in: None,
                description: "The 'macos_setup' key is being renamed to 'setup_experience'",
                file_patterns: &[],
            },
            // enable_managed_local_account -> enable_create_local_admin_account
            // (app.go: renameto — the rename has ALREADY happened in Fleet, so
            // this sits in the shipped 4.80.1 family, not the projected 4.90
            // controls renames. Fleet still accepts the old name silently and
            // the KEY_REGISTRY deliberately lists both; this entry adds the
            // migration warning. Found via real-repo replay of playground
            // commit 59b6ffb.)
            Deprecation {
                id: "enable-managed-local-account-rename",
                kind: DeprecationKind::KeyRename {
                    old_key: "enable_managed_local_account",
                    new_key: "enable_create_local_admin_account",
                    // Setup experience only: under windows_settings the
                    // same name is the CURRENT key (Fleet v4.92.0).
                    context_paths: &["controls.macos_setup", "controls.setup_experience"],
                },
                deprecated_in: Version::new(4, 85, 0),
                error_in: None,
                removed_in: None,
                description: "The 'enable_managed_local_account' key is being renamed to 'enable_create_local_admin_account'",
                file_patterns: &[],
            },
            // enable_release_device_manually -> apple_enable_release_device_manually
            Deprecation {
                id: "enable-release-device-to-apple-prefix",
                kind: DeprecationKind::KeyRename {
                    old_key: "enable_release_device_manually",
                    new_key: "apple_enable_release_device_manually",
                    context_paths: &["*"],
                },
                deprecated_in: Version::new(4, 83, 0),
                error_in: None,
                removed_in: None,
                description: "The 'enable_release_device_manually' key is being renamed to 'apple_enable_release_device_manually'",
                file_patterns: &[],
            },
            // macos_setup_assistant -> apple_setup_assistant
            Deprecation {
                id: "macos-setup-assistant-to-apple",
                kind: DeprecationKind::KeyRename {
                    old_key: "macos_setup_assistant",
                    new_key: "apple_setup_assistant",
                    context_paths: &["*"],
                },
                deprecated_in: Version::new(4, 83, 0),
                error_in: None,
                removed_in: None,
                description: "The 'macos_setup_assistant' key is being renamed to 'apple_setup_assistant'",
                file_patterns: &[],
            },
            // script -> macos_script (under setup_experience / macos_setup)
            Deprecation {
                id: "script-to-macos-script",
                kind: DeprecationKind::KeyRename {
                    old_key: "script",
                    new_key: "macos_script",
                    context_paths: &["controls"],
                },
                deprecated_in: Version::new(4, 83, 0),
                error_in: None,
                removed_in: None,
                description: "The 'script' key under setup_experience is being renamed to 'macos_script'",
                file_patterns: &[],
            },
            // manual_agent_install -> macos_manual_agent_install
            Deprecation {
                id: "manual-agent-install-to-macos-prefix",
                kind: DeprecationKind::KeyRename {
                    old_key: "manual_agent_install",
                    new_key: "macos_manual_agent_install",
                    context_paths: &["*"],
                },
                deprecated_in: Version::new(4, 83, 0),
                error_in: None,
                removed_in: None,
                description: "The 'manual_agent_install' key is being renamed to 'macos_manual_agent_install'",
                file_patterns: &[],
            },
            // labels -> labels_include_all on configuration/declaration
            // profile items (fleet/mdm.go MDMProfileSpec: "Deprecated: the
            // Labels field … is superseded by LabelsIncludeAll"; Fleet
            // transfers old values, so no error phase). Context-scoped to
            // profile item arrays only — a "*" context would misfire on the
            // legitimate top-level `labels:` section. MDMProfileSpec is
            // shared across all *_settings, hence all four OS variants.
            Deprecation {
                id: "profile-labels-to-labels-include-all",
                kind: DeprecationKind::KeyRename {
                    old_key: "labels",
                    new_key: "labels_include_all",
                    context_paths: &[
                        "controls.apple_settings.configuration_profiles[]",
                        "controls.apple_settings.custom_settings[]",
                        "controls.macos_settings.configuration_profiles[]",
                        "controls.macos_settings.custom_settings[]",
                        "controls.windows_settings.configuration_profiles[]",
                        "controls.windows_settings.custom_settings[]",
                        "controls.android_settings.configuration_profiles[]",
                        "controls.android_settings.custom_settings[]",
                        // Apple DDM assets share MDMProfileSpec.
                        "controls.apple_settings.assets[]",
                        "controls.macos_settings.assets[]",
                    ],
                },
                deprecated_in: Version::new(4, 54, 0),
                error_in: None,
                removed_in: None,
                description: "The 'labels' key on profiles is deprecated. Use 'labels_include_all' instead",
                file_patterns: &[],
            },
            // ── org logo mode-aware renames ─────────────────────────────
            // app.go:1368-1373 marks both old fields `// Deprecated:` and
            // NormalizeLogoFields (app.go:1381) migrates them, so the rename
            // has ALREADY happened in Fleet — same family as
            // enable_managed_local_account, not the projected 4.90 renames.
            // Surfaced by running the real `fleetctl gitops --dry-run`, which
            // printed both warnings on a config flint reported clean.
            Deprecation {
                id: "org-logo-url-to-dark-mode",
                kind: DeprecationKind::KeyRename {
                    old_key: "org_logo_url",
                    new_key: "org_logo_url_dark_mode",
                    context_paths: &["org_settings.org_info"],
                },
                deprecated_in: Version::new(4, 86, 0),
                error_in: None,
                removed_in: None,
                description: "The 'org_logo_url' key is being renamed to 'org_logo_url_dark_mode'",
                file_patterns: &[],
            },
            Deprecation {
                id: "org-logo-url-light-background-to-light-mode",
                kind: DeprecationKind::KeyRename {
                    old_key: "org_logo_url_light_background",
                    new_key: "org_logo_url_light_mode",
                    context_paths: &["org_settings.org_info"],
                },
                deprecated_in: Version::new(4, 86, 0),
                error_in: None,
                removed_in: None,
                description:
                    "The 'org_logo_url_light_background' key is being renamed to 'org_logo_url_light_mode'",
                file_patterns: &[],
            },
        ],
    }
});

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dormant_phase() {
        let dep = Deprecation {
            id: "test",
            kind: DeprecationKind::KeyRename {
                old_key: "old",
                new_key: "new",
                context_paths: &[""],
            },
            deprecated_in: Version::new(5, 0, 0),
            error_in: Some(Version::new(6, 0, 0)),
            removed_in: Some(Version::new(7, 0, 0)),
            description: "test",
            file_patterns: &[],
        };

        assert_eq!(
            dep.phase_for_version(&Version::new(4, 0, 0)),
            DeprecationPhase::Dormant
        );
        assert_eq!(
            dep.phase_for_version(&Version::new(4, 99, 99)),
            DeprecationPhase::Dormant
        );
    }

    #[test]
    fn test_warning_phase() {
        let dep = Deprecation {
            id: "test",
            kind: DeprecationKind::KeyRename {
                old_key: "old",
                new_key: "new",
                context_paths: &[""],
            },
            deprecated_in: Version::new(5, 0, 0),
            error_in: Some(Version::new(6, 0, 0)),
            removed_in: Some(Version::new(7, 0, 0)),
            description: "test",
            file_patterns: &[],
        };

        assert_eq!(
            dep.phase_for_version(&Version::new(5, 0, 0)),
            DeprecationPhase::Warning
        );
        assert_eq!(
            dep.phase_for_version(&Version::new(5, 50, 0)),
            DeprecationPhase::Warning
        );
    }

    #[test]
    fn test_error_phase() {
        let dep = Deprecation {
            id: "test",
            kind: DeprecationKind::KeyRename {
                old_key: "old",
                new_key: "new",
                context_paths: &[""],
            },
            deprecated_in: Version::new(5, 0, 0),
            error_in: Some(Version::new(6, 0, 0)),
            removed_in: Some(Version::new(7, 0, 0)),
            description: "test",
            file_patterns: &[],
        };

        assert_eq!(
            dep.phase_for_version(&Version::new(6, 0, 0)),
            DeprecationPhase::Error
        );
        assert_eq!(
            dep.phase_for_version(&Version::new(6, 50, 0)),
            DeprecationPhase::Error
        );
    }

    #[test]
    fn test_removed_phase() {
        let dep = Deprecation {
            id: "test",
            kind: DeprecationKind::KeyRename {
                old_key: "old",
                new_key: "new",
                context_paths: &[""],
            },
            deprecated_in: Version::new(5, 0, 0),
            error_in: Some(Version::new(6, 0, 0)),
            removed_in: Some(Version::new(7, 0, 0)),
            description: "test",
            file_patterns: &[],
        };

        assert_eq!(
            dep.phase_for_version(&Version::new(7, 0, 0)),
            DeprecationPhase::Removed
        );
        assert_eq!(
            dep.phase_for_version(&Version::new(8, 0, 0)),
            DeprecationPhase::Removed
        );
    }

    #[test]
    fn test_no_error_in_skips_to_removed() {
        let dep = Deprecation {
            id: "test",
            kind: DeprecationKind::KeyRename {
                old_key: "old",
                new_key: "new",
                context_paths: &[""],
            },
            deprecated_in: Version::new(5, 0, 0),
            error_in: None,
            removed_in: Some(Version::new(7, 0, 0)),
            description: "test",
            file_patterns: &[],
        };

        // Warning phase spans from 5.0.0 to 7.0.0 (no error phase)
        assert_eq!(
            dep.phase_for_version(&Version::new(6, 0, 0)),
            DeprecationPhase::Warning
        );
        assert_eq!(
            dep.phase_for_version(&Version::new(7, 0, 0)),
            DeprecationPhase::Removed
        );
    }

    #[test]
    fn test_find_deprecated_key() {
        let registry = &*DEPRECATION_REGISTRY;

        // Should find team_settings at top level
        let dep = registry.find_deprecated_key("team_settings", "");
        assert!(dep.is_some());
        assert_eq!(dep.unwrap().id, "team-settings-to-settings");

        // Should find queries at top level
        let dep = registry.find_deprecated_key("queries", "");
        assert!(dep.is_some());
        assert_eq!(dep.unwrap().id, "queries-to-reports");

        // Should not find unknown keys
        assert!(registry.find_deprecated_key("foobar", "").is_none());

        // Should not match at wrong context path
        assert!(registry
            .find_deprecated_key("team_settings", "org_settings")
            .is_none());
    }

    #[test]
    fn test_find_deprecated_directory() {
        let registry = &*DEPRECATION_REGISTRY;

        let dep = registry.find_deprecated_directory("teams");
        assert!(dep.is_some());
        assert_eq!(dep.unwrap().id, "teams-dir-to-fleets-dir");

        assert!(registry.find_deprecated_directory("foobar").is_none());
    }

    #[test]
    fn test_active_deprecations_dormant() {
        // Before the oldest rename (profile labels, Fleet 4.54) nothing is active.
        let active = DEPRECATION_REGISTRY.active_deprecations(&Version::new(4, 50, 0));
        assert!(active.is_empty(), "got {}", active.len());
    }

    #[test]
    fn test_active_deprecations_warning() {
        // Gated by the Fleet release that introduced each new name: at 4.85
        // the 4.82/4.83 renames are active, the 4.86 logo renames are not.
        let v = Version::new(4, 85, 0);
        let active = DEPRECATION_REGISTRY.active_deprecations(&v);
        let ids: Vec<&str> = active.iter().map(|d| d.id).collect();
        for id in ["team-settings-to-settings", "macos-settings-to-apple-settings", "enable-managed-local-account-rename"] {
            assert!(ids.contains(&id), "{id} should be active at 4.85: {ids:?}");
        }
        assert!(!ids.contains(&"org-logo-url-to-dark-mode"), "{ids:?}");
        assert!(active.iter().all(|d| d.phase_for_version(&v) == DeprecationPhase::Warning));
    }

    #[test]
    fn test_active_deprecations_error() {
        // Fleet 4.93 still accepts every old name with a warning, so nothing
        // escalates — not even at the latest pinned version.
        let v = crate::version_gate::VersionContext::latest().version;
        let active = DEPRECATION_REGISTRY.active_deprecations(&v);
        assert!(!active.is_empty());
        assert!(active.iter().all(|d| d.phase_for_version(&v) == DeprecationPhase::Warning));
    }
}
