//! Self-reference detection rule.
//!
//! Detects `path:` references that resolve back to the file itself,
//! which would create infinite loops during Fleet GitOps processing.

use super::error::LintError;
use super::fleet_config::FleetConfig;
use super::rules::Rule;
use super::util::normalize_path;
use super::yaml_utils::{find_path_value_line, walk_mappings};
use std::path::Path;

/// Detects `path:` values that resolve back to the file itself, which causes
/// Fleet GitOps to loop or fail silently.
pub struct SelfReferenceRule;

impl Rule for SelfReferenceRule {
    fn name(&self) -> &'static str {
        "self-reference"
    }

    fn description(&self) -> &'static str {
        "Detects path references that point to the file itself"
    }
    fn category(&self) -> &'static str {
        "structural"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml: serde_yaml::Value = match serde_yaml::from_str(source) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut errors = Vec::new();
        walk_mappings(&yaml, &mut |map| {
            if let Some(serde_yaml::Value::String(path_val)) =
                map.get(serde_yaml::Value::String("path".to_string()))
            {
                if is_self_reference(file, path_val) {
                    let mut err = LintError::warning(
                        "path references the file itself, creating a loop",
                        file,
                    )
                    .with_context(format!("path: {}", path_val))
                    .with_help(
                        "This path resolves to the current file. Change it to reference a different file.",
                    );
                    if let Some(span) = find_path_value_line(source, path_val) {
                        err = err.with_span(span);
                    }
                    errors.push(err);
                }
            }
        });
        errors
    }
}

/// Check whether `path_value` (relative to `file`'s parent dir) resolves to `file` itself.
fn is_self_reference(file: &Path, path_value: &str) -> bool {
    let base = match file.parent() {
        Some(p) => p,
        None => return false,
    };

    let resolved = base.join(path_value);

    // Try canonical comparison first (works when both files exist on disk).
    if let (Ok(canon_file), Ok(canon_resolved)) = (file.canonicalize(), resolved.canonicalize()) {
        return canon_file == canon_resolved;
    }

    // Fall back to manual normalization for non-existent paths.
    normalize_path(file) == normalize_path(&resolved)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn check_with_file(dir: &Path, file_name: &str, yaml: &str) -> Vec<LintError> {
        let file_path = dir.join(file_name);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file_path, yaml).unwrap();

        let config = FleetConfig::default();
        SelfReferenceRule.check(&config, &file_path, yaml)
    }

    #[test]
    fn test_self_reference_detected() {
        let tmp = TempDir::new().unwrap();
        let yaml = "policies:\n  - path: ./self.yml\n";
        let errors = check_with_file(tmp.path(), "self.yml", yaml);
        assert_eq!(errors.len(), 1);
        assert!(errors[0]
            .message
            .contains("path references the file itself"));
        assert_eq!(errors[0].severity, super::super::error::Severity::Warning);
    }

    #[test]
    fn test_relative_self_reference() {
        let tmp = TempDir::new().unwrap();
        let yaml = "policies:\n  - path: ../teams/t.yml\n";
        let errors = check_with_file(tmp.path(), "teams/t.yml", yaml);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_valid_reference_no_warning() {
        let tmp = TempDir::new().unwrap();
        // Create the referenced file so canonicalize works
        fs::create_dir_all(tmp.path().join("lib")).unwrap();
        fs::write(tmp.path().join("lib/other.yml"), "").unwrap();

        let yaml = "policies:\n  - path: ../lib/other.yml\n";
        let errors = check_with_file(tmp.path(), "teams/t.yml", yaml);
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_multiple_paths_one_self_ref() {
        let tmp = TempDir::new().unwrap();
        // Create the valid target
        fs::create_dir_all(tmp.path().join("lib")).unwrap();
        fs::write(tmp.path().join("lib/other.yml"), "").unwrap();

        let yaml = "policies:\n  - path: ./self.yml\n  - path: ../lib/other.yml\n";
        let errors = check_with_file(tmp.path(), "self.yml", yaml);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_nested_path_detected() {
        let tmp = TempDir::new().unwrap();
        let yaml = r#"
controls:
  scripts:
    - name: "Install Santa"
      path: ./deploy.yml
"#;
        let errors = check_with_file(tmp.path(), "deploy.yml", yaml);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_no_path_keys() {
        let tmp = TempDir::new().unwrap();
        let yaml = "policies:\n  - name: test\n    query: SELECT 1;\n";
        let errors = check_with_file(tmp.path(), "test.yml", yaml);
        assert_eq!(errors.len(), 0);
    }

    #[test]
    fn test_normalize_path() {
        use std::path::PathBuf;
        let p = PathBuf::from("teams/../teams/../teams/b.yml");
        assert_eq!(normalize_path(&p), PathBuf::from("teams/b.yml"));

        let p2 = PathBuf::from("a/./b/../c.yml");
        assert_eq!(normalize_path(&p2), PathBuf::from("a/c.yml"));
    }
}
