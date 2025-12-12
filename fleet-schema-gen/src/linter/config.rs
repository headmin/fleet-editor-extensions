//! Configuration file support for Fleet linter.
//!
//! Supports `.fleetlint.toml` configuration files that allow teams
//! to customize linting behavior and share settings via version control.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Configuration file name.
pub const CONFIG_FILE_NAME: &str = ".fleetlint.toml";

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

    /// Whether to allow unknown fields (default: true).
    pub allow_unknown_fields: bool,

    /// Whether to require explicit platform specification (default: false).
    pub require_platform: bool,
}

impl Default for SchemaConfig {
    fn default() -> Self {
        Self {
            validate: true,
            allow_unknown_fields: true,
            require_platform: false,
        }
    }
}

impl FleetLintConfig {
    /// Load configuration from a file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(path.to_path_buf(), e.to_string()))?;

        Self::from_str(&content)
    }

    /// Parse configuration from a TOML string.
    pub fn from_str(content: &str) -> Result<Self, ConfigError> {
        toml::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()))
    }

    /// Find and load configuration by searching up from a starting path.
    ///
    /// Searches for `.fleetlint.toml` starting from `start_path` and
    /// walking up to parent directories until found or root is reached.
    pub fn find_and_load(start_path: &Path) -> Option<(PathBuf, Self)> {
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

# Allow unknown/extra fields (default: true)
allow_unknown_fields = true

# Require explicit platform specification (default: false)
require_platform = false
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

        let config = FleetLintConfig::from_str(toml).unwrap();
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
        let config = FleetLintConfig::from_str(toml).unwrap();

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
        let config = FleetLintConfig::from_str(toml).unwrap();

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
        assert!(matches_glob("**/node_modules/**", "node_modules/foo/bar.yml"));
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

        let config = FleetLintConfig::from_str(toml).unwrap();
        assert_eq!(config.thresholds.min_interval, 120);
        // Other thresholds should be default
        assert_eq!(config.thresholds.max_interval, 86400);
        assert!(config.thresholds.warn_select_star);
        // Rules should be empty
        assert!(config.rules.disabled.is_empty());
    }
}
