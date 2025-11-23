use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use crate::schema::types::SchemaDefinition;

#[derive(Debug, Deserialize, Serialize)]
struct GitHubRelease {
    tag_name: String,
    name: String,
    published_at: String,
    body: String,
}

const FLEET_REPO: &str = "fleetdm/fleet";
const FLEET_GITOPS_REPO: &str = "fleetdm/fleet-gitops";

pub async fn fetch_schema(version: &str) -> Result<SchemaDefinition> {
    println!("  → Fetching Fleet version: {}", version);

    // Get latest release if version is "latest"
    let release_version = if version == "latest" {
        get_latest_release().await?
    } else {
        version.to_string()
    };

    println!("  → Using Fleet version: {}", release_version);

    // Fetch example YAML files from fleet-gitops repo
    let examples = fetch_gitops_examples(&release_version).await?;

    // Parse examples to infer schema
    let schema = infer_schema_from_examples(examples)?;

    Ok(schema)
}

pub async fn fetch_and_save(output_dir: &Path) -> Result<()> {
    let schema = fetch_schema("latest").await?;

    let output_path = output_dir.join("fleet-github-schema.json");
    std::fs::create_dir_all(output_dir)?;

    let json = serde_json::to_string_pretty(&schema)?;
    std::fs::write(&output_path, json)?;

    println!("  ✓ Saved to: {}", output_path.display());

    Ok(())
}

async fn get_latest_release() -> Result<String> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", FLEET_REPO);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "fleet-schema-gen")
        .send()
        .await?;

    let release: GitHubRelease = response.json().await?;

    Ok(release.tag_name)
}

async fn fetch_gitops_examples(version: &str) -> Result<Vec<String>> {
    // Fetch example files from fleet-gitops repository
    let files = vec![
        "default.yml",
        "teams/workstations.yml",
        "lib/policies/example.yml",
        "lib/queries/example.yml",
    ];

    let mut examples = Vec::new();

    for file in files {
        if let Ok(content) = fetch_file_from_repo(FLEET_GITOPS_REPO, file, "main").await {
            examples.push(content);
        }
    }

    Ok(examples)
}

async fn fetch_file_from_repo(repo: &str, path: &str, branch: &str) -> Result<String> {
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/{}",
        repo, branch, path
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "fleet-schema-gen")
        .send()
        .await?;

    if response.status().is_success() {
        Ok(response.text().await?)
    } else {
        anyhow::bail!("Failed to fetch file: {} (status: {})", path, response.status())
    }
}

fn infer_schema_from_examples(examples: Vec<String>) -> Result<SchemaDefinition> {
    use indexmap::IndexMap;
    use crate::schema::types::{SchemaProperty, SchemaType};

    let mut all_properties: IndexMap<String, SchemaProperty> = IndexMap::new();

    // Parse each example YAML and extract properties
    for example in examples {
        if let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(&example) {
            if let serde_yaml::Value::Mapping(map) = yaml {
                extract_properties(&map, &mut all_properties, "");
            }
        }
    }

    let schema = SchemaDefinition {
        schema: Some("https://json-schema.org/draft-07/schema#".to_string()),
        title: Some("Fleet Configuration (Inferred)".to_string()),
        description: Some("Schema inferred from Fleet example files".to_string()),
        schema_type: Some(SchemaType::Object),
        properties: if all_properties.is_empty() { None } else { Some(all_properties) },
        additional_properties: Some(crate::schema::types::AdditionalProperties::Boolean(true)),
        ..Default::default()
    };

    Ok(schema)
}

fn extract_properties(
    map: &serde_yaml::Mapping,
    all_properties: &mut indexmap::IndexMap<String, crate::schema::types::SchemaProperty>,
    _prefix: &str,
) {
    use crate::schema::types::{SchemaProperty, SchemaType};

    for (key, value) in map {
        if let serde_yaml::Value::String(prop_name) = key {
            let property = infer_property_from_value(value);

            // Merge with existing property if present
            if let Some(existing) = all_properties.get_mut(prop_name) {
                // If we see a property multiple times, make it more permissive
                if existing.schema_type != property.schema_type {
                    existing.schema_type = SchemaType::Any;
                }
            } else {
                all_properties.insert(prop_name.clone(), property);
            }
        }
    }
}

fn infer_property_from_value(value: &serde_yaml::Value) -> crate::schema::types::SchemaProperty {
    use crate::schema::types::{SchemaProperty, SchemaType};
    use indexmap::IndexMap;

    match value {
        serde_yaml::Value::String(_) => SchemaProperty {
            schema_type: SchemaType::String,
            description: None,
            ..Default::default()
        },
        serde_yaml::Value::Bool(_) => SchemaProperty {
            schema_type: SchemaType::Boolean,
            description: None,
            ..Default::default()
        },
        serde_yaml::Value::Number(_) => SchemaProperty {
            schema_type: SchemaType::Integer,
            description: None,
            ..Default::default()
        },
        serde_yaml::Value::Sequence(seq) => {
            let items = if !seq.is_empty() {
                Some(Box::new(infer_property_from_value(&seq[0])))
            } else {
                None
            };

            SchemaProperty {
                schema_type: SchemaType::Array,
                items,
                description: None,
                ..Default::default()
            }
        },
        serde_yaml::Value::Mapping(map) => {
            let mut nested_props = IndexMap::new();
            extract_properties(map, &mut nested_props, "");

            SchemaProperty {
                schema_type: SchemaType::Object,
                properties: if nested_props.is_empty() { None } else { Some(nested_props) },
                description: None,
                ..Default::default()
            }
        },
        serde_yaml::Value::Null => SchemaProperty {
            schema_type: SchemaType::Null,
            description: None,
            ..Default::default()
        },
        _ => SchemaProperty {
            schema_type: SchemaType::Any,
            description: None,
            ..Default::default()
        },
    }
}

pub async fn list_releases() -> Result<Vec<GitHubRelease>> {
    let url = format!("https://api.github.com/repos/{}/releases", FLEET_REPO);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "fleet-schema-gen")
        .send()
        .await?;

    let releases: Vec<GitHubRelease> = response.json().await?;

    Ok(releases)
}
