use anyhow::Result;
use std::path::Path;
use std::fs;
use indexmap::IndexMap;
use crate::schema::types::{YamlEnhancement, FieldEnhancement};

pub fn load_enhancements(schema_defs_path: &Path) -> Result<IndexMap<String, YamlEnhancement>> {
    let mut enhancements = IndexMap::new();

    // Look for enhancement files in the schema-defs directory
    if !schema_defs_path.exists() {
        println!("  ⚠ Schema definitions directory not found: {}", schema_defs_path.display());
        println!("    Creating directory with default enhancements...");
        fs::create_dir_all(schema_defs_path)?;
        create_default_enhancements(schema_defs_path)?;
    }

    // Load all .yml files from the directory
    for entry in fs::read_dir(schema_defs_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("yml")
            || path.extension().and_then(|s| s.to_str()) == Some("yaml")
        {
            let filename = path.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");

            println!("  → Loading: {}", path.display());

            let content = fs::read_to_string(&path)?;
            let enhancement: YamlEnhancement = serde_yaml::from_str(&content)?;

            enhancements.insert(filename.to_string(), enhancement);
        }
    }

    Ok(enhancements)
}

fn create_default_enhancements(schema_defs_path: &Path) -> Result<()> {
    // Create default enhancement files as examples

    // policies.yml
    let policies_content = r#"
fields:
  name:
    description: "Human-readable policy name shown in Fleet UI"
    examples:
      - "macOS - Firewall enabled"
      - "Windows - BitLocker encryption"
      - "Linux - SSH hardening"
    vscode_hint: "Use descriptive names that indicate platform and check"

  description:
    description: "Detailed explanation of what the policy checks"
    examples:
      - "Ensures the system firewall is enabled and running"
      - "Verifies full disk encryption is active"

  query:
    description: "SQL query that returns data if policy passes"
    examples:
      - "SELECT 1 FROM alf WHERE global_state >= 1;"
      - "SELECT 1 FROM bitlocker_info WHERE protection_status = 1;"
    vscode_hint: "Query should return rows when compliant, empty when failing"

  platform:
    description: "Target operating system for this policy"
    enum:
      - darwin
      - windows
      - linux
      - chrome
    examples:
      - "darwin"
      - "windows"

  critical:
    description: "If true, hosts failing this policy are marked as 'failing'"
    default: false
    examples:
      - false
      - true
    vscode_hint: "Set to true for critical security policies"

  resolution:
    description: "Instructions for resolving policy failures"
    examples:
      - "Enable the system firewall in System Preferences > Security & Privacy"
      - "Contact IT to enable BitLocker encryption"

  calendar_events_enabled:
    description: "Enable calendar event creation for policy failures"
    default: false
"#;

    fs::write(schema_defs_path.join("policies.yml"), policies_content)?;

    // queries.yml
    let queries_content = r#"
fields:
  name:
    description: "Unique query name"
    examples:
      - "get_usb_devices"
      - "chrome_extensions"
      - "running_containers"

  query:
    description: "SQL query using osquery tables"
    examples:
      - "SELECT * FROM usb_devices;"
      - "SELECT * FROM chrome_extensions WHERE name LIKE '%admin%';"
    vscode_hint: "Use osquery table documentation for reference"

  description:
    description: "What information this query retrieves"
    examples:
      - "Lists all connected USB devices"
      - "Shows installed Chrome extensions"

  interval:
    description: "Query execution interval in seconds"
    default: 3600
    examples:
      - 3600
      - 86400
    vscode_hint: "3600 = 1 hour, 86400 = 1 day"

  platform:
    description: "Target operating system"
    enum:
      - darwin
      - windows
      - linux
      - chrome

  observer_can_run:
    description: "Whether Fleet observers can run this query"
    default: false

  automations_enabled:
    description: "Enable automations for query results"
    default: false

  logging:
    description: "Logging type for query results"
    enum:
      - snapshot
      - differential
      - differential_ignore_removals
    default: "snapshot"
"#;

    fs::write(schema_defs_path.join("queries.yml"), queries_content)?;

    // labels.yml
    let labels_content = r#"
fields:
  name:
    description: "Label name"
    examples:
      - "macOS laptops"
      - "Windows servers"
      - "Devices with full disk encryption"

  query:
    description: "SQL query that determines label membership"
    examples:
      - "SELECT 1 FROM system_info WHERE hardware_model LIKE '%Book%';"
      - "SELECT 1 FROM bitlocker_info WHERE protection_status = 1;"
    vscode_hint: "Query should return rows for hosts that match this label"

  description:
    description: "What this label represents"
    examples:
      - "All macOS laptop devices"
      - "Servers running Windows"

  label_type:
    description: "Type of label"
    enum:
      - regular
      - builtin
    default: "regular"
"#;

    fs::write(schema_defs_path.join("labels.yml"), labels_content)?;

    // version.yml
    let version_content = r#"
fleet_version: "latest"
schema_version: "1.0.0"
last_updated: "2024-01-01"
"#;

    fs::write(schema_defs_path.join("version.yml"), version_content)?;

    println!("  ✓ Created default enhancement files");

    Ok(())
}

pub fn merge_field_enhancement(
    field: &mut crate::schema::types::SchemaProperty,
    enhancement: &FieldEnhancement,
) {
    if let Some(desc) = &enhancement.description {
        field.description = Some(desc.clone());
    }

    if let Some(examples) = &enhancement.examples {
        field.examples = Some(examples.clone());
    }

    if let Some(enum_vals) = &enhancement.enum_ {
        field.enum_ = Some(enum_vals.iter().map(|s| serde_json::json!(s)).collect());
    }

    if let Some(pattern) = &enhancement.pattern {
        field.pattern = Some(pattern.clone());
    }

    if let Some(default) = &enhancement.default {
        field.default = Some(default.clone());
    }

    if let Some(snippets) = &enhancement.default_snippets {
        field.default_snippets = Some(snippets.clone());
    }
}
