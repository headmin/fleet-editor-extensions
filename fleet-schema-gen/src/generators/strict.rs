use anyhow::Result;
use std::path::Path;
use std::fs;
use crate::schema::types::{FleetSchema, SchemaDefinition, AdditionalProperties};

pub fn generate(schema: &FleetSchema, output_dir: &Path) -> Result<()> {
    println!("\n=== Generating Strict Validation Schema ===");

    fs::create_dir_all(output_dir)?;

    // Create strict schema with additionalProperties: false
    let mut strict_schema = schema.default_schema.clone();

    // Enable strict mode recursively
    make_strict(&mut strict_schema);

    // Update to Draft 2020-12
    strict_schema.schema = Some("https://json-schema.org/draft/2020-12/schema".to_string());
    strict_schema.title = Some("Fleet GitOps Default Configuration (Strict)".to_string());
    strict_schema.description = Some(
        "Strict JSON Schema for Fleet default.yml files with no additional properties allowed".to_string()
    );

    let output_path = output_dir.join("fleet-gitops-default.strict.schema.json");
    let json = serde_json::to_string_pretty(&strict_schema)?;
    fs::write(&output_path, json)?;

    println!("  ✓ fleet-gitops-default.strict.schema.json");
    println!("✓ Strict schema generated at: {}", output_dir.display());

    Ok(())
}

fn make_strict(schema: &mut SchemaDefinition) {
    // Set additionalProperties to false for this level
    if schema.type_.as_ref().map(|t| matches!(t, crate::schema::types::SchemaType::Single(s) if s == "object")).unwrap_or(false) {
        schema.additional_properties = Some(AdditionalProperties::Boolean(false));
    }

    // Recursively apply to properties
    if let Some(props) = &mut schema.properties {
        for prop in props.values_mut() {
            make_strict(prop);
        }
    }

    // Recursively apply to definitions
    if let Some(defs) = &mut schema.defs {
        for def in defs.values_mut() {
            make_strict(def);
        }
    }

    // Apply to array items
    if let Some(items) = &mut schema.items {
        make_strict(items);
    }

    // Apply to oneOf/anyOf
    if let Some(one_of) = &mut schema.one_of {
        for item in one_of {
            make_strict(item);
        }
    }

    if let Some(any_of) = &mut schema.any_of {
        for item in any_of {
            make_strict(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::types::SchemaType;

    #[test]
    fn test_make_strict() {
        let mut schema = SchemaDefinition {
            type_: Some(SchemaType::Single("object".to_string())),
            ..Default::default()
        };

        make_strict(&mut schema);

        assert!(matches!(
            schema.additional_properties,
            Some(AdditionalProperties::Boolean(false))
        ));
    }
}
