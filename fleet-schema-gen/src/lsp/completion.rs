//! Completion provider for Fleet GitOps YAML files.
//!
//! Provides context-aware autocompletion for field names, values, and osquery tables.

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, Position,
};

use super::schema::{get_field_doc, LOGGING_DOCS, PLATFORM_DOCS};
use crate::linter::osquery::OSQUERY_TABLES;

/// Context types for completion.
#[derive(Debug, Clone, PartialEq)]
enum CompletionContext {
    /// At top level of document
    TopLevel,
    /// Inside a policies array item
    PolicyField,
    /// Inside a queries array item
    QueryField,
    /// Inside a labels array item
    LabelField,
    /// After platform: key
    PlatformValue,
    /// After logging: key
    LoggingValue,
    /// Inside an SQL query (for osquery tables)
    SqlContext { platform: Option<String> },
    /// Unknown context
    Unknown,
}

/// Provide completion items at a position in a Fleet YAML document.
pub fn complete_at(source: &str, position: Position) -> Vec<CompletionItem> {
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    // Get the line content (empty string if no line at that position)
    let line = source.lines().nth(line_idx).unwrap_or("");

    // Determine the context
    let context = determine_completion_context(source, line_idx, line, col_idx);

    match context {
        CompletionContext::TopLevel => complete_top_level_fields(),
        CompletionContext::PolicyField => complete_policy_fields(line, col_idx),
        CompletionContext::QueryField => complete_query_fields(line, col_idx),
        CompletionContext::LabelField => complete_label_fields(line, col_idx),
        CompletionContext::PlatformValue => complete_platform_values(),
        CompletionContext::LoggingValue => complete_logging_values(),
        CompletionContext::SqlContext { platform } => complete_osquery_tables(platform.as_deref()),
        CompletionContext::Unknown => vec![],
    }
}

/// Determine the completion context based on cursor position and surrounding content.
fn determine_completion_context(
    source: &str,
    line_idx: usize,
    line: &str,
    col_idx: usize,
) -> CompletionContext {
    let trimmed = line.trim();

    // Empty document or at start - suggest top-level
    if source.trim().is_empty() || (line_idx == 0 && trimmed.is_empty()) {
        return CompletionContext::TopLevel;
    }

    // Check if we're after a specific key (value position)
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return CompletionContext::PlatformValue,
            "logging" => return CompletionContext::LoggingValue,
            _ => {}
        }
    }

    // Check if we're in SQL context (inside a query field)
    if is_in_sql_context(source, line_idx, line) {
        let platform = find_platform_in_context(source, line_idx);
        return CompletionContext::SqlContext { platform };
    }

    // Determine parent context by looking at indentation and surrounding lines
    let indent = line.len() - line.trim_start().len();

    // If indent is 0 or we're at a top-level key position, suggest top-level fields
    if indent == 0 && (trimmed.is_empty() || !trimmed.contains(':')) {
        return CompletionContext::TopLevel;
    }

    // Look for parent context
    let parent = find_parent_context(source, line_idx);
    match parent.as_deref() {
        Some("policies") => CompletionContext::PolicyField,
        Some("queries") => CompletionContext::QueryField,
        Some("labels") => CompletionContext::LabelField,
        _ => {
            // Check if we're at a position that suggests top-level
            if indent <= 2 && (trimmed.is_empty() || trimmed.starts_with('-')) {
                return find_array_parent(source, line_idx);
            }
            CompletionContext::Unknown
        }
    }
}

/// Get the key if cursor is in a value position (after colon).
fn get_key_at_cursor(line: &str, col_idx: usize) -> Option<String> {
    let trimmed = line.trim().trim_start_matches('-').trim();
    if let Some(colon_pos) = line.find(':') {
        // Cursor is after the colon
        if col_idx > colon_pos {
            let key = trimmed.split(':').next()?.trim();
            return Some(key.to_string());
        }
    }
    None
}

/// Check if we're in an SQL context.
fn is_in_sql_context(source: &str, line_idx: usize, current_line: &str) -> bool {
    // Check if current line is part of a multiline query
    if current_line.trim().starts_with("SELECT")
        || current_line.trim().starts_with("FROM")
        || current_line.trim().starts_with("WHERE")
        || current_line.trim().starts_with("JOIN")
    {
        return true;
    }

    let lines: Vec<&str> = source.lines().collect();

    // Look for query: | indicator above
    for i in (0..line_idx).rev() {
        let check_line = lines.get(i).unwrap_or(&"");
        let trimmed = check_line.trim();

        if trimmed.starts_with("query:") && trimmed.contains('|') {
            return true;
        }

        // Found another key at same or less indent - not in query
        if trimmed.ends_with(':')
            && !trimmed.starts_with('-')
            && !trimmed.starts_with("query:")
        {
            let current_indent = current_line.len() - current_line.trim_start().len();
            let check_indent = check_line.len() - check_line.trim_start().len();
            if check_indent <= current_indent {
                return false;
            }
        }
    }

    false
}

/// Find the platform value in the current context (for filtering osquery tables).
fn find_platform_in_context(source: &str, line_idx: usize) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let current_indent = lines
        .get(line_idx)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    // Look backwards for platform: field at same or parent level
    for i in (0..=line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim().trim_start_matches('-').trim();

        if trimmed.starts_with("platform:") {
            let indent = line.len() - line.trim_start().len();
            if indent <= current_indent {
                let value = trimmed.strip_prefix("platform:")?.trim();
                return Some(value.to_string());
            }
        }

        // If we hit a new array item at parent level, stop looking
        if line.trim().starts_with("- name:") {
            let indent = line.len() - line.trim_start().len();
            if indent < current_indent {
                break;
            }
        }
    }

    None
}

/// Find the parent array context.
fn find_parent_context(source: &str, line_idx: usize) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();

    for i in (0..=line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim();

        // Check for top-level array keys
        if trimmed == "policies:" || trimmed.starts_with("policies:") {
            return Some("policies".to_string());
        }
        if trimmed == "queries:" || trimmed.starts_with("queries:") {
            return Some("queries".to_string());
        }
        if trimmed == "labels:" || trimmed.starts_with("labels:") {
            return Some("labels".to_string());
        }
    }

    None
}

/// Find the array parent for completing array item fields.
fn find_array_parent(source: &str, line_idx: usize) -> CompletionContext {
    match find_parent_context(source, line_idx).as_deref() {
        Some("policies") => CompletionContext::PolicyField,
        Some("queries") => CompletionContext::QueryField,
        Some("labels") => CompletionContext::LabelField,
        _ => CompletionContext::Unknown,
    }
}

/// Complete top-level field names.
fn complete_top_level_fields() -> Vec<CompletionItem> {
    let fields = [
        ("name", "Team or configuration name"),
        ("policies", "List of compliance policies"),
        ("queries", "List of osquery queries"),
        ("labels", "List of host labels"),
        ("agent_options", "osquery agent configuration"),
        ("controls", "MDM controls and settings"),
        ("software", "Software packages to install"),
        ("webhook_settings", "Webhook notification configuration"),
    ];

    fields
        .iter()
        .map(|(name, desc)| create_field_completion(name, desc, true))
        .collect()
}

/// Complete policy field names.
fn complete_policy_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in value position
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return complete_platform_values(),
            _ => {}
        }
    }

    let fields = [
        ("name", "Policy display name", true),
        ("description", "What this policy checks", false),
        ("query", "osquery SQL query", true),
        ("platform", "Target operating system", false),
        ("critical", "Whether policy is critical", false),
        ("resolution", "How to fix policy failures", false),
        ("team", "Team this policy belongs to", false),
        ("calendar_events_enabled", "Create calendar reminders", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete query field names.
fn complete_query_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in value position
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return complete_platform_values(),
            "logging" => return complete_logging_values(),
            _ => {}
        }
    }

    let fields = [
        ("name", "Query display name", true),
        ("description", "What this query collects", false),
        ("query", "osquery SQL query", true),
        ("interval", "How often to run (seconds)", false),
        ("platform", "Target operating system", false),
        ("logging", "How results are logged", false),
        ("min_osquery_version", "Minimum osquery version", false),
        ("observer_can_run", "Allow observers to run", false),
        ("automations_enabled", "Enable automations", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete label field names.
fn complete_label_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in value position
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return complete_platform_values(),
            "label_membership_type" => {
                return vec![
                    create_value_completion("dynamic", "Membership via query"),
                    create_value_completion("manual", "Explicit host assignment"),
                ];
            }
            _ => {}
        }
    }

    let fields = [
        ("name", "Label display name", true),
        ("description", "What hosts this label identifies", false),
        ("query", "osquery query for dynamic labels", false),
        ("platform", "Target operating system", false),
        ("label_membership_type", "dynamic or manual", false),
        ("hosts", "List of hosts (manual labels)", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete platform values.
fn complete_platform_values() -> Vec<CompletionItem> {
    PLATFORM_DOCS
        .iter()
        .map(|(platform, desc)| create_value_completion(platform, desc))
        .collect()
}

/// Complete logging type values.
fn complete_logging_values() -> Vec<CompletionItem> {
    LOGGING_DOCS
        .iter()
        .map(|(logging, desc)| create_value_completion(logging, desc))
        .collect()
}

/// Complete osquery table names, optionally filtered by platform.
fn complete_osquery_tables(platform: Option<&str>) -> Vec<CompletionItem> {
    OSQUERY_TABLES
        .iter()
        .filter(|(_, info)| {
            platform
                .map(|p| p == "all" || info.platforms.contains(&p))
                .unwrap_or(true)
        })
        .map(|(name, info)| {
            let platforms = info.platforms.join(", ");
            CompletionItem {
                label: (*name).to_string(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some(format!("osquery table ({})", platforms)),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**{}**\n\n{}\n\n**Platforms:** {}", name, info.description, platforms),
                })),
                ..Default::default()
            }
        })
        .collect()
}

/// Create a completion item for a field name.
fn create_field_completion(name: &str, description: &str, required: bool) -> CompletionItem {
    let detail = if required {
        format!("{} (required)", description)
    } else {
        description.to_string()
    };

    // Get richer documentation from schema if available
    let documentation = get_field_doc(name).map(|doc| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc.to_markdown(),
        })
    });

    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(detail),
        documentation,
        insert_text: Some(format!("{}: ", name)),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..Default::default()
    }
}

/// Create a completion item for a value.
fn create_value_completion(value: &str, description: &str) -> CompletionItem {
    CompletionItem {
        label: value.to_string(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some(description.to_string()),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_top_level() {
        let source = "";
        let completions = complete_at(source, Position { line: 0, character: 0 });
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"policies"));
        assert!(labels.contains(&"queries"));
        assert!(labels.contains(&"labels"));
    }

    #[test]
    fn test_complete_policy_fields() {
        let source = "policies:\n  - ";
        let completions = complete_at(source, Position { line: 1, character: 4 });
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"name"));
        assert!(labels.contains(&"query"));
        assert!(labels.contains(&"platform"));
    }

    #[test]
    fn test_complete_platform_values() {
        let source = "policies:\n  - name: test\n    platform: ";
        let completions = complete_at(source, Position { line: 2, character: 15 });

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"darwin"));
        assert!(labels.contains(&"windows"));
        assert!(labels.contains(&"linux"));
    }

    #[test]
    fn test_complete_osquery_tables() {
        let source = "policies:\n  - name: test\n    query: |\n      SELECT * FROM ";
        let completions = complete_at(source, Position { line: 3, character: 20 });

        // Should have osquery tables
        assert!(!completions.is_empty());
        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"processes"));
    }

    #[test]
    fn test_get_key_at_cursor() {
        assert_eq!(
            get_key_at_cursor("    platform: darwin", 15),
            Some("platform".to_string())
        );
        assert_eq!(
            get_key_at_cursor("  - name: test", 10),
            Some("name".to_string())
        );
        assert_eq!(get_key_at_cursor("    platform: ", 5), None); // cursor before colon
    }

    #[test]
    fn test_find_platform_in_context() {
        let source = "policies:\n  - name: test\n    platform: darwin\n    query: |";
        assert_eq!(
            find_platform_in_context(source, 3),
            Some("darwin".to_string())
        );
    }
}
