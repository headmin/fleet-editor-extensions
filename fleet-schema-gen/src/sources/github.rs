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
    // Parse YAML examples and infer schema structure
    let mut schema = SchemaDefinition {
        schema: Some("https://json-schema.org/draft-07/schema#".to_string()),
        ..Default::default()
    };

    // Parse each example YAML
    for example in examples {
        if let Ok(yaml) = serde_yaml::from_str::<serde_json::Value>(&example) {
            // Extract structure from parsed YAML
            // This is simplified - real implementation would recursively analyze structure
            println!("  → Parsed example YAML");
        }
    }

    Ok(schema)
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
