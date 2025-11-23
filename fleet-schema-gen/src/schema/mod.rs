pub mod types;
pub mod merger;

use anyhow::Result;
use std::path::Path;
use crate::sources;
use types::FleetSchema;

pub async fn build_schema(
    fleet_version: Option<String>,
    schema_defs_path: &Path,
    source: &str,
) -> Result<FleetSchema> {
    let version = fleet_version.unwrap_or_else(|| "latest".to_string());

    println!("Building schema from multiple sources...");

    match source {
        "go" => {
            // Parse Fleet Go source code only
            println!("  → Parsing Fleet Go source code...");
            let go_data = sources::go_parser::fetch_from_fleet_repo(Some(&version)).await?;

            // Still load local enhancements for IDE-specific features
            println!("  → Loading local YAML enhancements...");
            let local_data = sources::yaml_defs::load_enhancements(schema_defs_path)?;

            // Merge Go schema with local enhancements
            let merged = merger::merge_with_go_schema(go_data, local_data, &version)?;

            println!("✓ Schema built successfully from Go source");
            Ok(merged)
        }
        "examples" => {
            // Infer from YAML examples only
            println!("  → Fetching from GitHub examples...");
            let github_data = sources::github::fetch_schema(&version).await?;

            println!("  → Loading local YAML enhancements...");
            let local_data = sources::yaml_defs::load_enhancements(schema_defs_path)?;

            let merged = merger::merge_with_examples(github_data, local_data, &version)?;

            println!("✓ Schema built successfully from examples");
            Ok(merged)
        }
        "docs" => {
            // Scrape from Fleet docs only
            println!("  → Fetching from Fleet documentation...");
            let docs_data = sources::docs_scraper::fetch_schema().await?;

            println!("  → Loading local YAML enhancements...");
            let local_data = sources::yaml_defs::load_enhancements(schema_defs_path)?;

            let merged = merger::merge_with_docs(docs_data, local_data, &version)?;

            println!("✓ Schema built successfully from docs");
            Ok(merged)
        }
        "hybrid" | _ => {
            // Hybrid: Go source + Examples + Docs + Local
            println!("  → Parsing Fleet Go source code...");
            let go_data = sources::go_parser::fetch_from_fleet_repo(Some(&version)).await?;

            println!("  → Fetching from Fleet documentation...");
            let docs_data = sources::docs_scraper::fetch_schema().await?;

            println!("  → Fetching from GitHub examples...");
            let github_data = sources::github::fetch_schema(&version).await?;

            println!("  → Loading local YAML enhancements...");
            let local_data = sources::yaml_defs::load_enhancements(schema_defs_path)?;

            // Merge with priority: Go > Docs > Examples > Local
            println!("  → Merging schemas with priority: Go > Docs > Examples > Local");
            let merged = merger::merge_all_sources(go_data, docs_data, github_data, local_data, &version)?;

            println!("✓ Schema built successfully (hybrid mode)");
            Ok(merged)
        }
    }
}
