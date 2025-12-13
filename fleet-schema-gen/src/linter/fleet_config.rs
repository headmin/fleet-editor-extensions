use serde::{Deserialize, Serialize};

/// Fleet GitOps configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FleetConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub policies: Option<Vec<PolicyOrPath>>,

    #[serde(skip_serializing_if = "Option::is_none")]
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
/// NOTE: Path must come first in untagged enum for correct deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PolicyOrPath {
    Path { path: String },
    Policy(Policy),
}

/// Queries can be either inline definitions or path references
/// NOTE: Path must come first in untagged enum for correct deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QueryOrPath {
    Path { path: String },
    Query(Query),
}

/// Labels can be either inline definitions or path references
/// NOTE: Path must come first in untagged enum for correct deserialization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LabelOrPath {
    Path { path: String },
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
