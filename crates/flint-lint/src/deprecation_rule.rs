//! Lint rule for deprecated keys and directories.
//!
//! Implements the `Rule` trait. Walks YAML mappings, checks each key against
//! the deprecation registry, and emits version-phased warnings or errors
//! with suggestions for LSP quick-fix code actions.

use super::deprecations::{DeprecationKind, DeprecationPhase, DEPRECATION_REGISTRY};
use super::error::LintError;
use super::fleet_config::FleetConfig;
use super::rules::Rule;
use super::version_gate::VersionContext;
use std::path::Path;

/// Rule that detects usage of deprecated keys and directories.
pub(crate) struct DeprecationRule {
    version_ctx: VersionContext,
}

impl DeprecationRule {
    /// Create a rule with a specific version context.
    pub(crate) fn new(version_ctx: VersionContext) -> Self {
        Self { version_ctx }
    }

    /// Compute the effective phase, promoting `Dormant` to `Warning` when
    /// `future_names` is enabled in the version context.
    fn effective_phase(&self, phase: DeprecationPhase) -> DeprecationPhase {
        if phase == DeprecationPhase::Dormant && self.version_ctx.future_names {
            DeprecationPhase::Warning
        } else {
            phase
        }
    }

    /// Check the file path for directory deprecations.
    fn check_directory(&self, file: &Path) -> Vec<LintError> {
        let mut errors = Vec::new();
        let path_str = file.to_string_lossy();

        for entry in DEPRECATION_REGISTRY.entries() {
            if let DeprecationKind::DirectoryRename { old_dir, new_dir } = &entry.kind {
                let old_pattern = format!("{}/", old_dir);
                let old_pattern_win = format!("{}\\", old_dir);

                if path_str.contains(&old_pattern) || path_str.contains(&old_pattern_win) {
                    let phase =
                        self.effective_phase(entry.phase_for_version(&self.version_ctx.version));
                    match phase {
                        DeprecationPhase::Warning => {
                            errors.push(
                                LintError::warning(
                                    format!(
                                        "Directory '{}/' is deprecated since Fleet v{}. Use '{}/' instead.",
                                        old_dir, entry.deprecated_in, new_dir
                                    ),
                                    file,
                                )
                                .with_help(format!(
                                    "Rename the '{}/' directory to '{}/'",
                                    old_dir, new_dir
                                )),
                            );
                        }
                        DeprecationPhase::Error => {
                            errors.push(
                                LintError::error(
                                    format!(
                                        "Directory '{}/' was removed in Fleet v{}. Use '{}/' instead.",
                                        old_dir,
                                        entry.error_in.as_ref().unwrap_or(&entry.deprecated_in),
                                        new_dir
                                    ),
                                    file,
                                )
                                .with_help(format!(
                                    "Rename the '{}/' directory to '{}/'",
                                    old_dir, new_dir
                                )),
                            );
                        }
                        DeprecationPhase::Dormant | DeprecationPhase::Removed => {}
                    }
                }
            }
        }

        errors
    }

    /// Check every mapping's keys against the deprecation registry, keyed by
    /// the mapping's dotted context path (via the shared walker).
    fn check_mapping(
        &self,
        path: &str,
        map: &serde_yaml::Mapping,
        source: &str,
        file: &Path,
        errors: &mut Vec<LintError>,
    ) {
        {
            for (key, _child) in map {
                let key_str = match key.as_str() {
                    Some(s) => s,
                    None => continue,
                };

                // Check if this key is deprecated at this context path
                if let Some(dep) = DEPRECATION_REGISTRY.find_deprecated_key(key_str, path) {
                    let phase =
                        self.effective_phase(dep.phase_for_version(&self.version_ctx.version));

                    if let DeprecationKind::KeyRename {
                        old_key, new_key, ..
                    } = &dep.kind
                    {
                        let (line, col) = find_key_position(source, key_str, path);

                        match phase {
                            DeprecationPhase::Warning => {
                                let mut err = LintError::warning(
                                    format!(
                                        "Key '{}' is deprecated since Fleet v{}. Use '{}' instead.",
                                        old_key, dep.deprecated_in, new_key
                                    ),
                                    file,
                                )
                                .with_help(format!("Replace '{}' with '{}'", old_key, new_key))
                                .with_context(old_key.to_string())
                                .with_fix(super::error::Fix::Replace {
                                    old: Some(old_key.to_string()),
                                    new: new_key.to_string(),
                                    safety: super::error::FixSafety::Safe,
                                });

                                if let Some(l) = line {
                                    err = err.with_location(l, col.unwrap_or(1));
                                }
                                errors.push(err);
                            }
                            DeprecationPhase::Error => {
                                let mut err = LintError::error(
                                    format!(
                                        "Key '{}' was removed in Fleet v{}. Use '{}' instead.",
                                        old_key,
                                        dep.error_in.as_ref().unwrap_or(&dep.deprecated_in),
                                        new_key
                                    ),
                                    file,
                                )
                                .with_help(format!("Replace '{}' with '{}'", old_key, new_key))
                                .with_context(old_key.to_string())
                                .with_fix(super::error::Fix::Replace {
                                    old: Some(old_key.to_string()),
                                    new: new_key.to_string(),
                                    safety: super::error::FixSafety::Safe,
                                });

                                if let Some(l) = line {
                                    err = err.with_location(l, col.unwrap_or(1));
                                }
                                errors.push(err);
                            }
                            DeprecationPhase::Dormant | DeprecationPhase::Removed => {}
                        }
                    }
                }

            }
        }
    }
}

impl Rule for DeprecationRule {
    fn name(&self) -> &'static str {
        "deprecated-keys"
    }

    fn description(&self) -> &'static str {
        "Detects usage of deprecated keys and directories, with version-gated severity"
    }
    fn category(&self) -> &'static str {
        "deprecation"
    }
    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        // 1. Check file path for directory deprecations
        errors.extend(self.check_directory(file));

        // 1b. A deprecated key AND its replacement together is an apply
        // failure, not a style issue — Fleet cannot choose between them and
        // refuses to parse the file. Version-independent: it has nothing to
        // do with how far along the deprecation window is.
        errors.extend(super::deprecated_conflict::check(source, file));

        // 2. Parse YAML and walk keys
        let yaml_value: serde_yaml::Value = match serde_yaml::from_str(source) {
            Ok(v) => v,
            Err(_) => return errors,
        };

        super::yaml_utils::walk_mappings_with_path(&yaml_value, &mut |path, map| {
            self.check_mapping(path, map, source, file, &mut errors);
        });

        errors
    }
}

/// Find the line/column of a YAML key in source text.
fn find_key_position(source: &str, key: &str, _path: &str) -> (Option<usize>, Option<usize>) {
    let pattern = format!("{}:", key);

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(&pattern)
            || trimmed.starts_with(&format!("\"{}\":", key))
            || trimmed.starts_with(&format!("'{}':", key))
        {
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
    use crate::error::Severity;
    use crate::version::Version;
    use std::path::PathBuf;

    fn check_with_version(yaml: &str, file: &str, version: Version) -> Vec<LintError> {
        let ctx = VersionContext {
            version,
            source: super::super::version_gate::VersionSource::Config,
            future_names: false,
        };
        let rule = DeprecationRule::new(ctx);
        let config = FleetConfig::default();
        rule.check(&config, &PathBuf::from(file), yaml)
    }

    fn check_with_future_names(yaml: &str, file: &str) -> Vec<LintError> {
        let ctx = VersionContext {
            version: Version::new(4, 80, 1), // current version, dormant entries won't fire normally
            source: super::super::version_gate::VersionSource::Config,
            future_names: true,
        };
        let rule = DeprecationRule::new(ctx);
        let config = FleetConfig::default();
        rule.check(&config, &PathBuf::from(file), yaml)
    }

    #[test]
    fn test_dormant_no_diagnostics() {
        let yaml = r#"
team_settings:
  features:
    enable_host_users: true
queries:
  - name: "Test"
    query: "SELECT 1;"
"#;
        // v0.0.0 = dormant, nothing should fire
        let errors = check_with_version(yaml, "default.yml", Version::new(0, 0, 0));
        assert!(
            errors.is_empty(),
            "Expected no errors in dormant mode, got: {:?}",
            errors
        );
    }

    #[test]
    fn test_warning_produces_warning() {
        let yaml = r#"
team_settings:
  features:
    enable_host_users: true
"#;
        // Use v4.85.0 to trigger warning phase (between deprecated_in=4.80.1 and error_in=4.88.0)
        let errors = check_with_version(yaml, "default.yml", Version::new(4, 85, 0));
        assert!(!errors.is_empty(), "Expected warnings for deprecated key");

        let err = &errors[0];
        assert_eq!(err.severity, Severity::Warning);
        assert!(
            err.message.contains("team_settings"),
            "Message should mention deprecated key: {}",
            err.message
        );
        assert!(
            err.message.contains("settings"),
            "Message should mention new key: {}",
            err.message
        );
    }

    #[test]
    fn test_suggestion_field_set() {
        let yaml = r#"
team_settings:
  features:
    enable_host_users: true
"#;
        let errors = check_with_version(yaml, "default.yml", Version::new(4, 85, 0));
        assert!(!errors.is_empty());

        let err = &errors[0];
        assert_eq!(
            err.suggestion(),
            Some("settings"),
            "Suggestion should be the new key name"
        );
    }

    #[test]
    fn test_enable_managed_local_account_rename() {
        // Real-repo replay finding (playground 59b6ffb): the old key under
        // controls.macos_setup/setup_experience must warn with the new name.
        let yaml = r#"
controls:
  macos_setup:
    enable_managed_local_account: true
"#;
        // 4.80.1 = VersionContext::latest() — the rename already shipped in
        // Fleet (app.go renameto), so it must warn at the DEFAULT context,
        // not only behind future_names/4.90.
        let errors = check_with_version(yaml, "fleets/ccs.yml", Version::new(4, 80, 1));
        let err = errors
            .iter()
            .find(|e| e.message.contains("enable_managed_local_account"))
            .expect("expected deprecation warning for enable_managed_local_account");
        assert!(err.message.contains("enable_create_local_admin_account"));
        assert_eq!(err.suggestion(), Some("enable_create_local_admin_account"));

        // Same key under the new setup_experience spelling still matches
        // (context 'controls' is a prefix of 'controls.setup_experience').
        let yaml2 = r#"
controls:
  setup_experience:
    enable_managed_local_account: true
"#;
        let errors2 = check_with_version(yaml2, "fleets/ccs.yml", Version::new(4, 80, 1));
        assert!(errors2
            .iter()
            .any(|e| e.message.contains("enable_managed_local_account")));
    }

    #[test]
    fn test_profile_labels_context_scoped() {
        // `labels` on a profile item warns with the labels_include_all
        // suggestion — the walker's indexed context
        // (controls.apple_settings.configuration_profiles[0]) must match the
        // canonical `[]` context registered for the entry.
        let yaml = r#"
controls:
  apple_settings:
    configuration_profiles:
      - path: ../profiles/wifi.mobileconfig
        labels:
          - Engineering
"#;
        let errors = check_with_version(yaml, "fleets/eng.yml", Version::new(4, 85, 0));
        let err = errors
            .iter()
            .find(|e| e.message.contains("'labels'"))
            .expect("expected deprecation warning for profile labels");
        assert!(err.message.contains("labels_include_all"));
        assert_eq!(err.suggestion(), Some("labels_include_all"));

        // The legitimate top-level `labels:` section must stay silent —
        // this is why the entry is context-scoped instead of "*".
        let yaml2 = r#"
labels:
  - name: Engineering
    query: SELECT 1;
"#;
        let errors2 = check_with_version(yaml2, "default.yml", Version::new(4, 85, 0));
        assert!(
            !errors2.iter().any(|e| e.message.contains("'labels'")),
            "top-level labels wrongly flagged: {:?}",
            errors2
        );

        // `labels` under controls.scripts[] is NOT a profile context — the
        // deprecation must not fire there (structural-validation owns that
        // case as a misplaced/unknown key).
        let yaml3 = r#"
controls:
  scripts:
    - path: ../scripts/setup.sh
      labels:
        - Engineering
"#;
        let errors3 = check_with_version(yaml3, "fleets/eng.yml", Version::new(4, 85, 0));
        assert!(
            !errors3.iter().any(|e| e.message.contains("'labels'")),
            "labels under scripts wrongly matched profile deprecation: {:?}",
            errors3
        );
    }

    #[test]
    fn test_directory_rename_warning() {
        let yaml = r#"
name: Engineering
policies:
  - name: "Test"
    query: "SELECT 1;"
"#;
        let errors = check_with_version(yaml, "teams/engineering.yml", Version::new(4, 85, 0));

        let dir_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.message.contains("teams/") || e.message.contains("Directory"))
            .collect();
        assert!(
            !dir_errors.is_empty(),
            "Expected directory deprecation warning"
        );
        assert!(dir_errors[0].message.contains("fleets/"));
    }

    #[test]
    fn test_error_phase_produces_error() {
        // At v4.88.0 (error_in), the registry entries should produce errors
        let ctx = VersionContext {
            version: Version::new(4, 88, 0),
            source: super::super::version_gate::VersionSource::Config,
            future_names: false,
        };
        let rule = DeprecationRule::new(ctx);

        let yaml = "team_settings:\n  features:\n    enable_host_users: true\n";
        let config = FleetConfig::default();
        let errors = rule.check(&config, &PathBuf::from("default.yml"), yaml);

        let error_diags: Vec<_> = errors
            .iter()
            .filter(|e| e.severity == Severity::Error)
            .collect();
        assert!(
            !error_diags.is_empty(),
            "Expected errors at version >= error_in"
        );
        assert!(error_diags[0].message.contains("team_settings"));
    }

    #[test]
    fn test_queries_deprecation() {
        let yaml = r#"
queries:
  - name: "Test"
    query: "SELECT 1;"
"#;
        let errors = check_with_version(yaml, "default.yml", Version::new(4, 85, 0));

        let query_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.message.contains("queries"))
            .collect();
        assert!(
            !query_errors.is_empty(),
            "Expected deprecation warning for 'queries'"
        );
        assert!(query_errors[0].message.contains("reports"));
    }

    #[test]
    fn test_future_names_warns_on_team_settings() {
        let yaml = "team_settings:\n  features:\n    enable_host_users: true\n";
        let errors = check_with_future_names(yaml, "default.yml");
        assert!(
            !errors.is_empty(),
            "future_names should warn on team_settings"
        );
        assert_eq!(errors[0].severity, Severity::Warning);
        assert!(errors[0].message.contains("team_settings"));
        assert!(errors[0].message.contains("settings"));
    }

    #[test]
    fn test_future_names_warns_on_queries() {
        let yaml = "queries:\n  - name: Test\n    query: \"SELECT 1;\"\n";
        let errors = check_with_future_names(yaml, "default.yml");
        let query_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.message.contains("queries"))
            .collect();
        assert!(
            !query_errors.is_empty(),
            "future_names should warn on queries"
        );
        assert!(query_errors[0].message.contains("reports"));
    }

    #[test]
    fn test_future_names_warns_on_teams_directory() {
        let yaml = "name: Engineering\n";
        let errors = check_with_future_names(yaml, "teams/engineering.yml");
        let dir_errors: Vec<_> = errors
            .iter()
            .filter(|e| e.message.contains("teams/"))
            .collect();
        assert!(
            !dir_errors.is_empty(),
            "future_names should warn on teams/ directory"
        );
        assert!(dir_errors[0].message.contains("fleets/"));
    }

    #[test]
    fn test_future_names_off_no_warnings_below_deprecated_version() {
        let yaml = "team_settings:\n  features:\n    enable_host_users: true\n";
        // At v4.80.0 (below deprecated_in=4.80.1), entries should NOT fire without future_names
        let errors = check_with_version(yaml, "default.yml", Version::new(4, 80, 0));
        assert!(
            errors.is_empty(),
            "Without future_names, entries should not fire below deprecated_in version"
        );
    }
}
