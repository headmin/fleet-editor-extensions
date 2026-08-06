//! Linting engine — orchestrates rule execution and file traversal.
//!
//! The `Linter` struct is the main entry point. It loads configuration,
//! runs rules in parallel (via rayon), applies suppressions, and produces
//! a `LintReport` per file.

use super::config::FleetLintConfig;
use super::error::{LintError, LintReport, Severity};
use super::fleet_config::{
    FleetConfig, Label, LabelOrPath, Policy, PolicyOrPath, Query, QueryOrPath,
};
use super::rules::RuleSet;
use super::version_gate::VersionContext;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Linter {
    rules: RuleSet,
    config: Option<FleetLintConfig>,
    /// Directory containing the loaded `.fleetlint.toml`, when known.
    /// `[files]` include/exclude globs are resolved relative to this.
    config_root: Option<PathBuf>,
}

impl Linter {
    pub fn new() -> Self {
        Self {
            rules: RuleSet::default_rules(),
            config: None,
            config_root: None,
        }
    }

    pub fn with_rules(rules: RuleSet) -> Self {
        Self {
            rules,
            config: None,
            config_root: None,
        }
    }

    /// Create a linter with configuration.
    pub fn with_config(config: FleetLintConfig) -> Self {
        let version_ctx = VersionContext::resolve(
            Some(&config.deprecations.fleet_version),
            config.deprecations.future_names,
        );
        Self {
            rules: RuleSet::default_rules_with_version(version_ctx),
            config: Some(config),
            config_root: None,
        }
    }

    /// Create a linter by searching for configuration from a path.
    pub fn from_path(start_path: &Path) -> Self {
        let loaded = FleetLintConfig::find_and_load(start_path);
        let config_root = loaded.as_ref().and_then(|(config_path, _)| {
            let root = config_path.parent()?;
            Some(root.canonicalize().unwrap_or_else(|_| root.to_path_buf()))
        });
        let config = loaded.map(|(_, c)| c);
        let version_ctx = config
            .as_ref()
            .map(|c| {
                VersionContext::resolve(
                    Some(&c.deprecations.fleet_version),
                    c.deprecations.future_names,
                )
            })
            .unwrap_or_else(VersionContext::latest);
        Self {
            rules: RuleSet::default_rules_with_version(version_ctx),
            config,
            config_root,
        }
    }

    /// Get the current configuration, if any.
    pub fn config(&self) -> Option<&FleetLintConfig> {
        self.config.as_ref()
    }

    /// Get the current configuration mutably, if any.
    pub fn config_mut(&mut self) -> Option<&mut FleetLintConfig> {
        self.config.as_mut()
    }

    /// Set the configuration.
    pub fn set_config(&mut self, config: FleetLintConfig) {
        self.config = Some(config);
    }

    /// Set the directory that `[files]` include/exclude globs are relative
    /// to (the directory containing the loaded `.fleetlint.toml`).
    pub fn set_config_root(&mut self, root: PathBuf) {
        self.config_root = Some(root.canonicalize().unwrap_or(root));
    }

    /// Check whether a file should be linted per the config's `[files]`
    /// include/exclude patterns. Returns `true` when no config is loaded.
    ///
    /// Glob patterns are matched against the path relative to the config
    /// root when known, so `include = ["./fleets/*.yml"]` works regardless
    /// of whether callers pass absolute or relative paths.
    pub fn should_lint(&self, file_path: &Path) -> bool {
        let Some(config) = &self.config else {
            return true;
        };

        let candidate = self
            .config_root
            .as_ref()
            .and_then(|root| {
                let abs = file_path
                    .canonicalize()
                    .unwrap_or_else(|_| file_path.to_path_buf());
                abs.strip_prefix(root).ok().map(Path::to_path_buf)
            })
            .unwrap_or_else(|| file_path.to_path_buf());

        config.should_lint_file(&candidate)
    }

    /// Lint a single file
    pub fn lint_file(&self, file_path: &Path) -> Result<LintReport> {
        // Read file
        let source = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        self.lint_content(&source, file_path)
    }

    /// Lint content directly (for LSP - content already in memory).
    ///
    /// This method is useful when the file content is already available,
    /// such as in an LSP server where the client sends document content.
    pub fn lint_content(&self, content: &str, file_path: &Path) -> Result<LintReport> {
        // Run basic YAML hygiene checks first (before parsing)
        let mut report = LintReport::new();
        check_yaml_hygiene(content, file_path, &mut report);

        // Use file path to determine the expected type, then parse accordingly.
        // This prevents labels from being misidentified as policies, software files
        // from triggering policy checks, etc.
        let file_type = detect_file_type(file_path);

        // Software and agent-options lib files are not fleet configs —
        // return early with just hygiene checks, no structural/semantic rules.
        if matches!(
            file_type,
            FileType::Software | FileType::AgentOptions | FileType::NonYaml
        ) {
            return Ok(report);
        }

        let fleet_config: FleetConfig = match file_type {
            FileType::Labels => {
                // lib/*/labels/*.yml — parse as label array
                if let Ok(labels) = serde_yaml::from_str::<Vec<Label>>(content) {
                    FleetConfig {
                        labels: Some(labels.into_iter().map(LabelOrPath::Label).collect()),
                        ..Default::default()
                    }
                } else {
                    FleetConfig::default()
                }
            }
            FileType::Software | FileType::AgentOptions | FileType::NonYaml => {
                unreachable!("handled by early return above")
            }
            FileType::Policies => {
                // lib/*/policies/*.yml — parse as policy array
                if let Ok(policies) = serde_yaml::from_str::<Vec<Policy>>(content) {
                    FleetConfig {
                        policies: Some(policies.into_iter().map(PolicyOrPath::Policy).collect()),
                        ..Default::default()
                    }
                } else {
                    FleetConfig::default()
                }
            }
            FileType::Queries => {
                // lib/*/queries/*.yml or lib/*/reports/*.yml — parse as query array
                if let Ok(queries) = serde_yaml::from_str::<Vec<Query>>(content) {
                    FleetConfig {
                        queries: Some(queries.into_iter().map(QueryOrPath::Query).collect()),
                        ..Default::default()
                    }
                } else {
                    FleetConfig::default()
                }
            }
            FileType::FleetConfig => {
                // default.yml, fleets/*.yml, teams/*.yml — full fleet config
                match serde_yaml::from_str(content) {
                    Ok(config) => config,
                    Err(_) => {
                        // Last resort: try parsing as generic YAML for a parse error
                        match serde_yaml::from_str::<serde_yaml::Value>(content) {
                            Ok(_) => FleetConfig::default(),
                            Err(e) => {
                                let err_msg = e.to_string();

                                // Fleet's Go YAML parser accepts duplicate keys
                                // (e.g. multiple `path:` under `packages:` or
                                // `configuration_profiles:`). serde_yaml rejects
                                // them but this is valid Fleet GitOps YAML — skip.
                                if err_msg.contains("duplicate entry") {
                                    FleetConfig::default()
                                } else {
                                    let mut err = LintError::error(
                                        format!("YAML parse error: {}", e),
                                        file_path,
                                    )
                                    .with_rule_code("yaml-syntax".to_string());

                                    if let Some(location) = e.location() {
                                        err = err.with_location(location.line(), location.column());
                                    }

                                    report.add(err);
                                    return Ok(report);
                                }
                            }
                        }
                    }
                }
            }
        };

        // Run all rules
        // (report was initialized earlier with YAML hygiene checks)

        // Get disabled and warning rules from config
        let mut disabled_rules = self
            .config
            .as_ref()
            .map(|c| c.disabled_rules())
            .unwrap_or_default();
        let warning_rules = self
            .config
            .as_ref()
            .map(|c| c.warning_rules())
            .unwrap_or_default();

        // If allow_unknown_fields is enabled, disable structural validation
        let allow_unknown = self
            .config
            .as_ref()
            .map(|c| c.schema.allow_unknown_fields)
            .unwrap_or(false);
        if allow_unknown {
            disabled_rules.insert("structural-validation");
        }

        // Collect all errors first (for suppression filtering)
        let mut all_errors = Vec::new();

        for rule in self.rules.rules() {
            // Skip disabled rules
            if disabled_rules.contains(rule.name()) {
                continue;
            }

            let errors = rule.check(&fleet_config, file_path, content);

            // Downgrade to warnings if configured
            let should_warn = warning_rules.contains(rule.name());
            let rule_name = rule.name().to_string();

            for mut error in errors {
                if should_warn && error.severity == Severity::Error {
                    error.severity = Severity::Warning;
                }
                // Tag each error with its originating rule code
                if error.rule_code.is_none() {
                    error.rule_code = Some(rule_name.clone());
                }
                all_errors.push(error);
            }
        }

        // Apply inline suppressions (# fleet-lint: ignore [rule-code])
        let suppressions = parse_suppressions(content);
        if !suppressions.is_empty() {
            all_errors.retain(|error| !is_suppressed(error, &suppressions));
        }

        for error in all_errors {
            report.add(error);
        }

        Ok(report)
    }

    /// Lint multiple files. Uses rayon for parallel processing when > 3 files.
    pub fn lint_files(&self, files: &[&Path]) -> Result<Vec<(PathBuf, LintReport)>> {
        use rayon::prelude::*;

        let lint_one = |file: &&Path| -> (PathBuf, LintReport) {
            match self.lint_file(file) {
                Ok(report) => (file.to_path_buf(), report),
                Err(e) => {
                    let mut report = LintReport::new();
                    report.add(LintError::error(
                        format!("Failed to lint file: {}", e),
                        *file,
                    ));
                    (file.to_path_buf(), report)
                }
            }
        };

        let results = if files.len() > 3 {
            files.par_iter().map(lint_one).collect()
        } else {
            files.iter().map(lint_one).collect()
        };

        Ok(results)
    }

    /// Lint a directory recursively. Parallelized via rayon for large repos.
    pub fn lint_directory(
        &self,
        dir: &Path,
        pattern: Option<&str>,
    ) -> Result<Vec<(PathBuf, LintReport)>> {
        let pattern = pattern.unwrap_or("**/*.{yml,yaml}");

        // Find all YAML files, then apply the config's [files]
        // include/exclude patterns (issue #15)
        let yaml_files = find_yaml_files(dir, pattern)?;

        // Lint each file (parallel if > 3 files)
        let file_refs: Vec<&Path> = yaml_files
            .iter()
            .map(|p| p.as_path())
            .filter(|p| self.should_lint(p))
            .collect();
        self.lint_files(&file_refs)
    }
}

impl Default for Linter {
    fn default() -> Self {
        Self::new()
    }
}

/// File type classification based on path.
///
/// Used to determine how to parse a YAML file before attempting deserialization.
/// This prevents misidentification (e.g., labels parsed as policies).
#[derive(Debug, PartialEq)]
pub(crate) enum FileType {
    FleetConfig,  // default.yml, fleets/*.yml, teams/*.yml, unassigned.yml
    Policies,     // */policies/*.yml
    Queries,      // */queries/*.yml, */reports/*.yml
    Labels,       // */labels/*.yml, *.labels.yml
    Software,     // */software/*.yml
    AgentOptions, // agent-options*.yml
    NonYaml,      // profiles, scripts, icons, declarations, commands — not YAML to lint
}

/// Detect file type from path using directory names and file name patterns.
pub(crate) fn detect_file_type(path: &Path) -> FileType {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Agent options files
    if file_name.starts_with("agent-options") || file_name.starts_with("agent_options") {
        return FileType::AgentOptions;
    }

    // Check parent directory names
    if let Some(parent) = path.parent() {
        let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");

        match parent_name {
            "labels" => return FileType::Labels,
            "software" => return FileType::Software,
            "policies" => return FileType::Policies,
            "queries" | "reports" => return FileType::Queries,
            // v4.83 directories that contain non-YAML-to-lint files
            "configuration-profiles"
            | "declaration-profiles"
            | "enrollment-profiles"
            | "commands"
            | "scripts"
            | "icons"
            | "managed-app-configurations" => return FileType::NonYaml,
            _ => {}
        }
    }

    // File name patterns
    if file_name.contains(".labels.") {
        return FileType::Labels;
    }

    // Everything else is a fleet config (default.yml, fleets/*.yml, teams/*.yml, etc.)
    FileType::FleetConfig
}

/// Basic YAML hygiene checks that run before parsing.
///
/// These catch issues that serde_yaml would either silently accept or report
/// as opaque parse errors. Running them first gives clear, actionable diagnostics.
fn check_yaml_hygiene(content: &str, file: &Path, report: &mut LintReport) {
    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;

        // Tab indentation — YAML spec allows tabs but they cause subtle bugs
        if line.starts_with('\t') || (line.starts_with(' ') && line.contains('\t')) {
            let col = line.find('\t').unwrap_or(0) + 1;
            report.add(
                LintError::warning(
                    "Tab character found — use spaces for YAML indentation",
                    file,
                )
                .with_location(line_num, col)
                .with_rule_code("yaml-tabs".to_string())
                .with_help("YAML indentation must use spaces, not tabs"),
            );
        }

        // Trailing whitespace
        if line.len() > 1 && line != line.trim_end() && !line.trim().is_empty() {
            report.add(
                LintError::info("Trailing whitespace", file)
                    .with_location(line_num, line.trim_end().len() + 1)
                    .with_rule_code("yaml-trailing-whitespace".to_string()),
            );
        }
    }

    // Duplicate top-level keys (YAML spec says last wins, but it's almost always a mistake)
    let mut seen_keys: HashMap<String, usize> = HashMap::new();
    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        // Only check top-level keys (no leading whitespace, not a comment, not a list item)
        if !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.starts_with('#')
            && !trimmed.starts_with('-')
            && !trimmed.is_empty()
        {
            if let Some(key) = trimmed.split(':').next() {
                let key = key.trim().to_string();
                if !key.is_empty() {
                    if let Some(prev_line) = seen_keys.get(&key) {
                        report.add(
                            LintError::error(
                                format!("Duplicate top-level key '{}' (first seen at line {})", key, prev_line),
                                file,
                            )
                            .with_location(line_num, 1)
                            .with_rule_code("yaml-duplicate-key".to_string())
                            .with_help("YAML uses the last occurrence of duplicate keys — the first one is silently ignored")
                        );
                    } else {
                        seen_keys.insert(key, line_num);
                    }
                }
            }
        }
    }
}

/// Find YAML files in directory
fn find_yaml_files(dir: &Path, _pattern: &str) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    // Simple recursive search for YAML files
    fn visit_dirs(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    // Skip hidden directories and common ignores
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy();
                        if name_str.starts_with('.')
                            || name_str == "node_modules"
                            || name_str == "target"
                            || name_str == "dist"
                        {
                            continue;
                        }
                    }
                    visit_dirs(&path, files)?;
                } else if let Some(ext) = path.extension() {
                    if ext == "yml" || ext == "yaml" {
                        // Skip CI config files and other non-Fleet YAML
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if name.starts_with('.')
                                || name == "docker-compose.yml"
                                || name == "docker-compose.yaml"
                                || name == "action.yml"
                                || name == "action.yaml"
                            {
                                continue;
                            }
                        }
                        files.push(path);
                    }
                }
            }
        }
        Ok(())
    }

    visit_dirs(dir, &mut files)?;
    Ok(files)
}

// ============================================================================
// Inline Suppression Support
// ============================================================================

/// Parse inline suppression comments from YAML source.
///
/// Supports two forms:
/// - `# fleet-lint: ignore` — suppress all rules on this line
/// - `# fleet-lint: ignore rule-code` — suppress a specific rule
/// - `# fleet-lint: ignore rule-a, rule-b` — suppress multiple rules
///
/// Returns a map of 1-indexed line numbers to suppressed rule codes.
/// An empty Vec means "suppress all rules on this line".
fn parse_suppressions(source: &str) -> HashMap<usize, Vec<String>> {
    let mut suppressions = HashMap::new();

    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1; // 1-indexed to match LintError.line

        if let Some(comment_start) = line.find('#') {
            let comment = line[comment_start + 1..].trim();

            if let Some(rest) = comment.strip_prefix("fleet-lint:") {
                let rest = rest.trim();
                if let Some(codes) = rest.strip_prefix("ignore") {
                    let codes = codes.trim();
                    if codes.is_empty() {
                        // Ignore all rules
                        suppressions.insert(line_num, Vec::new());
                    } else {
                        // Ignore specific rule(s)
                        let rule_codes: Vec<String> = codes
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        suppressions.insert(line_num, rule_codes);
                    }
                }
            }
        }
    }

    suppressions
}

/// Check if a lint error is suppressed by an inline comment.
///
/// An error is suppressed if:
/// - Its line has a same-line suppression comment matching the rule code
/// - The line immediately before it has a standalone suppression comment matching the rule code
fn is_suppressed(error: &LintError, suppressions: &HashMap<usize, Vec<String>>) -> bool {
    let line = match error.line {
        Some(l) => l,
        None => return false, // Can't suppress errors without line info
    };

    // Check same-line suppression
    if let Some(codes) = suppressions.get(&line) {
        if matches_suppression(error, codes) {
            return true;
        }
    }

    // Check previous-line suppression
    if line > 1 {
        if let Some(codes) = suppressions.get(&(line - 1)) {
            if matches_suppression(error, codes) {
                return true;
            }
        }
    }

    false
}

/// Check if an error matches a suppression rule list.
/// Empty list means "suppress all". Otherwise, the error's rule_code must be in the list.
fn matches_suppression(error: &LintError, codes: &[String]) -> bool {
    if codes.is_empty() {
        return true; // Suppress all rules
    }
    if let Some(rule_code) = &error.rule_code {
        codes.iter().any(|c| c == rule_code)
    } else {
        false
    }
}

#[cfg(test)]
mod suppression_tests {
    use super::*;

    #[test]
    fn test_parse_suppression_ignore_all() {
        let source = "platform: macos  # fleet-lint: ignore\n";
        let s = parse_suppressions(source);
        assert_eq!(s.len(), 1);
        assert!(s.get(&1).unwrap().is_empty()); // empty = all rules
    }

    #[test]
    fn test_parse_suppression_specific_rule() {
        let source = "platform: macos  # fleet-lint: ignore type-validation\n";
        let s = parse_suppressions(source);
        assert_eq!(s.get(&1).unwrap(), &vec!["type-validation".to_string()]);
    }

    #[test]
    fn test_parse_suppression_multiple_rules() {
        let source = "query: bad  # fleet-lint: ignore query-syntax, type-validation\n";
        let s = parse_suppressions(source);
        let codes = s.get(&1).unwrap();
        assert_eq!(codes.len(), 2);
        assert!(codes.contains(&"query-syntax".to_string()));
        assert!(codes.contains(&"type-validation".to_string()));
    }

    #[test]
    fn test_parse_suppression_standalone_line() {
        let source = "# fleet-lint: ignore type-validation\nplatform: macos\n";
        let s = parse_suppressions(source);
        assert!(s.contains_key(&1)); // suppression on line 1
        assert!(!s.contains_key(&2)); // no suppression on line 2
    }

    #[test]
    fn test_is_suppressed_same_line() {
        let mut suppressions = HashMap::new();
        suppressions.insert(5, vec!["type-validation".to_string()]);

        let error = LintError::error("test", "test.yml")
            .with_location(5, 1)
            .with_rule_code("type-validation");
        assert!(is_suppressed(&error, &suppressions));

        let error2 = LintError::error("test", "test.yml")
            .with_location(5, 1)
            .with_rule_code("other-rule");
        assert!(!is_suppressed(&error2, &suppressions));
    }

    #[test]
    fn test_is_suppressed_previous_line() {
        let mut suppressions = HashMap::new();
        suppressions.insert(4, vec!["type-validation".to_string()]);

        let error = LintError::error("test", "test.yml")
            .with_location(5, 1)
            .with_rule_code("type-validation");
        assert!(is_suppressed(&error, &suppressions));
    }

    #[test]
    fn test_is_suppressed_all_rules() {
        let mut suppressions = HashMap::new();
        suppressions.insert(5, Vec::new()); // empty = all rules

        let error = LintError::error("test", "test.yml")
            .with_location(5, 1)
            .with_rule_code("any-rule");
        assert!(is_suppressed(&error, &suppressions));
    }

    #[test]
    fn test_no_suppression_without_line() {
        let mut suppressions = HashMap::new();
        suppressions.insert(5, Vec::new());

        let error = LintError::error("test", "test.yml"); // no line info
        assert!(!is_suppressed(&error, &suppressions));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Regression test for issue #15: lint_directory must honor the
    // [files] include/exclude patterns from .fleetlint.toml.
    #[test]
    fn test_lint_directory_respects_config_include_exclude() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(
            root.join(".fleetlint.toml"),
            r#"
[files]
include = ["./fleets/*.yml", "./lib/**/*.yaml"]
exclude = ["**/node_modules/**", "cspell.config.yaml"]
"#,
        )
        .unwrap();

        std::fs::create_dir_all(root.join("fleets")).unwrap();
        std::fs::create_dir_all(root.join("lib/macos")).unwrap();

        let policy_yaml = "policies:\n  - name: Test\n    query: SELECT 1;\n    platform: darwin\n";
        std::fs::write(root.join("fleets/prod.yml"), policy_yaml).unwrap();
        std::fs::write(root.join("lib/macos/policies.yaml"), policy_yaml).unwrap();
        // Files that must NOT be linted
        std::fs::write(root.join("cspell.config.yaml"), "version: '0.2'\nwords: []\n").unwrap();
        std::fs::write(root.join("random.yml"), "foo: bar\n").unwrap();

        let linter = Linter::from_path(root);
        let results = linter.lint_directory(root, None).unwrap();

        let linted: Vec<String> = results
            .iter()
            .map(|(p, _)| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();

        assert!(linted.contains(&"prod.yml".to_string()), "include pattern should match fleets/prod.yml, got {linted:?}");
        assert!(linted.contains(&"policies.yaml".to_string()), "include pattern should match lib/**, got {linted:?}");
        assert!(!linted.contains(&"cspell.config.yaml".to_string()), "excluded file was linted: {linted:?}");
        assert!(!linted.contains(&"random.yml".to_string()), "file outside include list was linted: {linted:?}");
    }

    #[test]
    fn test_lint_directory_no_config_lints_all_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();

        std::fs::write(root.join("a.yml"), "foo: bar\n").unwrap();
        std::fs::write(root.join("b.yaml"), "foo: bar\n").unwrap();

        let linter = Linter::new();
        let results = linter.lint_directory(root, None).unwrap();
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_lint_valid_config() {
        let yaml = r#"
policies:
  - name: "Test Policy"
    query: "SELECT 1 FROM users;"
    platform: darwin
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(file.path()).unwrap();

        assert!(!report.has_errors());
    }

    #[test]
    fn test_lint_missing_required_field() {
        let yaml = r#"
policies:
  - name: "Test Policy"
    # Missing query field
    platform: darwin
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(file.path()).unwrap();

        assert!(report.has_errors());
        assert!(report
            .errors
            .iter()
            .any(|e| e.message.contains("missing required field 'query'")));
    }

    #[test]
    fn test_lint_invalid_platform() {
        let yaml = r#"
policies:
  - name: "Test Policy"
    query: "SELECT 1;"
    platform: macos  # Should be 'darwin'
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(file.path()).unwrap();

        assert!(report.has_errors());
        assert!(report
            .errors
            .iter()
            .any(|e| e.message.contains("invalid platform")));
    }

    #[test]
    fn test_platform_compatibility() {
        let yaml = r#"
policies:
  - name: "Windows Firewall"
    query: "SELECT * FROM alf;"  # alf is macOS-only
    platform: windows
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(file.path()).unwrap();

        assert!(report.has_errors());
        assert!(report
            .errors
            .iter()
            .any(|e| e.message.contains("not available on platform")));
    }

    // ── File type detection tests ────────────────────────────────

    #[test]
    fn test_detect_file_type_fleet_config() {
        assert_eq!(
            detect_file_type(Path::new("default.yml")),
            FileType::FleetConfig
        );
        assert_eq!(
            detect_file_type(Path::new("fleets/engineering.yml")),
            FileType::FleetConfig
        );
        assert_eq!(
            detect_file_type(Path::new("teams/ops.yml")),
            FileType::FleetConfig
        );
    }

    #[test]
    fn test_detect_file_type_labels() {
        assert_eq!(
            detect_file_type(Path::new("labels/macos.yml")),
            FileType::Labels
        );
        assert_eq!(
            detect_file_type(Path::new("lib/all/labels/hosts.yml")),
            FileType::Labels
        );
        assert_eq!(
            detect_file_type(Path::new("my.labels.yml")),
            FileType::Labels
        );
    }

    #[test]
    fn test_detect_file_type_software() {
        assert_eq!(
            detect_file_type(Path::new("lib/macos/software/slack.yml")),
            FileType::Software
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/macos/software/chrome.yml")),
            FileType::Software
        );
    }

    #[test]
    fn test_detect_file_type_policies() {
        assert_eq!(
            detect_file_type(Path::new("lib/macos/policies/filevault.yml")),
            FileType::Policies
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/macos/policies/security.yml")),
            FileType::Policies
        );
    }

    #[test]
    fn test_detect_file_type_queries() {
        assert_eq!(
            detect_file_type(Path::new("lib/macos/queries/uptime.yml")),
            FileType::Queries
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/all/reports/compliance.yml")),
            FileType::Queries
        );
    }

    #[test]
    fn test_detect_file_type_agent_options() {
        assert_eq!(
            detect_file_type(Path::new("lib/agent-options.yml")),
            FileType::AgentOptions
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/all/agent-options.yml")),
            FileType::AgentOptions
        );
    }

    #[test]
    fn test_detect_file_type_non_yaml() {
        assert_eq!(
            detect_file_type(Path::new(
                "platforms/macos/configuration-profiles/wifi.mobileconfig"
            )),
            FileType::NonYaml
        );
        assert_eq!(
            detect_file_type(Path::new(
                "platforms/macos/declaration-profiles/activation.json"
            )),
            FileType::NonYaml
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/macos/scripts/setup.sh")),
            FileType::NonYaml
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/macos/commands/restart.plist")),
            FileType::NonYaml
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/all/icons/slack.png")),
            FileType::NonYaml
        );
    }

    // ── Label linting tests ─────────────────────────────────────

    #[test]
    fn test_label_dynamic_with_query_no_error() {
        let yaml = r#"
- name: macOS Hosts
  description: All macOS hosts
  platform: darwin
  label_membership_type: dynamic
  query: "SELECT 1 FROM os_version WHERE name = 'macOS';"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        // Rename to labels dir to trigger label parsing
        let label_dir = tempfile::tempdir().unwrap();
        let label_path = label_dir.path().join("labels");
        std::fs::create_dir_all(&label_path).unwrap();
        let label_file = label_path.join("macos.yml");
        std::fs::write(&label_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&label_file).unwrap();
        assert!(
            !report.has_errors(),
            "Dynamic label with query should have no errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_label_manual_no_error() {
        let yaml = r#"
- name: VIP Hosts
  description: Manually managed VIP hosts
  label_membership_type: manual
  hosts:
    - host1.example.com
    - host2.example.com
"#;
        let label_dir = tempfile::tempdir().unwrap();
        let label_path = label_dir.path().join("labels");
        std::fs::create_dir_all(&label_path).unwrap();
        let label_file = label_path.join("vip.yml");
        std::fs::write(&label_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&label_file).unwrap();
        assert!(
            !report.has_errors(),
            "Manual label should have no errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_label_host_vitals_no_error() {
        // As of the Fleet version this code tracks, parseHostVitalCriteria only
        // registers `end_user_idp_group` and `end_user_idp_department`, and
        // rejects and/or composites outright. Keep this test aligned with what
        // Fleet actually parses today.
        let yaml = r#"
- name: Engineering
  description: Hosts assigned to the Engineering IdP group
  label_membership_type: host_vitals
  criteria:
    vital: end_user_idp_department
    value: Engineering
"#;
        let label_dir = tempfile::tempdir().unwrap();
        let label_path = label_dir.path().join("labels");
        std::fs::create_dir_all(&label_path).unwrap();
        let label_file = label_path.join("sequoia.yml");
        std::fs::write(&label_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&label_file).unwrap();
        assert!(
            !report.has_errors(),
            "host_vitals label should have no errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_label_criteria_no_error() {
        let yaml = r#"
- name: Engineering IdP group
  description: Hosts whose end user is in the Engineering IdP group
  label_membership_type: host_vitals
  criteria:
    vital: end_user_idp_group
    value: Engineering
"#;
        let label_dir = tempfile::tempdir().unwrap();
        let label_path = label_dir.path().join("labels");
        std::fs::create_dir_all(&label_path).unwrap();
        let label_file = label_path.join("macos15.yml");
        std::fs::write(&label_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&label_file).unwrap();
        assert!(
            !report.has_errors(),
            "criteria label should have no errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_software_file_skips_rules() {
        let yaml = r#"
hash_sha256: abc123def456
"#;
        let sw_dir = tempfile::tempdir().unwrap();
        let sw_path = sw_dir.path().join("software");
        std::fs::create_dir_all(&sw_path).unwrap();
        let sw_file = sw_path.join("slack.yml");
        std::fs::write(&sw_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&sw_file).unwrap();
        // Software files skip structural rules — only hygiene checks
        assert!(
            !report.has_errors(),
            "Software file should skip structural validation: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_agent_options_file_skips_rules() {
        let yaml = r#"
config:
  decorators:
    load:
      - SELECT uuid AS host_uuid FROM system_info;
"#;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agent-options.yml");
        std::fs::write(&file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&file).unwrap();
        assert!(
            !report.has_errors(),
            "Agent options file should skip structural validation: {:?}",
            report.errors
        );
    }

    // -- Issue #5: .fleetlint.toml is loaded by Linter::from_path --

    #[test]
    fn test_from_path_loads_fleetlint_toml() {
        // Without config a missing-query policy yields a `required-fields`
        // error. With `[rules.required-fields] enabled = false` the rule
        // should be silenced. Regression for issue #5.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".fleetlint.toml"),
            "[rules]\ndisabled = [\"required-fields\"]\n",
        )
        .unwrap();
        let yaml_file = dir.path().join("default.yml");
        std::fs::write(&yaml_file, "policies:\n  - name: missing query\n").unwrap();

        let linter = Linter::from_path(&yaml_file);
        let report = linter.lint_file(&yaml_file).unwrap();
        let has_required_fields_error = report
            .errors
            .iter()
            .any(|e| e.rule_code.as_deref() == Some("required-fields"));
        assert!(
            !has_required_fields_error,
            "required-fields rule should be disabled by .fleetlint.toml; got errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_new_does_not_load_config() {
        // Documents the historical behavior of Linter::new(): it does NOT
        // load .fleetlint.toml. CLI now uses from_path() (issue #5 fix);
        // keep this test so any future "make new() load config" change is
        // a deliberate decision.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".fleetlint.toml"),
            "[rules]\ndisabled = [\"required-fields\"]\n",
        )
        .unwrap();
        let yaml_file = dir.path().join("default.yml");
        std::fs::write(&yaml_file, "policies:\n  - name: missing query\n").unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&yaml_file).unwrap();
        let has_required_fields_error = report
            .errors
            .iter()
            .any(|e| e.rule_code.as_deref() == Some("required-fields"));
        assert!(
            has_required_fields_error,
            "Linter::new() ignores .fleetlint.toml — required-fields should still fire"
        );
    }
}
