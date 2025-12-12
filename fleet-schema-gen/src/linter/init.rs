//! Configuration initialization for Fleet linter.
//!
//! Provides workspace detection and interactive configuration generation
//! for creating `.fleetlint.toml` files.

use super::config::{FleetLintConfig, FilesConfig, RulesConfig, SchemaConfig, ThresholdsConfig, CONFIG_FILE_NAME};
use colored::Colorize;
use std::collections::HashSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// Detected workspace configuration.
#[derive(Debug, Clone, Default)]
pub struct DetectedConfig {
    /// Whether a teams/ directory exists.
    pub has_teams_dir: bool,
    /// Number of team subdirectories found.
    pub team_count: usize,
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
}

/// User's answers to interactive prompts.
#[derive(Debug, Clone, Default)]
pub struct UserAnswers {
    /// Selected strictness level.
    pub strictness: StrictnessLevel,
    /// Whether to include all detected files.
    pub include_all_files: bool,
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

    // Check for teams/ directory
    let teams_dir = root.join("teams");
    if teams_dir.is_dir() {
        config.has_teams_dir = true;
        // Count team subdirectories
        if let Ok(entries) = fs::read_dir(&teams_dir) {
            config.team_count = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .count();
        }
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
                if name.starts_with('.') || name == "node_modules" || name == "target" || name == "dist" {
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
                if !platform.is_empty() && ["darwin", "windows", "linux", "chrome"].contains(&platform) {
                    platforms.insert(platform.to_string());
                }
            }
        }

        // Detect path references
        if trimmed.starts_with("- path:") || trimmed.starts_with("path:") {
            config.has_path_references = true;
        }
    }

    // Add detected platforms
    for platform in platforms {
        if !config.detected_platforms.contains(&platform) {
            config.detected_platforms.push(platform);
        }
    }
}

/// Run interactive prompts and return user's answers.
pub fn prompt_user(detected: &DetectedConfig) -> io::Result<UserAnswers> {
    let mut answers = UserAnswers::default();

    // Strictness level prompt
    println!("\n{}", "? What strictness level would you like?".bold());
    println!("  {} - Enforce best practices (require platform, warn on SELECT *)", "1. Strict".cyan());
    println!("  {} - Balanced defaults (recommended)", "2. Moderate".green());
    println!("  {} - Minimal warnings", "3. Relaxed".yellow());
    print!("\n  Enter choice [2]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    answers.strictness = match input {
        "1" | "strict" => StrictnessLevel::Strict,
        "3" | "relaxed" => StrictnessLevel::Relaxed,
        _ => StrictnessLevel::Moderate,
    };

    // Confirm file inclusion
    if detected.yaml_file_count > 0 {
        println!("\n{} {} YAML files detected.", "?".bold(), detected.yaml_file_count);
        print!("  Include all for linting? [Y/n]: ");
        io::stdout().flush()?;

        let mut input2 = String::new();
        io::stdin().read_line(&mut input2)?;
        let input2 = input2.trim().to_lowercase();

        answers.include_all_files = input2.is_empty() || input2 == "y" || input2 == "yes";
    } else {
        answers.include_all_files = true;
    }

    Ok(answers)
}

/// Generate a FleetLintConfig based on detection and user answers.
pub fn generate_config(detected: &DetectedConfig, answers: &UserAnswers) -> FleetLintConfig {
    let mut config = FleetLintConfig::default();

    // Set thresholds based on strictness
    match answers.strictness {
        StrictnessLevel::Strict => {
            config.thresholds.warn_select_star = true;
            config.thresholds.warn_trailing_semicolon = true;
            config.thresholds.min_interval = 60;
            config.schema.require_platform = true;
        }
        StrictnessLevel::Moderate => {
            // Use defaults
        }
        StrictnessLevel::Relaxed => {
            config.thresholds.warn_select_star = false;
            config.thresholds.warn_trailing_semicolon = false;
            config.rules.disabled.push("query-syntax".to_string());
        }
    }

    // Set root if teams structure detected
    if detected.has_teams_dir {
        config.files.root = Some(".".to_string());
    }

    config
}

/// Generate TOML content with comments based on detection and answers.
pub fn generate_config_toml(detected: &DetectedConfig, answers: &UserAnswers) -> String {
    let mut output = String::new();

    // Header
    output.push_str("# Fleet Linter Configuration\n");
    output.push_str("# Generated by `fleet-schema-gen init`\n");
    output.push_str("#\n");

    // Add detection summary as comment
    if detected.has_teams_dir || detected.has_lib_dir {
        output.push_str("# Detected structure:\n");
        if detected.has_teams_dir {
            output.push_str(&format!("#   - teams/ directory ({} team(s))\n", detected.team_count));
        }
        if detected.has_lib_dir {
            output.push_str("#   - lib/ directory (shared configs)\n");
        }
        if !detected.detected_platforms.is_empty() {
            output.push_str(&format!("#   - Platforms: {}\n", detected.detected_platforms.join(", ")));
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
    output.push_str("#   - platform-compatibility: Validates osquery tables work on specified platform\n");
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
            output.push_str("\n# Warn on trailing semicolons (disabled for relaxed mode)\n");
            output.push_str("warn_trailing_semicolon = false\n");
        }
        _ => {
            output.push_str("\n# Warn when using SELECT *\n");
            output.push_str("warn_select_star = true\n");
            output.push_str("\n# Warn on trailing semicolons in queries\n");
            output.push_str("warn_trailing_semicolon = true\n");
        }
    }
    output.push('\n');

    // Files section
    output.push_str("# File Patterns\n");
    output.push_str("[files]\n");
    output.push_str("# Glob patterns to include\n");
    output.push_str("include = [\"**/*.yml\", \"**/*.yaml\"]\n");
    output.push_str("\n# Glob patterns to exclude\n");
    output.push_str("exclude = [\n");
    output.push_str("    \"**/node_modules/**\",\n");
    output.push_str("    \"**/target/**\",\n");
    output.push_str("    \"**/.git/**\",\n");
    output.push_str("    \"**/dist/**\",\n");
    output.push_str("]\n");

    if detected.has_teams_dir {
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

/// Initialize Fleet linter configuration in the given directory.
pub fn init(root: &Path, output: Option<PathBuf>, interactive: bool, force: bool) -> anyhow::Result<()> {
    let config_path = output.unwrap_or_else(|| root.join(CONFIG_FILE_NAME));

    // Check if config already exists
    if config_path.exists() && !force {
        anyhow::bail!(
            "Configuration file already exists: {}\nUse --force to overwrite.",
            config_path.display()
        );
    }

    // Detect workspace structure
    println!("{} Detecting Fleet GitOps structure...\n", "🔍".cyan());

    let detected = detect_workspace(root);

    // Print detection summary
    println!("{}:", "Found".bold());
    println!("  • {} YAML file(s)", detected.yaml_file_count.to_string().cyan());

    if detected.has_teams_dir {
        println!("  • {} directory with {} team(s)", "teams/".green(), detected.team_count);
    }
    if detected.has_lib_dir {
        println!("  • {} directory (shared configs)", "lib/".green());
    }
    if !detected.root_yaml_files.is_empty() {
        println!("  • Root files: {}", detected.root_yaml_files.join(", ").dimmed());
    }
    if !detected.detected_platforms.is_empty() {
        println!("  • Platforms: {}", detected.detected_platforms.join(", ").yellow());
    }
    if detected.has_path_references {
        println!("  • Path references detected (cross-file includes)");
    }

    // Get user answers (interactive or defaults)
    let answers = if interactive {
        prompt_user(&detected)?
    } else {
        UserAnswers::default()
    };

    // Generate config
    let config_content = generate_config_toml(&detected, &answers);

    // Write config file
    fs::write(&config_path, &config_content)?;

    println!("\n{} Created {}", "✓".green().bold(), config_path.display().to_string().bold());
    println!("\n{}:", "Next steps".bold());
    println!("  • Run {} to validate your configs", "fleet-schema-gen lint .".cyan());
    println!("  • Edit {} to customize rules", config_path.display().to_string().cyan());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_empty_workspace() {
        let temp_dir = TempDir::new().unwrap();
        let detected = detect_workspace(temp_dir.path());

        assert!(!detected.has_teams_dir);
        assert!(!detected.has_lib_dir);
        assert_eq!(detected.yaml_file_count, 0);
        assert!(detected.detected_platforms.is_empty());
    }

    #[test]
    fn test_detect_teams_structure() {
        let temp_dir = TempDir::new().unwrap();

        // Create teams structure
        fs::create_dir(temp_dir.path().join("teams")).unwrap();
        fs::create_dir(temp_dir.path().join("teams/engineering")).unwrap();
        fs::create_dir(temp_dir.path().join("teams/security")).unwrap();
        fs::create_dir(temp_dir.path().join("lib")).unwrap();

        // Create some YAML files
        fs::write(
            temp_dir.path().join("default.yml"),
            "policies:\n  - name: Test\n    platform: darwin\n",
        ).unwrap();
        fs::write(
            temp_dir.path().join("teams/engineering/default.yml"),
            "policies:\n  - name: Team Policy\n    platform: linux\n",
        ).unwrap();

        let detected = detect_workspace(temp_dir.path());

        assert!(detected.has_teams_dir);
        assert_eq!(detected.team_count, 2);
        assert!(detected.has_lib_dir);
        assert_eq!(detected.yaml_file_count, 2);
        assert!(detected.detected_platforms.contains(&"darwin".to_string()));
        assert!(detected.detected_platforms.contains(&"linux".to_string()));
    }

    #[test]
    fn test_detect_path_references() {
        let temp_dir = TempDir::new().unwrap();

        fs::write(
            temp_dir.path().join("default.yml"),
            "policies:\n  - path: lib/policies.yml\n",
        ).unwrap();

        let detected = detect_workspace(temp_dir.path());

        assert!(detected.has_path_references);
    }

    #[test]
    fn test_generate_config_strict() {
        let detected = DetectedConfig::default();
        let answers = UserAnswers {
            strictness: StrictnessLevel::Strict,
            include_all_files: true,
        };

        let config = generate_config(&detected, &answers);

        assert!(config.schema.require_platform);
        assert!(config.thresholds.warn_select_star);
    }

    #[test]
    fn test_generate_config_relaxed() {
        let detected = DetectedConfig::default();
        let answers = UserAnswers {
            strictness: StrictnessLevel::Relaxed,
            include_all_files: true,
        };

        let config = generate_config(&detected, &answers);

        assert!(!config.thresholds.warn_select_star);
        assert!(config.rules.disabled.contains(&"query-syntax".to_string()));
    }

    #[test]
    fn test_generate_config_toml() {
        let detected = DetectedConfig {
            has_teams_dir: true,
            team_count: 2,
            has_lib_dir: true,
            yaml_file_count: 10,
            detected_platforms: vec!["darwin".to_string(), "linux".to_string()],
            has_path_references: true,
            root_yaml_files: vec!["default.yml".to_string()],
        };
        let answers = UserAnswers::default();

        let toml = generate_config_toml(&detected, &answers);

        assert!(toml.contains("[rules]"));
        assert!(toml.contains("[thresholds]"));
        assert!(toml.contains("[files]"));
        assert!(toml.contains("[schema]"));
        assert!(toml.contains("teams/ directory"));
        assert!(toml.contains("darwin, linux"));
    }
}
