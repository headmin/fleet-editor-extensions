use super::error::{LintError, LintReport};
use super::fleet_config::FleetConfig;
use super::rules::RuleSet;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub struct Linter {
    rules: RuleSet,
}

impl Linter {
    pub fn new() -> Self {
        Self {
            rules: RuleSet::default_rules(),
        }
    }

    pub fn with_rules(rules: RuleSet) -> Self {
        Self { rules }
    }

    /// Lint a single file
    pub fn lint_file(&self, file_path: &Path) -> Result<LintReport> {
        // Read file
        let source = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        // Parse YAML
        let config: FleetConfig = serde_yaml::from_str(&source)
            .with_context(|| format!("Failed to parse YAML: {}", file_path.display()))?;

        // Run all rules
        let mut report = LintReport::new();

        for rule in self.rules.rules() {
            let errors = rule.check(&config, file_path, &source);
            for error in errors {
                report.add(error);
            }
        }

        Ok(report)
    }

    /// Lint multiple files
    pub fn lint_files(&self, files: &[&Path]) -> Result<Vec<(String, LintReport)>> {
        let mut results = Vec::new();

        for file in files {
            match self.lint_file(file) {
                Ok(report) => {
                    results.push((file.display().to_string(), report));
                }
                Err(e) => {
                    // Create error report
                    let mut report = LintReport::new();
                    report.add(LintError::error(
                        format!("Failed to lint file: {}", e),
                        *file,
                    ));
                    results.push((file.display().to_string(), report));
                }
            }
        }

        Ok(results)
    }

    /// Lint a directory recursively
    pub fn lint_directory(&self, dir: &Path, pattern: Option<&str>) -> Result<Vec<(String, LintReport)>> {
        let pattern = pattern.unwrap_or("**/*.{yml,yaml}");

        // Find all YAML files
        let yaml_files = find_yaml_files(dir, pattern)?;

        // Lint each file
        let file_refs: Vec<&Path> = yaml_files.iter().map(|p| p.as_path()).collect();
        self.lint_files(&file_refs)
    }
}

impl Default for Linter {
    fn default() -> Self {
        Self::new()
    }
}

/// Find YAML files in directory
fn find_yaml_files(dir: &Path, pattern: &str) -> Result<Vec<std::path::PathBuf>> {
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
                            || name_str == "dist" {
                            continue;
                        }
                    }
                    visit_dirs(&path, files)?;
                } else if let Some(ext) = path.extension() {
                    if ext == "yml" || ext == "yaml" {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

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
        assert!(report.errors.iter().any(|e| e.message.contains("missing required field 'query'")));
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
        assert!(report.errors.iter().any(|e| e.message.contains("invalid platform")));
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
        assert!(report.errors.iter().any(|e| e.message.contains("not available on platform")));
    }
}
