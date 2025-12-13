//! Hover provider for Fleet GitOps YAML files.
//!
//! Provides rich documentation when hovering over field names and values.

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position, Range};

use super::schema::{get_field_doc, get_logging_doc, get_platform_doc, FIELD_DOCS};
use crate::linter::osquery::OSQUERY_TABLES;

/// Provide hover information at a position in a Fleet YAML document.
pub fn hover_at(source: &str, position: Position) -> Option<Hover> {
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    // Get the line content
    let line = source.lines().nth(line_idx)?;

    // Find the word at the cursor position
    let (word, word_start, word_end) = find_word_at(line, col_idx)?;

    // Determine context from line content and build appropriate hover
    let hover_content = determine_hover_content(source, line_idx, line, &word)?;

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover_content,
        }),
        range: Some(Range {
            start: Position {
                line: position.line,
                character: word_start as u32,
            },
            end: Position {
                line: position.line,
                character: word_end as u32,
            },
        }),
    })
}

/// Find the word at a given column position in a line.
/// Returns (word, start_col, end_col).
fn find_word_at(line: &str, col: usize) -> Option<(String, usize, usize)> {
    if col >= line.len() && !line.is_empty() {
        // Cursor is past end of line, try to get last word
        return find_word_at(line, line.len().saturating_sub(1));
    }

    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() {
        return None;
    }

    // Clamp col to valid range
    let col = col.min(chars.len().saturating_sub(1));

    // Find word boundaries (alphanumeric + underscore)
    let is_word_char = |c: char| c.is_alphanumeric() || c == '_';

    // If we're not on a word character, look for nearby words
    if !is_word_char(chars[col]) {
        // Check if we're on a colon (key:) - look left
        if chars[col] == ':' && col > 0 {
            return find_word_at(line, col - 1);
        }
        // Check right
        if col + 1 < chars.len() && is_word_char(chars[col + 1]) {
            return find_word_at(line, col + 1);
        }
        // Check left
        if col > 0 && is_word_char(chars[col - 1]) {
            return find_word_at(line, col - 1);
        }
        return None;
    }

    // Find start of word
    let mut start = col;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }

    // Find end of word
    let mut end = col;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    let word: String = chars[start..end].iter().collect();
    if word.is_empty() {
        return None;
    }

    Some((word, start, end))
}

/// Determine the hover content based on context.
fn determine_hover_content(source: &str, line_idx: usize, line: &str, word: &str) -> Option<String> {
    // Determine context by looking at surrounding lines
    let context = determine_full_yaml_context(source, line_idx);

    // Check if this is a YAML key (followed by colon)
    let is_key = line.contains(&format!("{}:", word));

    // Check if this is a value after a colon
    let is_value = is_value_context(line, word);

    if is_key {
        // This is a field name - look up field documentation with full context path
        let field_path = format!("{}.{}", context, word);
        if let Some(doc) = get_field_doc(&field_path) {
            return Some(doc.to_markdown());
        }

        // Try with simpler context (e.g., "software" instead of "software.packages")
        let simple_context = determine_yaml_context(source, line_idx);
        let simple_path = format!("{}.{}", simple_context, word);
        if let Some(doc) = get_field_doc(&simple_path) {
            return Some(doc.to_markdown());
        }

        // Try without context prefix
        if let Some(doc) = get_field_doc(word) {
            return Some(doc.to_markdown());
        }
    }

    if is_value {
        // Check what key this value belongs to
        let key = extract_key_from_line(line);

        match key.as_deref() {
            Some("platform") => {
                if let Some(desc) = get_platform_doc(word) {
                    return Some(format!("**{}**\n\n{}", word, desc));
                }
            }
            Some("logging") => {
                if let Some(desc) = get_logging_doc(word) {
                    return Some(format!("**{}**\n\n{}", word, desc));
                }
            }
            _ => {}
        }
    }

    // Check if it might be an osquery table name (in SQL context)
    if is_sql_context(source, line_idx, line) {
        if let Some(table_info) = OSQUERY_TABLES.get(word) {
            let platforms = table_info.platforms.join(", ");
            return Some(format!(
                "**{}** (osquery table)\n\n{}\n\n**Platforms:** {}",
                word, table_info.description, platforms
            ));
        }
    }

    // Fallback: try to find any matching field doc
    for (path, doc) in FIELD_DOCS.iter() {
        if path.ends_with(word) {
            return Some(doc.to_markdown());
        }
    }

    None
}

/// Determine the YAML context (policies, queries, labels, etc.) at a line.
fn determine_yaml_context(source: &str, line_idx: usize) -> &'static str {
    let lines: Vec<&str> = source.lines().collect();

    // Look backwards for context-defining lines
    for i in (0..=line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim();

        // Check for top-level array keys
        if trimmed.starts_with("policies:") || trimmed == "policies:" {
            return "policies";
        }
        if trimmed.starts_with("queries:") || trimmed == "queries:" {
            return "queries";
        }
        if trimmed.starts_with("labels:") || trimmed == "labels:" {
            return "labels";
        }
        if trimmed.starts_with("controls:") || trimmed == "controls:" {
            return "controls";
        }
        if trimmed.starts_with("software:") || trimmed == "software:" {
            return "software";
        }
        if trimmed.starts_with("agent_options:") || trimmed == "agent_options:" {
            return "agent_options";
        }
    }

    "root"
}

/// Determine the full YAML context path (e.g., "software.packages") at a line.
fn determine_full_yaml_context(source: &str, line_idx: usize) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut path_parts: Vec<&str> = Vec::new();
    let mut last_indent: i32 = -1;

    // Get the indentation of the current line
    let current_line = lines.get(line_idx).unwrap_or(&"");
    let current_indent = current_line.len() - current_line.trim_start().len();

    // Look backwards and build path based on decreasing indentation
    for i in (0..line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let indent = (line.len() - line.trim_start().len()) as i32;

        // Only consider lines with less indentation than current
        if indent < current_indent as i32 && (last_indent == -1 || indent < last_indent) {
            // Extract key if this line has one
            if let Some(key) = extract_key_from_yaml_line(trimmed) {
                path_parts.push(key);
                last_indent = indent;

                // Stop at root level
                if indent == 0 {
                    break;
                }
            }
        }
    }

    // Reverse to get root-to-leaf order
    path_parts.reverse();
    path_parts.join(".")
}

/// Extract the key from a YAML line (handles both "key:" and "- key:" formats).
fn extract_key_from_yaml_line(line: &str) -> Option<&str> {
    let trimmed = line.trim().trim_start_matches('-').trim();
    if let Some(colon_pos) = trimmed.find(':') {
        let key = trimmed[..colon_pos].trim();
        if !key.is_empty() && !key.contains(' ') {
            return Some(key);
        }
    }
    None
}

/// Check if we're in an SQL context (inside a query field).
fn is_sql_context(source: &str, line_idx: usize, current_line: &str) -> bool {
    // Check if current line contains query:
    if current_line.contains("query:") {
        return true;
    }

    let lines: Vec<&str> = source.lines().collect();

    // Look backwards for a query: field with multiline indicator
    for i in (0..line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim();

        // Found query with multiline indicator
        if trimmed.starts_with("query:") && trimmed.contains("|") {
            return true;
        }

        // Found another key at same or less indentation - not in query
        if trimmed.ends_with(':') && !trimmed.starts_with('-') {
            // Check indentation
            let current_indent = current_line.len() - current_line.trim_start().len();
            let check_indent = line.len() - line.trim_start().len();
            if check_indent <= current_indent && !trimmed.starts_with("query:") {
                return false;
            }
        }
    }

    false
}

/// Check if the word is in a value position (after a colon).
fn is_value_context(line: &str, word: &str) -> bool {
    // Look for pattern "key: value" where word is the value
    if let Some(colon_pos) = line.find(':') {
        let after_colon = &line[colon_pos + 1..];
        // Word should appear after the colon
        if after_colon.contains(word) {
            return true;
        }
    }
    false
}

/// Extract the key name from a line (the part before the colon).
fn extract_key_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim().trim_start_matches('-').trim();
    if let Some(colon_pos) = trimmed.find(':') {
        let key = trimmed[..colon_pos].trim();
        if !key.is_empty() {
            return Some(key.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_word_at() {
        let line = "  platform: darwin";
        let (word, start, end) = find_word_at(line, 4).unwrap();
        assert_eq!(word, "platform");
        assert_eq!(start, 2);
        assert_eq!(end, 10);

        let (word, _, _) = find_word_at(line, 14).unwrap();
        assert_eq!(word, "darwin");
    }

    #[test]
    fn test_find_word_on_colon() {
        let line = "  platform:";
        let (word, _, _) = find_word_at(line, 10).unwrap(); // on the colon
        assert_eq!(word, "platform");
    }

    #[test]
    fn test_determine_yaml_context() {
        let source = "policies:\n  - name: test\n    platform: darwin";
        assert_eq!(determine_yaml_context(source, 1), "policies");
        assert_eq!(determine_yaml_context(source, 2), "policies");
    }

    #[test]
    fn test_is_sql_context() {
        let source = "policies:\n  - name: test\n    query: |\n      SELECT * FROM processes";
        assert!(is_sql_context(source, 3, "      SELECT * FROM processes"));
    }

    #[test]
    fn test_hover_platform_field() {
        let source = "policies:\n  - name: test\n    platform: darwin";
        let hover = hover_at(source, Position { line: 2, character: 6 });
        assert!(hover.is_some());
        let content = match hover.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("Expected markup content"),
        };
        assert!(content.contains("platform"));
    }

    #[test]
    fn test_hover_platform_value() {
        let source = "policies:\n  - name: test\n    platform: darwin";
        let hover = hover_at(source, Position { line: 2, character: 16 });
        assert!(hover.is_some());
        let content = match hover.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("Expected markup content"),
        };
        assert!(content.contains("darwin") || content.contains("macOS"));
    }

    #[test]
    fn test_hover_osquery_table() {
        let source = "policies:\n  - name: test\n    query: SELECT * FROM processes";
        let hover = hover_at(source, Position { line: 2, character: 30 });
        assert!(hover.is_some());
        let content = match hover.unwrap().contents {
            HoverContents::Markup(m) => m.value,
            _ => panic!("Expected markup content"),
        };
        assert!(content.contains("processes") || content.contains("osquery"));
    }

    #[test]
    fn test_extract_key_from_line() {
        assert_eq!(extract_key_from_line("  platform: darwin"), Some("platform".to_string()));
        assert_eq!(extract_key_from_line("- name: test"), Some("name".to_string()));
        assert_eq!(extract_key_from_line("  - query: SELECT 1"), Some("query".to_string()));
    }
}
