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
            example: Some("name: macOS Tahoe"),
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
            example: Some("description: Hosts running macOS 26.x"),
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
            example: Some("query: SELECT 1 FROM os_version WHERE major = 26"),
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
            example: Some("software:\n  packages:\n    - path: ../lib/software/firefox.yml"),
            required: false,
            field_type: "object",
        },
    );

    // =========================================================================
    // Software package fields
    // =========================================================================
    m.insert(
        "software.packages",
        FieldDoc {
            name: "packages",
            description: "List of software packages to install on hosts. Each item references a package definition file via `path`.",
            valid_values: None,
            example: Some("packages:\n  - path: ../lib/software/firefox.yml\n    self_service: true"),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "software.packages.path",
        FieldDoc {
            name: "path",
            description: "Path to a YAML file defining the software package (URL, install scripts, etc). Paths are relative to the current file.",
            valid_values: None,
            example: Some("path: ../lib/macos/software/firefox.yml"),
            required: true,
            field_type: "string (file path)",
        },
    );

    m.insert(
        "software.packages.self_service",
        FieldDoc {
            name: "self_service",
            description: "Whether end users can install this package themselves through Fleet Desktop.",
            valid_values: Some(&["true", "false"]),
            example: Some("self_service: true"),
            required: false,
            field_type: "boolean",
        },
    );

    m.insert(
        "software.packages.install_during_setup",
        FieldDoc {
            name: "install_during_setup",
            description: "Whether to install this package during device setup (MDM enrollment).",
            valid_values: Some(&["true", "false"]),
            example: Some("install_during_setup: true"),
            required: false,
            field_type: "boolean",
        },
    );

    m.insert(
        "software.packages.categories",
        FieldDoc {
            name: "categories",
            description: "Categories for organizing the software package in Fleet Desktop.",
            valid_values: None,
            example: Some("categories:\n  - Productivity\n  - Communication"),
            required: false,
            field_type: "array of strings",
        },
    );

    m.insert(
        "software.packages.labels_include_any",
        FieldDoc {
            name: "labels_include_any",
            description: "Only install on hosts that have ANY of these labels.",
            valid_values: None,
            example: Some("labels_include_any:\n  - Engineering\n  - Product"),
            required: false,
            field_type: "array of strings",
        },
    );

    m.insert(
        "software.packages.labels_exclude_any",
        FieldDoc {
            name: "labels_exclude_any",
            description: "Do not install on hosts that have ANY of these labels.",
            valid_values: None,
            example: Some("labels_exclude_any:\n  - Contractors"),
            required: false,
            field_type: "array of strings",
        },
    );

    m.insert(
        "software.app_store_apps",
        FieldDoc {
            name: "app_store_apps",
            description: "List of App Store apps (VPP) to install via MDM.",
            valid_values: None,
            example: Some("app_store_apps:\n  - app_store_id: \"497799835\""),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "software.fleet_maintained_apps",
        FieldDoc {
            name: "fleet_maintained_apps",
            description: "List of Fleet-maintained applications to install. These are automatically updated by Fleet.",
            valid_values: None,
            example: Some("fleet_maintained_apps:\n  - slug: 1password"),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "software.fleet_maintained_apps.slug",
        FieldDoc {
            name: "slug",
            description: "The identifier slug for a Fleet-maintained app. Fleet maintains installers for popular apps.",
            valid_values: None,
            example: Some("slug: 1password"),
            required: true,
            field_type: "string",
        },
    );

    m.insert(
        "software.fleet_maintained_apps.self_service",
        FieldDoc {
            name: "self_service",
            description: "Whether end users can install this app themselves through Fleet Desktop.",
            valid_values: Some(&["true", "false"]),
            example: Some("self_service: true"),
            required: false,
            field_type: "boolean",
        },
    );

    m.insert(
        "software.fleet_maintained_apps.setup_experience",
        FieldDoc {
            name: "setup_experience",
            description: "Whether to install this app during the macOS Setup Assistant experience.",
            valid_values: Some(&["true", "false"]),
            example: Some("setup_experience: true"),
            required: false,
            field_type: "boolean",
        },
    );

    m.insert(
        "software.app_store_apps.app_store_id",
        FieldDoc {
            name: "app_store_id",
            description: "The Apple App Store ID for the app to install via VPP.",
            valid_values: None,
            example: Some("app_store_id: \"497799835\""),
            required: true,
            field_type: "string",
        },
    );

    m.insert(
        "software.app_store_apps.self_service",
        FieldDoc {
            name: "self_service",
            description: "Whether end users can install this app themselves through Fleet Desktop.",
            valid_values: Some(&["true", "false"]),
            example: Some("self_service: true"),
            required: false,
            field_type: "boolean",
        },
    );

    m.insert(
        "software.packages.setup_experience",
        FieldDoc {
            name: "setup_experience",
            description: "Whether to install this package during the macOS Setup Assistant experience.",
            valid_values: Some(&["true", "false"]),
            example: Some("setup_experience: true"),
            required: false,
            field_type: "boolean",
        },
    );

    // =========================================================================
    // Controls fields
    // =========================================================================
    m.insert(
        "controls.enable_disk_encryption",
        FieldDoc {
            name: "enable_disk_encryption",
            description: "Whether to enable disk encryption (FileVault on macOS, BitLocker on Windows) via MDM.",
            valid_values: Some(&["true", "false"]),
            example: Some("enable_disk_encryption: true"),
            required: false,
            field_type: "boolean",
        },
    );

    m.insert(
        "controls.macos_settings",
        FieldDoc {
            name: "macos_settings",
            description: "MDM settings specific to macOS devices.",
            valid_values: None,
            example: Some("macos_settings:\n  custom_settings:\n    - path: profiles/filevault.mobileconfig"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "controls.macos_settings.custom_settings",
        FieldDoc {
            name: "custom_settings",
            description: "List of custom configuration profiles to install on macOS devices.",
            valid_values: None,
            example: Some("custom_settings:\n  - path: profiles/security.mobileconfig\n    labels_include_any:\n      - Engineering"),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "controls.macos_settings.macos_setup",
        FieldDoc {
            name: "macos_setup",
            description: "Configuration for the macOS Setup Assistant experience.",
            valid_values: None,
            example: Some("macos_setup:\n  bootstrap_package: bootstrap/pkg.pkg\n  enable_end_user_authentication: true"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "controls.macos_settings.macos_updates",
        FieldDoc {
            name: "macos_updates",
            description: "macOS software update enforcement settings.",
            valid_values: None,
            example: Some("macos_updates:\n  minimum_version: \"15.0\"\n  deadline: \"2024-12-31\""),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "controls.windows_settings",
        FieldDoc {
            name: "windows_settings",
            description: "MDM settings specific to Windows devices.",
            valid_values: None,
            example: Some("windows_settings:\n  custom_settings:\n    - path: profiles/security.xml"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "controls.windows_settings.custom_settings",
        FieldDoc {
            name: "custom_settings",
            description: "List of custom configuration profiles to install on Windows devices.",
            valid_values: None,
            example: Some("custom_settings:\n  - path: profiles/bitlocker.xml"),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "controls.windows_settings.windows_updates",
        FieldDoc {
            name: "windows_updates",
            description: "Windows Update enforcement settings.",
            valid_values: None,
            example: Some("windows_updates:\n  deadline_days: 7\n  grace_period_days: 2"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "controls.scripts",
        FieldDoc {
            name: "scripts",
            description: "List of scripts to run on hosts. Each item references a script file via `path`.",
            valid_values: None,
            example: Some("scripts:\n  - path: scripts/setup.sh"),
            required: false,
            field_type: "array",
        },
    );

    // =========================================================================
    // Team settings fields
    // =========================================================================
    m.insert(
        "team_settings",
        FieldDoc {
            name: "team_settings",
            description: "Settings specific to this team.",
            valid_values: None,
            example: Some("team_settings:\n  secrets:\n    - secret: $ENROLL_SECRET"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "team_settings.secrets",
        FieldDoc {
            name: "secrets",
            description: "Enrollment secrets for adding hosts to this team.",
            valid_values: None,
            example: Some("secrets:\n  - secret: $ENROLL_SECRET"),
            required: false,
            field_type: "array",
        },
    );

    m.insert(
        "team_settings.features",
        FieldDoc {
            name: "features",
            description: "Feature flags for this team.",
            valid_values: None,
            example: Some("features:\n  enable_host_users: true\n  enable_software_inventory: true"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "team_settings.webhook_settings",
        FieldDoc {
            name: "webhook_settings",
            description: "Webhook configuration for this team.",
            valid_values: None,
            example: Some("webhook_settings:\n  failing_policies_webhook:\n    enable_failing_policies_webhook: true"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "team_settings.integrations",
        FieldDoc {
            name: "integrations",
            description: "Third-party integrations for this team (Google Calendar, etc.).",
            valid_values: None,
            example: Some("integrations:\n  google_calendar:\n    enable_calendar_events: true"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "team_settings.host_expiry_settings",
        FieldDoc {
            name: "host_expiry_settings",
            description: "Settings for automatically removing inactive hosts.",
            valid_values: None,
            example: Some("host_expiry_settings:\n  host_expiry_enabled: true\n  host_expiry_window: 30"),
            required: false,
            field_type: "object",
        },
    );

    // =========================================================================
    // Agent options fields
    // =========================================================================
    m.insert(
        "agent_options.config",
        FieldDoc {
            name: "config",
            description: "osquery configuration options.",
            valid_values: None,
            example: Some("config:\n  options:\n    distributed_interval: 10"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "agent_options.config.options",
        FieldDoc {
            name: "options",
            description: "osquery daemon options (intervals, endpoints, etc.).",
            valid_values: None,
            example: Some("options:\n  distributed_interval: 10\n  logger_tls_period: 60"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "agent_options.config.decorators",
        FieldDoc {
            name: "decorators",
            description: "osquery decorators that add extra columns to query results.",
            valid_values: None,
            example: Some("decorators:\n  load:\n    - SELECT hostname FROM system_info"),
            required: false,
            field_type: "object",
        },
    );

    m.insert(
        "agent_options.update_channels",
        FieldDoc {
            name: "update_channels",
            description: "Update channels for Fleet agent components (osqueryd, orbit, desktop).",
            valid_values: None,
            example: Some("update_channels:\n  osqueryd: stable\n  orbit: stable"),
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

    /// Test that schema covers all critical Fleet GitOps fields from workstations.yml
    /// Reference: https://github.com/fleetdm/fleet/blob/main/it-and-security/teams/workstations.yml
    #[test]
    fn test_schema_coverage_for_fleet_gitops() {
        // Top-level sections
        let top_level = ["name", "team_settings", "agent_options", "controls", "policies", "queries", "software"];
        for field in top_level {
            assert!(get_field_doc(field).is_some(), "Missing doc for top-level field: {}", field);
        }

        // Software section - IMPORTANT: packages use `path`, not `name`
        let software_fields = [
            "software.packages",
            "software.packages.path",
            "software.packages.self_service",
            "software.packages.setup_experience",
            "software.app_store_apps",
            "software.app_store_apps.app_store_id",
            "software.fleet_maintained_apps",
            "software.fleet_maintained_apps.slug",
        ];
        for field in software_fields {
            assert!(get_field_doc(field).is_some(), "Missing doc for software field: {}", field);
        }

        // Verify software.packages does NOT have a `name` field (it uses path references)
        // Use FIELD_DOCS.get() directly to check exact key, since get_field_doc() has fallbacks
        assert!(
            FIELD_DOCS.get("software.packages.name").is_none(),
            "software.packages should not have a 'name' field - it uses 'path' to reference package files"
        );

        // Controls section
        let controls_fields = [
            "controls.enable_disk_encryption",
            "controls.macos_settings",
            "controls.macos_settings.custom_settings",
            "controls.windows_settings",
            "controls.scripts",
        ];
        for field in controls_fields {
            assert!(get_field_doc(field).is_some(), "Missing doc for controls field: {}", field);
        }

        // Team settings section
        let team_settings_fields = [
            "team_settings",
            "team_settings.secrets",
            "team_settings.features",
        ];
        for field in team_settings_fields {
            assert!(get_field_doc(field).is_some(), "Missing doc for team_settings field: {}", field);
        }

        // Agent options section
        let agent_options_fields = [
            "agent_options.config",
            "agent_options.config.options",
        ];
        for field in agent_options_fields {
            assert!(get_field_doc(field).is_some(), "Missing doc for agent_options field: {}", field);
        }
    }

    /// Test that examples don't contain incorrect field structures
    #[test]
    fn test_examples_are_valid() {
        for (path, doc) in FIELD_DOCS.iter() {
            if let Some(example) = doc.example {
                // software.packages examples should use `path:`, not `name:`
                if path.starts_with("software.packages") || *path == "software" {
                    assert!(
                        !example.contains("- name:") || !example.contains("packages"),
                        "Example for {} incorrectly shows 'name:' under packages. Should use 'path:'. Example: {}",
                        path, example
                    );
                }
            }
        }
    }
}
