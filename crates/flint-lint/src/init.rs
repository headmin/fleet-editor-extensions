//! Workspace detection and config generation for `flint init`.
//!
//! Everything here is pure: detect, generate, write. The prompting AND the
//! terminal output both live in the CLI (`cli/src/commands/init.rs`), so this
//! module contains no `println!` — a library that prints cannot be driven by
//! anything except a terminal. The scope analysis the questions are asked
//! over lives in [`super::scope`].

use super::config::CONFIG_FILE_NAME;
use super::scope::{self, ScopePreview};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Detected workspace configuration.
#[derive(Debug, Clone, Default)]
pub struct DetectedConfig {
    /// Whether a fleets/ (or legacy teams/) directory exists.
    pub has_fleets_dir: bool,
    /// Number of fleet (team) specs found — files with a top-level `name:`,
    /// which is how Fleet itself identifies one. NOT a subdirectory count.
    pub fleet_count: usize,
    /// Whether the legacy `teams/` directory exists (without a `fleets/` directory).
    pub has_legacy_teams_dir: bool,
    /// Whether a lib/ directory exists.
    pub has_lib_dir: bool,
    /// Total number of YAML files found.
    pub yaml_file_count: usize,
    /// Platforms detected in YAML files.
    pub detected_platforms: Vec<String>,
    /// Whether path references (- path:) were found.
    pub has_path_references: bool,
    /// Root YAML files (default.yml, etc.).
    pub root_yaml_files: Vec<String>,
    /// Whether the legacy `queries:` key was found (should be `reports:`).
    pub has_legacy_queries: bool,
}

/// User's answers to interactive prompts.
#[derive(Debug, Clone, Default)]
pub struct UserAnswers {
    /// Selected strictness level.
    pub strictness: StrictnessLevel,
    /// What the scope questions produced, already measured against the repo.
    /// The default narrows nothing — see [`ScopePreview::default`].
    pub scope: ScopePreview,
}

/// Strictness level for linting.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum StrictnessLevel {
    /// Enforce best practices strictly.
    Strict,
    /// Balanced defaults.
    #[default]
    Moderate,
    /// Minimal warnings.
    Relaxed,
}

/// Detect Fleet GitOps structure in the given directory.
pub fn detect_workspace(root: &Path) -> DetectedConfig {
    let mut config = DetectedConfig::default();

    // Check for fleets/ directory first, fall back to legacy teams/
    let fleets_dir = root.join("fleets");
    let teams_dir = root.join("teams");
    if fleets_dir.is_dir() {
        config.has_fleets_dir = true;
        config.fleet_count = count_fleet_specs(&fleets_dir);
    } else if teams_dir.is_dir() {
        config.has_fleets_dir = true;
        config.has_legacy_teams_dir = true;
        config.fleet_count = count_fleet_specs(&teams_dir);
    }

    // Check for lib/ directory
    config.has_lib_dir = root.join("lib").is_dir();

    // Check for root YAML files
    if let Ok(entries) = fs::read_dir(root) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    if ext == "yml" || ext == "yaml" {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            config.root_yaml_files.push(name.to_string());
                        }
                    }
                }
            }
        }
    }

    // Recursively scan for YAML files and extract info
    scan_yaml_files(root, &mut config);

    config
}

/// Count the fleet (team) specs under `dir`, recursively.
///
/// Fleet's own discriminator, not a layout guess: `GitOpsFromFile` reads the
/// top-level keys and treats a file with `name:` as a fleet spec, one with
/// `org_settings:` as the org file, and errors on a file with neither
/// (`pkg/spec/gitops.go:533-544`, Fleet @ 1b3288426f). Everything else under
/// `fleets/` is a fragment pulled in by `path:`/`paths:`, or a README.
///
/// This used to count SUBDIRECTORIES, which was wrong both ways: a repo that
/// keeps one YAML per fleet directly in `fleets/` reported 0 fleets, and a
/// leftover empty directory counted as one.
fn count_fleet_specs(dir: &Path) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            // A fleet-per-directory layout keeps the spec in a nested
            // default.yml; recursing covers both conventions.
            count += count_fleet_specs(&path);
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("yml" | "yaml")) {
            if let Ok(content) = fs::read_to_string(&path) {
                if declares_fleet_name(&content) {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Whether a document has a TOP-LEVEL `name:` key.
///
/// Column 0 is the whole test: YAML requires a block scalar's content to be
/// indented past its key, so an unindented `name:` cannot be anything but a
/// top-level mapping key. Line-scanned rather than parsed so a repo with one
/// malformed file still gets a usable count — the number is a summary, and
/// refusing to count is worse than counting a file whose YAML is broken.
fn declares_fleet_name(content: &str) -> bool {
    content.lines().any(|line| {
        line.strip_prefix("name:")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t']))
    })
}

/// Recursively scan for YAML files and extract configuration info.
fn scan_yaml_files(dir: &Path, config: &mut DetectedConfig) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();

        if path.is_dir() {
            // Skip common ignore directories
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "target"
                    || name == "dist"
                {
                    continue;
                }
            }
            scan_yaml_files(&path, config);
        } else if path.is_file() {
            if let Some(ext) = path.extension() {
                if ext == "yml" || ext == "yaml" {
                    config.yaml_file_count += 1;
                    // Parse file for platform and path references
                    if let Ok(content) = fs::read_to_string(&path) {
                        extract_info_from_yaml(&content, config);
                    }
                }
            }
        }
    }
}

/// Extract platform and path reference info from YAML content.
fn extract_info_from_yaml(content: &str, config: &mut DetectedConfig) {
    let mut platforms: HashSet<String> = HashSet::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Detect platform values
        if trimmed.starts_with("platform:") {
            if let Some(value) = trimmed.strip_prefix("platform:") {
                let platform = value.trim().trim_matches('"').trim_matches('\'');
                if !platform.is_empty()
                    && [
                        "darwin", "windows", "linux", "chrome", "ios", "ipados", "android",
                    ]
                    .contains(&platform)
                {
                    platforms.insert(platform.to_string());
                }
            }
        }

        // Detect path references
        if trimmed.starts_with("- path:") || trimmed.starts_with("path:") {
            config.has_path_references = true;
        }

        // Detect legacy queries: key (should be reports:)
        if trimmed == "queries:" || trimmed.starts_with("queries:") {
            config.has_legacy_queries = true;
        }
    }

    // Add detected platforms
    for platform in platforms {
        if !config.detected_platforms.contains(&platform) {
            config.detected_platforms.push(platform);
        }
    }
}

/// Map a strictness answer typed at a prompt. Shared by the CLI so the
/// accepted spellings live next to the enum they produce.
pub fn parse_strictness(input: &str) -> StrictnessLevel {
    match input.trim().to_lowercase().as_str() {
        "1" | "strict" => StrictnessLevel::Strict,
        "3" | "relaxed" => StrictnessLevel::Relaxed,
        _ => StrictnessLevel::Moderate,
    }
}

/// Generate TOML content with comments based on detection and answers.
pub fn generate_config_toml(detected: &DetectedConfig, answers: &UserAnswers) -> String {
    let mut output = String::new();

    // Header
    output.push_str("# Fleet Linter Configuration\n");
    output.push_str("# Generated by `flint init`\n");
    output.push_str("#\n");

    // Add detection summary as comment
    if detected.has_fleets_dir || detected.has_lib_dir {
        output.push_str("# Detected structure:\n");
        if detected.has_fleets_dir {
            if detected.has_legacy_teams_dir {
                output.push_str(&format!(
                    "#   - teams/ directory ({} fleet(s)) — consider renaming to fleets/\n",
                    detected.fleet_count
                ));
            } else {
                output.push_str(&format!(
                    "#   - fleets/ directory ({} fleet(s))\n",
                    detected.fleet_count
                ));
            }
        }
        if detected.has_lib_dir {
            output.push_str("#   - lib/ directory (deprecated legacy layout — migrate to platforms/)\n");
        }
        if !detected.detected_platforms.is_empty() {
            output.push_str(&format!(
                "#   - Platforms: {}\n",
                detected.detected_platforms.join(", ")
            ));
        }
        output.push_str("#\n");
    }
    output.push('\n');

    // Rules section
    output.push_str("# Rule Configuration\n");
    output.push_str("[rules]\n");
    output.push_str("# Rules to disable entirely (by name)\n");
    output.push_str("# Available rules:\n");
    output.push_str("#   - required-fields: Ensures required fields are present\n");
    output.push_str(
        "#   - platform-compatibility: Validates osquery tables work on specified platform\n",
    );
    output.push_str("#   - type-validation: Validates field types\n");
    output.push_str("#   - security: Detects hardcoded secrets\n");
    output.push_str("#   - interval-validation: Warns about extreme interval values\n");
    output.push_str("#   - duplicate-names: Detects duplicate policy/query/label names\n");
    output.push_str("#   - query-syntax: Validates SQL query syntax\n");

    match answers.strictness {
        StrictnessLevel::Relaxed => {
            output.push_str("disabled = [\"query-syntax\"]\n");
        }
        _ => {
            output.push_str("disabled = []\n");
        }
    }

    output.push_str("\n# Rules to downgrade from error to warning\n");
    output.push_str("warn = []\n");
    output.push('\n');

    // Thresholds section
    output.push_str("# Threshold Configuration\n");
    output.push_str("[thresholds]\n");
    output.push_str("# Minimum query interval in seconds\n");
    output.push_str("min_interval = 60\n");
    output.push_str("\n# Maximum query interval in seconds (24 hours)\n");
    output.push_str("max_interval = 86400\n");
    output.push_str("\n# Maximum query length in characters\n");
    output.push_str("max_query_length = 10000\n");

    match answers.strictness {
        StrictnessLevel::Relaxed => {
            output.push_str("\n# Warn when using SELECT * (disabled for relaxed mode)\n");
            output.push_str("warn_select_star = false\n");
        }
        _ => {
            output.push_str("\n# Warn when using SELECT *\n");
            output.push_str("warn_select_star = true\n");
        }
    }
    output.push('\n');

    // Files section — rendered from the measured scope selection, so the
    // globs written here are the globs the delta was computed from. Both
    // shapes (unset `include`, or a directory allowlist) come out of
    // `scope::render_files_section`; neither can be an extension glob.
    output.push_str(&scope::render_files_section(&answers.scope));

    if detected.has_fleets_dir {
        output.push_str("\n# Root directory for path resolution\n");
        output.push_str("# root = \".\"\n");
    }
    output.push('\n');

    // Schema section
    output.push_str("# Schema Validation\n");
    output.push_str("[schema]\n");
    output.push_str("# Validate against Fleet's schema\n");
    output.push_str("validate = true\n");
    output.push_str("\n# Allow unknown/extra fields\n");
    output.push_str("allow_unknown_fields = true\n");

    match answers.strictness {
        StrictnessLevel::Strict => {
            output.push_str("\n# Require explicit platform specification (strict mode)\n");
            output.push_str("require_platform = true\n");
        }
        _ => {
            output.push_str("\n# Require explicit platform specification\n");
            output.push_str("require_platform = false\n");
        }
    }

    output
}

/// Where `flint init` will write, given an optional explicit `--output`.
///
/// Defaults to the VISIBLE [`CONFIG_FILE_NAME`]; the hidden `.fleetlint.toml`
/// spelling is still read by the loader, so an existing repo keeps working
/// untouched.
pub fn config_path_for(root: &Path, output: Option<PathBuf>) -> PathBuf {
    output.unwrap_or_else(|| root.join(CONFIG_FILE_NAME))
}

/// Write a generated config, refusing to clobber an existing file unless
/// `force`.
///
/// Deliberately silent: this crate is a library, so rendering progress and
/// next steps belongs to the caller. `flint init`'s terminal output lives in
/// the CLI (`cli/src/commands/init.rs`), which is also where the prompting
/// is — keeping both on one side of the boundary is what lets the library
/// stay free of `println!`.
pub fn write_config(path: &Path, content: &str, force: bool) -> anyhow::Result<()> {
    if path.exists() && !force {
        anyhow::bail!(
            "Configuration file already exists: {}\nUse --force to overwrite.",
            path.display()
        );
    }
    fs::write(path, content)?;
    Ok(())
}

/// Walk up from a path to the GitOps repo root. `.git` or `.fleetlint.toml`
/// wins immediately; the shallowest ancestor holding a `default.yml` is
/// remembered as a fallback; failing both, the starting directory itself.
pub fn discover_gitops_root(start: &Path) -> PathBuf {
    // Start at `start` itself if it's a directory, else its parent.
    let first = if start.is_dir() {
        Some(start)
    } else {
        start.parent()
    };
    let mut cur = first;
    let mut gitops = None;
    while let Some(dir) = cur {
        // Either config spelling marks the repo root, same as `.git`.
        if dir.join(".git").exists()
            || super::config::CONFIG_FILE_NAMES
                .iter()
                .any(|n| dir.join(n).exists())
        {
            return dir.to_path_buf();
        }
        if dir.join("default.yml").exists() {
            gitops = Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }
    gitops
        .or_else(|| first.map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scope::ScopeSelection;
    use tempfile::TempDir;

    #[test]
    fn discover_root_prefers_git_marker() {
        let tmp = TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        let deep = tmp.path().join("platforms/macos/profiles");
        std::fs::create_dir_all(&deep).unwrap();
        assert_eq!(
            discover_gitops_root(&deep).canonicalize().unwrap(),
            tmp.path().canonicalize().unwrap()
        );
    }

    #[test]
    fn test_detect_empty_workspace() {
        let temp_dir = TempDir::new().unwrap();
        let detected = detect_workspace(temp_dir.path());

        assert!(!detected.has_fleets_dir);
        assert!(!detected.has_lib_dir);
        assert_eq!(detected.yaml_file_count, 0);
        assert!(detected.detected_platforms.is_empty());
    }

    #[test]
    fn test_detect_fleets_structure() {
        let temp_dir = TempDir::new().unwrap();

        // Create fleets structure: one real fleet-per-directory spec, and a
        // leftover empty directory next to it.
        fs::create_dir(temp_dir.path().join("fleets")).unwrap();
        fs::create_dir(temp_dir.path().join("fleets/engineering")).unwrap();
        fs::create_dir(temp_dir.path().join("fleets/security")).unwrap();
        fs::create_dir(temp_dir.path().join("lib")).unwrap();

        // Create some YAML files
        fs::write(
            temp_dir.path().join("default.yml"),
            "policies:\n  - name: Test\n    platform: darwin\n",
        )
        .unwrap();
        fs::write(
            temp_dir.path().join("fleets/engineering/default.yml"),
            "name: Engineering\npolicies:\n  - name: Fleet Policy\n    platform: linux\n",
        )
        .unwrap();

        let detected = detect_workspace(temp_dir.path());

        assert!(detected.has_fleets_dir);
        assert!(!detected.has_legacy_teams_dir);
        // One spec, not two directories: `fleets/security/` is empty and is
        // not a fleet. The indented `- name: Fleet Policy` is not one either.
        assert_eq!(detected.fleet_count, 1);
        assert!(detected.has_lib_dir);
        assert_eq!(detected.yaml_file_count, 2);
        assert!(detected.detected_platforms.contains(&"darwin".to_string()));
        assert!(detected.detected_platforms.contains(&"linux".to_string()));
    }

    #[test]
    fn test_detect_legacy_teams_dir() {
        let temp_dir = TempDir::new().unwrap();

        // Create legacy teams/ structure (no fleets/)
        fs::create_dir(temp_dir.path().join("teams")).unwrap();
        fs::create_dir(temp_dir.path().join("teams/engineering")).unwrap();
        fs::write(
            temp_dir.path().join("teams/engineering/default.yml"),
            "name: Engineering\n",
        )
        .unwrap();

        let detected = detect_workspace(temp_dir.path());

        assert!(detected.has_fleets_dir);
        assert!(detected.has_legacy_teams_dir);
        assert_eq!(detected.fleet_count, 1);
    }

    /// The layout the reference repo uses — one YAML per fleet, directly in
    /// `fleets/`. The old subdirectory count reported 0 fleets for 25 files.
    ///
    /// Fleet decides this by top-level key, so the fixture carries one of
    /// each thing that is NOT a fleet: a fragment (no `name:`), an org file
    /// (`org_settings:`), a README, and an empty directory. If the count
    /// comes back as anything but 3, one of those is being miscounted.
    #[test]
    fn fleet_count_counts_specs_not_directories() {
        let temp_dir = TempDir::new().unwrap();
        let fleets = temp_dir.path().join("fleets");
        fs::create_dir(&fleets).unwrap();

        fs::write(fleets.join("ABC-ALPHA.yml"), "name: ABC-ALPHA\ncontrols: {}\n").unwrap();
        fs::write(fleets.join("CFG-ONE.yml"), "name: CFG-ONE\n").unwrap();
        fs::write(fleets.join("unassigned.yml"), "name: Unassigned\n").unwrap();

        // Not fleets:
        fs::write(
            fleets.join("shared-policies.yml"),
            "policies:\n  - name: Some Policy\n",
        )
        .unwrap();
        fs::write(fleets.join("org.yml"), "org_settings:\n  org_info: {}\n").unwrap();
        fs::write(fleets.join("README.md"), "# fleets\n").unwrap();
        fs::create_dir(fleets.join("_wip")).unwrap();

        let detected = detect_workspace(temp_dir.path());
        assert!(detected.has_fleets_dir);
        assert_eq!(detected.fleet_count, 3);
    }

    /// THE CONTROL for the old bug, in isolation: subdirectories alone are
    /// not fleets. Before this fix `fleets/{a,b}/` reported 2.
    #[test]
    fn empty_fleet_directories_count_as_zero() {
        let temp_dir = TempDir::new().unwrap();
        fs::create_dir_all(temp_dir.path().join("fleets/engineering")).unwrap();
        fs::create_dir_all(temp_dir.path().join("fleets/security")).unwrap();

        let detected = detect_workspace(temp_dir.path());
        assert!(detected.has_fleets_dir, "the directory does exist");
        assert_eq!(
            detected.fleet_count, 0,
            "empty directories are not fleets — this is the bug that reported \
             '2 fleet(s)' for a repo with none, and '0' for one with 25"
        );
    }

    #[test]
    fn top_level_name_detection_ignores_indented_and_commented_keys() {
        assert!(declares_fleet_name("name: ABC-ALPHA\n"));
        assert!(declares_fleet_name("controls: {}\nname: ABC-ALPHA\n"));
        // Value on the following line is still a top-level key.
        assert!(declares_fleet_name("name:\n"));

        assert!(!declares_fleet_name("policies:\n  - name: Some Policy\n"));
        assert!(!declares_fleet_name("# name: commented out\n"));
        assert!(!declares_fleet_name("org_settings:\n  org_info: {}\n"));
        // `name:value` without a space is a scalar, not a mapping key.
        assert!(!declares_fleet_name("name:nospace\n"));
    }

    #[test]
    fn test_detect_legacy_queries() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("default.yml"),
            "queries:\n  - name: Test Query\n    query: SELECT 1;\n",
        )
        .unwrap();

        let detected = detect_workspace(temp_dir.path());

        assert!(detected.has_legacy_queries);
    }

    #[test]
    fn test_detect_path_references() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("default.yml"),
            "policies:\n  - path: lib/policies.yml\n",
        )
        .unwrap();

        let detected = detect_workspace(temp_dir.path());

        assert!(detected.has_path_references);
    }

    #[test]
    fn test_generate_config_toml() {
        let detected = DetectedConfig {
            has_fleets_dir: true,
            fleet_count: 2,
            has_legacy_teams_dir: false,
            has_lib_dir: true,
            yaml_file_count: 10,
            detected_platforms: vec!["darwin".to_string(), "linux".to_string()],
            has_path_references: true,
            root_yaml_files: vec!["default.yml".to_string()],
            has_legacy_queries: false,
        };
        let answers = UserAnswers::default();

        let toml = generate_config_toml(&detected, &answers);

        assert!(toml.contains("[rules]"));
        assert!(toml.contains("[thresholds]"));
        assert!(toml.contains("[files]"));
        assert!(toml.contains("[schema]"));
        assert!(toml.contains("fleets/ directory"));
        assert!(toml.contains("darwin, linux"));
    }

    /// A generated config must never set an EXTENSION-based `include`.
    ///
    /// A non-empty `include` is authoritative and also scopes the workspace
    /// rules — orphaned-file, duplicate-content, case-collision,
    /// unregistered-script — which report on scripts, profiles and payloads,
    /// not YAML. `include = ["**/*.yml"]` therefore reads as a harmless
    /// tautology while switching all of them off for every non-YAML file.
    ///
    /// This is the belt to `files_config_default_include_is_empty`'s braces:
    /// that test fixes the DEFAULT, this one stops the template handing users
    /// an explicit copy of the same mistake. Narrowing by DIRECTORY
    /// (`platforms/**`) is fine — scripts under it stay in scope. Only
    /// extension globs are the trap.
    #[test]
    fn generated_config_never_sets_extension_only_include() {
        let detected = DetectedConfig::default();
        let toml = generate_config_toml(&detected, &UserAnswers::default());

        for line in toml.lines() {
            let t = line.trim();
            if t.starts_with('#') || !t.starts_with("include") {
                continue;
            }
            panic!(
                "generated config sets an active `include` ({t}) — an \
                 extension-based include silently disables the cross-file \
                 rules on scripts and profiles. Leave it unset, or narrow \
                 by directory."
            );
        }
    }

    /// The scope assistant DOES emit an active `include` — the rule it must
    /// still obey is that every entry names a directory or an exact file.
    /// The full type-level argument and the extension-glob control live in
    /// `scope::tests::include_entries_are_never_extension_globs`; this
    /// checks what actually lands in the file.
    #[test]
    fn a_narrowed_config_writes_only_directory_globs() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("platforms/macos")).unwrap();
        fs::create_dir_all(tmp.path().join("tools")).unwrap();
        fs::write(tmp.path().join("default.yml"), "org_settings: {}\n").unwrap();
        fs::write(tmp.path().join("platforms/macos/a.sh"), "#!/bin/sh\n").unwrap();
        fs::write(tmp.path().join("tools/build.sh"), "#!/bin/sh\n").unwrap();

        let scan = scope::scan(tmp.path());
        let mut selection = ScopeSelection::default();
        for unit in scan.top_level() {
            selection.decide(unit, unit.rel != "tools");
        }
        let answers = UserAnswers {
            strictness: StrictnessLevel::Moderate,
            scope: scope::preview(&scan, &selection),
        };

        let toml = generate_config_toml(&DetectedConfig::default(), &answers);
        let parsed: super::super::config::FleetLintConfig =
            toml::from_str(&toml).expect("generated config must parse");

        assert!(!parsed.files.include.is_empty(), "control: this selection narrows");
        for g in &parsed.files.include {
            assert!(!g.contains("*."), "include entry {g:?} is an extension glob");
        }
        assert!(!parsed.is_out_of_scope_file(Path::new("platforms/macos/a.sh")));
        assert!(parsed.is_out_of_scope_file(Path::new("tools/build.sh")));
    }

    /// End to end through the pieces the CLI orchestrates: `flint init` must
    /// write the VISIBLE `fleetlint.toml` and it must carry the answers.
    ///
    /// There is no trait to script any more — the library stopped driving the
    /// flow when the printing moved to the CLI, so a test just calls the same
    /// three functions `cli/src/commands/init.rs` calls, in the same order.
    #[test]
    fn writes_the_visible_config_with_the_chosen_scope() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("fleets")).unwrap();
        fs::create_dir_all(tmp.path().join("tools")).unwrap();
        fs::write(tmp.path().join("default.yml"), "org_settings: {}\n").unwrap();
        fs::write(tmp.path().join("fleets/a.yml"), "name: A\n").unwrap();
        fs::write(tmp.path().join("tools/build.sh"), "#!/bin/sh\n").unwrap();

        let scan = scope::scan(tmp.path());
        let mut selection = ScopeSelection::default();
        for unit in scan.top_level() {
            selection.decide(unit, unit.rel != "tools");
        }
        let answers = UserAnswers {
            strictness: StrictnessLevel::Strict,
            scope: scope::preview(&scan, &selection),
        };

        let path = config_path_for(tmp.path(), None);
        write_config(&path, &generate_config_toml(&detect_workspace(tmp.path()), &answers), false)
            .unwrap();

        assert_eq!(CONFIG_FILE_NAME, "fleetlint.toml");
        assert!(path.exists(), "init must write the visible spelling");
        assert!(
            !tmp.path().join(".fleetlint.toml").exists(),
            "init must not write the hidden spelling"
        );

        let written = fs::read_to_string(&path).unwrap();
        let parsed: super::super::config::FleetLintConfig = toml::from_str(&written).unwrap();
        assert_eq!(parsed.files.include, vec!["default.yml", "fleets/**"]);
        assert!(parsed.files.exclude.contains(&"tools/**".to_string()));
        assert!(parsed.schema.require_platform, "strict answer must survive");
    }

    /// The non-interactive path: no scope questions, no `include`, cross-file
    /// rules armed everywhere.
    #[test]
    fn non_interactive_init_narrows_nothing() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("default.yml"), "org_settings: {}\n").unwrap();

        let path = config_path_for(tmp.path(), None);
        let content =
            generate_config_toml(&detect_workspace(tmp.path()), &UserAnswers::default());
        write_config(&path, &content, false).unwrap();

        let parsed: super::super::config::FleetLintConfig =
            toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert!(parsed.files.include.is_empty());
        assert!(!parsed.is_out_of_scope_file(Path::new("anything/at/all.sh")));
    }

    /// `write_config` must refuse to clobber without `force` — the guard the
    /// CLI relies on when a repo already has a config.
    #[test]
    fn write_config_refuses_to_clobber_without_force() {
        let tmp = TempDir::new().unwrap();
        let path = config_path_for(tmp.path(), None);
        write_config(&path, "# first\n", false).unwrap();

        assert!(write_config(&path, "# second\n", false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "# first\n");

        write_config(&path, "# second\n", true).unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "# second\n");
    }
}
