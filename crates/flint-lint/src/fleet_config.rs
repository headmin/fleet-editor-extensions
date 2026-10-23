//! Fleet GitOps YAML deserialization types.
//!
//! Serde structs for parsing Fleet configuration files (policies, queries,
//! labels, software, agent options, controls). Used by rules that need
//! typed access to the configuration beyond raw YAML.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Fleet GitOps configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FleetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub policies: Option<Vec<PolicyOrPath>>,

    /// `reports:` is the name since Fleet 4.82 (`queries:` is the deprecated
    /// spelling). Without the alias every query rule — syntax, interval,
    /// length, platform — silently skipped repos that had migrated.
    #[serde(alias = "reports", skip_serializing_if = "Option::is_none")]
    pub queries: Option<Vec<QueryOrPath>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<LabelOrPath>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_options: Option<serde_yaml::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_settings: Option<WebhookSettings>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub integrations: Option<serde_yaml::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub macos_settings: Option<serde_yaml::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub windows_settings: Option<serde_yaml::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub controls: Option<serde_yaml::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub software: Option<serde_yaml::Value>,

    // Catch-all for unknown fields
    #[serde(flatten)]
    pub other: serde_yaml::Value,
}

/// Policies can be either inline definitions or path references
/// NOTE: Path variants must come first in untagged enum for correct deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
// Policy has many optional fields (~280 bytes); the size disparity between
// variants here is expected. Boxing would force every callsite to deref.
#[allow(clippy::large_enum_variant)]
pub enum PolicyOrPath {
    Path { path: String },
    Paths { paths: String }, // glob pattern (e.g., "../platforms/macos/policies/*.yml")
    Policy(Policy),
}

/// Queries can be either inline definitions or path references
/// NOTE: Path must come first in untagged enum for correct deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryOrPath {
    Path { path: String },
    Paths { paths: String },
    Query(Query),
}

/// Labels can be either inline definitions or path references
/// NOTE: Path variants must come first in untagged enum for correct deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LabelOrPath {
    Path { path: String },
    Paths { paths: String },
    Label(Label),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub critical: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub team: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub calendar_events_enabled: Option<bool>,

    /// Patch policies (`type: patch`) auto-generate their `query` from the
    /// Fleet-Maintained App metadata, so `query` is optional when set.
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub policy_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub fleet_maintained_app_slug: Option<String>,

    /// Pin a specific Fleet-Maintained App version for a patch policy.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Can be `true` (boolean, on patch policies, to install the FMA on fail)
    /// or an object with `package_path`, `hash_sha256`, `app_store_id`, or
    /// `fleet_maintained_app_slug` (PolicyInstallSoftware in gitops.go:231-236).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_software: Option<JsonValue>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_script: Option<RunScript>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunScript {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Query {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval: Option<i64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub logging: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_osquery_version: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub observer_can_run: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub automations_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub label_membership_type: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub hosts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_host_status_webhook: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_vulnerabilities_webhook: Option<bool>,
}

/// Software package definition (lib file format)
/// These are standalone files that define a software installer
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SoftwarePackage {
    /// URL to download the software package
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Whether this is self-service software
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_service: Option<bool>,

    /// Icon for the software package
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<SoftwareAsset>,

    /// Pre-install query to check before installing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_install_query: Option<SoftwareAsset>,

    /// Install script
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install_script: Option<SoftwareAsset>,

    /// Post-install script
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_install_script: Option<SoftwareAsset>,

    /// Uninstall script
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uninstall_script: Option<SoftwareAsset>,

    /// SHA256 hash of the package
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_sha256: Option<String>,

    /// Catch-all for unknown fields
    #[serde(flatten)]
    pub other: Option<serde_yaml::Value>,
}

/// Asset reference (path to a file)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareAsset {
    pub path: String,
}

/// Agent options lib file structure
/// These are standalone files that define agent options configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentOptionsLib {
    /// osquery configuration options
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<serde_yaml::Value>,

    /// Update channels for Fleet components (osqueryd, orbit, desktop)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_channels: Option<serde_yaml::Value>,

    /// Catch-all for unknown fields
    #[serde(flatten)]
    pub other: Option<serde_yaml::Value>,
}
