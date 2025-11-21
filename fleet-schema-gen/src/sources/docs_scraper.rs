use anyhow::Result;
use scraper::{Html, Selector};
use std::path::Path;
use indexmap::IndexMap;
use crate::schema::types::{SchemaDefinition, SchemaProperty, SchemaType};

pub async fn fetch_schema() -> Result<SchemaDefinition> {
    let url = "https://fleetdm.com/docs/configuration/yaml-files";

    let response = reqwest::get(url).await?;
    let body = response.text().await?;

    let document = Html::parse_document(&body);

    // Parse the documentation HTML to extract schema information
    let schema = parse_documentation(&document)?;

    Ok(schema)
}

pub async fn fetch_and_save(output_dir: &Path) -> Result<()> {
    let schema = fetch_schema().await?;

    let output_path = output_dir.join("fleet-docs-schema.json");
    std::fs::create_dir_all(output_dir)?;

    let json = serde_json::to_string_pretty(&schema)?;
    std::fs::write(&output_path, json)?;

    println!("  ✓ Saved to: {}", output_path.display());

    Ok(())
}

fn parse_documentation(document: &Html) -> Result<SchemaDefinition> {
    let mut schema = SchemaDefinition {
        schema: Some("https://json-schema.org/draft-07/schema#".to_string()),
        type_: Some(SchemaType::Single("object".to_string())),
        properties: Some(IndexMap::new()),
        ..Default::default()
    };

    // Parse the documentation structure
    // This is a simplified version - real implementation would parse tables, code blocks, etc.

    // Look for code blocks with YAML examples
    let code_selector = Selector::parse("pre code").unwrap();
    let heading_selector = Selector::parse("h2, h3, h4").unwrap();

    let mut current_section = String::new();

    for element in document.select(&heading_selector) {
        let text = element.text().collect::<String>();
        current_section = text.trim().to_lowercase();

        // Detect sections like "Labels", "Policies", "Queries"
        if let Some(props) = &mut schema.properties {
            if current_section.contains("label") {
                props.insert("labels".to_string(), create_array_schema("Label definitions"));
            } else if current_section.contains("polic") {
                props.insert("policies".to_string(), create_array_schema("Policy definitions"));
            } else if current_section.contains("quer") {
                props.insert("queries".to_string(), create_array_schema("Query definitions"));
            } else if current_section.contains("controls") {
                props.insert("controls".to_string(), create_object_schema("macOS settings controls"));
            } else if current_section.contains("org_settings") || current_section.contains("organization settings") {
                props.insert("org_settings".to_string(), create_object_schema("Organization settings"));
            } else if current_section.contains("agent_options") {
                props.insert("agent_options".to_string(), create_object_schema("Agent options configuration"));
            }
        }
    }

    // Add common fields that are always present
    if let Some(props) = &mut schema.properties {
        if !props.contains_key("apiVersion") {
            props.insert("apiVersion".to_string(), SchemaProperty {
                type_: Some(SchemaType::Single("string".to_string())),
                description: Some("API version".to_string()),
                default: Some(serde_json::json!("v1")),
                ..Default::default()
            });
        }

        if !props.contains_key("kind") {
            props.insert("kind".to_string(), SchemaProperty {
                type_: Some(SchemaType::Single("string".to_string())),
                description: Some("Resource kind".to_string()),
                ..Default::default()
            });
        }

        if !props.contains_key("spec") {
            props.insert("spec".to_string(), create_object_schema("Specification of the resource"));
        }
    }

    Ok(schema)
}

fn create_array_schema(description: &str) -> SchemaProperty {
    SchemaProperty {
        type_: Some(SchemaType::Single("array".to_string())),
        description: Some(description.to_string()),
        items: Some(Box::new(SchemaDefinition {
            type_: Some(SchemaType::Single("object".to_string())),
            ..Default::default()
        })),
        ..Default::default()
    }
}

fn create_object_schema(description: &str) -> SchemaProperty {
    SchemaProperty {
        type_: Some(SchemaType::Single("object".to_string())),
        description: Some(description.to_string()),
        ..Default::default()
    }
}
