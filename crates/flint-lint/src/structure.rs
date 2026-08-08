//! Schema tree definitions for structural YAML validation.
//!
//! Defines valid YAML structure for each Fleet GitOps file type,
//! sourced from the strict JSON schema. Used by `StructuralValidationRule`
//! to detect unknown keys, misplaced keys, and missing wrappers.

use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Describes the valid structure at a single level of the YAML tree.
#[derive(Debug, Clone)]
pub(crate) enum SchemaNode {
    /// A mapping with known child keys.
    Mapping(HashMap<&'static str, SchemaNode>),
    /// An array whose items follow a given schema.
    Array(Box<SchemaNode>),
    /// A scalar or opaque value (no children to validate).
    Leaf,
    /// A boolean value — only `true`/`false` (and YAML 1.1 equivalents) are valid.
    BooleanLeaf,
    /// A mapping that allows arbitrary keys (e.g. `additionalProperties: true`).
    OpenMapping,
}


/// Test-support accessors — production code walks schema nodes through
/// `structural.rs`'s validator, not these.
#[cfg(test)]
impl SchemaNode {
    /// Look up a child key in a Mapping node.
    pub(crate) fn get_child(&self, key: &str) -> Option<&SchemaNode> {
        match self {
            SchemaNode::Mapping(children) => children.get(key),
            _ => None,
        }
    }
}

/// Global registry mapping every known key name to the path(s) where it's valid.
/// Used for misplaced-key detection.
#[derive(Debug)]
pub(crate) struct KeyRegistry {
    /// key_name -> list of dot-separated paths where it's valid
    entries: HashMap<&'static str, Vec<&'static str>>,
}

impl KeyRegistry {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    fn register(&mut self, key: &'static str, path: &'static str) {
        self.entries.entry(key).or_default().push(path);
    }

    /// Look up the valid paths for a key name.
    pub(crate) fn lookup(&self, key: &str) -> Option<&[&'static str]> {
        self.entries.get(key).map(|v| v.as_slice())
    }

    /// Get all known key names (test support — coverage assertions).
    #[cfg(test)]
    pub(crate) fn all_keys(&self) -> Vec<&'static str> {
        self.entries.keys().copied().collect()
    }
}

// ---------------------------------------------------------------------------
// Helper macros / builders
// ---------------------------------------------------------------------------

fn mapping(children: Vec<(&'static str, SchemaNode)>) -> SchemaNode {
    SchemaNode::Mapping(children.into_iter().collect())
}

fn array(item: SchemaNode) -> SchemaNode {
    SchemaNode::Array(Box::new(item))
}

/// Extend an inline-item mapping with `fleet.BaseItem` keys (`path`, `paths`).
///
/// Fleet's gitops structs embed `fleet.BaseItem` alongside the full spec
/// (pkg/spec/gitops.go: `type Policy struct { fleet.BaseItem; GitOpsPolicySpec }`,
/// same shape for Query and Label), so an external-file reference may carry
/// any inline field next to `path:` (e.g. `labels_include_any` on a policy
/// path ref). Modeling this as one mapping instead of a oneOf also produces
/// precise unknown-key errors (issue #13: a oneOf best-variant pick blamed
/// `path` under `policies[0]` and pointed at `agent_options.path`).
fn with_base_item(node: SchemaNode) -> SchemaNode {
    match node {
        SchemaNode::Mapping(mut children) => {
            children.insert("path", SchemaNode::Leaf);
            children.insert("paths", SchemaNode::Leaf);
            SchemaNode::Mapping(children)
        }
        other => other,
    }
}

fn leaf() -> SchemaNode {
    SchemaNode::Leaf
}

fn boolean_leaf() -> SchemaNode {
    SchemaNode::BooleanLeaf
}

fn open_mapping() -> SchemaNode {
    SchemaNode::OpenMapping
}

// ---------------------------------------------------------------------------
// Shared sub-schemas (matching the strict JSON schema $defs)
// ---------------------------------------------------------------------------

fn controls_script_targeting() -> SchemaNode {
    mapping(vec![
        ("path", leaf()),
        ("paths", leaf()),
        ("labels_include_all", array(leaf())),
        ("labels_include_any", array(leaf())),
        ("labels_exclude_any", array(leaf())),
    ])
}

/// Configuration/declaration profile item — script targeting plus
/// `activation`, a path to a custom DDM activation JSON, only valid
/// alongside an Apple declaration and never with `paths` globs
/// (fleet/mdm.go MDMProfileSpec, Fleet PR #50557).
fn profile_item() -> SchemaNode {
    mapping(vec![
        ("path", leaf()),
        ("paths", leaf()),
        ("activation", leaf()),
        ("labels_include_all", array(leaf())),
        ("labels_include_any", array(leaf())),
        ("labels_exclude_any", array(leaf())),
    ])
}

fn macos_updates() -> SchemaNode {
    mapping(vec![
        ("deadline", leaf()),
        ("minimum_version", leaf()),
        ("update_new_hosts", boolean_leaf()),
    ])
}

fn windows_updates() -> SchemaNode {
    mapping(vec![
        ("deadline_days", leaf()),
        ("grace_period_days", leaf()),
    ])
}

fn os_settings() -> SchemaNode {
    mapping(vec![
        ("custom_settings", array(profile_item())),
        ("configuration_profiles", array(profile_item())), // rename of custom_settings
        // Hidden managed local admin account for Windows Autopilot/OOBE —
        // fleet/app.go (ManagedLocalAccountSettings), windows_settings only
        // in Fleet; shared here because flint uses one schema per *_settings.
        (
            "managed_local_account_settings",
            mapping(vec![("enabled", boolean_leaf())]),
        ),
    ])
}

fn android_settings_schema() -> SchemaNode {
    mapping(vec![
        ("custom_settings", array(profile_item())),
        ("configuration_profiles", array(profile_item())), // rename of custom_settings
        (
            "certificates",
            // Source: server/fleet/certificate_templates.go — SAN support
            // added in Fleet 4.86 (issue #14).
            array(mapping(vec![
                ("name", leaf()),
                ("certificate_authority_name", leaf()),
                ("subject_name", leaf()),
                ("subject_alternative_name", leaf()),
            ])),
        ),
    ])
}

fn macos_setup() -> SchemaNode {
    mapping(vec![
        // Old names (json tags)
        ("bootstrap_package", leaf()),
        ("manual_agent_install", boolean_leaf()),
        ("enable_end_user_authentication", boolean_leaf()),
        ("lock_end_user_info", boolean_leaf()),
        ("require_all_software", boolean_leaf()),
        ("enable_release_device_manually", boolean_leaf()),
        ("macos_setup_assistant", leaf()),
        ("script", leaf()),
        // New names (renameto tags from Go code)
        ("macos_bootstrap_package", leaf()),
        ("macos_manual_agent_install", boolean_leaf()),
        // `require_all_software` rename — per
        // fleet/server/fleet/apple_mdm.go:529 (MDMAppleSetupPayload),
        // the JSON tag is `require_all_software_macos`. Keep the old
        // short name (line above) for backwards compat.
        ("require_all_software_macos", boolean_leaf()),
        ("require_all_software_windows", boolean_leaf()),
        ("apple_enable_release_device_manually", boolean_leaf()),
        ("apple_setup_assistant", leaf()),
        ("macos_script", leaf()),
        // Additional setup_experience fields per MDMAppleSetupPayload
        // (apple_mdm.go:524-534). `enable_end_user_authentication` is
        // already listed above in the old-names block.
        ("enable_managed_local_account", boolean_leaf()),
        // Current Fleet name (app.go: renameto on enable_managed_local_account).
        ("enable_create_local_admin_account", boolean_leaf()),
        ("end_user_local_account_type", leaf()),
    ])
}

fn macos_migration() -> SchemaNode {
    mapping(vec![
        ("enable", boolean_leaf()),
        ("mode", leaf()),
        ("webhook_url", leaf()),
    ])
}

fn software_asset() -> SchemaNode {
    mapping(vec![("path", leaf())])
}

fn software_package_item() -> SchemaNode {
    mapping(vec![
        ("path", leaf()),
        ("url", leaf()),
        ("hash_sha256", leaf()),
        // ETag-based conditional download opt-out — SoftwarePackageSpec
        // (AlwaysDownload), docs/Configuration/yaml-files.md.
        ("always_download", boolean_leaf()),
        ("display_name", leaf()),
        ("self_service", boolean_leaf()),
        ("setup_experience", boolean_leaf()),
        // Platform targeting for .sh script-only packages in setup
        // experience — SoftwarePackageSpec (SetupExperiencePlatform).
        ("setup_experience_platform", leaf()),
        ("categories", array(leaf())),
        ("labels_include_any", array(leaf())),
        ("labels_exclude_any", array(leaf())),
        ("labels_include_all", array(leaf())),
        ("pre_install_query", software_asset()),
        ("install_script", software_asset()),
        ("uninstall_script", software_asset()),
        ("post_install_script", software_asset()),
        ("icon", software_asset()),
    ])
}

fn app_store_app_item() -> SchemaNode {
    mapping(vec![
        ("app_store_id", leaf()),
        ("platform", leaf()),
        ("display_name", leaf()),
        ("self_service", boolean_leaf()),
        ("setup_experience", boolean_leaf()),
        ("categories", array(leaf())),
        ("labels_include_any", array(leaf())),
        ("labels_exclude_any", array(leaf())),
        ("labels_include_all", array(leaf())),
        ("icon", software_asset()),
        ("configuration", software_asset()),
        ("auto_update_enabled", boolean_leaf()),
        ("auto_update_window_start", leaf()),
        ("auto_update_window_end", leaf()),
    ])
}

fn fleet_maintained_app_item() -> SchemaNode {
    mapping(vec![
        ("slug", leaf()),
        ("version", leaf()),
        ("display_name", leaf()),
        ("self_service", boolean_leaf()),
        ("setup_experience", boolean_leaf()),
        ("categories", array(leaf())),
        ("labels_include_any", array(leaf())),
        ("labels_exclude_any", array(leaf())),
        ("labels_include_all", array(leaf())),
        ("pre_install_query", software_asset()),
        ("install_script", software_asset()),
        ("uninstall_script", software_asset()),
        ("post_install_script", software_asset()),
        ("icon", software_asset()),
    ])
}

fn software_schema() -> SchemaNode {
    mapping(vec![
        ("packages", array(software_package_item())),
        ("app_store_apps", array(app_store_app_item())),
        ("fleet_maintained_apps", array(fleet_maintained_app_item())),
    ])
}

fn controls_schema() -> SchemaNode {
    mapping(vec![
        ("scripts", array(controls_script_targeting())),
        ("windows_enabled_and_configured", boolean_leaf()),
        ("windows_entra_tenant_ids", array(leaf())),
        // Entra application (client) IDs — docs/Configuration/yaml-files.md,
        // pkg/spec/gitops.go (WindowsEntraClientIDs).
        ("windows_entra_client_ids", array(leaf())),
        // pkg/spec/gitops.go (AndroidEnabledAndConfigured).
        ("android_enabled_and_configured", boolean_leaf()),
        ("enable_turn_on_windows_mdm_manually", boolean_leaf()),
        ("windows_migration_enabled", boolean_leaf()),
        ("enable_disk_encryption", boolean_leaf()),
        ("enable_recovery_lock_password", boolean_leaf()),
        // Per fleet/server/fleet/app.go:243 (AppleRequireHardwareAttestation)
        // and pkg/spec/gitops.go:187. Verifies devices are genuine Apple
        // hardware before MDM enrollment is allowed.
        ("apple_require_hardware_attestation", boolean_leaf()),
        ("volume_purchasing_program", open_mapping()),
        ("windows_require_bitlocker_pin", boolean_leaf()),
        ("macos_updates", macos_updates()),
        ("ios_updates", macos_updates()),
        ("ipados_updates", macos_updates()),
        ("windows_updates", windows_updates()),
        ("macos_settings", os_settings()),
        ("apple_settings", os_settings()), // rename of macos_settings
        ("windows_settings", os_settings()),
        ("android_settings", android_settings_schema()),
        ("macos_setup", macos_setup()),
        ("setup_experience", macos_setup()), // rename of macos_setup
        ("macos_migration", macos_migration()),
        // OAuth ROPG account provisioning — fleet/app.go
        // (AppleAccountProvisioning), default.yml only, macOS only today.
        (
            "apple_account_provisioning",
            mapping(vec![
                ("oauth_idp_token_url", leaf()),
                ("oauth_idp_client_id", leaf()),
                ("oauth_idp_client_secret", leaf()),
            ]),
        ),
    ])
}

fn policy_inline_strict() -> SchemaNode {
    // Source of truth: `pkg/spec/gitops.go` (GitOpsPolicySpec) +
    // `server/fleet/policies.go` (PolicySpec). Cross-validated against
    // `cmd/fleetctl/fleetctl/testdata/generateGitops/expectedTeamPolicies.yaml`.
    mapping(vec![
        ("name", leaf()),
        ("description", leaf()),
        ("resolution", leaf()),
        ("query", leaf()),
        ("platform", leaf()),
        ("critical", boolean_leaf()),
        // `team` is renameto:"fleet" in Fleet Go — both names valid for compat.
        ("team", leaf()),
        ("fleet", leaf()),
        ("calendar_events_enabled", boolean_leaf()),
        ("conditional_access_enabled", boolean_leaf()),
        ("conditional_access_bypass_enabled", boolean_leaf()),
        // GitOps-only convenience: maps the policy ID into
        // failing_policies_webhook.policy_ids on apply (gitops.go:221-224).
        ("webhooks_and_tickets_enabled", boolean_leaf()),
        ("software_title_id", leaf()),
        ("script_id", leaf()),
        // All four label scopes are valid on policies — GitOpsPolicySpec
        // carries them (policies.go:646-649) and verifyPolicyLabelScopes
        // (policies.go:205) validates the include pair AND the exclude pair.
        // `labels_exclude_all` is policies-only: no other context defines it.
        ("labels_include_any", array(leaf())),
        ("labels_include_all", array(leaf())),
        ("labels_exclude_any", array(leaf())),
        ("labels_exclude_all", array(leaf())),
        ("run_script", mapping(vec![("path", leaf())])),
        // Patch policy fields (yaml-files.md:141-149). `type: patch` marks a
        // patch policy; `fleet_maintained_app_slug` selects which FMA to track.
        // `version` pins to a specific FMA version (yaml-files.md:147 — "If
        // `version` is set for `fleet_maintained_apps`, that version is
        // included in the query.").
        ("type", leaf()),
        ("fleet_maintained_app_slug", leaf()),
        ("version", leaf()),
        // install_software can be either:
        //   - a mapping (PolicyInstallSoftware fields per gitops.go:231-236), or
        //   - boolean `true` for patch policies to install the FMA on fail.
        // The structural validator's Mapping arm is a no-op on non-mapping
        // values, so a boolean `true` passes through this schema silently.
        (
            "install_software",
            mapping(vec![
                ("package_path", leaf()),
                ("hash_sha256", leaf()),
                ("app_store_id", leaf()),
                ("fleet_maintained_app_slug", leaf()),
            ]),
        ),
    ])
}

fn query_inline_strict() -> SchemaNode {
    mapping(vec![
        ("name", leaf()),
        ("description", leaf()),
        ("query", leaf()),
        ("platform", leaf()),
        ("interval", leaf()),
        ("logging", leaf()),
        ("min_osquery_version", leaf()),
        ("observer_can_run", boolean_leaf()),
        ("automations_enabled", boolean_leaf()),
        ("discard_data", boolean_leaf()),
        ("labels_include_any", array(leaf())),
        ("labels_include_all", array(leaf())),
        ("labels_exclude_any", array(leaf())),
    ])
}

fn label_inline_strict() -> SchemaNode {
    mapping(vec![
        ("name", leaf()),
        ("description", leaf()),
        ("platform", leaf()),
        ("label_membership_type", leaf()),
        ("query", leaf()),
        ("hosts", array(leaf())),
        ("host_ids", array(leaf())),
        // host_vitals criteria — supports nested and/or logic
        // json:"criteria" in Fleet Go source (HostVitalCriteria struct)
        ("criteria", host_vital_criteria()),
        // Some GitOps repos use "host_vitals" as an alias
        ("host_vitals", open_mapping()),
    ])
}

fn host_vital_criteria() -> SchemaNode {
    // Per Fleet's REST API docs and the Go `parseHostVitalCriteria()`, a
    // criteria is a single leaf {vital, value}. `and`/`or` are present in
    // the Go struct but rejected at parse time with "And/Or criteria not
    // supported in host vitals labels yet" — we accept them structurally
    // so the semantic rule can emit that exact message instead of a
    // generic "unknown key" from the structural rule.
    mapping(vec![
        ("vital", leaf()),
        ("value", leaf()),
        ("and", array(open_mapping())),
        ("or", array(open_mapping())),
    ])
}

fn integrations_strict() -> SchemaNode {
    // `webhooks_and_tickets_enabled` was previously listed here by mistake —
    // it's a per-policy field (see `policy_inline_strict`), not an
    // integrations-level setting.
    mapping(vec![
        ("conditional_access_enabled", boolean_leaf()),
        ("enable_conditional_access", boolean_leaf()),
        ("enable_conditional_access_bypass", boolean_leaf()),
        (
            "google_calendar",
            array(mapping(vec![
                ("api_key_json", leaf()),
                ("domain", leaf()),
                ("enable_calendar_events", boolean_leaf()),
                ("webhook_url", leaf()),
            ])),
        ),
        (
            "jira",
            array(mapping(vec![
                ("url", leaf()),
                ("username", leaf()),
                ("api_token", leaf()),
                ("project_key", leaf()),
                ("enable_failing_policies", boolean_leaf()),
                ("enable_software_vulnerabilities", boolean_leaf()),
            ])),
        ),
        (
            "zendesk",
            array(mapping(vec![
                ("url", leaf()),
                ("email", leaf()),
                ("api_token", leaf()),
                ("group_id", leaf()),
                ("enable_failing_policies", boolean_leaf()),
                ("enable_software_vulnerabilities", boolean_leaf()),
            ])),
        ),
    ])
}

// Webhook sub-schemas shared between org and per-fleet webhook settings.
fn activities_webhook() -> SchemaNode {
    mapping(vec![
        ("enable_activities_webhook", boolean_leaf()),
        ("destination_url", leaf()),
    ])
}

fn failing_policies_webhook() -> SchemaNode {
    mapping(vec![
        ("enable_failing_policies_webhook", boolean_leaf()),
        ("destination_url", leaf()),
        ("policy_ids", array(leaf())),
        ("host_batch_size", leaf()),
    ])
}

fn host_status_webhook() -> SchemaNode {
    mapping(vec![
        ("enable_host_status_webhook", boolean_leaf()),
        ("destination_url", leaf()),
        ("days_count", leaf()),
        ("host_percentage", leaf()),
    ])
}

/// Org-level `webhook_settings`. Per Fleet docs (yaml-files.md §webhook_settings),
/// all four webhooks plus `interval` are valid here.
fn webhook_settings_strict() -> SchemaNode {
    mapping(vec![
        ("activities_webhook", activities_webhook()),
        ("failing_policies_webhook", failing_policies_webhook()),
        ("host_status_webhook", host_status_webhook()),
        (
            "vulnerabilities_webhook",
            mapping(vec![
                ("enable_vulnerabilities_webhook", boolean_leaf()),
                ("destination_url", leaf()),
                ("host_batch_size", leaf()),
            ]),
        ),
        ("interval", leaf()),
    ])
}

/// Per-fleet `webhook_settings` (under `settings:` / `team_settings:`).
/// `vulnerabilities_webhook` is org-only per Fleet docs (yaml-files.md:1151,
/// "Can only be configured for 'All fleets' (org_settings)") — omitted here
/// so the structural rule rejects it with a clear "unknown key" error.
fn webhook_settings_per_fleet() -> SchemaNode {
    mapping(vec![
        ("activities_webhook", activities_webhook()),
        ("failing_policies_webhook", failing_policies_webhook()),
        ("host_status_webhook", host_status_webhook()),
        ("interval", leaf()),
    ])
}

fn features_schema() -> SchemaNode {
    // Source: server/fleet/app.go Features struct. Shared between
    // org_settings.features and team-level settings.features.
    mapping(vec![
        ("additional_queries", open_mapping()),
        ("enable_host_users", boolean_leaf()),
        ("enable_software_inventory", boolean_leaf()),
        ("detail_query_overrides", open_mapping()),
        ("osquery_detail", open_mapping()),
        ("osquery_policy", open_mapping()),
        // Per-dataset dashboard collection toggles (app.go
        // HistoricalDataSettings, issue #12). Sub-keys default to true.
        (
            "historical_data",
            mapping(vec![
                ("uptime", boolean_leaf()),
                ("vulnerabilities", boolean_leaf()),
            ]),
        ),
    ])
}

fn org_settings_strict() -> SchemaNode {
    mapping(vec![
        ("features", features_schema()),
        (
            "fleet_desktop",
            mapping(vec![
                ("transparency_url", leaf()),
                ("alternative_browser_host", leaf()),
            ]),
        ),
        (
            "host_expiry_settings",
            mapping(vec![
                ("host_expiry_enabled", boolean_leaf()),
                ("host_expiry_window", leaf()),
            ]),
        ),
        (
            "org_info",
            mapping(vec![
                ("org_name", leaf()),
                // Deprecated URL keys (app.go:1368-1371 `// Deprecated:`),
                // auto-migrated by NormalizeLogoFields — still accepted.
                ("org_logo_url", leaf()),
                ("org_logo_url_light_background", leaf()),
                // Mode-aware replacements, plus the path variants
                // (validateOrgInfoLogo, pkg/spec/gitops.go:845-846).
                ("org_logo_url_dark_mode", leaf()),
                ("org_logo_url_light_mode", leaf()),
                ("org_logo_path_dark_mode", leaf()),
                ("org_logo_path_light_mode", leaf()),
                ("contact_url", leaf()),
            ]),
        ),
        ("secrets", array(mapping(vec![("secret", leaf())]))),
        (
            "server_settings",
            mapping(vec![
                ("ai_features_disabled", boolean_leaf()),
                ("deferred_save_host", boolean_leaf()),
                ("enable_analytics", boolean_leaf()),
                ("live_query_disabled", boolean_leaf()),
                ("live_reporting_disabled", boolean_leaf()),
                ("query_reports_disabled", boolean_leaf()),
                ("discard_reports_data", boolean_leaf()),
                ("query_report_cap", leaf()),
                ("report_cap", leaf()),
                ("scripts_disabled", boolean_leaf()),
                ("server_url", leaf()),
            ]),
        ),
        (
            "sso_settings",
            mapping(vec![
                ("enable_sso", boolean_leaf()),
                ("idp_name", leaf()),
                ("idp_image_url", leaf()),
                ("entity_id", leaf()),
                ("metadata", leaf()),
                ("metadata_url", leaf()),
                ("enable_jit_provisioning", boolean_leaf()),
                ("enable_jit_role_sync", boolean_leaf()),
                ("enable_sso_idp_login", boolean_leaf()),
                ("issuer_uri", leaf()),
                ("sso_server_url", leaf()),
            ]),
        ),
        ("integrations", integrations_strict()),
        ("certificate_authorities", open_mapping()),
        ("webhook_settings", webhook_settings_strict()),
        ("mdm", open_mapping()),
        (
            "smtp_settings",
            mapping(vec![
                ("authentication_method", leaf()),
                ("authentication_type", leaf()),
                ("domain", leaf()),
                ("enable_smtp", boolean_leaf()),
                ("enable_ssl_tls", boolean_leaf()),
                ("enable_start_tls", boolean_leaf()),
                ("password", leaf()),
                ("port", leaf()),
                ("sender_address", leaf()),
                ("server", leaf()),
                ("user_name", leaf()),
                ("verify_ssl_certs", boolean_leaf()),
            ]),
        ),
        (
            "vulnerability_settings",
            mapping(vec![
                ("databases_path", leaf()),
                ("periodicity", leaf()),
                ("cpe_database_url", leaf()),
                ("cpe_translations_url", leaf()),
                ("cve_feed_prefix_url", leaf()),
                ("disable_data_sync", boolean_leaf()),
                ("disable_win_os_vulnerabilities", boolean_leaf()),
                ("recent_vulnerability_max_age", leaf()),
            ]),
        ),
        (
            "activity_expiry_settings",
            mapping(vec![
                ("activity_expiry_enabled", boolean_leaf()),
                ("activity_expiry_window", leaf()),
            ]),
        ),
        ("yara_rules", array(open_mapping())),
        (
            "gitops",
            mapping(vec![
                ("gitops_mode_enabled", boolean_leaf()),
                ("repository_url", leaf()),
            ]),
        ),
    ])
}

fn team_settings_strict() -> SchemaNode {
    mapping(vec![
        ("features", features_schema()),
        (
            "host_expiry_settings",
            mapping(vec![
                ("host_expiry_enabled", boolean_leaf()),
                ("host_expiry_window", leaf()),
            ]),
        ),
        ("secrets", array(mapping(vec![("secret", leaf())]))),
        ("integrations", integrations_strict()),
        ("webhook_settings", webhook_settings_per_fleet()),
    ])
}

fn agent_options_inline() -> SchemaNode {
    // Source of truth: server/fleet/agent_options.go AgentOptions struct.
    // The `agent_options` mapping can be either an inline object or a
    // `path:` reference to a separate lib YAML file (yaml-files.md
    // agent_options section).
    mapping(vec![
        ("path", leaf()),
        ("config", open_mapping()),
        ("overrides", open_mapping()),
        ("command_line_flags", open_mapping()),
        ("update_channels", open_mapping()),
        // Maximum allowed: 18000 seconds (5h). Validated by Fleet at apply time.
        ("script_execution_timeout", leaf()),
        // osquery extensions configuration. Open mapping because Fleet
        // accepts arbitrary platform/extension nesting.
        ("extensions", open_mapping()),
    ])
}

// ---------------------------------------------------------------------------
// Top-level schemas for each file type
// ---------------------------------------------------------------------------

/// Schema for `default.yml` files.
pub(crate) fn default_schema() -> SchemaNode {
    mapping(vec![
        (
            "labels",
            array(with_base_item(label_inline_strict())),
        ),
        (
            "policies",
            array(with_base_item(policy_inline_strict())),
        ),
        (
            "queries",
            array(with_base_item(query_inline_strict())),
        ),
        (
            "reports",
            array(with_base_item(query_inline_strict())),
        ),
        ("agent_options", agent_options_inline()),
        ("controls", controls_schema()),
        ("software", software_schema()),
        ("org_settings", org_settings_strict()),
        ("team_settings", team_settings_strict()),
        ("settings", team_settings_strict()),
        // Custom host vitals — default.yml ONLY (deliberately absent from
        // fleet_schema; Fleet rejects it in fleets/*.yml). fleet/
        // api_custom_host_vitals.go, docs/Configuration/yaml-files.md.
        ("custom_host_vitals", array(mapping(vec![("name", leaf())]))),
    ])
}

/// Schema for `fleets/*.yml` (and legacy `teams/*.yml`) files. Same structure as default.
pub(crate) fn fleet_schema() -> SchemaNode {
    mapping(vec![
        ("name", leaf()),
        (
            "labels",
            array(with_base_item(label_inline_strict())),
        ),
        (
            "policies",
            array(with_base_item(policy_inline_strict())),
        ),
        (
            "queries",
            array(with_base_item(query_inline_strict())),
        ),
        (
            "reports",
            array(with_base_item(query_inline_strict())),
        ),
        ("agent_options", agent_options_inline()),
        ("controls", controls_schema()),
        ("software", software_schema()),
        ("team_settings", team_settings_strict()),
        ("settings", team_settings_strict()),
    ])
}

/// Schema for `lib/policies/*.yml` files (array of policies).
pub(crate) fn policy_schema() -> SchemaNode {
    array(with_base_item(policy_inline_strict()))
}

/// Schema for `lib/queries/*.yml` files (array of queries).
pub(crate) fn query_schema() -> SchemaNode {
    array(with_base_item(query_inline_strict()))
}

/// Schema for `lib/labels/*.yml` files (array of labels).
pub(crate) fn label_schema() -> SchemaNode {
    array(with_base_item(label_inline_strict()))
}

// ---------------------------------------------------------------------------
// Static instances
// ---------------------------------------------------------------------------

pub(crate) static DEFAULT_SCHEMA: Lazy<SchemaNode> = Lazy::new(default_schema);
pub(crate) static FLEET_SCHEMA: Lazy<SchemaNode> = Lazy::new(fleet_schema);
pub(crate) static POLICY_SCHEMA: Lazy<SchemaNode> = Lazy::new(policy_schema);
pub(crate) static QUERY_SCHEMA: Lazy<SchemaNode> = Lazy::new(query_schema);
pub(crate) static LABEL_SCHEMA: Lazy<SchemaNode> = Lazy::new(label_schema);

// Standalone software file shapes (software/*.yml, *.package.yml). A file is
// either a `packages:`/`app_store_apps:`/`fleet_maintained_apps:` list file,
// a single item mapping, or a sequence of item mappings — `structural.rs`
// classifies each item by its discriminating key and picks the schema.
pub(crate) static SOFTWARE_LIST_SCHEMA: Lazy<SchemaNode> = Lazy::new(software_schema);
pub(crate) static PACKAGE_ITEM_SCHEMA: Lazy<SchemaNode> = Lazy::new(software_package_item);
pub(crate) static APP_STORE_ITEM_SCHEMA: Lazy<SchemaNode> = Lazy::new(app_store_app_item);
pub(crate) static FMA_ITEM_SCHEMA: Lazy<SchemaNode> = Lazy::new(fleet_maintained_app_item);

pub(crate) static KEY_REGISTRY: Lazy<KeyRegistry> = Lazy::new(|| {
    let mut reg = KeyRegistry::new();

    // Top-level keys (default.yml)
    reg.register("labels", "");
    reg.register("policies", "");
    reg.register("queries", "");
    reg.register("reports", "");
    reg.register("agent_options", "");
    // agent_options children. Source: server/fleet/agent_options.go AgentOptions struct.
    // Inline OR a `path:` reference pointing at a lib YAML file.
    reg.register("path", "agent_options");
    reg.register("config", "agent_options");
    reg.register("overrides", "agent_options");
    reg.register("command_line_flags", "agent_options");
    reg.register("update_channels", "agent_options");
    reg.register("script_execution_timeout", "agent_options");
    reg.register("extensions", "agent_options");
    reg.register("controls", "");
    reg.register("software", "");
    reg.register("org_settings", "");
    reg.register("team_settings", "");
    reg.register("settings", "");
    reg.register("name", "");
    reg.register("custom_host_vitals", "");
    reg.register("name", "custom_host_vitals[]");

    // controls children
    reg.register("scripts", "controls");
    reg.register("windows_enabled_and_configured", "controls");
    reg.register("windows_entra_tenant_ids", "controls");
    reg.register("windows_entra_client_ids", "controls");
    reg.register("android_enabled_and_configured", "controls");
    reg.register("apple_account_provisioning", "controls");
    reg.register("oauth_idp_token_url", "controls.apple_account_provisioning");
    reg.register("oauth_idp_client_id", "controls.apple_account_provisioning");
    reg.register("oauth_idp_client_secret", "controls.apple_account_provisioning");
    reg.register("enable_turn_on_windows_mdm_manually", "controls");
    reg.register("windows_migration_enabled", "controls");
    reg.register("enable_disk_encryption", "controls");
    reg.register("enable_recovery_lock_password", "controls");
    reg.register("apple_require_hardware_attestation", "controls");
    reg.register("volume_purchasing_program", "controls");
    reg.register("windows_require_bitlocker_pin", "controls");
    reg.register("macos_updates", "controls");
    reg.register("ios_updates", "controls");
    reg.register("ipados_updates", "controls");
    reg.register("windows_updates", "controls");
    reg.register("macos_settings", "controls");
    reg.register("apple_settings", "controls"); // rename of macos_settings
    reg.register("windows_settings", "controls");
    reg.register("android_settings", "controls");
    reg.register("macos_setup", "controls");
    reg.register("setup_experience", "controls"); // rename of macos_setup
    reg.register("macos_migration", "controls");

    // controls.macos_settings / apple_settings / windows_settings children
    reg.register("custom_settings", "controls.macos_settings");
    reg.register("configuration_profiles", "controls.macos_settings");
    reg.register("custom_settings", "controls.apple_settings");
    reg.register("configuration_profiles", "controls.apple_settings");
    reg.register("custom_settings", "controls.windows_settings");
    reg.register("configuration_profiles", "controls.windows_settings");

    // controls.android_settings children
    reg.register("custom_settings", "controls.android_settings");
    reg.register("configuration_profiles", "controls.android_settings");
    reg.register("certificates", "controls.android_settings");

    // controls.android_settings.certificates[] fields
    reg.register("name", "controls.android_settings.certificates[]");
    reg.register(
        "certificate_authority_name",
        "controls.android_settings.certificates[]",
    );
    reg.register("subject_name", "controls.android_settings.certificates[]");
    reg.register(
        "subject_alternative_name",
        "controls.android_settings.certificates[]",
    );

    // controls.macos_updates / ios_updates / ipados_updates children
    reg.register("deadline", "controls.macos_updates");
    reg.register("minimum_version", "controls.macos_updates");
    reg.register("update_new_hosts", "controls.macos_updates");
    reg.register("deadline", "controls.ios_updates");
    reg.register("minimum_version", "controls.ios_updates");
    reg.register("update_new_hosts", "controls.ios_updates");
    reg.register("deadline", "controls.ipados_updates");
    reg.register("minimum_version", "controls.ipados_updates");
    reg.register("update_new_hosts", "controls.ipados_updates");

    // controls.windows_updates children
    reg.register("deadline_days", "controls.windows_updates");
    reg.register("grace_period_days", "controls.windows_updates");

    // controls.macos_setup / setup_experience children
    for parent in &["controls.macos_setup", "controls.setup_experience"] {
        // Old names (json tags)
        reg.register("bootstrap_package", parent);
        reg.register("manual_agent_install", parent);
        reg.register("enable_end_user_authentication", parent);
        reg.register("lock_end_user_info", parent);
        reg.register("require_all_software", parent);
        reg.register("enable_release_device_manually", parent);
        reg.register("macos_setup_assistant", parent);
        reg.register("script", parent);
        // New names (renameto tags)
        reg.register("macos_bootstrap_package", parent);
        reg.register("macos_manual_agent_install", parent);
        reg.register("require_all_software_macos", parent);
        reg.register("require_all_software_windows", parent);
        reg.register("apple_enable_release_device_manually", parent);
        reg.register("apple_setup_assistant", parent);
        reg.register("macos_script", parent);
        // Additional MDMAppleSetupPayload fields (apple_mdm.go:524-534).
        reg.register("enable_managed_local_account", parent);
        // Current name (app.go: renameto on enable_managed_local_account).
        reg.register("enable_create_local_admin_account", parent);
        reg.register("end_user_local_account_type", parent);
    }

    // controls.macos_migration children
    reg.register("enable", "controls.macos_migration");
    reg.register("mode", "controls.macos_migration");
    reg.register("webhook_url", "controls.macos_migration");

    // org_settings children
    reg.register("features", "org_settings");
    reg.register("fleet_desktop", "org_settings");
    reg.register("host_expiry_settings", "org_settings");
    reg.register("org_info", "org_settings");
    reg.register("secrets", "org_settings");
    reg.register("server_settings", "org_settings");
    reg.register("sso_settings", "org_settings");
    reg.register("integrations", "org_settings");
    reg.register("certificate_authorities", "org_settings");
    reg.register("gitops", "org_settings");

    // org_settings.features children
    reg.register("additional_queries", "org_settings.features");
    reg.register("enable_host_users", "org_settings.features");
    reg.register("enable_software_inventory", "org_settings.features");
    reg.register("historical_data", "org_settings.features");
    // features.historical_data children (app.go HistoricalDataSettings)
    reg.register("uptime", "org_settings.features.historical_data");
    reg.register("vulnerabilities", "org_settings.features.historical_data");

    // org_settings.fleet_desktop children
    reg.register("transparency_url", "org_settings.fleet_desktop");
    reg.register("alternative_browser_host", "org_settings.fleet_desktop");

    // org_settings.host_expiry_settings children
    reg.register("host_expiry_enabled", "org_settings.host_expiry_settings");
    reg.register("host_expiry_window", "org_settings.host_expiry_settings");

    // org_settings.org_info children
    reg.register("org_name", "org_settings.org_info");
    reg.register("org_logo_url", "org_settings.org_info");
    reg.register("org_logo_url_light_background", "org_settings.org_info");
    reg.register("org_logo_url_dark_mode", "org_settings.org_info");
    reg.register("org_logo_url_light_mode", "org_settings.org_info");
    reg.register("org_logo_path_dark_mode", "org_settings.org_info");
    reg.register("org_logo_path_light_mode", "org_settings.org_info");
    reg.register("contact_url", "org_settings.org_info");

    // org_settings.server_settings children
    reg.register("ai_features_disabled", "org_settings.server_settings");
    reg.register("enable_analytics", "org_settings.server_settings");
    reg.register("live_query_disabled", "org_settings.server_settings");
    reg.register("live_reporting_disabled", "org_settings.server_settings");
    reg.register("query_reports_disabled", "org_settings.server_settings");
    reg.register("discard_reports_data", "org_settings.server_settings");
    reg.register("query_report_cap", "org_settings.server_settings");
    reg.register("report_cap", "org_settings.server_settings");
    reg.register("scripts_disabled", "org_settings.server_settings");
    reg.register("server_url", "org_settings.server_settings");
    reg.register("discard_reports_data", "org_settings.server_settings");

    // org_settings.sso_settings children
    reg.register("enable_sso", "org_settings.sso_settings");
    reg.register("idp_name", "org_settings.sso_settings");
    reg.register("idp_image_url", "org_settings.sso_settings");
    reg.register("entity_id", "org_settings.sso_settings");
    reg.register("metadata", "org_settings.sso_settings");
    reg.register("metadata_url", "org_settings.sso_settings");
    reg.register("enable_jit_provisioning", "org_settings.sso_settings");
    reg.register("enable_jit_role_sync", "org_settings.sso_settings");
    reg.register("enable_sso_idp_login", "org_settings.sso_settings");
    reg.register("issuer_uri", "org_settings.sso_settings");
    reg.register("sso_server_url", "org_settings.sso_settings");

    // org_settings.vulnerability_settings children
    reg.register("vulnerability_settings", "org_settings");
    reg.register("databases_path", "org_settings.vulnerability_settings");
    reg.register("periodicity", "org_settings.vulnerability_settings");
    reg.register("cpe_database_url", "org_settings.vulnerability_settings");
    reg.register(
        "cpe_translations_url",
        "org_settings.vulnerability_settings",
    );
    reg.register("cve_feed_prefix_url", "org_settings.vulnerability_settings");
    reg.register("disable_data_sync", "org_settings.vulnerability_settings");
    reg.register(
        "disable_win_os_vulnerabilities",
        "org_settings.vulnerability_settings",
    );
    reg.register(
        "recent_vulnerability_max_age",
        "org_settings.vulnerability_settings",
    );

    // org_settings.activity_expiry_settings children
    reg.register("activity_expiry_settings", "org_settings");
    reg.register(
        "activity_expiry_enabled",
        "org_settings.activity_expiry_settings",
    );
    reg.register(
        "activity_expiry_window",
        "org_settings.activity_expiry_settings",
    );

    // webhook_settings is valid under org_settings, settings, and team_settings
    reg.register("webhook_settings", "org_settings");

    // Webhook sub-schemas shared between org and per-fleet contexts.
    reg.register("activities_webhook", "org_settings.webhook_settings");
    reg.register("activities_webhook", "settings.webhook_settings");
    reg.register("activities_webhook", "team_settings.webhook_settings");
    reg.register("failing_policies_webhook", "org_settings.webhook_settings");
    reg.register("failing_policies_webhook", "settings.webhook_settings");
    reg.register("failing_policies_webhook", "team_settings.webhook_settings");
    reg.register("host_status_webhook", "org_settings.webhook_settings");
    reg.register("host_status_webhook", "settings.webhook_settings");
    reg.register("host_status_webhook", "team_settings.webhook_settings");
    reg.register("interval", "org_settings.webhook_settings");
    reg.register("interval", "settings.webhook_settings");
    reg.register("interval", "team_settings.webhook_settings");

    // Org-only per yaml-files.md:1151 ("Can only be configured for 'All fleets'").
    reg.register("vulnerabilities_webhook", "org_settings.webhook_settings");

    // activities_webhook leaf fields
    reg.register("enable_activities_webhook", "org_settings.webhook_settings.activities_webhook");
    reg.register("destination_url", "org_settings.webhook_settings.activities_webhook");
    reg.register("enable_activities_webhook", "settings.webhook_settings.activities_webhook");
    reg.register("destination_url", "settings.webhook_settings.activities_webhook");
    reg.register("enable_activities_webhook", "team_settings.webhook_settings.activities_webhook");
    reg.register("destination_url", "team_settings.webhook_settings.activities_webhook");

    // failing_policies_webhook leaf fields
    reg.register("enable_failing_policies_webhook", "org_settings.webhook_settings.failing_policies_webhook");
    reg.register("destination_url", "org_settings.webhook_settings.failing_policies_webhook");
    reg.register("policy_ids", "org_settings.webhook_settings.failing_policies_webhook");
    reg.register("host_batch_size", "org_settings.webhook_settings.failing_policies_webhook");
    reg.register("enable_failing_policies_webhook", "settings.webhook_settings.failing_policies_webhook");
    reg.register("destination_url", "settings.webhook_settings.failing_policies_webhook");
    reg.register("policy_ids", "settings.webhook_settings.failing_policies_webhook");
    reg.register("host_batch_size", "settings.webhook_settings.failing_policies_webhook");
    reg.register("enable_failing_policies_webhook", "team_settings.webhook_settings.failing_policies_webhook");
    reg.register("destination_url", "team_settings.webhook_settings.failing_policies_webhook");
    reg.register("policy_ids", "team_settings.webhook_settings.failing_policies_webhook");
    reg.register("host_batch_size", "team_settings.webhook_settings.failing_policies_webhook");

    // host_status_webhook leaf fields
    reg.register("enable_host_status_webhook", "org_settings.webhook_settings.host_status_webhook");
    reg.register("destination_url", "org_settings.webhook_settings.host_status_webhook");
    reg.register("days_count", "org_settings.webhook_settings.host_status_webhook");
    reg.register("host_percentage", "org_settings.webhook_settings.host_status_webhook");
    reg.register("enable_host_status_webhook", "settings.webhook_settings.host_status_webhook");
    reg.register("destination_url", "settings.webhook_settings.host_status_webhook");
    reg.register("days_count", "settings.webhook_settings.host_status_webhook");
    reg.register("host_percentage", "settings.webhook_settings.host_status_webhook");
    reg.register("enable_host_status_webhook", "team_settings.webhook_settings.host_status_webhook");
    reg.register("destination_url", "team_settings.webhook_settings.host_status_webhook");
    reg.register("days_count", "team_settings.webhook_settings.host_status_webhook");
    reg.register("host_percentage", "team_settings.webhook_settings.host_status_webhook");

    // vulnerabilities_webhook leaf fields (org-only)
    reg.register("enable_vulnerabilities_webhook", "org_settings.webhook_settings.vulnerabilities_webhook");
    reg.register("destination_url", "org_settings.webhook_settings.vulnerabilities_webhook");
    reg.register("host_batch_size", "org_settings.webhook_settings.vulnerabilities_webhook");

    // org_settings.mdm
    reg.register("mdm", "org_settings");
    reg.register("smtp_settings", "org_settings");
    reg.register("yara_rules", "org_settings");

    // org_settings.gitops children
    reg.register("gitops_mode_enabled", "org_settings.gitops");
    reg.register("repository_url", "org_settings.gitops");

    // team_settings children
    reg.register("features", "team_settings");
    reg.register("host_expiry_settings", "team_settings");
    reg.register("secrets", "team_settings");
    reg.register("integrations", "team_settings");
    reg.register("webhook_settings", "team_settings");

    // settings children (alias for team_settings)
    reg.register("features", "settings");
    reg.register("host_expiry_settings", "settings");
    reg.register("secrets", "settings");
    reg.register("integrations", "settings");
    reg.register("webhook_settings", "settings");

    // reports fields (alias for queries)
    reg.register("name", "reports[]");
    reg.register("description", "reports[]");
    reg.register("query", "reports[]");
    reg.register("platform", "reports[]");
    reg.register("interval", "reports[]");
    reg.register("logging", "reports[]");
    reg.register("min_osquery_version", "reports[]");
    reg.register("observer_can_run", "reports[]");
    reg.register("automations_enabled", "reports[]");
    reg.register("discard_data", "reports[]");
    reg.register("labels_include_any", "reports[]");
    reg.register("labels_include_all", "reports[]");
    reg.register("labels_exclude_any", "reports[]");
    reg.register("path", "reports[]");
    reg.register("paths", "reports[]");

    // integrations children (under org_settings.integrations or team_settings.integrations)
    reg.register("conditional_access_enabled", "org_settings.integrations");
    reg.register("google_calendar", "org_settings.integrations");
    reg.register("jira", "org_settings.integrations");
    reg.register("zendesk", "org_settings.integrations");

    // Policy fields. Authoritative source: pkg/spec/gitops.go:206-225 +
    // server/fleet/policies.go:448-485. Cross-validated against
    // testdata/generateGitops/expectedTeamPolicies.yaml.
    reg.register("name", "policies[]");
    reg.register("description", "policies[]");
    reg.register("resolution", "policies[]");
    reg.register("query", "policies[]");
    reg.register("platform", "policies[]");
    reg.register("critical", "policies[]");
    reg.register("team", "policies[]");
    reg.register("fleet", "policies[]"); // renameto target of `team`
    reg.register("calendar_events_enabled", "policies[]");
    reg.register("conditional_access_enabled", "policies[]");
    reg.register("conditional_access_bypass_enabled", "policies[]");
    reg.register("webhooks_and_tickets_enabled", "policies[]");
    reg.register("software_title_id", "policies[]");
    reg.register("script_id", "policies[]");
    reg.register("labels_include_any", "policies[]");
    reg.register("labels_include_all", "policies[]");
    reg.register("labels_exclude_any", "policies[]");
    // Policies-only exclude scope (policies.go:649 GitOpsPolicySpec).
    reg.register("labels_exclude_all", "policies[]");
    reg.register("run_script", "policies[]");
    reg.register("install_software", "policies[]");
    // Patch policy fields (Fleet Premium).
    reg.register("type", "policies[]");
    reg.register("fleet_maintained_app_slug", "policies[]");
    reg.register("version", "policies[]");
    reg.register("path", "policies[]");
    reg.register("paths", "policies[]");
    // install_software leaf fields (PolicyInstallSoftware in gitops.go:231-236)
    reg.register("package_path", "policies[].install_software");
    reg.register("hash_sha256", "policies[].install_software");
    reg.register("app_store_id", "policies[].install_software");
    reg.register("fleet_maintained_app_slug", "policies[].install_software");

    // Query fields
    reg.register("name", "queries[]");
    reg.register("description", "queries[]");
    reg.register("query", "queries[]");
    reg.register("platform", "queries[]");
    reg.register("interval", "queries[]");
    reg.register("logging", "queries[]");
    reg.register("min_osquery_version", "queries[]");
    reg.register("observer_can_run", "queries[]");
    reg.register("automations_enabled", "queries[]");
    reg.register("discard_data", "queries[]");
    reg.register("labels_include_any", "queries[]");
    reg.register("labels_include_all", "queries[]");
    reg.register("labels_exclude_any", "queries[]");
    reg.register("path", "queries[]");
    reg.register("paths", "queries[]");

    // Label fields
    reg.register("name", "labels[]");
    reg.register("description", "labels[]");
    reg.register("platform", "labels[]");
    reg.register("label_membership_type", "labels[]");
    reg.register("query", "labels[]");
    reg.register("hosts", "labels[]");
    reg.register("host_ids", "labels[]");
    reg.register("criteria", "labels[]");
    reg.register("host_vitals", "labels[]");
    // host_vital_criteria fields (documented: vital + value only)
    reg.register("vital", "labels[].criteria");
    reg.register("value", "labels[].criteria");
    // and/or are rejected by Fleet today but registered so the semantic
    // rule owns the diagnostic instead of the structural rule.
    reg.register("and", "labels[].criteria");
    reg.register("or", "labels[].criteria");
    reg.register("path", "labels[]");
    reg.register("paths", "labels[]");

    // Script targeting fields (used in controls.scripts[], custom_settings[])
    reg.register("path", "controls.scripts[]");
    reg.register("paths", "controls.scripts[]");
    reg.register("labels_include_all", "controls.scripts[]");
    reg.register("labels_include_any", "controls.scripts[]");
    reg.register("labels_exclude_any", "controls.scripts[]");

    // Profile items: custom DDM activation path (MDMProfileSpec, PR #50557)
    for parent in &[
        "controls.apple_settings.configuration_profiles[]",
        "controls.apple_settings.custom_settings[]",
        "controls.macos_settings.configuration_profiles[]",
        "controls.macos_settings.custom_settings[]",
    ] {
        reg.register("activation", parent);
    }

    // Windows managed local admin account (fleet/app.go)
    reg.register(
        "managed_local_account_settings",
        "controls.windows_settings",
    );
    reg.register(
        "enabled",
        "controls.windows_settings.managed_local_account_settings",
    );

    // software children
    reg.register("packages", "software");
    reg.register("app_store_apps", "software");
    reg.register("fleet_maintained_apps", "software");

    // software.packages[] fields
    reg.register("path", "software.packages[]");
    reg.register("url", "software.packages[]");
    reg.register("hash_sha256", "software.packages[]");
    reg.register("always_download", "software.packages[]");
    reg.register("setup_experience_platform", "software.packages[]");
    reg.register("display_name", "software.packages[]");
    reg.register("self_service", "software.packages[]");
    reg.register("setup_experience", "software.packages[]");
    reg.register("categories", "software.packages[]");
    reg.register("labels_include_any", "software.packages[]");
    reg.register("labels_exclude_any", "software.packages[]");
    reg.register("labels_include_all", "software.packages[]");
    reg.register("pre_install_query", "software.packages[]");
    reg.register("install_script", "software.packages[]");
    reg.register("uninstall_script", "software.packages[]");
    reg.register("post_install_script", "software.packages[]");
    reg.register("icon", "software.packages[]");

    // software.app_store_apps[] fields
    reg.register("app_store_id", "software.app_store_apps[]");
    reg.register("platform", "software.app_store_apps[]");
    reg.register("display_name", "software.app_store_apps[]");
    reg.register("self_service", "software.app_store_apps[]");
    reg.register("setup_experience", "software.app_store_apps[]");
    reg.register("categories", "software.app_store_apps[]");
    reg.register("labels_include_any", "software.app_store_apps[]");
    reg.register("labels_exclude_any", "software.app_store_apps[]");
    reg.register("labels_include_all", "software.app_store_apps[]");
    reg.register("icon", "software.app_store_apps[]");
    reg.register("configuration", "software.app_store_apps[]");
    reg.register("auto_update_enabled", "software.app_store_apps[]");
    reg.register("auto_update_window_start", "software.app_store_apps[]");
    reg.register("auto_update_window_end", "software.app_store_apps[]");

    // software.fleet_maintained_apps[] fields
    reg.register("slug", "software.fleet_maintained_apps[]");
    reg.register("version", "software.fleet_maintained_apps[]");
    reg.register("display_name", "software.fleet_maintained_apps[]");
    reg.register("self_service", "software.fleet_maintained_apps[]");
    reg.register("setup_experience", "software.fleet_maintained_apps[]");
    reg.register("categories", "software.fleet_maintained_apps[]");
    reg.register("labels_include_any", "software.fleet_maintained_apps[]");
    reg.register("labels_exclude_any", "software.fleet_maintained_apps[]");
    reg.register("labels_include_all", "software.fleet_maintained_apps[]");
    reg.register("pre_install_query", "software.fleet_maintained_apps[]");
    reg.register("install_script", "software.fleet_maintained_apps[]");
    reg.register("uninstall_script", "software.fleet_maintained_apps[]");
    reg.register("post_install_script", "software.fleet_maintained_apps[]");
    reg.register("icon", "software.fleet_maintained_apps[]");

    reg
});

// ---------------------------------------------------------------------------
// File-type detection
// ---------------------------------------------------------------------------

/// Determine which schema to use based on a file path.
pub(crate) fn schema_for_path(path: &std::path::Path) -> &'static SchemaNode {
    let path_str = path.to_string_lossy();

    // fleets/*.yml or legacy teams/*.yml
    if path_str.contains("fleets/")
        || path_str.contains("fleets\\")
        || path_str.contains("teams/")
        || path_str.contains("teams\\")
    {
        return &FLEET_SCHEMA;
    }

    // lib/policies/*.yml or just policies (including under platforms/)
    if path_str.contains("policies") {
        return &POLICY_SCHEMA;
    }

    // lib/queries/*.yml, lib/reports/*.yml, or just queries/reports (including under platforms/)
    if path_str.contains("queries") || path_str.contains("reports") {
        return &QUERY_SCHEMA;
    }

    // lib/labels/*.yml or just labels (including under platforms/)
    if path_str.contains("labels") {
        return &LABEL_SCHEMA;
    }

    // platforms/<name>/default.yml or platforms/<name>/*.yml
    if path_str.contains("platforms/") || path_str.contains("platforms\\") {
        return &DEFAULT_SCHEMA;
    }

    // Default: default.yml or any other top-level config
    &DEFAULT_SCHEMA
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_schema_for_default() {
        let schema = schema_for_path(Path::new("default.yml"));
        assert!(matches!(schema, SchemaNode::Mapping(_)));
        assert!(schema.get_child("policies").is_some());
        assert!(schema.get_child("controls").is_some());
        assert!(schema.get_child("org_settings").is_some());
    }

    #[test]
    fn test_schema_for_fleet() {
        let schema = schema_for_path(Path::new("fleets/engineering.yml"));
        assert!(matches!(schema, SchemaNode::Mapping(_)));
        assert!(schema.get_child("name").is_some());
        assert!(schema.get_child("policies").is_some());
    }

    #[test]
    fn test_schema_for_legacy_teams() {
        // Backward compatibility: teams/ still routes to FLEET_SCHEMA
        let schema = schema_for_path(Path::new("teams/engineering.yml"));
        assert!(matches!(schema, SchemaNode::Mapping(_)));
        assert!(schema.get_child("name").is_some());
        assert!(schema.get_child("policies").is_some());
    }

    #[test]
    fn test_schema_for_policies() {
        let schema = schema_for_path(Path::new("lib/policies/security.yml"));
        assert!(matches!(schema, SchemaNode::Array(_)));
    }

    #[test]
    fn test_schema_for_queries() {
        let schema = schema_for_path(Path::new("lib/queries/compliance.yml"));
        assert!(matches!(schema, SchemaNode::Array(_)));
    }

    #[test]
    fn test_schema_for_labels() {
        let schema = schema_for_path(Path::new("lib/labels/hosts.yml"));
        assert!(matches!(schema, SchemaNode::Array(_)));
    }

    #[test]
    fn test_schema_for_reports() {
        let schema = schema_for_path(Path::new("lib/reports/compliance.yml"));
        assert!(matches!(schema, SchemaNode::Array(_)));
    }

    #[test]
    fn test_key_registry_lookup() {
        let reg = &*KEY_REGISTRY;
        let paths = reg.lookup("scripts").unwrap();
        assert!(paths.contains(&"controls"));

        let paths = reg.lookup("custom_settings").unwrap();
        assert!(paths.contains(&"controls.macos_settings"));
    }

    #[test]
    fn test_setup_experience_local_admin_account_keys() {
        // Both the legacy and current Fleet names (app.go renameto:
        // enable_managed_local_account → enable_create_local_admin_account) are
        // recognized under controls.macos_setup / controls.setup_experience.
        let reg = &*KEY_REGISTRY;
        for key in ["enable_managed_local_account", "enable_create_local_admin_account"] {
            let paths = reg.lookup(key).unwrap_or_else(|| panic!("'{key}' missing from KEY_REGISTRY"));
            assert!(paths.contains(&"controls.setup_experience"), "{key} not under setup_experience");
            assert!(paths.contains(&"controls.macos_setup"), "{key} not under macos_setup");
        }
    }

    #[test]
    fn test_key_registry_paths_field() {
        let reg = &*KEY_REGISTRY;
        let paths = reg.lookup("paths").unwrap();
        assert!(paths.contains(&"reports[]"));
        assert!(paths.contains(&"policies[]"));
        assert!(paths.contains(&"queries[]"));
        assert!(paths.contains(&"labels[]"));
        assert!(paths.contains(&"controls.scripts[]"));
    }

    #[test]
    fn test_schema_key_count_does_not_regress() {
        // This test ensures we don't accidentally remove schema keys.
        // If this fails after a refactor, verify no keys were lost.
        // Update the count when intentionally adding/removing keys.
        let reg = &*KEY_REGISTRY;
        let total_keys = reg.all_keys().len();
        assert!(
            total_keys >= 160,
            "KEY_REGISTRY has {total_keys} keys, expected >= 160. Did keys get removed?"
        );
    }

    #[test]
    fn test_policy_label_scopes_registered() {
        // Fleet's GitOpsPolicySpec (policies.go:646-649) carries all four
        // label scopes, and verifyPolicyLabelScopes validates both pairs.
        let reg = &*KEY_REGISTRY;
        for key in [
            "labels_include_any",
            "labels_include_all",
            "labels_exclude_any",
            "labels_exclude_all",
        ] {
            let paths = reg.lookup(key).unwrap_or_else(|| panic!("{key} missing"));
            assert!(
                paths.contains(&"policies[]"),
                "{key} must be valid on policies[] (policies.go:646-649), got {paths:?}"
            );
        }
    }

    #[test]
    fn test_labels_exclude_all_is_policies_only() {
        // No other Fleet context defines this key: software items use the
        // strict 3-way set, profiles/scripts only exclude_any. The lone
        // `LabelsExcludeAll` mention in mdm.go:803 is a stale doc comment on
        // a field actually named LabelsExcludeAny.
        let reg = &*KEY_REGISTRY;
        assert_eq!(
            reg.lookup("labels_exclude_all"),
            Some(["policies[]"].as_slice()),
            "labels_exclude_all is policies-only"
        );
    }

    #[test]
    fn test_fleet_gitops_keys_present() {
        // Core Fleet GitOps YAML keys that must always be in the schema.
        // Source: fleet/pkg/spec/gitops.go + fleet/server/fleet/*.go
        let reg = &*KEY_REGISTRY;
        let required_keys = [
            // Top-level
            "policies",
            "reports",
            "queries",
            "labels",
            "controls",
            "software",
            "agent_options",
            "org_settings",
            "settings",
            "team_settings",
            "name",
            // Policy fields
            "query",
            "description",
            "resolution",
            "platform",
            "critical",
            "calendar_events_enabled",
            "software_title_id",
            "script_id",
            // Report/query fields
            "interval",
            "logging",
            "observer_can_run",
            "automations_enabled",
            "discard_data",
            "min_osquery_version",
            // Label fields
            "label_membership_type",
            "hosts",
            // Controls
            "enable_disk_encryption",
            "macos_updates",
            "ios_updates",
            "ipados_updates",
            "windows_updates",
            "scripts",
            "macos_settings",
            "apple_settings",
            "windows_settings",
            "android_settings",
            "custom_settings",
            "configuration_profiles",
            // Software
            "packages",
            "fleet_maintained_apps",
            "app_store_apps",
            // org_settings sections
            "features",
            "fleet_desktop",
            "host_expiry_settings",
            "server_settings",
            "sso_settings",
            "smtp_settings",
            "webhook_settings",
            "vulnerability_settings",
            "activity_expiry_settings",
            // Deprecation targets
            "path",
            "paths",
        ];

        for key in &required_keys {
            assert!(
                reg.lookup(key).is_some(),
                "Required Fleet GitOps key '{key}' missing from KEY_REGISTRY"
            );
        }
    }

    #[test]
    fn test_controls_schema_structure() {
        let schema = default_schema();
        let controls = schema.get_child("controls").unwrap();
        assert!(controls.get_child("scripts").is_some());
        assert!(controls.get_child("macos_settings").is_some());

        let macos = controls.get_child("macos_settings").unwrap();
        assert!(macos.get_child("custom_settings").is_some());
        // scripts should NOT be valid under macos_settings
        assert!(macos.get_child("scripts").is_none());
    }
}
