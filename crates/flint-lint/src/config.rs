//! Configuration file support for Fleet linter.
//!
//! Supports `.fleetlint.toml` configuration files that allow teams
//! to customize linting behavior and share settings via version control.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Configuration file name (repo-local, in `.fleetlint.toml` form).
pub const CONFIG_FILE_NAME: &str = ".fleetlint.toml";

/// Resolve the user-level config path from the process environment.
///
/// Search order: `$XDG_CONFIG_HOME/flint/config.toml`, then
/// `$HOME/.config/flint/config.toml`. Returns `None` if neither env var
/// is set (minimal CI containers, daemonized contexts).
pub(crate) fn user_config_path() -> Option<PathBuf> {
    resolve_user_config_path(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Pure resolver — testable without env mutation. Production callers go
/// through [`user_config_path`]; tests construct env values inline.
///
/// An empty `XDG_CONFIG_HOME` is treated as unset (matches XDG basedir
/// spec: "if XDG_CONFIG_HOME is either not set or empty, a default of
/// $HOME/.config should be used").
fn resolve_user_config_path(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = xdg
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("flint").join("config.toml"))
}

/// Fleet linter configuration loaded from `.fleetlint.toml`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FleetLintConfig {
    /// Rule configuration.
    pub rules: RulesConfig,

    /// Validation thresholds.
    pub thresholds: ThresholdsConfig,

    /// File patterns to include/exclude.
    pub files: FilesConfig,

    /// Schema validation options.
    pub schema: SchemaConfig,

    /// Deprecation settings.
    pub deprecations: DeprecationsConfig,

    /// Fleet server connection settings.
    pub fleet: FleetConnectionConfig,
}

/// Rule enable/disable configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RulesConfig {
    /// Rules to disable (by name).
    /// Example: `disabled = ["query-syntax", "interval-validation"]`
    #[serde(default)]
    pub disabled: Vec<String>,

    /// Rules to set as warnings instead of errors.
    /// Example: `warn = ["duplicate-names"]`
    #[serde(default)]
    pub warn: Vec<String>,

    /// Additional custom rule configurations.
    #[serde(flatten)]
    pub custom: std::collections::HashMap<String, toml::Value>,
}

/// Threshold configuration for various checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThresholdsConfig {
    /// Minimum allowed interval in seconds (default: 60).
    pub min_interval: i64,

    /// Maximum allowed interval in seconds (default: 86400).
    pub max_interval: i64,

    /// Maximum query length in characters (default: 10000).
    pub max_query_length: usize,

    /// Whether to warn on SELECT * queries (default: true).
    pub warn_select_star: bool,

    /// Whether to warn on trailing semicolons (default: true).
    pub warn_trailing_semicolon: bool,
}

impl Default for ThresholdsConfig {
    fn default() -> Self {
        Self {
            min_interval: 60,
            max_interval: 86400,
            warn_select_star: true,
            warn_trailing_semicolon: true,
            max_query_length: 10000,
        }
    }
}

/// File include/exclude patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesConfig {
    /// Glob patterns to include.
    /// Default: `["**/*.yml", "**/*.yaml"]`
    #[serde(default = "default_include_patterns")]
    pub include: Vec<String>,

    /// Glob patterns to exclude.
    /// Default: `["**/node_modules/**", "**/target/**", "**/.git/**"]`
    #[serde(default = "default_exclude_patterns")]
    pub exclude: Vec<String>,

    /// Root directory for file resolution (relative to config file).
    pub root: Option<String>,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self {
            include: default_include_patterns(),
            exclude: default_exclude_patterns(),
            root: None,
        }
    }
}

fn default_include_patterns() -> Vec<String> {
    vec!["**/*.yml".to_string(), "**/*.yaml".to_string()]
}

fn default_exclude_patterns() -> Vec<String> {
    vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/.git/**".to_string(),
        "**/dist/**".to_string(),
    ]
}

/// Schema validation options.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SchemaConfig {
    /// Whether to validate against Fleet's JSON schema (default: true).
    pub validate: bool,

    /// Whether to allow unknown fields (default: false).
    /// Set to `true` to disable structural validation.
    pub allow_unknown_fields: bool,

    /// Whether to require explicit platform specification (default: false).
    pub require_platform: bool,
}

impl Default for SchemaConfig {
    fn default() -> Self {
        Self {
            validate: true,
            allow_unknown_fields: false,
            require_platform: false,
        }
    }
}

/// Deprecation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeprecationsConfig {
    /// Target Fleet version for deprecation phase calculation.
    /// Accepts a semver string (e.g. `"4.80.0"`) or `"latest"`.
    pub fleet_version: String,

    /// Opt in to future naming conventions before they become mandatory.
    /// When `true`, completions suggest new names (`reports`, `settings`, `fleets/`)
    /// and deprecation warnings fire on old names (`queries`, `team_settings`, `teams/`).
    pub future_names: bool,
}

impl Default for DeprecationsConfig {
    fn default() -> Self {
        Self {
            fleet_version: "latest".to_string(),
            future_names: false,
        }
    }
}

/// Fleet server connection configuration.
///
/// Credentials are resolved in order:
/// 1. Fields in `.fleetlint.toml` (`url`, `token`)
/// 2. Environment variables (`FLEET_URL`, `FLEET_API_TOKEN`)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct FleetConnectionConfig {
    /// Enable gitops dry-run validation on save (default: false).
    pub gitops_validation: bool,

    /// Enable live completions from Fleet instance (default: false).
    pub live_completions: bool,

    /// Fleet server URL. Falls back to `FLEET_URL` env var.
    #[serde(default)]
    pub url: String,

    /// Fleet API token. Falls back to `FLEET_API_TOKEN` env var.
    /// Avoid committing tokens — use env vars in shared repos.
    #[serde(default)]
    pub token: String,

    /// Path to fleetctl binary. Falls back to `fleetctl` on PATH.
    #[serde(default)]
    pub fleetctl_path: String,

    /// Extra environment variables to pass to fleetctl.
    /// Values support `op://` references for 1Password secrets.
    /// These are needed when gitops YAML references `$VAR` placeholders.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

impl FleetConnectionConfig {
    /// Resolve Fleet URL: config field first, then env var.
    /// Supports `op://` references resolved via 1Password CLI.
    pub fn resolved_url(&self) -> Option<String> {
        if !self.url.is_empty() {
            return Some(resolve_secret(&self.url));
        }
        std::env::var("FLEET_URL").ok().filter(|s| !s.is_empty())
    }

    /// Resolve Fleet API token: config field first, then env var.
    /// Supports `op://` references resolved via 1Password CLI.
    pub fn resolved_token(&self) -> Option<String> {
        if !self.token.is_empty() {
            return Some(resolve_secret(&self.token));
        }
        std::env::var("FLEET_API_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
    }

    /// Resolve fleetctl binary path: config field first, then PATH lookup.
    pub fn resolved_fleetctl(&self) -> String {
        if !self.fleetctl_path.is_empty() {
            self.fleetctl_path.clone()
        } else {
            "fleetctl".to_string()
        }
    }

    /// Resolve all extra env vars (with `op://` support) into key-value pairs.
    pub fn resolved_env(&self) -> Vec<(String, String)> {
        self.env
            .iter()
            .map(|(k, v)| (k.clone(), resolve_secret(v)))
            .collect()
    }

    /// Check if any Fleet features are enabled and credentials are available.
    pub fn is_active(&self) -> bool {
        (self.gitops_validation || self.live_completions)
            && self.resolved_url().is_some()
            && self.resolved_token().is_some()
    }
}

/// Resolve a config value that may be a 1Password secret reference.
///
/// If the value starts with `op://`, runs `op read <ref>` to fetch the secret.
/// Otherwise returns the value as-is.
fn resolve_secret(value: &str) -> String {
    if value.starts_with("op://") {
        match std::process::Command::new("op")
            .args(["read", value])
            .output()
        {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Ok(output) => {
                eprintln!(
                    "op read failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                value.to_string()
            }
            Err(e) => {
                eprintln!("Failed to run `op`: {e} — is 1Password CLI installed?");
                value.to_string()
            }
        }
    } else {
        value.to_string()
    }
}

impl FleetLintConfig {
    /// Load configuration from a file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(path.to_path_buf(), e.to_string()))?;

        Self::parse(&content)
    }

    /// Parse configuration from a TOML string.
    pub fn parse(content: &str) -> Result<Self, ConfigError> {
        toml::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// Find and load configuration, walking up from `start_path` and
    /// then falling back to the user's XDG config directory.
    ///
    /// Search order (first match wins):
    /// 1. `.fleetlint.toml` in `start_path` or any ancestor directory.
    /// 2. `$XDG_CONFIG_HOME/flint/config.toml` (or `$HOME/.config/flint/config.toml`).
    ///
    /// The user-level fallback lets contributors share lint preferences
    /// across multiple Fleet GitOps checkouts without committing a config
    /// to each repo — matches fleet-plan's `~/.config/fleet-plan.json`
    /// convention. Repo-local config always wins, so a team file can
    /// override a personal one without surprises.
    pub fn find_and_load(start_path: &Path) -> Option<(PathBuf, Self)> {
        if let Some(hit) = Self::find_in_ancestors(start_path) {
            return Some(hit);
        }
        Self::find_in_user_config()
    }

    /// Walk up from `start_path` looking for `.fleetlint.toml` (legacy
    /// behavior, preserved for backward compatibility). Returns the
    /// repo-local config if found and parseable.
    fn find_in_ancestors(start_path: &Path) -> Option<(PathBuf, Self)> {
        let mut current = if start_path.is_file() {
            start_path.parent()?.to_path_buf()
        } else {
            start_path.to_path_buf()
        };

        loop {
            let config_path = current.join(CONFIG_FILE_NAME);
            if config_path.exists() {
                match Self::from_file(&config_path) {
                    Ok(config) => return Some((config_path, config)),
                    Err(_) => return None, // Config exists but is invalid
                }
            }

            match current.parent() {
                Some(parent) => current = parent.to_path_buf(),
                None => return None,
            }
        }
    }

    /// Look up the user-level config at `$XDG_CONFIG_HOME/flint/config.toml`
    /// (or `$HOME/.config/flint/config.toml` if XDG_CONFIG_HOME isn't set).
    /// Returns `None` if no home directory can be resolved, the file
    /// doesn't exist, or it fails to parse.
    fn find_in_user_config() -> Option<(PathBuf, Self)> {
        let path = user_config_path()?;
        if !path.exists() {
            return None;
        }
        match Self::from_file(&path) {
            Ok(config) => Some((path, config)),
            Err(_) => None,
        }
    }

    /// Check if a rule is disabled.
    pub fn is_rule_disabled(&self, rule_name: &str) -> bool {
        self.rules.disabled.iter().any(|r| r == rule_name)
    }

    /// Check if a rule should be downgraded to warning.
    pub fn is_rule_warning(&self, rule_name: &str) -> bool {
        self.rules.warn.iter().any(|r| r == rule_name)
    }

    /// Get the set of disabled rules.
    pub fn disabled_rules(&self) -> HashSet<&str> {
        self.rules.disabled.iter().map(|s| s.as_str()).collect()
    }

    /// Get the set of warning-only rules.
    pub fn warning_rules(&self) -> HashSet<&str> {
        self.rules.warn.iter().map(|s| s.as_str()).collect()
    }

    /// Check if a file path should be linted based on include/exclude patterns.
    pub fn should_lint_file(&self, file_path: &Path) -> bool {
        let path_str = file_path.to_string_lossy();

        // Check excludes first
        for pattern in &self.files.exclude {
            if matches_glob(pattern, &path_str) {
                return false;
            }
        }

        // Then check includes
        for pattern in &self.files.include {
            if matches_glob(pattern, &path_str) {
                return true;
            }
        }

        // Default to including YAML files
        matches!(
            file_path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        )
    }

    /// Write a default configuration to a path.
    pub fn write_default(path: &Path) -> Result<(), ConfigError> {
        let default_config = Self::default_with_comments();
        std::fs::write(path, default_config)
            .map_err(|e| ConfigError::WriteError(path.to_path_buf(), e.to_string()))
    }

    /// Generate default configuration with explanatory comments.
    pub fn default_with_comments() -> String {
        r#"# Fleet Linter Configuration
# Place this file at the root of your GitOps repository as `.fleetlint.toml`

# Rule Configuration
[rules]
# Rules to disable entirely (by name)
# Available rules:
#   - required-fields: Ensures required fields are present
#   - platform-compatibility: Validates osquery tables work on specified platform
#   - type-validation: Validates field types
#   - security: Detects hardcoded secrets
#   - interval-validation: Warns about extreme interval values
#   - duplicate-names: Detects duplicate policy/query/label names
#   - query-syntax: Validates SQL query syntax
#   - structural-validation: Validates YAML structure (unknown/misplaced keys)
disabled = []

# Rules to downgrade from error to warning
warn = []

# Threshold Configuration
[thresholds]
# Minimum query interval in seconds (default: 60)
min_interval = 60

# Maximum query interval in seconds (default: 86400 = 24 hours)
max_interval = 86400

# Maximum query length in characters (default: 10000)
max_query_length = 10000

# Warn when using SELECT * (default: true)
warn_select_star = true

# Warn on trailing semicolons in queries (default: true)
warn_trailing_semicolon = true

# File Patterns
[files]
# Glob patterns to include
include = ["**/*.yml", "**/*.yaml"]

# Glob patterns to exclude
exclude = [
    "**/node_modules/**",
    "**/target/**",
    "**/.git/**",
    "**/dist/**",
]

# Optional: Root directory for path resolution (relative to this config file)
# root = "."

# Schema Validation
[schema]
# Validate against Fleet's schema (default: true)
validate = true

# Allow unknown/extra fields (default: false)
# Set to true to disable structural validation of YAML keys
allow_unknown_fields = false

# Require explicit platform specification (default: false)
require_platform = false

# Deprecation Settings
[deprecations]
# Target Fleet version for deprecation warnings (default: "latest")
# Set to a specific version like "4.80.0" to see deprecations for that version
fleet_version = "latest"

# Opt in to future naming conventions (default: false)
# When true, completions suggest new names and old names get deprecation warnings:
#   queries -> reports, team_settings -> settings, teams/ -> fleets/
future_names = false

# Fleet Server Connection
[fleet]
# Enable gitops validation on save (default: false)
gitops_validation = false

# Enable live completions from Fleet instance (default: false)
live_completions = false

# Fleet server URL (falls back to FLEET_URL env var)
# url = "https://fleet.example.com"

# Fleet API token (falls back to FLEET_API_TOKEN env var)
# Supports 1Password references: op://vault/item/field
# token = ""
# token = "op://Work/Fleet/api-token"

# Path to fleetctl binary (falls back to "fleetctl" on PATH)
# fleetctl_path = "/usr/local/bin/fleetctl"

# Extra env vars passed to fleetctl (for $VAR references in gitops YAML)
# Values support op:// references for 1Password secrets
# [fleet.env]
# FLEET_GLOBAL_ENROLL_SECRET = "op://Vault/Item/field"
"#
        .to_string()
    }
}

/// Simple glob pattern matching.
fn matches_glob(pattern: &str, path: &str) -> bool {
    // Convert glob pattern to regex
    let mut regex_pattern = String::new();
    let mut chars = pattern.chars().peekable();
    let mut at_start = true;

    while let Some(c) = chars.next() {
        match c {
            '*' => {
                // Check for **
                if chars.peek() == Some(&'*') {
                    chars.next(); // consume second *
                                  // ** matches any path segment (including /)
                                  // Skip following / if present
                    if chars.peek() == Some(&'/') {
                        chars.next();
                    }
                    // At start of pattern, ** can match empty string (for paths like "node_modules/foo")
                    if at_start {
                        regex_pattern.push_str("(.*/)?");
                    } else {
                        regex_pattern.push_str("(.*)?");
                    }
                } else {
                    // Single * matches anything except /
                    regex_pattern.push_str("[^/]*");
                }
                at_start = false;
            }
            '?' => {
                // ? matches any single character except /
                regex_pattern.push_str("[^/]");
                at_start = false;
            }
            '.' | '+' | '^' | '$' | '(' | ')' | '[' | ']' | '{' | '}' | '|' | '\\' => {
                // Escape regex special characters
                regex_pattern.push('\\');
                regex_pattern.push(c);
                at_start = false;
            }
            _ => {
                regex_pattern.push(c);
                at_start = false;
            }
        }
    }

    if let Ok(re) = regex::Regex::new(&format!("^{}$", regex_pattern)) {
        return re.is_match(path);
    }

    false
}

/// Configuration error types.
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// Failed to read config file.
    ReadError(PathBuf, String),
    /// Failed to parse TOML.
    ParseError(String),
    /// Failed to write config file.
    WriteError(PathBuf, String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::ReadError(path, msg) => {
                write!(f, "Failed to read config file {}: {}", path.display(), msg)
            }
            ConfigError::ParseError(msg) => {
                write!(f, "Failed to parse config: {}", msg)
            }
            ConfigError::WriteError(path, msg) => {
                write!(f, "Failed to write config file {}: {}", path.display(), msg)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = FleetLintConfig::default();
        assert!(config.rules.disabled.is_empty());
        assert_eq!(config.thresholds.min_interval, 60);
        assert_eq!(config.thresholds.max_interval, 86400);
        assert!(config.thresholds.warn_select_star);
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
[rules]
disabled = ["query-syntax"]
warn = ["interval-validation"]

[thresholds]
min_interval = 30
max_interval = 3600

[files]
exclude = ["**/test/**"]
"#;

        let config = FleetLintConfig::parse(toml).unwrap();
        assert_eq!(config.rules.disabled, vec!["query-syntax"]);
        assert_eq!(config.rules.warn, vec!["interval-validation"]);
        assert_eq!(config.thresholds.min_interval, 30);
        assert_eq!(config.thresholds.max_interval, 3600);
        assert!(config.files.exclude.contains(&"**/test/**".to_string()));
    }

    #[test]
    fn test_is_rule_disabled() {
        let toml = r#"
[rules]
disabled = ["query-syntax", "security"]
"#;
        let config = FleetLintConfig::parse(toml).unwrap();

        assert!(config.is_rule_disabled("query-syntax"));
        assert!(config.is_rule_disabled("security"));
        assert!(!config.is_rule_disabled("required-fields"));
    }

    #[test]
    fn test_is_rule_warning() {
        let toml = r#"
[rules]
warn = ["duplicate-names"]
"#;
        let config = FleetLintConfig::parse(toml).unwrap();

        assert!(config.is_rule_warning("duplicate-names"));
        assert!(!config.is_rule_warning("query-syntax"));
    }

    #[test]
    fn test_matches_glob() {
        // ** pattern
        assert!(matches_glob("**/*.yml", "lib/policies.yml"));
        assert!(matches_glob("**/*.yml", "teams/engineering/default.yml"));
        assert!(!matches_glob("**/*.yml", "lib/policies.yaml"));

        // Simple * pattern
        assert!(matches_glob("*.yml", "default.yml"));
        assert!(!matches_glob("*.yml", "lib/default.yml"));

        // Exclude patterns
        assert!(matches_glob(
            "**/node_modules/**",
            "node_modules/foo/bar.yml"
        ));
        assert!(matches_glob("**/target/**", "some/target/debug/test.yml"));
    }

    #[test]
    fn test_should_lint_file() {
        let config = FleetLintConfig::default();

        assert!(config.should_lint_file(Path::new("default.yml")));
        assert!(config.should_lint_file(Path::new("lib/policies.yaml")));
        assert!(!config.should_lint_file(Path::new("node_modules/foo.yml")));
        assert!(!config.should_lint_file(Path::new("target/test.yml")));
        assert!(!config.should_lint_file(Path::new("script.js")));
    }

    #[test]
    fn test_default_with_comments() {
        let content = FleetLintConfig::default_with_comments();
        assert!(content.contains("[rules]"));
        assert!(content.contains("[thresholds]"));
        assert!(content.contains("[files]"));
        assert!(content.contains("[schema]"));
        assert!(content.contains("disabled = []"));
    }

    #[test]
    fn test_partial_config() {
        // Only specify some fields, rest should use defaults
        let toml = r#"
[thresholds]
min_interval = 120
"#;

        let config = FleetLintConfig::parse(toml).unwrap();
        assert_eq!(config.thresholds.min_interval, 120);
        // Other thresholds should be default
        assert_eq!(config.thresholds.max_interval, 86400);
        assert!(config.thresholds.warn_select_star);
        // Rules should be empty
        assert!(config.rules.disabled.is_empty());
    }

    // User-level config fallback — XDG path resolution. The fs-touching
    // outer `find_in_user_config` is not tested here (would need tempdir
    // fixtures); the pure resolver covers all the branching logic.

    #[test]
    fn user_config_prefers_xdg_when_set() {
        let p = resolve_user_config_path(Some("/custom/xdg"), Some("/home/alice")).unwrap();
        assert_eq!(p, PathBuf::from("/custom/xdg/flint/config.toml"));
    }

    #[test]
    fn user_config_falls_back_to_home_dot_config() {
        let p = resolve_user_config_path(None, Some("/home/alice")).unwrap();
        assert_eq!(p, PathBuf::from("/home/alice/.config/flint/config.toml"));
    }

    #[test]
    fn user_config_empty_xdg_is_treated_as_unset() {
        // XDG basedir spec: empty value falls back to $HOME/.config rather
        // than resolving to a bare `flint/config.toml` in the cwd.
        let p = resolve_user_config_path(Some(""), Some("/home/alice")).unwrap();
        assert_eq!(p, PathBuf::from("/home/alice/.config/flint/config.toml"));
    }

    #[test]
    fn user_config_returns_none_when_no_env_available() {
        // Daemonized / minimal-container case — no usable path, caller
        // must skip the user-level fallback entirely.
        assert!(resolve_user_config_path(None, None).is_none());
        assert!(resolve_user_config_path(Some(""), None).is_none());
    }
}
