use anyhow::Result;
use std::path::Path;
use std::fs;
use serde_json::json;
use crate::schema::types::FleetSchema;

pub fn generate(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n=== Generating VSCode Schemas ===");

    // Create .vscode directory structure
    let vscode_dir = output_dir.join(".vscode");
    let schema_dir = vscode_dir.join("fleet-gitops-schema");
    fs::create_dir_all(&schema_dir)?;

    // Generate individual schema files
    generate_schema_file(&schema.default_schema, &schema_dir.join("default.schema.json"), "Fleet Default Configuration")?;
    generate_schema_file(&schema.team_schema, &schema_dir.join("team.schema.json"), "Fleet Team Configuration")?;
    generate_schema_file(&schema.policy_schema, &schema_dir.join("policy.schema.json"), "Fleet Policy")?;
    generate_schema_file(&schema.query_schema, &schema_dir.join("query.schema.json"), "Fleet Query")?;
    generate_schema_file(&schema.label_schema, &schema_dir.join("label.schema.json"), "Fleet Label")?;

    // Generate VSCode settings.json in .vscode/
    generate_settings_file(&vscode_dir)?;

    // Generate metadata file
    generate_metadata(schema, &schema_dir)?;

    println!("✓ VSCode schemas generated in: {}/.vscode/", output_dir.display());

    Ok(())
}

fn generate_schema_file(
    schema: &crate::schema::types::SchemaDefinition,
    path: &Path,
    title: &str,
) -> Result<()> {
    let mut output_schema = schema.clone();
    output_schema.title = Some(title.to_string());

    let json = serde_json::to_string_pretty(&output_schema)?;
    fs::write(path, json)?;

    println!("  ✓ {}", path.file_name().unwrap().to_str().unwrap());

    Ok(())
}

fn generate_settings_file(vscode_dir: &Path) -> Result<()> {
    let settings = json!({
        "yaml.schemas": {
            ".vscode/fleet-gitops-schema/default.schema.json": ["default.yml", "default.yaml"],
            ".vscode/fleet-gitops-schema/team.schema.json": ["teams/*.yml", "teams/*.yaml"],
            ".vscode/fleet-gitops-schema/policy.schema.json": ["lib/policies/*.yml", "lib/policies/*.yaml"],
            ".vscode/fleet-gitops-schema/query.schema.json": ["lib/queries/*.yml", "lib/queries/*.yaml"],
            ".vscode/fleet-gitops-schema/label.schema.json": ["lib/labels/*.yml", "lib/labels/*.yaml"]
        },
        "yaml.validate": true,
        "yaml.completion": true,
        "yaml.hover": true,
        "yaml.format.enable": true,
        "editor.quickSuggestions": {
            "strings": true
        }
    });

    let settings_path = vscode_dir.join("settings.json");
    let json = serde_json::to_string_pretty(&settings)?;
    fs::write(&settings_path, json)?;

    println!("  ✓ .vscode/settings.json");

    Ok(())
}

fn generate_metadata(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    let metadata = json!({
        "generated_at": schema.metadata.generated_at,
        "fleet_version": schema.metadata.fleet_version,
        "sources": schema.metadata.sources,
        "generator": "fleet-schema-gen",
        "generator_version": env!("CARGO_PKG_VERSION")
    });

    let metadata_path = output_dir.join("metadata.json");
    let json = serde_json::to_string_pretty(&metadata)?;
    fs::write(&metadata_path, json)?;

    println!("  ✓ metadata.json");

    Ok(())
}

pub fn generate_snippets(output_dir: &Path) -> Result<()> {
    let snippets = json!({
        "Fleet Policy - Firewall Check": {
            "prefix": "fleet-policy-firewall",
            "body": [
                "- name: \"${1:Platform} - Firewall enabled\"",
                "  description: \"${2:Ensure the system firewall is enabled}\"",
                "  query: \"${3:SELECT 1 FROM alf WHERE global_state >= 1;}\"",
                "  platform: \"${4|darwin,windows,linux,chrome|}\"",
                "  critical: ${5|false,true|}"
            ],
            "description": "Create a firewall policy"
        },
        "Fleet Query - USB Devices": {
            "prefix": "fleet-query-usb",
            "body": [
                "- name: \"${1:get_usb_devices}\"",
                "  query: \"${2:SELECT * FROM usb_devices;}\"",
                "  description: \"${3:List all connected USB devices}\"",
                "  interval: ${4:3600}",
                "  platform: \"${5|darwin,windows,linux|}\""
            ],
            "description": "Create a USB devices query"
        },
        "Fleet Label - Device Type": {
            "prefix": "fleet-label-device",
            "body": [
                "- name: \"${1:macOS laptops}\"",
                "  query: \"${2:SELECT 1 FROM system_info WHERE hardware_model LIKE '%Book%';}\"",
                "  description: \"${3:All macOS laptop devices}\""
            ],
            "description": "Create a device label"
        }
    });

    let snippets_path = output_dir.join("fleet-gitops.code-snippets");
    let json = serde_json::to_string_pretty(&snippets)?;
    fs::write(&snippets_path, json)?;

    println!("  ✓ fleet-gitops.code-snippets");

    Ok(())
}
