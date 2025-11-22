use anyhow::Result;
use indexmap::IndexMap;
use crate::schema::types::{FleetSchema, SchemaDefinition, SchemaMetadata, YamlEnhancement, AdditionalProperties};
use crate::sources::yaml_defs;
use chrono::Utc;

pub fn merge_schemas(
    docs_schema: SchemaDefinition,
    github_schema: SchemaDefinition,
    enhancements: IndexMap<String, YamlEnhancement>,
    version: &str,
) -> Result<FleetSchema> {
    println!("  → Merging schemas with priority: Local YAML > Docs > GitHub");

    // Start with base schema from docs
    let mut base_schema = docs_schema;

    // Merge GitHub schema (for fields not in docs)
    merge_schema_definitions(&mut base_schema, github_schema);

    // Apply manual enhancements from YAML files
    apply_enhancements(&mut base_schema, &enhancements)?;

    // Split into specialized schemas for different file types
    let mut default_schema = base_schema.clone();

    // Enable strict validation for default schema
    default_schema.additional_properties = Some(AdditionalProperties::Boolean(false));
    let team_schema = create_team_schema(&base_schema);
    let policy_schema = create_policy_schema(&enhancements);
    let query_schema = create_query_schema(&enhancements);
    let label_schema = create_label_schema(&enhancements);

    let metadata = SchemaMetadata {
        generated_at: Utc::now().to_rfc3339(),
        fleet_version: version.to_string(),
        sources: vec![
            "Fleet Documentation".to_string(),
            "GitHub fleet-gitops".to_string(),
            "Local YAML Enhancements".to_string(),
        ],
    };

    Ok(FleetSchema {
        version: version.to_string(),
        default_schema,
        team_schema,
        policy_schema,
        query_schema,
        label_schema,
        metadata,
    })
}

fn merge_schema_definitions(base: &mut SchemaDefinition, overlay: SchemaDefinition) {
    // Merge properties
    if let Some(overlay_props) = overlay.properties {
        let base_props = base.properties.get_or_insert_with(IndexMap::new);

        for (key, value) in overlay_props {
            if !base_props.contains_key(&key) {
                base_props.insert(key, value);
            }
        }
    }

    // Merge definitions
    if let Some(overlay_defs) = overlay.defs {
        let base_defs = base.defs.get_or_insert_with(IndexMap::new);

        for (key, value) in overlay_defs {
            if !base_defs.contains_key(&key) {
                base_defs.insert(key, value);
            }
        }
    }
}

fn apply_enhancements(
    schema: &mut SchemaDefinition,
    enhancements: &IndexMap<String, YamlEnhancement>,
) -> Result<()> {
    // Apply field-level enhancements
    for (name, enhancement) in enhancements {
        if let Some(fields) = &enhancement.fields {
            // Apply to matching properties in schema
            if let Some(props) = &mut schema.properties {
                if let Some(prop) = props.get_mut(name) {
                    for (field_name, field_enhancement) in fields {
                        if let Some(field_props) = &mut prop.properties {
                            if let Some(field_prop) = field_props.get_mut(field_name) {
                                yaml_defs::merge_field_enhancement(field_prop, field_enhancement);
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn create_team_schema(base: &SchemaDefinition) -> SchemaDefinition {
    // Team schema is similar to default but without some fields
    let mut team = base.clone();

    // Teams don't have certain top-level fields
    if let Some(props) = &mut team.properties {
        props.swap_remove("org_settings");
        props.swap_remove("controls");
    }

    team.title = Some("Fleet Team Configuration".to_string());
    team.description = Some("Schema for Fleet team YAML files (teams/*.yml)".to_string());

    // Enable strict validation
    team.additional_properties = Some(AdditionalProperties::Boolean(false));

    team
}

fn create_policy_schema(enhancements: &IndexMap<String, YamlEnhancement>) -> SchemaDefinition {
    use crate::schema::types::{SchemaType, SchemaProperty};

    let mut properties = IndexMap::new();

    // Get policy enhancements if available
    let policy_enhancements = enhancements.get("policies")
        .and_then(|e| e.fields.as_ref());

    // Define policy fields
    let fields = vec![
        ("name", "string", "Policy name", true),
        ("description", "string", "Policy description", false),
        ("query", "string", "SQL query for the policy", true),
        ("platform", "string", "Target platform", false),
        ("critical", "boolean", "Whether policy is critical", false),
        ("resolution", "string", "Resolution instructions", false),
        ("calendar_events_enabled", "boolean", "Enable calendar events", false),
    ];

    for (name, type_, desc, _required) in fields {
        let mut prop = SchemaProperty {
            type_: Some(SchemaType::Single(type_.to_string())),
            description: Some(desc.to_string()),
            ..Default::default()
        };

        // Apply enhancements if available
        if let Some(enhancements) = policy_enhancements {
            if let Some(enhancement) = enhancements.get(name) {
                yaml_defs::merge_field_enhancement(&mut prop, enhancement);
            }
        }

        properties.insert(name.to_string(), prop);
    }

    SchemaDefinition {
        schema: Some("https://json-schema.org/draft-07/schema#".to_string()),
        title: Some("Fleet Policy".to_string()),
        description: Some("Schema for individual Fleet policy definitions".to_string()),
        type_: Some(SchemaType::Single("object".to_string())),
        properties: Some(properties),
        required: Some(vec!["name".to_string(), "query".to_string()]),
        additional_properties: Some(AdditionalProperties::Boolean(false)),
        ..Default::default()
    }
}

fn create_query_schema(enhancements: &IndexMap<String, YamlEnhancement>) -> SchemaDefinition {
    use crate::schema::types::{SchemaType, SchemaProperty};

    let mut properties = IndexMap::new();

    let query_enhancements = enhancements.get("queries")
        .and_then(|e| e.fields.as_ref());

    let fields = vec![
        ("name", "string", "Query name", true),
        ("query", "string", "SQL query", true),
        ("description", "string", "Query description", false),
        ("interval", "integer", "Execution interval in seconds", false),
        ("platform", "string", "Target platform", false),
        ("observer_can_run", "boolean", "Whether observers can run this query", false),
        ("automations_enabled", "boolean", "Enable automations", false),
        ("logging", "string", "Logging type", false),
    ];

    for (name, type_, desc, _required) in fields {
        let mut prop = SchemaProperty {
            type_: Some(SchemaType::Single(type_.to_string())),
            description: Some(desc.to_string()),
            ..Default::default()
        };

        if let Some(enhancements) = query_enhancements {
            if let Some(enhancement) = enhancements.get(name) {
                yaml_defs::merge_field_enhancement(&mut prop, enhancement);
            }
        }

        properties.insert(name.to_string(), prop);
    }

    SchemaDefinition {
        schema: Some("https://json-schema.org/draft-07/schema#".to_string()),
        title: Some("Fleet Query".to_string()),
        description: Some("Schema for individual Fleet query definitions".to_string()),
        type_: Some(SchemaType::Single("object".to_string())),
        properties: Some(properties),
        required: Some(vec!["name".to_string(), "query".to_string()]),
        additional_properties: Some(AdditionalProperties::Boolean(false)),
        ..Default::default()
    }
}

fn create_label_schema(enhancements: &IndexMap<String, YamlEnhancement>) -> SchemaDefinition {
    use crate::schema::types::{SchemaType, SchemaProperty};

    let mut properties = IndexMap::new();

    let label_enhancements = enhancements.get("labels")
        .and_then(|e| e.fields.as_ref());

    let fields = vec![
        ("name", "string", "Label name", true),
        ("query", "string", "SQL query for label membership", true),
        ("description", "string", "Label description", false),
        ("label_type", "string", "Label type", false),
    ];

    for (name, type_, desc, _required) in fields {
        let mut prop = SchemaProperty {
            type_: Some(SchemaType::Single(type_.to_string())),
            description: Some(desc.to_string()),
            ..Default::default()
        };

        if let Some(enhancements) = label_enhancements {
            if let Some(enhancement) = enhancements.get(name) {
                yaml_defs::merge_field_enhancement(&mut prop, enhancement);
            }
        }

        properties.insert(name.to_string(), prop);
    }

    SchemaDefinition {
        schema: Some("https://json-schema.org/draft-07/schema#".to_string()),
        title: Some("Fleet Label".to_string()),
        description: Some("Schema for individual Fleet label definitions".to_string()),
        type_: Some(SchemaType::Single("object".to_string())),
        properties: Some(properties),
        required: Some(vec!["name".to_string(), "query".to_string()]),
        additional_properties: Some(AdditionalProperties::Boolean(false)),
        ..Default::default()
    }
}
