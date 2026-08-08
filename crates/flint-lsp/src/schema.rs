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
    /// Optional `fleetctl` CLI hint for interacting with this field
    pub cli_hint: Option<&'static str>,
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

        if let Some(cli) = self.cli_hint {
            md.push_str(&format!("\n\n**CLI:**\n```\n{}\n```", cli));
        }

        md
    }
}

/// Documentation for platform values.
pub static PLATFORM_DOCS: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("darwin", "macOS - Apple desktop and laptop computers");
    m.insert("windows", "Microsoft Windows operating systems");
    m.insert(
        "linux",
        "Linux distributions (Ubuntu, CentOS, Debian, etc.)",
    );
    m.insert("chrome", "ChromeOS - Chromebook devices");
    m.insert("ios", "iOS - Apple iPhone devices");
    m.insert("ipados", "iPadOS - Apple iPad devices");
    m.insert("android", "Android - Android mobile devices");
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
            cli_hint: None,
        },
    );

    m.insert(
        "policies.description",
        FieldDoc {
            name: "description",
            description: "A detailed description of what this policy checks and why it matters.",
            valid_values: None,
            example: Some(
                "description: Verifies that disk encryption is enabled to protect data at rest",
            ),
            required: false,
            field_type: "string",
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "policies.platform",
        FieldDoc {
            name: "platform",
            description: "The operating system(s) this policy applies to. The query must use tables available on this platform.",
            valid_values: Some(&["darwin", "windows", "linux", "chrome", "ios", "ipados", "android"]),
            example: Some("platform: darwin"),
            required: false,
            field_type: "string",
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "policies.team",
        FieldDoc {
            name: "team",
            description: "The fleet this policy belongs to. If not specified, applies globally.",
            valid_values: None,
            example: Some("team: Engineering"),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.calendar_events_enabled",
        FieldDoc {
            name: "calendar_events_enabled",
            description: "Whether to create calendar events for policy failures to remind users to fix issues. **Fleet-only** — per `yaml-files.md:245`, can only be configured on policies in a fleet file (`fleets/<name>.yml` or `fleets/unassigned.yml`), not in `default.yml`.",
            valid_values: Some(&["true", "false"]),
            example: Some("calendar_events_enabled: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.run_script",
        FieldDoc {
            name: "run_script",
            description: "Run a script when the policy fails. **Fleet-only** (per `yaml-files.md:245`) — configure in a fleet file, not `default.yml`. The script must be defined in the same fleet. Requires Fleet Premium.",
            valid_values: None,
            example: Some("run_script:\n  path: ./disable-guest-account.sh"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.software_title_id",
        FieldDoc {
            name: "software_title_id",
            description: "ID of a software title to install when the policy fails. If the software has label-based targeting, the policy inherits those labels. Requires Fleet Premium.",
            valid_values: None,
            example: Some("software_title_id: 42"),
            required: false,
            field_type: "integer",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.script_id",
        FieldDoc {
            name: "script_id",
            description: "ID of a script to run when the policy fails. Requires Fleet Premium.",
            valid_values: None,
            example: Some("script_id: 7"),
            required: false,
            field_type: "integer",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.conditional_access_enabled",
        FieldDoc {
            name: "conditional_access_enabled",
            description: "When `true`, hosts failing this policy are blocked from accessing resources gated by conditional access. Requires Fleet Premium.",
            valid_values: Some(&["true", "false"]),
            example: Some("conditional_access_enabled: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.conditional_access_bypass_enabled",
        FieldDoc {
            name: "conditional_access_bypass_enabled",
            description: "When `true`, this policy's failures can be bypassed by conditional-access-enabled hosts. Critical policies never bypass. Requires Fleet Premium.",
            valid_values: Some(&["true", "false"]),
            example: Some("conditional_access_bypass_enabled: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.webhooks_and_tickets_enabled",
        FieldDoc {
            name: "webhooks_and_tickets_enabled",
            description: "When `true`, this policy is added to the failing-policies webhook (and any ticket integrations). GitOps-only convenience: at apply time Fleet adds this policy's ID to `failing_policies_webhook.policy_ids`. Cannot be combined with explicit `policy_ids` in webhook_settings.",
            valid_values: Some(&["true", "false"]),
            example: Some("webhooks_and_tickets_enabled: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.team",
        FieldDoc {
            name: "team",
            description: "The fleet (team) this policy belongs to. Renamed to `fleet` in newer Fleet versions — both keys are accepted for compatibility.",
            valid_values: None,
            example: Some("team: Engineering"),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.type",
        FieldDoc {
            name: "type",
            description: "Policy type. `dynamic` (default) is a classic policy with an editable query. `patch` is tied to a Fleet-Maintained App via `fleet_maintained_app_slug` and auto-updates its query from the FMA metadata; on its own it only updates the policy — add `install_software: true` to install the app on failure. Patch policies require Fleet Premium.",
            valid_values: Some(&["dynamic", "patch"]),
            example: Some("type: patch"),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.fleet_maintained_app_slug",
        FieldDoc {
            name: "fleet_maintained_app_slug",
            description: "The Fleet-Maintained App slug this patch policy tracks, typically in `name/platform` form. Required when `type: patch`. See https://github.com/fleetdm/fleet/tree/main/ee/maintained-apps/outputs for available slugs.",
            valid_values: None,
            example: Some("fleet_maintained_app_slug: zoom/darwin"),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.install_software",
        FieldDoc {
            name: "install_software",
            description: "Trigger a software install when the policy fails. Two forms: (1) mapping `{package_path}` or `{hash_sha256}` for a regular policy installing a custom package; (2) boolean `true` on a patch policy to install the Fleet-Maintained App. **Fleet-only** (per `yaml-files.md:245`) — configure in a fleet file, not `default.yml`. The software must be defined in the same fleet. Requires Fleet Premium.",
            valid_values: None,
            example: Some("install_software:\n  package_path: ./firefox.package.yml"),
            required: false,
            field_type: "object | boolean",
            cli_hint: None,
        },
    );

    // Policy label scoping. Fleet validates these in
    // `server/fleet/policies.go` verifyPolicyLabelScopes: at most ONE include
    // form and at most ONE exclude form may carry values, an include and an
    // exclude form MAY be combined, and no label may appear in both an
    // include and an exclude list. Without these entries the hover fallback
    // served the `software.packages.*` wording ("Only install on hosts…"),
    // which is wrong for a policy.
    m.insert(
        "policies.labels_include_any",
        FieldDoc {
            name: "labels_include_any",
            description: "Run this policy only on hosts that are members of ANY of these labels. Mutually exclusive with `labels_include_all`; may be combined with one exclude form. Requires Fleet Premium.",
            valid_values: None,
            example: Some("labels_include_any:\n  - Engineering\n  - Product"),
            required: false,
            field_type: "array of strings",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.labels_include_all",
        FieldDoc {
            name: "labels_include_all",
            description: "Run this policy only on hosts that are members of ALL of these labels. Mutually exclusive with `labels_include_any`; may be combined with one exclude form. Requires Fleet Premium.",
            valid_values: None,
            example: Some("labels_include_all:\n  - macOS\n  - Managed"),
            required: false,
            field_type: "array of strings",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.labels_exclude_any",
        FieldDoc {
            name: "labels_exclude_any",
            description: "Do not run this policy on hosts that are members of ANY of these labels. Mutually exclusive with `labels_exclude_all`; may be combined with one include form. Requires Fleet Premium.",
            valid_values: None,
            example: Some("labels_exclude_any:\n  - Contractors"),
            required: false,
            field_type: "array of strings",
            cli_hint: None,
        },
    );

    m.insert(
        "policies.labels_exclude_all",
        FieldDoc {
            name: "labels_exclude_all",
            description: "Do not run this policy on hosts that are members of ALL of these labels — a host is skipped only when it matches every listed label. Policies only: no other GitOps context defines this key. Mutually exclusive with `labels_exclude_any`; may be combined with one include form. Requires Fleet Premium.",
            valid_values: None,
            example: Some("labels_exclude_all:\n  - Contractors\n  - Kiosk"),
            required: false,
            field_type: "array of strings",
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "queries.interval",
        FieldDoc {
            name: "interval",
            description:
                "How often to run this query, in seconds. Lower values increase resource usage.",
            valid_values: None,
            example: Some("interval: 3600  # Run every hour"),
            required: false,
            field_type: "integer (seconds)",
            cli_hint: None,
        },
    );

    m.insert(
        "queries.platform",
        FieldDoc {
            name: "platform",
            description: "The operating system(s) this query runs on. The query must use tables available on this platform.",
            valid_values: Some(&["darwin", "windows", "linux", "chrome", "ios", "ipados", "android", "all"]),
            example: Some("platform: darwin"),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "queries.logging",
        FieldDoc {
            name: "logging",
            description:
                "How query results are logged. Affects log volume and what data is captured.",
            valid_values: Some(&["snapshot", "differential", "differential_ignore_removals"]),
            example: Some("logging: differential"),
            required: false,
            field_type: "string",
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "queries.discard_data",
        FieldDoc {
            name: "discard_data",
            description: "Whether to discard query results after processing. Useful for queries that only trigger automations.",
            valid_values: Some(&["true", "false"]),
            example: Some("discard_data: false"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "labels.platform",
        FieldDoc {
            name: "platform",
            description: "The operating system(s) this label applies to.",
            valid_values: Some(&[
                "darwin", "windows", "linux", "chrome", "ios", "ipados", "android", "all",
            ]),
            example: Some("platform: darwin"),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "labels.label_membership_type",
        FieldDoc {
            name: "label_membership_type",
            description: "How hosts are assigned to this label: 'dynamic' (via query), 'manual' (explicit assignment), or 'host_vitals' (criteria on host vitals).",
            valid_values: Some(&["dynamic", "manual", "host_vitals"]),
            example: Some("label_membership_type: dynamic"),
            required: false,
            field_type: "string",
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "labels.criteria",
        FieldDoc {
            name: "criteria",
            description: "Criteria for adding hosts to a host_vitals label. A single `{vital, value}` leaf — `and`/`or` composites are not yet supported by Fleet.",
            valid_values: None,
            example: Some("criteria:\n  vital: end_user_idp_department\n  value: Engineering"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "labels.criteria.vital",
        FieldDoc {
            name: "vital",
            description: "The type of host vital to use when creating a host vital label.",
            valid_values: Some(&["end_user_idp_group", "end_user_idp_department"]),
            example: Some("vital: end_user_idp_department"),
            required: true,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "labels.criteria.value",
        FieldDoc {
            name: "value",
            description: "Hosts with vital data matching this value will be added to the label.",
            valid_values: None,
            example: Some("value: Engineering"),
            required: true,
            field_type: "string",
            cli_hint: None,
        },
    );

    // =========================================================================
    // Top-level fields
    // =========================================================================
    m.insert(
        "name",
        FieldDoc {
            name: "name",
            description: "The name of this configuration file or fleet.",
            valid_values: None,
            example: Some("name: Engineering Team"),
            required: false,
            field_type: "string",
            cli_hint: Some("fleetctl get teams --yaml               # list all fleets\nfleetctl get teams --name \"Name\" --yaml  # export fleet config"),
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
            cli_hint: Some("fleetctl get policies --yaml          # export policies\nfleetctl apply -f policies.yml        # apply changes\nfleetctl get policies --team \"Name\"   # fleet-specific"),
        },
    );

    m.insert(
        "queries",
        FieldDoc {
            name: "queries",
            description: "List of osquery queries to run on hosts for data collection. Renamed to 'reports' in newer Fleet GitOps naming (version-gated notice via deprecated-keys).",
            valid_values: None,
            example: Some("queries:\n  - name: Running Processes\n    query: SELECT * FROM processes"),
            required: false,
            field_type: "array",
            cli_hint: Some("fleetctl get queries --yaml          # export reports\nfleetctl apply -f reports.yml        # apply changes\nfleetctl get queries --team \"Name\"   # fleet-specific"),
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
            cli_hint: Some("fleetctl get labels --yaml            # export labels\nfleetctl apply -f labels.yml          # apply changes"),
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
            cli_hint: Some("fleetctl get config --yaml            # export (includes agent_options)\nfleetctl apply -f agent-options.yml   # apply changes"),
        },
    );

    m.insert(
        "controls",
        FieldDoc {
            name: "controls",
            description: "MDM controls and settings for managed devices.",
            valid_values: None,
            example: Some("controls:\n  apple_settings:\n    configuration_profiles:\n      - path: profiles/filevault.mobileconfig"),
            required: false,
            field_type: "object",
            cli_hint: Some("fleetctl apply -f controls.yml        # apply MDM profiles"),
        },
    );

    m.insert(
        "software",
        FieldDoc {
            name: "software",
            description: "Software packages to install or manage on hosts.",
            valid_values: None,
            example: Some("software:\n  packages:\n    - path: ../platforms/macos/software/firefox.yml"),
            required: false,
            field_type: "object",
            cli_hint: Some("fleetctl get software --yaml          # export software\nfleetctl apply -f software.yml        # apply changes"),
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
            example: Some("packages:\n  - path: ../platforms/macos/software/firefox.yml\n    self_service: true"),
            required: false,
            field_type: "array",
            cli_hint: None,
        },
    );

    m.insert(
        "software.packages.path",
        FieldDoc {
            name: "path",
            description: "Path to a YAML file defining the software package (URL, install scripts, etc). Paths are relative to the current file.",
            valid_values: None,
            example: Some("path: ../platforms/macos/software/firefox.yml"),
            required: true,
            field_type: "string (file path)",
            cli_hint: None,
        },
    );

    m.insert(
        "software.packages.self_service",
        FieldDoc {
            name: "self_service",
            description:
                "Whether end users can install this package themselves through Fleet Desktop.",
            valid_values: Some(&["true", "false"]),
            example: Some("self_service: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "software.packages.setup_experience",
        FieldDoc {
            name: "setup_experience",
            description:
                "Whether to install this package during the macOS Setup Assistant experience.",
            valid_values: Some(&["true", "false"]),
            example: Some("setup_experience: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    // =========================================================================
    // Software lib file fields (standalone package definitions)
    // These are used in lib/*/software/*.yml files
    // =========================================================================
    m.insert(
        "software_lib.url",
        FieldDoc {
            name: "url",
            description: "URL to download the software installer package (.pkg, .dmg, .msi, etc.).",
            valid_values: None,
            example: Some("url: https://downloads.1password.com/mac/1Password.pkg"),
            required: true,
            field_type: "string (URL)",
            cli_hint: None,
        },
    );

    m.insert(
        "software_lib.icon",
        FieldDoc {
            name: "icon",
            description: "Icon to display for this software in Fleet Desktop.",
            valid_values: None,
            example: Some("icon:\n  path: ../../all/icons/app-logo.png"),
            required: false,
            field_type: "object with path",
            cli_hint: None,
        },
    );

    m.insert(
        "software_lib.install_script",
        FieldDoc {
            name: "install_script",
            description: "Custom script to run for installation instead of the default installer.",
            valid_values: None,
            example: Some("install_script:\n  path: ./scripts/install.sh"),
            required: false,
            field_type: "object with path",
            cli_hint: None,
        },
    );

    m.insert(
        "software_lib.post_install_script",
        FieldDoc {
            name: "post_install_script",
            description: "Script to run after the software is installed.",
            valid_values: None,
            example: Some("post_install_script:\n  path: ./scripts/post-install.sh"),
            required: false,
            field_type: "object with path",
            cli_hint: None,
        },
    );

    m.insert(
        "software_lib.uninstall_script",
        FieldDoc {
            name: "uninstall_script",
            description: "Script to run when uninstalling the software.",
            valid_values: None,
            example: Some("uninstall_script:\n  path: ./scripts/uninstall.sh"),
            required: false,
            field_type: "object with path",
            cli_hint: None,
        },
    );

    m.insert(
        "software_lib.pre_install_query",
        FieldDoc {
            name: "pre_install_query",
            description: "osquery SQL query to check before installing. Installation proceeds only if the query returns results.",
            valid_values: None,
            example: Some("pre_install_query:\n  path: ./queries/check-requirements.sql"),
            required: false,
            field_type: "object with path",
            cli_hint: None,
        },
    );

    m.insert(
        "software_lib.hash_sha256",
        FieldDoc {
            name: "hash_sha256",
            description: "SHA256 hash of the installer package for verification.",
            valid_values: None,
            example: Some("hash_sha256: abc123..."),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    // =========================================================================
    // Agent options lib file fields (standalone agent options definitions)
    // These are used in lib/*/agent-options/*.yml files
    // =========================================================================
    m.insert(
        "agent_options.config",
        FieldDoc {
            name: "config",
            description: "osquery configuration options including decorators and runtime settings.",
            valid_values: None,
            example: Some("config:\n  decorators:\n    load:\n      - SELECT host_uuid AS uuid FROM system_info\n  options:\n    distributed_interval: 10"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "agent_options.config.decorators",
        FieldDoc {
            name: "decorators",
            description: "osquery decorators that add extra columns to query results. Commonly used for host identification.",
            valid_values: None,
            example: Some("decorators:\n  load:\n    - SELECT host_uuid AS uuid FROM system_info\n    - SELECT hostname FROM system_info"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "agent_options.config.options",
        FieldDoc {
            name: "options",
            description: "osquery daemon runtime options (intervals, endpoints, logging settings).",
            valid_values: None,
            example: Some("options:\n  distributed_interval: 10\n  distributed_tls_max_attempts: 3\n  logger_tls_period: 60"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "agent_options.update_channels",
        FieldDoc {
            name: "update_channels",
            description: "Update channels for Fleet agent components. Use 'stable', 'edge', or specific versions.",
            valid_values: Some(&["stable", "edge"]),
            example: Some("update_channels:\n  osqueryd: stable\n  orbit: stable\n  desktop: edge"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    // =========================================================================
    // Labels fields (for lib label files)
    // =========================================================================
    m.insert(
        "labels.label_membership_type",
        FieldDoc {
            name: "label_membership_type",
            description: "How hosts are assigned to this label. 'dynamic' uses the query, 'manual' requires explicit assignment, 'host_vitals' uses a criteria expression.",
            valid_values: Some(&["dynamic", "manual", "host_vitals"]),
            example: Some("label_membership_type: dynamic"),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "labels.hosts",
        FieldDoc {
            name: "hosts",
            description: "List of host identifiers for manual label membership. Only used when label_membership_type is 'manual'.",
            valid_values: None,
            example: Some("hosts:\n  - host1.example.com\n  - host2.example.com"),
            required: false,
            field_type: "array of strings",
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "controls.macos_settings",
        FieldDoc {
            name: "macos_settings",
            description: "Deprecated: renamed to 'apple_settings' (and 'custom_settings' to 'configuration_profiles'). MDM settings specific to macOS devices.",
            valid_values: None,
            example: Some(
                "apple_settings:\n  configuration_profiles:\n    - path: profiles/filevault.mobileconfig",
            ),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.macos_settings.custom_settings",
        FieldDoc {
            name: "custom_settings",
            description: "Deprecated: renamed to 'configuration_profiles'. List of custom configuration profiles to install on macOS devices.",
            valid_values: None,
            example: Some("custom_settings:\n  - path: profiles/security.mobileconfig\n    labels_include_any:\n      - Engineering"),
            required: false,
            field_type: "array",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.macos_settings.macos_setup",
        FieldDoc {
            name: "macos_setup",
            description: "Deprecated: renamed to 'setup_experience'. Configuration for the macOS Setup Assistant experience.",
            valid_values: None,
            example: Some("macos_setup:\n  bootstrap_package: bootstrap/pkg.pkg\n  enable_end_user_authentication: true"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.macos_settings.macos_updates",
        FieldDoc {
            name: "macos_updates",
            description: "macOS software update enforcement settings.",
            valid_values: None,
            example: Some(
                "macos_updates:\n  minimum_version: \"15.0\"\n  deadline: \"2024-12-31\"",
            ),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.windows_settings",
        FieldDoc {
            name: "windows_settings",
            description: "MDM settings specific to Windows devices.",
            valid_values: None,
            example: Some(
                "windows_settings:\n  custom_settings:\n    - path: profiles/security.xml",
            ),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.windows_settings.custom_settings",
        FieldDoc {
            name: "custom_settings",
            description: "Deprecated: renamed to 'configuration_profiles'. List of custom configuration profiles to install on Windows devices.",
            valid_values: None,
            example: Some("custom_settings:\n  - path: profiles/bitlocker.xml"),
            required: false,
            field_type: "array",
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "controls.scripts",
        FieldDoc {
            name: "scripts",
            description:
                "List of scripts to run on hosts. Each item references a script file via `path`.",
            valid_values: None,
            example: Some("scripts:\n  - path: scripts/setup.sh"),
            required: false,
            field_type: "array",
            cli_hint: Some(
                "fleetctl run-script --script-path ./scripts/setup.sh --host \"hostname\"",
            ),
        },
    );

    // =========================================================================
    // setup_experience (formerly macos_setup) — PR #42968
    // =========================================================================
    m.insert(
        "controls.setup_experience",
        FieldDoc {
            name: "setup_experience",
            description: "Configuration for the out-of-the-box setup experience (formerly `macos_setup`). Controls bootstrap packages, end user authentication, and setup scripts.",
            valid_values: None,
            example: Some("setup_experience:\n  bootstrap_package: https://example.org/bootstrap.pkg\n  enable_end_user_authentication: true\n  apple_setup_assistant: ./dep-profile.json\n  macos_script: ./post_setup.sh"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );
    m.insert(
        "setup_experience",
        FieldDoc {
            name: "setup_experience",
            description: "Configuration for the out-of-the-box setup experience (formerly `macos_setup`). Controls bootstrap packages, end user authentication, and setup scripts.",
            valid_values: None,
            example: Some("setup_experience:\n  bootstrap_package: https://example.org/bootstrap.pkg\n  enable_end_user_authentication: true"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );
    m.insert(
        "apple_enable_release_device_manually",
        FieldDoc {
            name: "apple_enable_release_device_manually",
            description: "When enabled, you're responsible for sending the `DeviceConfigured` command. End users stay in Setup Assistant until sent. Applies to Apple (macOS, iOS, iPadOS) hosts enrolled via ABM. Formerly `enable_release_device_manually`.",
            valid_values: Some(&["true", "false"]),
            example: Some("apple_enable_release_device_manually: false"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );
    m.insert(
        "apple_setup_assistant",
        FieldDoc {
            name: "apple_setup_assistant",
            description: "Path to a custom automatic enrollment (ADE) profile (.json). Applies to macOS and iOS/iPadOS hosts. Formerly `macos_setup_assistant`.",
            valid_values: None,
            example: Some("apple_setup_assistant: ./setup_assistant.json"),
            required: false,
            field_type: "string (file path)",
            cli_hint: None,
        },
    );
    m.insert(
        "macos_script",
        FieldDoc {
            name: "macos_script",
            description: "Path to a custom setup script to run after the host is first set up. Applies to macOS only. Formerly `script` under `macos_setup`.",
            valid_values: None,
            example: Some("macos_script: ./post_setup.sh"),
            required: false,
            field_type: "string (file path)",
            cli_hint: None,
        },
    );
    m.insert(
        "macos_manual_agent_install",
        FieldDoc {
            name: "macos_manual_agent_install",
            description: "Whether Fleet's agent (fleetd) will be installed as part of setup experience. Applies to macOS only. Formerly `manual_agent_install`.",
            valid_values: Some(&["true", "false"]),
            example: Some("macos_manual_agent_install: false"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    // =========================================================================
    // Team settings fields
    // =========================================================================
    m.insert(
        "team_settings",
        FieldDoc {
            name: "team_settings",
            description: "Settings specific to this fleet. Deprecated: use 'settings' instead.",
            valid_values: None,
            example: Some("team_settings:\n  secrets:\n    - secret: $ENROLL_SECRET"),
            required: false,
            field_type: "object",
            cli_hint: Some("fleetctl get teams --name \"Name\" --yaml  # export fleet settings\nfleetctl apply -f fleet.yml               # apply changes"),
        },
    );

    m.insert(
        "team_settings.secrets",
        FieldDoc {
            name: "secrets",
            description: "Enrollment secrets for adding hosts to this fleet.",
            valid_values: None,
            example: Some("secrets:\n  - secret: $ENROLL_SECRET"),
            required: false,
            field_type: "array",
            cli_hint: None,
        },
    );

    m.insert(
        "team_settings.features",
        FieldDoc {
            name: "features",
            description: "Feature flags for this fleet.",
            valid_values: None,
            example: Some(
                "features:\n  enable_host_users: true\n  enable_software_inventory: true",
            ),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "team_settings.webhook_settings",
        FieldDoc {
            name: "webhook_settings",
            description: "Per-fleet webhook configuration. Supports `activities_webhook`, `failing_policies_webhook`, `host_status_webhook`, and `interval`. `vulnerabilities_webhook` is org-only.",
            valid_values: None,
            example: Some("webhook_settings:\n  failing_policies_webhook:\n    enable_failing_policies_webhook: true\n    destination_url: https://example.com/hook"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "webhook_settings.activities_webhook",
        FieldDoc {
            name: "activities_webhook",
            description: "Fires when Fleet generates an activity. Can be configured at org or per-fleet level.",
            valid_values: None,
            example: Some("activities_webhook:\n  enable_activities_webhook: true\n  destination_url: https://example.org/webhook_handler"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "webhook_settings.failing_policies_webhook",
        FieldDoc {
            name: "failing_policies_webhook",
            description: "Fires for failing policies. Can be configured at org (`org_settings.webhook_settings`) or per-fleet (`settings.webhook_settings`).",
            valid_values: None,
            example: Some("failing_policies_webhook:\n  enable_failing_policies_webhook: true\n  destination_url: https://example.org/webhook_handler\n  policy_ids:\n    - 1\n    - 2"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "webhook_settings.host_status_webhook",
        FieldDoc {
            name: "host_status_webhook",
            description: "Fires when the percentage of offline hosts crosses a threshold. Can be configured at org or per-fleet level.",
            valid_values: None,
            example: Some("host_status_webhook:\n  enable_host_status_webhook: true\n  destination_url: https://example.org/webhook_handler\n  days_count: 7\n  host_percentage: 25"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "webhook_settings.vulnerabilities_webhook",
        FieldDoc {
            name: "vulnerabilities_webhook",
            description: "Fires when Fleet detects a new vulnerability. **Org-only** — per Fleet docs, can only be configured under `org_settings.webhook_settings`.",
            valid_values: None,
            example: Some("vulnerabilities_webhook:\n  enable_vulnerabilities_webhook: true\n  destination_url: https://example.org/webhook_handler\n  host_batch_size: 0"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "team_settings.integrations",
        FieldDoc {
            name: "integrations",
            description: "Third-party integrations for this fleet (Google Calendar, etc.).",
            valid_values: None,
            example: Some("integrations:\n  google_calendar:\n    enable_calendar_events: true"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "team_settings.host_expiry_settings",
        FieldDoc {
            name: "host_expiry_settings",
            description: "Settings for automatically removing inactive hosts.",
            valid_values: None,
            example: Some(
                "host_expiry_settings:\n  host_expiry_enabled: true\n  host_expiry_window: 30",
            ),
            required: false,
            field_type: "object",
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
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
            cli_hint: None,
        },
    );

    m.insert(
        "path",
        FieldDoc {
            name: "path",
            description: "Reference to another YAML file containing configuration. Paths are relative to the repository root.",
            valid_values: None,
            example: Some("- path: ../platforms/macos/policies/security.yml"),
            required: false,
            field_type: "string (file path)",
            cli_hint: None,
        },
    );

    // =========================================================================
    // Org settings (default.yml / global config)
    // =========================================================================
    m.insert(
        "org_settings",
        FieldDoc {
            name: "org_settings",
            description: "Organization-wide settings for the Fleet instance.",
            valid_values: None,
            example: Some("org_settings:\n  server_settings:\n    server_url: https://fleet.example.com"),
            required: false,
            field_type: "object",
            cli_hint: Some("fleetctl get config --yaml            # export org settings\nfleetctl apply -f default.yml         # apply changes"),
        },
    );

    // =========================================================================
    // GitOps workflow (whole-file context)
    // =========================================================================
    m.insert(
        "gitops",
        FieldDoc {
            name: "gitops",
            description: "Fleet GitOps workflow — manage Fleet configuration as code via YAML files.",
            valid_values: None,
            example: None,
            required: false,
            field_type: "workflow",
            cli_hint: Some("fleetctl gitops -f default.yml          # dry-run (preview changes)\nfleetctl gitops -f default.yml --force  # apply to Fleet instance"),
        },
    );

    // =========================================================================
    // v4.82+ renames
    // =========================================================================

    m.insert(
        "reports",
        FieldDoc {
            name: "reports",
            description: "Scheduled queries (reports) for data collection. Renamed from 'queries' in Fleet 4.82+.",
            valid_values: None,
            example: Some("reports:\n  - paths: ../platforms/all/reports/*.yml\n  - paths: ../platforms/macos/reports/*.yml"),
            required: false,
            field_type: "array",
            cli_hint: Some("fleetctl get queries --yaml             # export reports"),
        },
    );

    m.insert(
        "settings",
        FieldDoc {
            name: "settings",
            description: "Fleet-level settings (enrollment secrets, features, integrations). Renamed from 'team_settings' in Fleet 4.82+.",
            valid_values: None,
            example: Some("settings:\n  secrets:\n    - secret: $ENROLL_SECRET\n  features:\n    enable_host_users: true"),
            required: false,
            field_type: "object",
            cli_hint: Some("fleetctl get teams --name \"Name\" --yaml  # export fleet settings"),
        },
    );

    // =========================================================================
    // Controls nested keys
    // =========================================================================

    m.insert(
        "controls.apple_settings",
        FieldDoc {
            name: "apple_settings",
            description: "Apple device settings (macOS, iOS, iPadOS). Contains configuration and declaration profiles. Renamed from 'macos_settings' in Fleet 4.83.",
            valid_values: None,
            example: Some("apple_settings:\n  configuration_profiles:\n    - paths: ../platforms/macos/configuration-profiles/*.mobileconfig\n    - paths: ../platforms/macos/declaration-profiles/*.json"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.windows_settings",
        FieldDoc {
            name: "windows_settings",
            description: "Windows device settings. Contains configuration profiles (.xml CSP profiles).",
            valid_values: None,
            example: Some("windows_settings:\n  configuration_profiles:\n    - paths: ../platforms/windows/configuration-profiles/*.xml"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.android_settings",
        FieldDoc {
            name: "android_settings",
            description:
                "Android device settings. Contains configuration profiles and certificates.",
            valid_values: None,
            example: Some("android_settings:\n  configuration_profiles: []\n  certificates: []"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "configuration_profiles",
        FieldDoc {
            name: "configuration_profiles",
            description: "MDM configuration profiles to deploy. Renamed from 'custom_settings' in Fleet 4.83. Supports glob patterns with 'paths:'.",
            valid_values: None,
            example: Some("configuration_profiles:\n  - paths: ../platforms/macos/configuration-profiles/*.mobileconfig"),
            required: false,
            field_type: "array",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.enable_disk_encryption",
        FieldDoc {
            name: "enable_disk_encryption",
            description: "Enable FileVault (macOS) or BitLocker (Windows) disk encryption on managed devices.",
            valid_values: Some(&["true", "false"]),
            example: Some("enable_disk_encryption: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.macos_updates",
        FieldDoc {
            name: "macos_updates",
            description: "macOS software update enforcement settings.",
            valid_values: None,
            example: Some("macos_updates:\n  deadline: \"2025-06-15\"\n  minimum_version: \"15.1\"\n  update_new_hosts: true"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.ios_updates",
        FieldDoc {
            name: "ios_updates",
            description: "iOS software update enforcement settings.",
            valid_values: None,
            example: Some("ios_updates:\n  deadline: \"2025-06-15\"\n  minimum_version: \"18.0\""),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.ipados_updates",
        FieldDoc {
            name: "ipados_updates",
            description: "iPadOS software update enforcement settings.",
            valid_values: None,
            example: Some(
                "ipados_updates:\n  deadline: \"2025-06-15\"\n  minimum_version: \"18.0\"",
            ),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.windows_updates",
        FieldDoc {
            name: "windows_updates",
            description: "Windows update enforcement settings.",
            valid_values: None,
            example: Some("windows_updates:\n  deadline_days: 7\n  grace_period_days: 2"),
            required: false,
            field_type: "object",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.scripts",
        FieldDoc {
            name: "scripts",
            description: "Scripts available for execution on managed hosts. Must reference files via path or paths (glob).",
            valid_values: None,
            example: Some("scripts:\n  - paths: ../platforms/macos/scripts/*.sh\n  - paths: ../platforms/windows/scripts/*.ps1"),
            required: false,
            field_type: "array",
            cli_hint: None,
        },
    );

    m.insert(
        "deadline",
        FieldDoc {
            name: "deadline",
            description: "Date by which the OS update must be installed. Format: YYYY-MM-DD.",
            valid_values: None,
            example: Some("deadline: \"2025-06-15\""),
            required: false,
            field_type: "string (date)",
            cli_hint: None,
        },
    );

    m.insert(
        "minimum_version",
        FieldDoc {
            name: "minimum_version",
            description:
                "Minimum required OS version. Hosts below this version will be prompted to update.",
            valid_values: None,
            example: Some("minimum_version: \"15.1\""),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "update_new_hosts",
        FieldDoc {
            name: "update_new_hosts",
            description: "Whether to enforce OS updates on newly enrolled hosts.",
            valid_values: Some(&["true", "false"]),
            example: Some("update_new_hosts: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    // =========================================================================
    // Software nested keys
    // =========================================================================

    m.insert(
        "software.fleet_maintained_apps",
        FieldDoc {
            name: "fleet_maintained_apps",
            description: "Fleet-maintained apps installed via slug. Fleet handles install/uninstall scripts and auto-updates.",
            valid_values: None,
            example: Some("fleet_maintained_apps:\n  - slug: 1password/darwin\n    self_service: true"),
            required: false,
            field_type: "array",
            cli_hint: Some("See: https://github.com/fleetdm/fleet/tree/main/ee/maintained-apps"),
        },
    );

    m.insert(
        "software.app_store_apps",
        FieldDoc {
            name: "app_store_apps",
            description: "App Store apps deployed via VPP (Volume Purchase Program).",
            valid_values: None,
            example: Some("app_store_apps:\n  - app_store_id: \"1091189122\""),
            required: false,
            field_type: "array",
            cli_hint: None,
        },
    );

    m.insert(
        "software.packages",
        FieldDoc {
            name: "packages",
            description:
                "Custom software packages (.pkg, .msi, .deb) with install/uninstall scripts.",
            valid_values: None,
            example: Some("packages:\n  - path: ../platforms/macos/software/firefox.yml"),
            required: false,
            field_type: "array",
            cli_hint: None,
        },
    );

    m.insert(
        "slug",
        FieldDoc {
            name: "slug",
            description: "Fleet-maintained app identifier. Format: app-name/platform (e.g., 'santa/darwin', '1password/darwin').",
            valid_values: None,
            example: Some("slug: santa/darwin"),
            required: false,
            field_type: "string",
            cli_hint: Some("See: https://github.com/fleetdm/fleet/tree/main/ee/maintained-apps"),
        },
    );

    m.insert(
        "self_service",
        FieldDoc {
            name: "self_service",
            description: "Whether this software is available for users to install via Fleet Desktop self-service.",
            valid_values: Some(&["true", "false"]),
            example: Some("self_service: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    // =========================================================================
    // Path reference keys (context-specific)
    // =========================================================================

    // Configuration profiles — path/paths
    m.insert(
        "controls.apple_settings.configuration_profiles.paths",
        FieldDoc {
            name: "paths",
            description: "Glob pattern referencing Apple configuration or declaration profile files. Matches `.mobileconfig` (configuration profiles) and `.json` (declaration profiles) files.",
            valid_values: None,
            example: Some("configuration_profiles:\n  - paths: ../platforms/macos/configuration-profiles/*.mobileconfig\n  - paths: ../platforms/macos/declaration-profiles/*.json"),
            required: false,
            field_type: "string (glob)",
            cli_hint: Some("fleetctl get config --yaml  # see active profiles"),
        },
    );
    m.insert(
        "controls.apple_settings.configuration_profiles.path",
        FieldDoc {
            name: "path",
            description: "Path to a single Apple configuration profile (`.mobileconfig`) or declaration profile (`.json`). Relative to the repository root or current file.",
            valid_values: None,
            example: Some("configuration_profiles:\n  - path: ../platforms/macos/configuration-profiles/wifi.mobileconfig"),
            required: false,
            field_type: "string (file path)",
            cli_hint: None,
        },
    );
    m.insert(
        "controls.windows_settings.configuration_profiles.paths",
        FieldDoc {
            name: "paths",
            description: "Glob pattern referencing Windows configuration profile files (`.xml`).",
            valid_values: None,
            example: Some("configuration_profiles:\n  - paths: ../platforms/windows/configuration-profiles/*.xml"),
            required: false,
            field_type: "string (glob)",
            cli_hint: None,
        },
    );
    m.insert(
        "controls.windows_settings.configuration_profiles.path",
        FieldDoc {
            name: "path",
            description: "Path to a single Windows configuration profile (`.xml`). Relative to the repository root or current file.",
            valid_values: None,
            example: Some("configuration_profiles:\n  - path: ../platforms/windows/configuration-profiles/security.xml"),
            required: false,
            field_type: "string (file path)",
            cli_hint: None,
        },
    );

    // Scripts — path/paths
    m.insert(
        "controls.scripts.paths",
        FieldDoc {
            name: "paths",
            description: "Glob pattern referencing script files to deploy to hosts. Supports `.sh` (macOS/Linux) and `.ps1` (Windows).",
            valid_values: None,
            example: Some("scripts:\n  - paths: ../platforms/macos/scripts/*.sh\n  - paths: ../platforms/windows/scripts/*.ps1"),
            required: false,
            field_type: "string (glob)",
            cli_hint: Some("fleetctl run-script --script-path ./scripts/setup.sh --host \"hostname\""),
        },
    );
    m.insert(
        "controls.scripts.path",
        FieldDoc {
            name: "path",
            description: "Path to a single script file to deploy to hosts. Supports `.sh` (macOS/Linux) and `.ps1` (Windows). Relative to the repository root or current file.",
            valid_values: None,
            example: Some("scripts:\n  - path: ../platforms/macos/scripts/setup.sh"),
            required: false,
            field_type: "string (file path)",
            cli_hint: Some("fleetctl run-script --script-path ./scripts/setup.sh --host \"hostname\""),
        },
    );

    // ---- Patch policy version + agent_options.path + Windows/Apple MDM controls ----

    m.insert(
        "policies.version",
        FieldDoc {
            name: "version",
            description: "Pin a specific Fleet-Maintained App version for a patch policy. Per yaml-files.md:147, when set, the version is included in the auto-generated query. Combined with `type: patch` and `fleet_maintained_app_slug`.",
            valid_values: None,
            example: Some("version: \"5.17.0\""),
            required: false,
            field_type: "string",
            cli_hint: None,
        },
    );

    m.insert(
        "agent_options.path",
        FieldDoc {
            name: "path",
            description: "Path to an external YAML file containing agent options. Relative to the file using it. Use this to keep `agent_options` in a separate `lib/agent-options.yml` file.",
            valid_values: None,
            example: Some("agent_options:\n  path: ../lib/agent-options.yml"),
            required: false,
            field_type: "string (file path)",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.apple_require_hardware_attestation",
        FieldDoc {
            name: "apple_require_hardware_attestation",
            description: "When true, Fleet verifies that devices are genuine Apple hardware (via the Secure Enclave) before allowing MDM enrollment. Source: fleet/server/fleet/app.go (AppleRequireHardwareAttestation).",
            valid_values: Some(&["true", "false"]),
            example: Some("apple_require_hardware_attestation: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.enable_recovery_lock_password",
        FieldDoc {
            name: "enable_recovery_lock_password",
            description: "When true, Fleet sets and rotates a random Recovery Lock password on eligible macOS hosts (yaml-files.md:369). Available in Fleet Premium.",
            valid_values: Some(&["true", "false"]),
            example: Some("enable_recovery_lock_password: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.windows_require_bitlocker_pin",
        FieldDoc {
            name: "windows_require_bitlocker_pin",
            description: "Require end users on Windows hosts to set a BitLocker PIN at startup. `enable_disk_encryption` must be true. Default: false (yaml-files.md:368).",
            valid_values: Some(&["true", "false"]),
            example: Some("windows_require_bitlocker_pin: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.windows_enabled_and_configured",
        FieldDoc {
            name: "windows_enabled_and_configured",
            description: "Master switch for Windows MDM. Must be true to manage Windows hosts via Fleet MDM.",
            valid_values: Some(&["true", "false"]),
            example: Some("windows_enabled_and_configured: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.windows_entra_tenant_ids",
        FieldDoc {
            name: "windows_entra_tenant_ids",
            description: "Microsoft Entra (Azure AD) tenant IDs allowed for Windows MDM enrollment.",
            valid_values: None,
            example: Some("windows_entra_tenant_ids:\n  - 4e342a0d-ec1a-4353-bdeb-785542e0a8fb"),
            required: false,
            field_type: "array of strings",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.enable_turn_on_windows_mdm_manually",
        FieldDoc {
            name: "enable_turn_on_windows_mdm_manually",
            description: "When true, end users must manually enable MDM in Windows Settings > Access work or school. When false, Fleet auto-enables MDM for unmanaged Windows hosts. Org-only (yaml-files.md:365).",
            valid_values: Some(&["true", "false"]),
            example: Some("enable_turn_on_windows_mdm_manually: false"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "controls.windows_migration_enabled",
        FieldDoc {
            name: "windows_migration_enabled",
            description: "When true, automatically migrate Windows hosts connected to a third-party MDM. Requires `enable_turn_on_windows_mdm_manually: false`. Org-only (yaml-files.md:366).",
            valid_values: Some(&["true", "false"]),
            example: Some("windows_migration_enabled: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "macos_setup.require_all_software_macos",
        FieldDoc {
            name: "require_all_software_macos",
            description: "When true, cancel macOS setup experience if any software install fails. New canonical name (`require_all_software` is the legacy form). Source: fleet/server/fleet/apple_mdm.go:529.",
            valid_values: Some(&["true", "false"]),
            example: Some("require_all_software_macos: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    m.insert(
        "macos_setup.require_all_software_windows",
        FieldDoc {
            name: "require_all_software_windows",
            description: "Windows counterpart to `require_all_software_macos` — cancel setup if any software install fails on Windows. Source: fleet/server/fleet/apple_mdm.go:530.",
            valid_values: Some(&["true", "false"]),
            example: Some("require_all_software_windows: true"),
            required: false,
            field_type: "boolean",
            cli_hint: None,
        },
    );

    // Generic fallbacks
    m.insert(
        "paths",
        FieldDoc {
            name: "paths",
            description: "Glob pattern referencing multiple files. Must contain glob characters (*, ?, [, {). Use `path` for a single file.",
            valid_values: None,
            example: Some("- paths: ../platforms/macos/policies/*.yml\n- paths: ../platforms/macos/configuration-profiles/*.mobileconfig"),
            required: false,
            field_type: "string (glob)",
            cli_hint: None,
        },
    );

    // Current names for renamed top-level keys — these must exist so the
    // new spelling gets first-class hover docs (children resolve via the
    // canonicalization fallback in `get_field_doc`).
    m.insert(
        "reports",
        FieldDoc {
            name: "reports",
            description: "List of osquery reports (formerly `queries`) to run on hosts for data collection.",
            valid_values: None,
            example: Some("reports:\n  - name: Collect failed login attempts\n    query: SELECT * FROM last WHERE type = 7;\n    interval: 3600"),
            required: false,
            field_type: "array",
            cli_hint: None,
        },
    );
    m.insert(
        "settings",
        FieldDoc {
            name: "settings",
            description: "Settings specific to this fleet (formerly `team_settings`).",
            valid_values: None,
            example: Some("settings:\n  secrets:\n    - secret: $ENROLL_SECRET"),
            required: false,
            field_type: "object",
            cli_hint: None,
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

    // NEW-name paths resolve to docs written under the old spelling —
    // children keep working under `reports:`/`settings:`/`apple_settings:`
    // without duplicating every entry. Only path SEGMENTS are rewritten;
    // the returned doc is the child's own (its name isn't deprecated).
    // Top-level renamed keys have their own new-name entries, so hovering
    // `reports:` never shows a doc titled `queries`.
    let canonical = path
        .split('.')
        .map(|seg| match seg {
            "reports" => "queries",
            "settings" => "team_settings",
            "apple_settings" => "macos_settings",
            "configuration_profiles" => "custom_settings",
            "setup_experience" => "macos_setup",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(".");
    if canonical != path && !path.contains('.') {
        // never map a bare top-level new name to an old-name doc
    } else if canonical != path {
        if let Some(doc) = FIELD_DOCS.get(canonical.as_str()) {
            return Some(doc);
        }
    }

    // Try just the field name (last segment), requiring exact segment
    // boundaries so `webhook` doesn't match `failing_policies_webhook`.
    // Prefer entries that share the same top-level context as the request.
    let field_name = path.split('.').next_back().unwrap_or(path);
    let segment_suffix = format!(".{}", field_name);
    let context_prefix = path.split('.').next().unwrap_or("");
    let context_prefix_dot = format!("{}.", context_prefix);
    for (key, doc) in FIELD_DOCS.iter() {
        if (key.ends_with(segment_suffix.as_str()) || *key == field_name)
            && key.starts_with(context_prefix_dot.as_str())
        {
            return Some(doc);
        }
    }
    // No context-matching entry — fall back to any segment match.
    for (key, doc) in FIELD_DOCS.iter() {
        if key.ends_with(segment_suffix.as_str()) || *key == field_name {
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
    &[
        "darwin", "windows", "linux", "chrome", "ios", "ipados", "android",
    ]
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
        let top_level = [
            "name",
            "team_settings",
            "agent_options",
            "controls",
            "policies",
            "queries",
            "software",
        ];
        for field in top_level {
            assert!(
                get_field_doc(field).is_some(),
                "Missing doc for top-level field: {}",
                field
            );
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
            assert!(
                get_field_doc(field).is_some(),
                "Missing doc for software field: {}",
                field
            );
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
            assert!(
                get_field_doc(field).is_some(),
                "Missing doc for controls field: {}",
                field
            );
        }

        // Team settings section
        let team_settings_fields = [
            "team_settings",
            "team_settings.secrets",
            "team_settings.features",
        ];
        for field in team_settings_fields {
            assert!(
                get_field_doc(field).is_some(),
                "Missing doc for team_settings field: {}",
                field
            );
        }

        // Agent options section
        let agent_options_fields = ["agent_options.config", "agent_options.config.options"];
        for field in agent_options_fields {
            assert!(
                get_field_doc(field).is_some(),
                "Missing doc for agent_options field: {}",
                field
            );
        }
    }

    #[test]
    fn test_cli_hints_on_top_level_fields() {
        let fields_with_hints = [
            "policies",
            "queries",
            "labels",
            "controls",
            "software",
            "agent_options",
            "team_settings",
            "name",
            "org_settings",
        ];
        for field in fields_with_hints {
            let doc = FIELD_DOCS
                .get(field)
                .unwrap_or_else(|| panic!("Missing FieldDoc for top-level key: {}", field));
            assert!(
                doc.cli_hint.is_some(),
                "Expected cli_hint on top-level field '{}', but it was None",
                field,
            );
        }
    }

    #[test]
    fn test_cli_hint_rendered_in_markdown() {
        let doc = FIELD_DOCS.get("policies").unwrap();
        let md = doc.to_markdown();
        assert!(
            md.contains("**CLI:**"),
            "Markdown should contain CLI section header"
        );
        assert!(
            md.contains("fleetctl"),
            "CLI hint should reference fleetctl"
        );
    }

    #[test]
    fn test_cli_hint_absent_for_leaf_fields() {
        // Leaf fields like policies.name shouldn't have CLI hints
        let doc = FIELD_DOCS.get("policies.name").unwrap();
        assert!(
            doc.cli_hint.is_none(),
            "Leaf field 'policies.name' should not have a cli_hint"
        );
        let md = doc.to_markdown();
        assert!(
            !md.contains("**CLI:**"),
            "Leaf field markdown should not contain CLI section"
        );
    }

    #[test]
    fn test_gitops_workflow_doc() {
        let doc = FIELD_DOCS.get("gitops").unwrap();
        assert!(doc.cli_hint.is_some());
        let cli = doc.cli_hint.unwrap();
        assert!(
            cli.contains("fleetctl gitops"),
            "gitops doc should reference fleetctl gitops"
        );
        assert!(
            cli.contains("--force"),
            "gitops doc should mention --force for apply"
        );
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
    /// GUARD (ADR-004/ADR-010): hover docs must never advertise a
    /// deprecated key as current. For every KeyRename in the deprecation
    /// registry, any FIELD_DOCS entry whose key path ends in the old name
    /// must say "Deprecated" and point at the new name — and its example
    /// must not showcase the old spelling as the primary form (deprecated
    /// entries excepted only when the description carries the note).
    /// Found via a live report: `macos_settings` hover showed
    /// "MDM settings specific to macOS devices" with an old-name example
    /// and no deprecation note. Any future Fleet rename fails this test
    /// until the docs surface is updated.
    #[test]
    fn field_docs_never_advertise_deprecated_keys() {
        use flint_lint::deprecations::{DeprecationKind, DEPRECATION_REGISTRY};

        for (key, doc) in FIELD_DOCS.iter() {
            let (parent, last) = match key.rsplit_once('.') {
                Some((p, l)) => (p, l),
                None => ("", *key),
            };
            // Context-aware: top-level `labels:` is NOT the deprecated
            // profile-item `labels` — the registry matcher knows which
            // contexts each rename applies to.
            let Some(dep) = DEPRECATION_REGISTRY.find_deprecated_key(last, parent) else {
                continue;
            };
            if let DeprecationKind::KeyRename { new_key, .. } = &dep.kind {
                let desc_lower = doc.description.to_lowercase();
                assert!(
                    (desc_lower.contains("deprecated") || desc_lower.contains("renamed"))
                        && doc.description.contains(new_key),
                    "FIELD_DOCS['{key}'] documents deprecated key '{last}' without a \
                     'Deprecated … {new_key}' note: {}",
                    doc.description
                );
            }
        }

        // EXAMPLES advertise too: a healthy entry (e.g. `controls`) can
        // still showcase deprecated keys in its example snippet — the exact
        // escape a user caught after the first guard landed. Context-scoped
        // renames (labels, script) are excluded: their bare names are
        // legitimate keys elsewhere and a line scan can't tell contexts
        // apart.
        for (key, doc) in FIELD_DOCS.iter() {
            let Some(example) = doc.example else { continue };
            let desc_lower = doc.description.to_lowercase();
            let entry_is_deprecated =
                desc_lower.contains("deprecated") || desc_lower.contains("renamed");
            if entry_is_deprecated {
                continue; // a deprecated entry may show its own old spelling
            }
            for dep in DEPRECATION_REGISTRY.entries() {
                if let DeprecationKind::KeyRename { old_key, new_key, .. } = &dep.kind {
                    if matches!(*old_key, "labels" | "script") {
                        continue;
                    }
                    let as_key = format!("{}:", old_key);
                    assert!(
                        !example.lines().any(|l| l.trim_start().starts_with(&as_key)),
                        "FIELD_DOCS['{key}'] example advertises deprecated key \
                         '{old_key}' (use '{new_key}'): {example}"
                    );
                }
            }
        }

        // Every rename that actually has old-name docs (context-matched
        // above) must have docs under the NEW spelling somewhere too —
        // otherwise the old spelling is better-documented than the
        // current one.
        let documented_old: Vec<&str> = FIELD_DOCS
            .keys()
            .filter_map(|key| {
                let (parent, last) = key.rsplit_once('.').unwrap_or(("", key));
                DEPRECATION_REGISTRY
                    .find_deprecated_key(last, parent)
                    .and_then(|d| match &d.kind {
                        DeprecationKind::KeyRename { new_key, .. } => Some(*new_key),
                        _ => None,
                    })
            })
            .collect();
        for new_key in documented_old {
            assert!(
                FIELD_DOCS
                    .keys()
                    .any(|k| k.rsplit('.').next() == Some(new_key)),
                "a deprecated key has hover docs but its replacement '{new_key}' has \
                 none anywhere — the old spelling is better-documented than the \
                 current one"
            );
        }
    }

    #[test]
    fn new_name_child_paths_resolve_via_canonicalization() {
        // reports.* / settings.* children reuse the docs written under the
        // old spelling without duplication.
        assert!(get_field_doc("reports.interval").is_some());
        assert!(get_field_doc("settings.secrets").is_some());
        // Bare top-level new names get their OWN docs, never an old-name one.
        assert_eq!(get_field_doc("reports").unwrap().name, "reports");
        assert_eq!(get_field_doc("settings").unwrap().name, "settings");
    }

}
