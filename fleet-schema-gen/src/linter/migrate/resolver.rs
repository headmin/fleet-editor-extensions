use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::fs;

/// Resolves path references in Fleet GitOps configs
pub struct PathResolver {
    cache: HashMap<PathBuf, serde_yaml::Value>,
    visited: HashSet<PathBuf>,
}

impl PathResolver {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
            visited: HashSet::new(),
        }
    }

    /// Resolve a relative path from a base file
    pub fn resolve_path(&self, base: &Path, relative: &str) -> Result<PathBuf> {
        let base_dir = base.parent()
            .ok_or_else(|| anyhow::anyhow!("No parent directory"))?;

        let resolved = base_dir.join(relative);
        let canonical = resolved.canonicalize()
            .with_context(|| format!("Failed to resolve path: {}", relative))?;

        Ok(canonical)
    }

    /// Load a YAML file (with caching)
    pub fn load_file(&mut self, path: &Path) -> Result<&serde_yaml::Value> {
        if !self.cache.contains_key(path) {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Failed to read {}", path.display()))?;

            let yaml: serde_yaml::Value = serde_yaml::from_str(&content)
                .with_context(|| format!("Failed to parse YAML in {}", path.display()))?;

            self.cache.insert(path.to_path_buf(), yaml);
        }

        Ok(self.cache.get(path).unwrap())
    }

    /// Find all files referenced by path: entries
    pub fn find_referenced_files(&mut self, root: &Path) -> Result<Vec<PathBuf>> {
        let mut referenced = Vec::new();
        self.visited.clear();

        self.find_references_recursive(root, &mut referenced)?;

        Ok(referenced)
    }

    fn find_references_recursive(&mut self, file: &Path, referenced: &mut Vec<PathBuf>) -> Result<()> {
        // Avoid circular references
        if self.visited.contains(file) {
            return Ok(());
        }
        self.visited.insert(file.to_path_buf());

        // Load file first, then extract paths
        let yaml = self.load_file(file)?.clone();

        // Look for path: references
        self.extract_paths(&yaml, file, referenced)?;

        Ok(())
    }

    fn extract_paths(&mut self, yaml: &serde_yaml::Value, base: &Path, referenced: &mut Vec<PathBuf>) -> Result<()> {
        match yaml {
            serde_yaml::Value::Mapping(map) => {
                // Check for path: key
                if let Some(serde_yaml::Value::String(path_str)) = map.get(&serde_yaml::Value::String("path".to_string())) {
                    let resolved = self.resolve_path(base, path_str)?;
                    if !referenced.contains(&resolved) {
                        referenced.push(resolved.clone());
                        // Recursively process referenced file
                        self.find_references_recursive(&resolved, referenced)?;
                    }
                }

                // Recurse into all values
                for (_, value) in map {
                    self.extract_paths(value, base, referenced)?;
                }
            }
            serde_yaml::Value::Sequence(seq) => {
                for item in seq {
                    self.extract_paths(item, base, referenced)?;
                }
            }
            _ => {}
        }

        Ok(())
    }

    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.visited.clear();
    }
}

impl Default for PathResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_path() {
        let temp = TempDir::new().unwrap();
        let base = temp.path().join("teams/team1.yml");

        // Create directory structure and files
        fs::create_dir_all(temp.path().join("teams")).unwrap();
        fs::create_dir_all(temp.path().join("shared/packages")).unwrap();
        fs::File::create(&base).unwrap();
        fs::File::create(temp.path().join("shared/packages/example.yml")).unwrap();

        let resolver = PathResolver::new();
        let resolved = resolver.resolve_path(&base, "../shared/packages/example.yml").unwrap();

        assert!(resolved.to_string_lossy().contains("shared"));
        assert!(resolved.to_string_lossy().contains("packages"));
    }

    #[test]
    fn test_find_referenced_files() {
        let temp = TempDir::new().unwrap();

        // Create structure
        fs::create_dir_all(temp.path().join("teams")).unwrap();
        fs::create_dir_all(temp.path().join("shared/packages")).unwrap();

        // Create team file with path reference
        let team_file = temp.path().join("teams/team1.yml");
        let mut f = fs::File::create(&team_file).unwrap();
        writeln!(f, "software:").unwrap();
        writeln!(f, "  packages:").unwrap();
        writeln!(f, "    - path: ../shared/packages/example.yml").unwrap();

        // Create referenced file
        let software_file = temp.path().join("shared/packages/example.yml");
        let mut f = fs::File::create(&software_file).unwrap();
        writeln!(f, "url: https://example.com/package.pkg").unwrap();

        let mut resolver = PathResolver::new();
        let referenced = resolver.find_referenced_files(&team_file).unwrap();

        assert_eq!(referenced.len(), 1);
        assert!(referenced[0].to_string_lossy().contains("example.yml"));
    }
}
