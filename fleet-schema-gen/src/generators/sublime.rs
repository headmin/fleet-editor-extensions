use anyhow::Result;
use std::path::Path;
use std::fs;
use serde_json::json;
use crate::schema::types::FleetSchema;

pub fn generate(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n=== Generating Sublime Text Package ===");

    fs::create_dir_all(output_dir)?;

    // 1. Generate JSON schemas for LSP-json
    generate_schemas(schema, output_dir)?;

    // 2. Generate completions files
    generate_completions(schema, output_dir)?;

    // 3. Generate snippets
    generate_snippets(output_dir)?;

    // 4. Generate syntax highlighting
    generate_syntax(output_dir)?;

    // 5. Generate project settings
    generate_project_settings(output_dir)?;

    // 6. Generate package metadata
    generate_package_metadata(output_dir)?;

    println!("✓ Sublime Text package generated at: {}", output_dir.display());

    Ok(())
}

fn generate_schemas(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n  → Generating JSON schemas for LSP-json...");

    let schemas_dir = output_dir.join("schemas");
    fs::create_dir_all(&schemas_dir)?;

    // Generate schema files (compatible with LSP-json)
    let schemas = vec![
        ("default.schema.json", &schema.default_schema, "Fleet Default Configuration"),
        ("team.schema.json", &schema.team_schema, "Fleet Team Configuration"),
        ("policy.schema.json", &schema.policy_schema, "Fleet Policy"),
        ("query.schema.json", &schema.query_schema, "Fleet Query"),
        ("label.schema.json", &schema.label_schema, "Fleet Label"),
    ];

    for (filename, schema_def, title) in schemas {
        let mut output_schema = schema_def.clone();
        output_schema.title = Some(title.to_string());

        let json = serde_json::to_string_pretty(&output_schema)?;
        fs::write(schemas_dir.join(filename), json)?;

        println!("    ✓ {}", filename);
    }

    Ok(())
}

fn generate_completions(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n  → Generating Sublime completions...");

    let completions_dir = output_dir.join("completions");
    fs::create_dir_all(&completions_dir)?;

    // Generate policy completions
    generate_policy_completions(&schema.policy_schema, &completions_dir)?;

    // Generate query completions
    generate_query_completions(&schema.query_schema, &completions_dir)?;

    // Generate label completions
    generate_label_completions(&schema.label_schema, &completions_dir)?;

    Ok(())
}

fn generate_policy_completions(
    schema: &crate::schema::types::SchemaDefinition,
    output_dir: &Path,
) -> Result<()> {
    let mut completions = Vec::new();

    // Extract completions from schema
    if let Some(props) = &schema.properties {
        for (name, prop) in props {
            let mut completion = json!({
                "trigger": name,
                "kind": "keyword"
            });

            if let Some(desc) = &prop.description {
                completion["details"] = json!(desc);
            }

            // Add enum values as separate completions
            if let Some(enum_vals) = &prop.enum_ {
                for val in enum_vals {
                    completions.push(json!({
                        "trigger": format!("{}\t{}", val.as_str().unwrap_or(""), name),
                        "contents": val.as_str().unwrap_or(""),
                        "kind": "variable",
                        "details": format!("{} value", name)
                    }));
                }
            } else {
                completions.push(completion);
            }
        }
    }

    let output = json!({
        "scope": "source.yaml meta.policy - comment - string",
        "completions": completions
    });

    let path = output_dir.join("fleet-policies.sublime-completions");
    let json_str = serde_json::to_string_pretty(&output)?;
    fs::write(&path, json_str)?;

    println!("    ✓ fleet-policies.sublime-completions");

    Ok(())
}

fn generate_query_completions(
    schema: &crate::schema::types::SchemaDefinition,
    output_dir: &Path,
) -> Result<()> {
    let mut completions = Vec::new();

    if let Some(props) = &schema.properties {
        for (name, prop) in props {
            let mut completion = json!({
                "trigger": name,
                "kind": "keyword"
            });

            if let Some(desc) = &prop.description {
                completion["details"] = json!(desc);
            }

            completions.push(completion);
        }
    }

    let output = json!({
        "scope": "source.yaml meta.query - comment - string",
        "completions": completions
    });

    let path = output_dir.join("fleet-queries.sublime-completions");
    let json_str = serde_json::to_string_pretty(&output)?;
    fs::write(&path, json_str)?;

    println!("    ✓ fleet-queries.sublime-completions");

    Ok(())
}

fn generate_label_completions(
    schema: &crate::schema::types::SchemaDefinition,
    output_dir: &Path,
) -> Result<()> {
    let mut completions = Vec::new();

    if let Some(props) = &schema.properties {
        for (name, prop) in props {
            let mut completion = json!({
                "trigger": name,
                "kind": "keyword"
            });

            if let Some(desc) = &prop.description {
                completion["details"] = json!(desc);
            }

            completions.push(completion);
        }
    }

    let output = json!({
        "scope": "source.yaml meta.label - comment - string",
        "completions": completions
    });

    let path = output_dir.join("fleet-labels.sublime-completions");
    let json_str = serde_json::to_string_pretty(&output)?;
    fs::write(&path, json_str)?;

    println!("    ✓ fleet-labels.sublime-completions");

    Ok(())
}

fn generate_snippets(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating Sublime snippets...");

    let snippets_dir = output_dir.join("snippets");
    fs::create_dir_all(&snippets_dir)?;

    // Policy snippet
    let policy_snippet = r#"<snippet>
    <content><![CDATA[
- name: "${1:Platform} - ${2:Check name}"
  description: "${3:Policy description}"
  query: "${4:SELECT 1 FROM table WHERE condition;}"
  platform: "${5:darwin}"
  critical: ${6:false}
]]></content>
    <tabTrigger>fleet-policy</tabTrigger>
    <scope>source.yaml</scope>
    <description>Fleet Policy Template</description>
</snippet>"#;

    fs::write(snippets_dir.join("fleet-policy.sublime-snippet"), policy_snippet)?;
    println!("    ✓ fleet-policy.sublime-snippet");

    // Query snippet
    let query_snippet = r#"<snippet>
    <content><![CDATA[
- name: "${1:query_name}"
  query: "${2:SELECT * FROM table;}"
  description: "${3:Query description}"
  interval: ${4:3600}
  platform: "${5:darwin}"
]]></content>
    <tabTrigger>fleet-query</tabTrigger>
    <scope>source.yaml</scope>
    <description>Fleet Query Template</description>
</snippet>"#;

    fs::write(snippets_dir.join("fleet-query.sublime-snippet"), query_snippet)?;
    println!("    ✓ fleet-query.sublime-snippet");

    // Label snippet
    let label_snippet = r#"<snippet>
    <content><![CDATA[
- name: "${1:Label name}"
  query: "${2:SELECT 1 FROM system_info WHERE condition;}"
  description: "${3:Label description}"
]]></content>
    <tabTrigger>fleet-label</tabTrigger>
    <scope>source.yaml</scope>
    <description>Fleet Label Template</description>
</snippet>"#;

    fs::write(snippets_dir.join("fleet-label.sublime-snippet"), label_snippet)?;
    println!("    ✓ fleet-label.sublime-snippet");

    Ok(())
}

fn generate_syntax(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating syntax highlighting...");

    // Generate a simple syntax file for Fleet YAML
    let syntax = json!({
        "name": "Fleet GitOps YAML",
        "file_extensions": ["fleet.yml", "fleet.yaml"],
        "scope": "source.yaml.fleet",
        "contexts": {
            "main": [
                {
                    "match": "\\b(policies|queries|labels|controls|org_settings|team_settings|agent_options|software)\\b",
                    "scope": "keyword.other.fleet"
                },
                {
                    "match": "\\b(name|description|query|platform|critical|resolution|interval|logging)\\b:",
                    "scope": "variable.parameter.fleet"
                },
                {
                    "match": "\\b(darwin|windows|linux|chrome)\\b",
                    "scope": "constant.language.platform.fleet"
                },
                {
                    "match": "\\b(true|false)\\b",
                    "scope": "constant.language.boolean.yaml"
                }
            ]
        }
    });

    let path = output_dir.join("Fleet-GitOps.sublime-syntax");
    let json_str = serde_json::to_string_pretty(&syntax)?;
    fs::write(&path, json_str)?;

    println!("    ✓ Fleet-GitOps.sublime-syntax");

    Ok(())
}

fn generate_project_settings(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating project settings...");

    let settings = json!({
        "settings": {
            "LSP": {
                "LSP-json": {
                    "settings": {
                        "json.schemas": [
                            {
                                "fileMatch": ["default.yml", "default.yaml"],
                                "url": "./sublime/schemas/default.schema.json"
                            },
                            {
                                "fileMatch": ["teams/*.yml", "teams/*.yaml"],
                                "url": "./sublime/schemas/team.schema.json"
                            },
                            {
                                "fileMatch": ["lib/policies/*.yml"],
                                "url": "./sublime/schemas/policy.schema.json"
                            },
                            {
                                "fileMatch": ["lib/queries/*.yml"],
                                "url": "./sublime/schemas/query.schema.json"
                            },
                            {
                                "fileMatch": ["lib/labels/*.yml"],
                                "url": "./sublime/schemas/label.schema.json"
                            }
                        ]
                    }
                }
            }
        }
    });

    let path = output_dir.join("fleet-gitops.sublime-project");
    let json_str = serde_json::to_string_pretty(&settings)?;
    fs::write(&path, json_str)?;

    println!("    ✓ fleet-gitops.sublime-project");

    Ok(())
}

fn generate_package_metadata(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating package metadata...");

    // Create README for Sublime package
    let readme = r#"# Fleet GitOps - Sublime Text Package

Auto-generated Sublime Text package for Fleet GitOps YAML editing.

## Features

- JSON Schema validation via LSP-json
- Auto-completion for Fleet fields
- Code snippets for policies, queries, and labels
- Syntax highlighting for Fleet YAML files

## Installation

### Prerequisites

1. Install [LSP](https://packagecontrol.io/packages/LSP) package
2. Install [LSP-json](https://packagecontrol.io/packages/LSP-json) package

### Setup

1. Copy this package to your Sublime Text `Packages/User/` directory
2. Open your Fleet GitOps project
3. Enable LSP-json for YAML files in LSP settings

## Usage

### Snippets

- `fleet-policy` - Create a new policy
- `fleet-query` - Create a new query
- `fleet-label` - Create a new label

### Completions

Auto-completions will appear as you type in Fleet YAML files.

### Schema Validation

The LSP-json integration will validate your YAML files against the Fleet schema and show errors inline.

## Generated by fleet-schema-gen

This package was automatically generated. Do not edit manually.
"#;

    fs::write(output_dir.join("README.md"), readme)?;
    println!("    ✓ README.md");

    // Create package.json metadata
    let package_json = json!({
        "name": "Fleet GitOps",
        "description": "Sublime Text support for Fleet GitOps YAML files",
        "version": "1.0.0",
        "author": "Generated by fleet-schema-gen",
        "license": "MIT"
    });

    let path = output_dir.join("package.json");
    let json_str = serde_json::to_string_pretty(&package_json)?;
    fs::write(&path, json_str)?;

    println!("    ✓ package.json");

    Ok(())
}
