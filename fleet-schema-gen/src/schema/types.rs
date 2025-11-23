use serde::{Deserialize, Serialize};
use indexmap::IndexMap;

/// Internal representation of a JSON Schema
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchemaDefinition {
    #[serde(rename = "$schema", skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<SchemaType>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<IndexMap<String, SchemaProperty>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,

    #[serde(rename = "additionalProperties", skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<AdditionalProperties>,

    #[serde(rename = "$defs", skip_serializing_if = "Option::is_none")]
    pub defs: Option<IndexMap<String, SchemaDefinition>>,

    #[serde(rename = "$ref", skip_serializing_if = "Option::is_none")]
    pub ref_: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<SchemaDefinition>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_: Option<Vec<serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<serde_json::Value>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    #[serde(rename = "oneOf", skip_serializing_if = "Option::is_none")]
    pub one_of: Option<Vec<SchemaDefinition>>,

    #[serde(rename = "anyOf", skip_serializing_if = "Option::is_none")]
    pub any_of: Option<Vec<SchemaDefinition>>,

    #[serde(rename = "defaultSnippets", skip_serializing_if = "Option::is_none")]
    pub default_snippets: Option<Vec<DefaultSnippet>>,
}

/// VSCode YAML extension defaultSnippet for autocomplete
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultSnippet {
    pub label: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    pub body: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SchemaType {
    Single(String),
    Multiple(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AdditionalProperties {
    Boolean(bool),
    Schema(Box<SchemaDefinition>),
}

pub type SchemaProperty = SchemaDefinition;

/// Fleet-specific schema metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetSchema {
    pub version: String,
    pub default_schema: SchemaDefinition,
    pub team_schema: SchemaDefinition,
    pub policy_schema: SchemaDefinition,
    pub query_schema: SchemaDefinition,
    pub label_schema: SchemaDefinition,
    pub metadata: SchemaMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMetadata {
    pub generated_at: String,
    pub fleet_version: String,
    pub sources: Vec<String>,
}

/// YAML definition for manual enhancements
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlEnhancement {
    pub fields: Option<IndexMap<String, FieldEnhancement>>,
    pub nested: Option<IndexMap<String, YamlEnhancement>>,

    #[serde(rename = "defaultSnippets", skip_serializing_if = "Option::is_none")]
    pub default_snippets: Option<Vec<DefaultSnippet>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldEnhancement {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub examples: Option<Vec<serde_json::Value>>,

    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_: Option<Vec<String>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub vscode_hint: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sublime_hint: Option<String>,

    #[serde(rename = "defaultSnippets", skip_serializing_if = "Option::is_none")]
    pub default_snippets: Option<Vec<DefaultSnippet>>,
}
