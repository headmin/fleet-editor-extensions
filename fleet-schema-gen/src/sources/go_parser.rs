use anyhow::{anyhow, Context, Result};
use indexmap::IndexMap;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, Query, QueryCursor};

use crate::schema::types::{SchemaDefinition, SchemaProperty, SchemaType, AdditionalProperties};

/// Represents a parsed Go struct
#[derive(Debug, Clone)]
pub struct GoStruct {
    pub name: String,
    pub fields: Vec<GoField>,
    pub doc_comment: Option<String>,
}

/// Represents a field in a Go struct
#[derive(Debug, Clone)]
pub struct GoField {
    pub name: String,
    pub go_type: String,
    pub json_tag: Option<String>,
    pub yaml_tag: Option<String>,
    pub omitempty: bool,
    pub doc_comment: Option<String>,
}

/// Main Go parser for Fleet source code
pub struct FleetGoParser {
    parser: Parser,
    /// Maps struct names to their definitions
    struct_cache: HashMap<String, GoStruct>,
}

impl FleetGoParser {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        let language = tree_sitter_go::language();
        parser
            .set_language(language)
            .context("Failed to set Go language for tree-sitter")?;

        Ok(Self {
            parser,
            struct_cache: HashMap::new(),
        })
    }

    /// Parse Fleet repository and extract schema definitions
    pub fn parse_fleet_repo(&mut self, fleet_repo_path: &Path) -> Result<SchemaDefinition> {
        println!("  → Parsing Fleet Go source code...");

        // Key files containing GitOps struct definitions
        let files_to_parse = vec![
            "pkg/spec/gitops.go",
            "server/fleet/teams.go",
            "server/fleet/policies.go",
            "server/fleet/queries.go",
            "server/fleet/labels.go",
            "server/fleet/software.go",
        ];

        for file_path in files_to_parse {
            let full_path = fleet_repo_path.join(file_path);
            if full_path.exists() {
                self.parse_file(&full_path)
                    .with_context(|| format!("Failed to parse {}", file_path))?;
            } else {
                eprintln!("  ⚠ File not found: {}", file_path);
            }
        }

        println!("  → Parsed {} struct definitions", self.struct_cache.len());

        // Build schema from parsed structs
        self.build_team_schema()
    }

    /// Parse a single Go file
    fn parse_file(&mut self, file_path: &Path) -> Result<()> {
        let source = fs::read_to_string(file_path)?;
        let tree = self
            .parser
            .parse(&source, None)
            .ok_or_else(|| anyhow!("Failed to parse Go file"))?;

        let root_node = tree.root_node();

        // Find all struct definitions
        self.extract_structs(&root_node, &source)?;

        Ok(())
    }

    /// Extract all struct definitions from the AST
    fn extract_structs(&mut self, node: &Node, source: &str) -> Result<()> {
        let mut cursor = node.walk();

        // Look for type declarations
        for child in node.children(&mut cursor) {
            if child.kind() == "type_declaration" {
                if let Some(go_struct) = self.parse_type_declaration(&child, source)? {
                    self.struct_cache.insert(go_struct.name.clone(), go_struct);
                }
            }

            // Recursively process children
            self.extract_structs(&child, source)?;
        }

        Ok(())
    }

    /// Parse a type declaration node
    fn parse_type_declaration(&self, node: &Node, source: &str) -> Result<Option<GoStruct>> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();

        // Find the type_spec node
        let type_spec = children
            .iter()
            .find(|n| n.kind() == "type_spec")
            .ok_or_else(|| anyhow!("No type_spec found"))?;

        // Get struct name
        let mut spec_cursor = type_spec.walk();
        let spec_children: Vec<_> = type_spec.children(&mut spec_cursor).collect();

        let name_node = spec_children
            .iter()
            .find(|n| n.kind() == "type_identifier")
            .ok_or_else(|| anyhow!("No type_identifier found"))?;

        let struct_name = name_node.utf8_text(source.as_bytes())?;

        // Check if it's a struct type
        let struct_type = spec_children
            .iter()
            .find(|n| n.kind() == "struct_type");

        if let Some(struct_node) = struct_type {
            let fields = self.parse_struct_fields(struct_node, source)?;
            let doc_comment = self.extract_doc_comment(node, source);

            return Ok(Some(GoStruct {
                name: struct_name.to_string(),
                fields,
                doc_comment,
            }));
        }

        Ok(None)
    }

    /// Parse struct fields
    fn parse_struct_fields(&self, struct_node: &Node, source: &str) -> Result<Vec<GoField>> {
        let mut fields = Vec::new();
        let mut cursor = struct_node.walk();

        for child in struct_node.children(&mut cursor) {
            if child.kind() == "field_declaration_list" {
                let mut field_cursor = child.walk();
                for field_decl in child.children(&mut field_cursor) {
                    if field_decl.kind() == "field_declaration" {
                        if let Some(field) = self.parse_field(&field_decl, source)? {
                            fields.push(field);
                        }
                    }
                }
            }
        }

        Ok(fields)
    }

    /// Parse a single field declaration
    fn parse_field(&self, field_node: &Node, source: &str) -> Result<Option<GoField>> {
        let mut cursor = field_node.walk();
        let children: Vec<_> = field_node.children(&mut cursor).collect();

        // Get field name
        let name_node = children.iter().find(|n| n.kind() == "field_identifier");
        if name_node.is_none() {
            // Anonymous field (embedded struct)
            return Ok(None);
        }
        let field_name = name_node.unwrap().utf8_text(source.as_bytes())?;

        // Get field type
        let type_node = children
            .iter()
            .find(|n| {
                matches!(
                    n.kind(),
                    "type_identifier"
                        | "pointer_type"
                        | "slice_type"
                        | "map_type"
                        | "qualified_type"
                        | "interface_type"
                )
            })
            .ok_or_else(|| anyhow!("No type found for field {}", field_name))?;

        let go_type = type_node.utf8_text(source.as_bytes())?;

        // Get struct tags
        let tag_node = children.iter().find(|n| n.kind() == "raw_string_literal");
        let (json_tag, yaml_tag, omitempty) = if let Some(tag) = tag_node {
            let tag_str = tag.utf8_text(source.as_bytes())?;
            self.parse_struct_tags(tag_str)?
        } else {
            (None, None, false)
        };

        // Get doc comment
        let doc_comment = self.extract_doc_comment(field_node, source);

        Ok(Some(GoField {
            name: field_name.to_string(),
            go_type: go_type.to_string(),
            json_tag,
            yaml_tag,
            omitempty,
            doc_comment,
        }))
    }

    /// Parse struct tags (e.g., json:"name,omitempty" yaml:"name")
    fn parse_struct_tags(&self, tag_str: &str) -> Result<(Option<String>, Option<String>, bool)> {
        let tag_re = Regex::new(r#"(?:json|yaml):"([^"]+)""#)?;

        let mut json_tag = None;
        let mut yaml_tag = None;
        let mut omitempty = false;

        for cap in tag_re.captures_iter(tag_str) {
            let tag_content = cap.get(1).map(|m| m.as_str()).unwrap_or("");
            let parts: Vec<&str> = tag_content.split(',').collect();

            if tag_str.contains("json:") && json_tag.is_none() {
                json_tag = Some(parts[0].to_string());
                if parts.contains(&"omitempty") {
                    omitempty = true;
                }
            } else if tag_str.contains("yaml:") && yaml_tag.is_none() {
                yaml_tag = Some(parts[0].to_string());
                if parts.contains(&"omitempty") {
                    omitempty = true;
                }
            }
        }

        Ok((json_tag, yaml_tag, omitempty))
    }

    /// Extract doc comment from preceding comment nodes
    fn extract_doc_comment(&self, node: &Node, source: &str) -> Option<String> {
        // Look for comment nodes before this node
        if let Some(prev_sibling) = node.prev_sibling() {
            if prev_sibling.kind() == "comment" {
                if let Ok(comment_text) = prev_sibling.utf8_text(source.as_bytes()) {
                    // Remove // or /* */ and trim
                    let cleaned = comment_text
                        .trim_start_matches("//")
                        .trim_start_matches("/*")
                        .trim_end_matches("*/")
                        .trim();
                    return Some(cleaned.to_string());
                }
            }
        }
        None
    }

    /// Build team schema from parsed structs
    fn build_team_schema(&self) -> Result<SchemaDefinition> {
        // Find the GitOps struct
        let gitops_struct = self
            .struct_cache
            .get("GitOps")
            .ok_or_else(|| anyhow!("GitOps struct not found"))?;

        let mut properties = IndexMap::new();

        for field in &gitops_struct.fields {
            // Use json_tag if available, otherwise use field name
            let property_name = field
                .json_tag
                .as_ref()
                .unwrap_or(&field.name)
                .to_lowercase();

            // Skip fields with "-" tag (means don't serialize)
            if property_name == "-" {
                continue;
            }

            let schema_prop = self.convert_go_type_to_schema(&field.go_type, &field)?;
            properties.insert(property_name, schema_prop);
        }

        Ok(SchemaDefinition {
            schema: Some("https://json-schema.org/draft-07/schema#".to_string()),
            title: Some("Fleet Team Configuration (from Go source)".to_string()),
            description: Some("Schema generated from Fleet Go source code".to_string()),
            type_: Some(SchemaType::Single("object".to_string())),
            properties: Some(properties),
            additional_properties: Some(AdditionalProperties::Boolean(false)),
            ..Default::default()
        })
    }

    /// Convert Go type to JSON Schema property
    fn convert_go_type_to_schema(&self, go_type: &str, field: &GoField) -> Result<SchemaProperty> {
        let mut prop = SchemaProperty {
            description: field.doc_comment.clone(),
            ..Default::default()
        };

        // Handle pointer types
        let clean_type = go_type.trim_start_matches('*');

        // Map Go types to JSON Schema types
        match clean_type {
            "string" => {
                prop.type_ = Some(SchemaType::Single("string".to_string()));
            }
            "bool" => {
                prop.type_ = Some(SchemaType::Single("boolean".to_string()));
            }
            "int" | "int32" | "int64" | "uint" | "uint32" | "uint64" => {
                prop.type_ = Some(SchemaType::Single("integer".to_string()));
            }
            "float32" | "float64" => {
                prop.type_ = Some(SchemaType::Single("number".to_string()));
            }
            t if t.starts_with("[]") => {
                // Array type
                prop.type_ = Some(SchemaType::Single("array".to_string()));
                // TODO: Parse array item type and set items
            }
            t if t.starts_with("map[") => {
                // Map type - represents as object
                prop.type_ = Some(SchemaType::Single("object".to_string()));
                prop.additional_properties = Some(AdditionalProperties::Boolean(true));
            }
            "interface{}" | "interface {}" => {
                // Any type - don't constrain
                prop.type_ = None;
            }
            "json.RawMessage" => {
                // Raw JSON - could be anything
                prop.type_ = None;
            }
            _ => {
                // Custom struct type - make it an object
                prop.type_ = Some(SchemaType::Single("object".to_string()));

                // If we have this struct in cache, expand its properties
                if let Some(nested_struct) = self.struct_cache.get(clean_type) {
                    let nested_props = self.expand_struct_properties(nested_struct)?;
                    if !nested_props.is_empty() {
                        prop.properties = Some(nested_props);
                    }
                }
            }
        }

        Ok(prop)
    }

    /// Expand struct properties recursively
    fn expand_struct_properties(&self, go_struct: &GoStruct) -> Result<IndexMap<String, SchemaProperty>> {
        let mut properties = IndexMap::new();

        for field in &go_struct.fields {
            let property_name = field
                .json_tag
                .as_ref()
                .unwrap_or(&field.name)
                .to_lowercase();

            if property_name == "-" {
                continue;
            }

            let schema_prop = self.convert_go_type_to_schema(&field.go_type, field)?;
            properties.insert(property_name, schema_prop);
        }

        Ok(properties)
    }
}

/// Fetch Fleet repository from GitHub and parse schemas
pub async fn fetch_from_fleet_repo(version: &str) -> Result<SchemaDefinition> {
    println!("  → Cloning/updating Fleet repository...");

    // TODO: Clone Fleet repo if not exists, or update if exists
    // For now, assume user has Fleet repo locally
    let fleet_repo_path = std::env::var("FLEET_REPO_PATH")
        .unwrap_or_else(|_| "/tmp/fleet".to_string());

    let mut parser = FleetGoParser::new()?;
    parser.parse_fleet_repo(Path::new(&fleet_repo_path))
}
