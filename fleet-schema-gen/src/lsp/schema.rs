//! Schema documentation for Fleet GitOps fields.
//!
//! This module provides documentation for all Fleet configuration fields,
//! used by hover and completion providers.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Documentation for a Fleet configuration field.
#[derive(Debug, Clone)]
pub struct FieldDoc {
    /// The field name (e.g., "platform", "query")
    pub name: &'static str,
    /// Description of the field
    pub description: &'static str,
    /// Valid values for enum fields
    pub valid_values: Option<&'static [&'static str]>,
    /// Example usage
    pub example: Option<&'static str>,
    /// Whether this field is required
    pub required: bool,
    /// The field's data type
    pub field_type: &'static str,
}

impl FieldDoc {
    /// Format the field documentation as markdown for hover display.
    pub fn to_markdown(&self) -> String {
        let mut md = format!("**{}**\n\n{}", self.name, self.description);

        if self.required {
            md.push_str("\n\n*Required*");
        }

        md.push_str(&format!("\n\n**Type:** `{}`", self.field_type));

        if let Some(values) = self.valid_values {
            md.push_str("\n\n**Valid values:**\n");
            for v in values {
                md.push_str(&format!("- `{}`\n", v));
            }
        }

        if let Some(example) = self.example {
            md.push_str(&format!("\n**Example:**\n```yaml\n{}\n```", example));
        }

        md
    }
}

/// Documentation for platform values.
pub static PLATFORM_DOCS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("darwin", "macOS - Apple desktop and laptop computers");
    m.insert("windows", "Microsoft Windows operating systems");
    m.insert("linux", "Linux distributions (Ubuntu, CentOS, Debian, etc.)");
    m.insert("chrome", "ChromeOS - Chromebook devices");
    m.insert("all", "All supported platforms");
    m
});

/// Documentation for logging type values.
pub static LOGGING_DOCS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert(
        "snapshot",
        "Logs all results from each query execution. Best for point-in-time data.",
    );
    m.insert(
        "differential",
        "Only logs changes (additions and removals) between query executions. Reduces log volume.",
    );
    m.insert(
        "differential_ignore_removals",
        "Like differential, but only logs additions. Useful when removals are expected.",
    );
    m
});

/// Field documentation organized by context (policies, queries, labels, etc.)
pub static FIELD_DOCS: Lazy<HashMap<&'static str, FieldDoc>> = Lazy::new(|| {
    let mut m = HashMap::new();

    // =========================================================================
    // Policy fields
    // =========================================================================
    m.insert(
        "policies.name",
        FieldDoc {
            name: "name",
            description: "The display name of the policy. Must be unique within the organization.",
            valid_values: None,
            example: Some("name: Ensure FileVault is enabled"),
            required: true,
            field_type: "string",
        },
    );

    m.insert(
        "policies.description",
        FieldDoc {
            name: "description",
            description: "A detailed description of what this policy checks and why it matters.",
            valid_values: None,
            example: Some("description: Verifies that disk encryption is enabled to protect data at rest"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "policies.query",
        FieldDoc {
            name: "query",
            description: "The osquery SQL query that determines policy compliance. Returns results when the policy is violated (failing).",
            valid_values: None,
            example: Some("query: SELECT 1 FROM disk_encryption WHERE encrypted = 0"),
            required: true,
            field_type: "string (osquery SQL)",
        },
    );

    m.insert(
        "policies.platform",
        FieldDoc {
            name: "platform",
            description: "The operating system(s) this policy applies to. The query must use tables available on this platform.",
            valid_values: Some(&["darwin", "windows", "linux", "chrome"]),
            example: Some("platform: darwin"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "policies.critical",
        FieldDoc {
            name: "critical",
            description: "Whether this policy is critical. Critical policy failures are highlighted and may trigger alerts.",
            valid_values: Some(&["true", "false"]),
            example: Some("critical: true"),
            required: false,
            field_type: "boolean",
        },
    );

    m.insert(
        "policies.resolution",
        FieldDoc {
            name: "resolution",
            description: "Instructions for end users on how to resolve a policy failure. Shown in Fleet Desktop.",
            valid_values: None,
            example: Some("resolution: Enable FileVault in System Preferences > Security & Privacy"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "policies.team",
        FieldDoc {
            name: "team",
            description: "The team this policy belongs to. If not specified, applies to all teams.",
            valid_values: None,
            example: Some("team: Engineering"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "policies.calendar_events_enabled",
        FieldDoc {
            name: "calendar_events_enabled",
            description: "Whether to create calendar events for policy failures to remind users to fix issues.",
            valid_values: Some(&["true", "false"]),
            example: Some("calendar_events_enabled: true"),
            required: false,
            field_type: "boolean",
        },
    );

    // =========================================================================
    // Query fields
    // =========================================================================
    m.insert(
        "queries.name",
        FieldDoc {
            name: "name",
            description: "The display name of the query. Must be unique within the organization.",
            valid_values: None,
            example: Some("name: Get running processes"),
            required: true,
            field_type: "string",
        },
    );

    m.insert(
        "queries.description",
        FieldDoc {
            name: "description",
            description: "A description of what this query collects and its purpose.",
            valid_values: None,
            example: Some("description: Collects all running processes for security analysis"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "queries.query",
        FieldDoc {
            name: "query",
            description: "The osquery SQL query to execute on hosts.",
            valid_values: None,
            example: Some("query: SELECT name, path, pid FROM processes"),
            required: true,
            field_type: "string (osquery SQL)",
        },
    );

    m.insert(
        "queries.interval",
        FieldDoc {
            name: "interval",
            description: "How often to run this query, in seconds. Lower values increase resource usage.",
            valid_values: None,
            example: Some("interval: 3600  # Run every hour"),
            required: false,
            field_type: "integer (seconds)",
        },
    );

    m.insert(
        "queries.platform",
        FieldDoc {
            name: "platform",
            description: "The operating system(s) this query runs on. The query must use tables available on this platform.",
            valid_values: Some(&["darwin", "windows", "linux", "chrome", "all"]),
            example: Some("platform: darwin"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "queries.logging",
        FieldDoc {
            name: "logging",
            description: "How query results are logged. Affects log volume and what data is captured.",
            valid_values: Some(&["snapshot", "differential", "differential_ignore_removals"]),
            example: Some("logging: differential"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "queries.min_osquery_version",
        FieldDoc {
            name: "min_osquery_version",
            description: "Minimum osquery version required to run this query. Hosts with older versions will skip it.",
            valid_values: None,
            example: Some("min_osquery_version: 5.0.0"),
            required: false,
            field_type: "string (semver)",
        },
    );

    m.insert(
        "queries.observer_can_run",
        FieldDoc {
            name: "observer_can_run",
            description: "Whether users with Observer role can run this query on-demand.",
            valid_values: Some(&["true", "false"]),
            example: Some("observer_can_run: true"),
            required: false,
            field_type: "boolean",
        },
    );

    m.insert(
        "queries.automations_enabled",
        FieldDoc {
            name: "automations_enabled",
            description: "Whether this query can trigger automations (webhooks, integrations).",
            valid_values: Some(&["true", "false"]),
            example: Some("automations_enabled: true"),
            required: false,
            field_type: "boolean",
        },
    );

    // =========================================================================
    // Label fields
    // =========================================================================
    m.insert(
        "labels.name",
        FieldDoc {
            name: "name",
            description: "The display name of the label. Must be unique within the organization.",
            valid_values: None,
            example: Some("name: macOS Monterey"),
            required: true,
            field_type: "string",
        },
    );

    m.insert(
        "labels.description",
        FieldDoc {
            name: "description",
            description: "A description of what hosts this label identifies.",
            valid_values: None,
            example: Some("description: Hosts running macOS 12.x"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "labels.query",
        FieldDoc {
            name: "query",
            description: "For dynamic labels, the osquery query that determines label membership. Returns results for matching hosts.",
            valid_values: None,
            example: Some("query: SELECT 1 FROM os_version WHERE major = 12"),
            required: false,
            field_type: "string (osquery SQL)",
        },
    );

    m.insert(
        "labels.platform",
        FieldDoc {
            name: "platform",
            description: "The operating system(s) this label applies to.",
            valid_values: Some(&["darwin", "windows", "linux", "chrome", "all"]),
            example: Some("platform: darwin"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "labels.label_membership_type",
        FieldDoc {
            name: "label_membership_type",
            description: "How hosts are assigned to this label: 'dynamic' (via query) or 'manual' (explicit assignment).",
            valid_values: Some(&["dynamic", "manual"]),
            example: Some("label_membership_type: dynamic"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "labels.hosts",
        FieldDoc {
            name: "hosts",
            description: "For manual labels, the list of host identifiers to include.",
            valid_values: None,
            example: Some("hosts:\n  - host1.example.com\n  - host2.example.com"),
            required: false,
            field_type: "array of strings",
        },
    );

    // =========================================================================
    // Top-level fields
    // =========================================================================
    m.insert(
        "name",
        FieldDoc {
            name: "name",
            description: "The name of this configuration file or team.",
            valid_values: None,
            example: Some("name: Engineering Team"),
            required: false,
            field_type: "string",
        },
    );

    m.insert(
        "policies",
        FieldDoc {
            name: "policies",
            description: "List of compliance policies to enforce on hosts. Policies return results when violated.",
            valid_values: None,
            example: Some("policies:\n  - name: Disk Encryption\n    query: SELECT 1 FROM disk_encryption WHERE encrypted = 0"),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "queries",
        FieldDoc {
            name: "queries",
            description: "List of osquery queries to run on hosts for data collection.",
            valid_values: None,
            example: Some("queries:\n  - name: Running Processes\n    query: SELECT * FROM processes"),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "labels",
        FieldDoc {
            name: "labels",
            description: "List of labels to categorize hosts for targeting policies and queries.",
            valid_values: None,
            example: Some("labels:\n  - name: Production Servers\n    query: SELECT 1 FROM system_info WHERE hostname LIKE 'prod-%'"),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "agent_options",
        FieldDoc {
            name: "agent_options",
            description: "osquery agent configuration options applied to hosts.",
            valid_values: None,
            example: Some("agent_options:\n  config:\n    options:\n      logger_plugin: tls"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "controls",
        FieldDoc {
            name: "controls",
            description: "MDM controls and settings for managed devices.",
            valid_values: None,
            example: Some("controls:\n  macos_settings:\n    custom_settings:\n      - path: profiles/filevault.mobileconfig"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "software",
        FieldDoc {
            name: "software",
            description: "Software packages to install or manage on hosts.",
            valid_values: None,
            example: Some("software:\n  packages:\n    - name: google-chrome"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "webhook_settings",
        FieldDoc {
            name: "webhook_settings",
            description: "Configuration for webhook notifications.",
            valid_values: None,
            example: Some("webhook_settings:\n  url: https://example.com/webhook"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "path",
        FieldDoc {
            name: "path",
            description: "Reference to another YAML file containing configuration. Paths are relative to the repository root.",
            valid_values: None,
            example: Some("- path: lib/policies/security.yml"),
            required: false,
            field_type: "string (file path)",
        },
    );

    m
});

/// Get field documentation by path (e.g., "policies.platform" or just "platform").
pub fn get_field_doc(path: &str) -> Option<&'static FieldDoc> {
    // Try exact match first
    if let Some(doc) = FIELD_DOCS.get(path) {
        return Some(doc);
    }

    // Try with common prefixes
    for prefix in &["policies", "queries", "labels"] {
        let full_path = format!("{}.{}", prefix, path);
        if let Some(doc) = FIELD_DOCS.get(full_path.as_str()) {
            return Some(doc);
        }
    }

    // Try just the field name (last segment)
    let field_name = path.split('.').last().unwrap_or(path);
    for (key, doc) in FIELD_DOCS.iter() {
        if key.ends_with(field_name) {
            return Some(doc);
        }
    }

    None
}

/// Get documentation for a platform value.
pub fn get_platform_doc(platform: &str) -> Option<&'static str> {
    PLATFORM_DOCS.get(platform).copied()
}

/// Get documentation for a logging type value.
pub fn get_logging_doc(logging: &str) -> Option<&'static str> {
    LOGGING_DOCS.get(logging).copied()
}

/// Get all valid platform values.
pub fn valid_platforms() -> &'static [&'static str] {
    &["darwin", "windows", "linux", "chrome"]
}

/// Get all valid logging type values.
pub fn valid_logging_types() -> &'static [&'static str] {
    &["snapshot", "differential", "differential_ignore_removals"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_field_doc_exact() {
        let doc = get_field_doc("policies.platform");
        assert!(doc.is_some());
        assert_eq!(doc.unwrap().name, "platform");
    }

    #[test]
    fn test_get_field_doc_simple() {
        let doc = get_field_doc("platform");
        assert!(doc.is_some());
    }

    #[test]
    fn test_field_doc_to_markdown() {
        let doc = FIELD_DOCS.get("policies.platform").unwrap();
        let md = doc.to_markdown();
        assert!(md.contains("**platform**"));
        assert!(md.contains("darwin"));
    }

    #[test]
    fn test_platform_docs() {
        assert!(get_platform_doc("darwin").is_some());
        assert!(get_platform_doc("invalid").is_none());
    }

    #[test]
    fn test_logging_docs() {
        assert!(get_logging_doc("snapshot").is_some());
        assert!(get_logging_doc("differential").is_some());
    }
}
