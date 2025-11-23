use super::types::{FileChange, MatchStrategy, Transformation};
use super::resolver::PathResolver;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Execute a field move transformation (like Fleet's gitops-migrate)
pub fn execute_field_move(
    transformation: &Transformation,
    source_file: &Path,
    target_file: &Path,
    resolver: &mut PathResolver,
) -> Result<Vec<FileChange>> {
    let mut changes = Vec::new();

    if let Transformation::FieldMove {
        fields,
        match_strategy,
        target_location,
        ..
    } = transformation
    {
        // Load source YAML
        let source_yaml = resolver.load_file(source_file)?.clone();

        // Extract field values from source
        let mut extracted_values = serde_yaml::Mapping::new();

        for field in fields {
            if let Some(value) = source_yaml.get(field) {
                extracted_values.insert(
                    serde_yaml::Value::String(field.clone()),
                    value.clone(),
                );

                // Record removal from source
                changes.push(FileChange::RemoveField {
                    path: field.clone(),
                });
            }
        }

        // Add fields to target
        for (key, value) in extracted_values {
            if let serde_yaml::Value::String(field_name) = key {
                changes.push(FileChange::AddField {
                    path: format!("{}.{}", target_location, field_name),
                    value,
                });
            }
        }
    }

    Ok(changes)
}

/// Execute a field rename transformation
pub fn execute_field_rename(
    transformation: &Transformation,
    file: &Path,
    resolver: &mut PathResolver,
) -> Result<Vec<FileChange>> {
    let mut changes = Vec::new();

    if let Transformation::FieldRename {
        old_path,
        new_path,
        ..
    } = transformation
    {
        // Load YAML
        let yaml = resolver.load_file(file)?.clone();

        // Get value at old path
        if let Some(value) = get_nested_value(&yaml, old_path) {
            changes.push(FileChange::RenameField {
                old_path: old_path.clone(),
                new_path: new_path.clone(),
            });
        }
    }

    Ok(changes)
}

/// Execute a field delete transformation
pub fn execute_field_delete(
    transformation: &Transformation,
    file: &Path,
    resolver: &mut PathResolver,
) -> Result<Vec<FileChange>> {
    let mut changes = Vec::new();

    if let Transformation::FieldDelete { fields, .. } = transformation {
        // Load YAML
        let yaml = resolver.load_file(file)?.clone();

        for field in fields {
            if get_nested_value(&yaml, field).is_some() {
                changes.push(FileChange::RemoveField {
                    path: field.clone(),
                });
            }
        }
    }

    Ok(changes)
}

/// Apply file changes to a YAML file
pub fn apply_changes(file: &Path, changes: &[FileChange]) -> Result<String> {
    let content = fs::read_to_string(file)
        .with_context(|| format!("Failed to read {}", file.display()))?;

    let mut yaml: serde_yaml::Value = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML in {}", file.display()))?;

    for change in changes {
        match change {
            FileChange::AddField { path, value } => {
                set_nested_value(&mut yaml, path, value.clone())?;
            }
            FileChange::RemoveField { path } => {
                remove_nested_value(&mut yaml, path)?;
            }
            FileChange::RenameField { old_path, new_path } => {
                // Clone the value first to avoid borrow checker issues
                let value = get_nested_value(&yaml, old_path).cloned();
                if let Some(value) = value {
                    set_nested_value(&mut yaml, new_path, value)?;
                    remove_nested_value(&mut yaml, old_path)?;
                }
            }
            FileChange::ModifyValue { path, new_value, .. } => {
                set_nested_value(&mut yaml, path, new_value.clone())?;
            }
        }
    }

    // Serialize back to YAML
    let new_content = serde_yaml::to_string(&yaml)?;

    Ok(new_content)
}

// Helper functions for nested YAML path access

fn get_nested_value<'a>(yaml: &'a serde_yaml::Value, path: &str) -> Option<&'a serde_yaml::Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = yaml;

    for part in parts {
        current = current.get(part)?;
    }

    Some(current)
}

fn set_nested_value(
    yaml: &mut serde_yaml::Value,
    path: &str,
    value: serde_yaml::Value,
) -> Result<()> {
    let parts: Vec<&str> = path.split('.').collect();

    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty path"));
    }

    // Navigate to parent and set value
    let mut current = yaml;

    for (idx, part) in parts.iter().enumerate() {
        if idx == parts.len() - 1 {
            // Last part - set the value
            if let serde_yaml::Value::Mapping(map) = current {
                map.insert(serde_yaml::Value::String(part.to_string()), value);
                return Ok(());
            } else {
                return Err(anyhow::anyhow!("Cannot set field on non-mapping"));
            }
        } else {
            // Intermediate part - navigate or create
            if let serde_yaml::Value::Mapping(map) = current {
                let key = serde_yaml::Value::String(part.to_string());

                if !map.contains_key(&key) {
                    // Create intermediate mapping
                    map.insert(key.clone(), serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
                }

                current = map.get_mut(&key).unwrap();
            } else {
                return Err(anyhow::anyhow!("Cannot navigate non-mapping"));
            }
        }
    }

    Ok(())
}

fn remove_nested_value(yaml: &mut serde_yaml::Value, path: &str) -> Result<()> {
    let parts: Vec<&str> = path.split('.').collect();

    if parts.is_empty() {
        return Err(anyhow::anyhow!("Empty path"));
    }

    if parts.len() == 1 {
        // Top-level field
        if let serde_yaml::Value::Mapping(map) = yaml {
            map.remove(&serde_yaml::Value::String(parts[0].to_string()));
            return Ok(());
        }
    }

    // Navigate to parent
    let mut current = yaml;
    for (idx, part) in parts.iter().enumerate() {
        if idx == parts.len() - 1 {
            // Last part - remove it
            if let serde_yaml::Value::Mapping(map) = current {
                map.remove(&serde_yaml::Value::String(part.to_string()));
                return Ok(());
            }
        } else {
            // Navigate
            if let serde_yaml::Value::Mapping(map) = current {
                let key = serde_yaml::Value::String(part.to_string());
                current = map.get_mut(&key)
                    .ok_or_else(|| anyhow::anyhow!("Path not found: {}", path))?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_nested_value() {
        let yaml: serde_yaml::Value = serde_yaml::from_str(r#"
software:
  packages:
    - name: Chrome
"#).unwrap();

        let value = get_nested_value(&yaml, "software");
        assert!(value.is_some());

        let value = get_nested_value(&yaml, "software.packages");
        assert!(value.is_some());

        let value = get_nested_value(&yaml, "nonexistent");
        assert!(value.is_none());
    }

    #[test]
    fn test_set_nested_value() {
        let mut yaml: serde_yaml::Value = serde_yaml::from_str("{}").unwrap();

        set_nested_value(
            &mut yaml,
            "software.packages",
            serde_yaml::Value::Sequence(vec![]),
        ).unwrap();

        let value = get_nested_value(&yaml, "software.packages");
        assert!(value.is_some());
    }

    #[test]
    fn test_remove_nested_value() {
        let mut yaml: serde_yaml::Value = serde_yaml::from_str(r#"
software:
  packages: []
  other: value
"#).unwrap();

        remove_nested_value(&mut yaml, "software.packages").unwrap();

        let value = get_nested_value(&yaml, "software.packages");
        assert!(value.is_none());

        let value = get_nested_value(&yaml, "software.other");
        assert!(value.is_some());
    }
}

