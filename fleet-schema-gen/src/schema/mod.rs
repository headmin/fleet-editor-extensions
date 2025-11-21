pub mod types;
pub mod merger;

use anyhow::Result;
use std::path::Path;
use crate::sources;
use types::FleetSchema;

pub async fn build_schema(
    fleet_version: Option<String>,
    schema_defs_path: &Path,
) -> Result<FleetSchema> {
    let version = fleet_version.unwrap_or_else(|| "latest".to_string());

    println!("Building schema from multiple sources...");

    // 1. Fetch from Fleet documentation
    println!("  → Fetching from Fleet documentation...");
    let docs_data = sources::docs_scraper::fetch_schema().await?;

    // 2. Fetch from GitHub (releases, examples)
    println!("  → Fetching from GitHub...");
    let github_data = sources::github::fetch_schema(&version).await?;

    // 3. Load local YAML enhancements
    println!("  → Loading local YAML enhancements...");
    let local_data = sources::yaml_defs::load_enhancements(schema_defs_path)?;

    // 4. Merge all sources
    println!("  → Merging schemas...");
    let merged = merger::merge_schemas(docs_data, github_data, local_data, &version)?;

    println!("✓ Schema built successfully");

    Ok(merged)
}
