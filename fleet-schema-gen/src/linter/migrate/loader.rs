use super::types::{MatchStrategy, Migration, Transformation, Version};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// TOML representation of migrations
#[derive(Debug, Deserialize, Serialize)]
struct MigrationsToml {
    migration: Vec<MigrationToml>,
}

#[derive(Debug, Deserialize, Serialize)]
struct MigrationToml {
    id: String,
    from_version: String,
    to_version: String,
    description: String,
    transformations: Vec<TransformationToml>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TransformationToml {
    FieldMove {
        source_pattern: String,
        target_pattern: String,
        match_strategy: String,
        target_location: String,
        fields: Vec<String>,
    },
    FieldRename {
        pattern: String,
        old_path: String,
        new_path: String,
    },
    FieldDelete {
        pattern: String,
        fields: Vec<String>,
        #[serde(default)]
        reason: Option<String>,
    },
    Restructure {
        name: String,
        description: String,
    },
}

/// Load migrations from a TOML file
pub fn load_migrations_from_file(path: &Path) -> Result<Vec<Migration>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read migrations file: {}", path.display()))?;

    load_migrations_from_str(&content)
}

/// Load migrations from a TOML string
pub fn load_migrations_from_str(content: &str) -> Result<Vec<Migration>> {
    let migrations_toml: MigrationsToml = toml::from_str(content)
        .context("Failed to parse migrations TOML")?;

    migrations_toml
        .migration
        .into_iter()
        .map(|m| migration_from_toml(m))
        .collect()
}

fn migration_from_toml(m: MigrationToml) -> Result<Migration> {
    let from_version = Version::parse(&m.from_version)
        .with_context(|| format!("Invalid from_version: {}", m.from_version))?;

    let to_version = Version::parse(&m.to_version)
        .with_context(|| format!("Invalid to_version: {}", m.to_version))?;

    let transformations = m
        .transformations
        .into_iter()
        .map(|t| transformation_from_toml(t))
        .collect::<Result<Vec<_>>>()?;

    Ok(Migration {
        id: m.id,
        from_version,
        to_version,
        description: m.description,
        transformations,
    })
}

fn transformation_from_toml(t: TransformationToml) -> Result<Transformation> {
    match t {
        TransformationToml::FieldMove {
            source_pattern,
            target_pattern,
            match_strategy,
            target_location,
            fields,
        } => {
            let strategy = match match_strategy.as_str() {
                "path_reference" => MatchStrategy::PathReference,
                "filename" => MatchStrategy::FileName,
                s if s.starts_with("custom:") => {
                    MatchStrategy::Custom(s.strip_prefix("custom:").unwrap().to_string())
                }
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unknown match strategy: {} (valid: path_reference, filename, custom:*)",
                        match_strategy
                    ))
                }
            };

            Ok(Transformation::FieldMove {
                source_pattern,
                target_pattern,
                fields,
                match_strategy: strategy,
                target_location,
            })
        }
        TransformationToml::FieldRename {
            pattern,
            old_path,
            new_path,
        } => Ok(Transformation::FieldRename {
            pattern,
            old_path,
            new_path,
        }),
        TransformationToml::FieldDelete {
            pattern,
            fields,
            reason,
        } => Ok(Transformation::FieldDelete {
            pattern,
            fields,
            reason,
        }),
        TransformationToml::Restructure { name, description } => {
            Ok(Transformation::Restructure { name, description })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_migrations() {
        let toml = r#"
[[migration]]
id = "test-migration"
from_version = "4.73.0"
to_version = "4.74.0"
description = "Test migration"

[[migration.transformations]]
type = "field_move"
source_pattern = "lib/**/*.yml"
target_pattern = "teams/**/*.yml"
match_strategy = "path_reference"
target_location = "software.packages"
fields = ["self_service", "categories"]
"#;

        let migrations = load_migrations_from_str(toml).unwrap();
        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].id, "test-migration");
        assert_eq!(migrations[0].from_version, Version::new(4, 73, 0));
        assert_eq!(migrations[0].to_version, Version::new(4, 74, 0));
        assert_eq!(migrations[0].transformations.len(), 1);
    }

    #[test]
    fn test_field_rename_transformation() {
        let toml = r#"
[[migration]]
id = "rename-test"
from_version = "4.29.0"
to_version = "4.30.0"
description = "Rename test"

[[migration.transformations]]
type = "field_rename"
pattern = "teams/**/*.yml"
old_path = "enable_disk_encryption"
new_path = "macos_settings.enable_disk_encryption"
"#;

        let migrations = load_migrations_from_str(toml).unwrap();
        assert_eq!(migrations.len(), 1);

        match &migrations[0].transformations[0] {
            Transformation::FieldRename {
                pattern,
                old_path,
                new_path,
            } => {
                assert_eq!(pattern, "teams/**/*.yml");
                assert_eq!(old_path, "enable_disk_encryption");
                assert_eq!(new_path, "macos_settings.enable_disk_encryption");
            }
            _ => panic!("Expected FieldRename transformation"),
        }
    }

    #[test]
    fn test_field_delete_transformation() {
        let toml = r#"
[[migration]]
id = "delete-test"
from_version = "4.50.0"
to_version = "4.51.0"
description = "Delete deprecated fields"

[[migration.transformations]]
type = "field_delete"
pattern = "**/*.yml"
fields = ["deprecated_field"]
reason = "Field no longer supported"
"#;

        let migrations = load_migrations_from_str(toml).unwrap();
        assert_eq!(migrations.len(), 1);

        match &migrations[0].transformations[0] {
            Transformation::FieldDelete {
                pattern,
                fields,
                reason,
            } => {
                assert_eq!(pattern, "**/*.yml");
                assert_eq!(fields, &vec!["deprecated_field"]);
                assert_eq!(reason.as_ref().unwrap(), "Field no longer supported");
            }
            _ => panic!("Expected FieldDelete transformation"),
        }
    }
}
