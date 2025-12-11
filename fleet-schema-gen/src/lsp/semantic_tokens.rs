//! Semantic tokens provider for Fleet GitOps YAML files.
//!
//! Provides enhanced syntax highlighting through semantic tokens,
//! allowing editors to distinguish between different element types.

use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensLegend,
};

/// Token types used by Fleet GitOps files.
/// The order here defines the token type index.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE,  // 0: Top-level sections (policies, queries, labels)
    SemanticTokenType::PROPERTY,   // 1: Field names (name, query, platform, etc.)
    SemanticTokenType::STRING,     // 2: String values
    SemanticTokenType::NUMBER,     // 3: Numeric values (interval, etc.)
    SemanticTokenType::KEYWORD,    // 4: SQL keywords (SELECT, FROM, WHERE, etc.)
    SemanticTokenType::ENUM_MEMBER, // 5: Enum values (darwin, windows, snapshot, etc.)
    SemanticTokenType::FUNCTION,   // 6: osquery table names
    SemanticTokenType::VARIABLE,   // 7: Environment variable references ($VAR)
    SemanticTokenType::COMMENT,    // 8: YAML comments
];

/// Token modifiers (optional enhancements).
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION,  // 0: Declaring a new item
    SemanticTokenModifier::DEFINITION,   // 1: Definition
    SemanticTokenModifier::READONLY,     // 2: Read-only value
    SemanticTokenModifier::DEPRECATED,   // 3: Deprecated field
];

/// Create the semantic tokens legend for capability registration.
pub fn create_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: TOKEN_TYPES.to_vec(),
        token_modifiers: TOKEN_MODIFIERS.to_vec(),
    }
}

/// Token type indices for easier reference.
mod token_type {
    pub const NAMESPACE: u32 = 0;
    pub const PROPERTY: u32 = 1;
    pub const STRING: u32 = 2;
    pub const NUMBER: u32 = 3;
    pub const KEYWORD: u32 = 4;
    pub const ENUM_MEMBER: u32 = 5;
    pub const FUNCTION: u32 = 6;
    pub const VARIABLE: u32 = 7;
    pub const COMMENT: u32 = 8;
}

/// Generate semantic tokens for a Fleet YAML document.
pub fn compute_semantic_tokens(source: &str) -> SemanticTokens {
    let mut tokens: Vec<RawToken> = Vec::new();

    let lines: Vec<&str> = source.lines().collect();
    let mut in_sql_block = false;
    let mut sql_indent = 0;

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Handle comments
        if trimmed.starts_with('#') {
            if let Some(start) = line.find('#') {
                tokens.push(RawToken {
                    line: line_idx as u32,
                    start: start as u32,
                    length: (line.len() - start) as u32,
                    token_type: token_type::COMMENT,
                    modifiers: 0,
                });
            }
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        // Check for SQL block end (less indented line after query:)
        if in_sql_block && indent <= sql_indent && !trimmed.starts_with('|') {
            in_sql_block = false;
        }

        // Detect top-level sections
        if indent == 0 && trimmed.ends_with(':') {
            let section_name = trimmed.trim_end_matches(':');
            if is_top_level_section(section_name) {
                tokens.push(RawToken {
                    line: line_idx as u32,
                    start: 0,
                    length: section_name.len() as u32,
                    token_type: token_type::NAMESPACE,
                    modifiers: 1, // DEFINITION
                });
            }
            continue;
        }

        // Handle key: value pairs
        if let Some(colon_pos) = find_yaml_colon(line) {
            let key_part = &line[..colon_pos];
            let key = key_part.trim().trim_start_matches('-').trim();
            let key_start = line.find(key).unwrap_or(0);

            // Determine token type for the key
            let key_token_type = if is_top_level_section(key) {
                token_type::NAMESPACE
            } else {
                token_type::PROPERTY
            };

            tokens.push(RawToken {
                line: line_idx as u32,
                start: key_start as u32,
                length: key.len() as u32,
                token_type: key_token_type,
                modifiers: 0,
            });

            // Check if this starts a SQL block
            if key == "query" {
                let value_part = &line[colon_pos + 1..];
                if value_part.trim() == "|" || value_part.trim() == "|-" || value_part.trim() == "|+" {
                    in_sql_block = true;
                    sql_indent = indent;
                } else if !value_part.trim().is_empty() {
                    // Inline SQL
                    tokenize_sql(value_part, line_idx as u32, (colon_pos + 1) as u32, &mut tokens);
                }
            } else {
                // Handle value part
                let value_part = &line[colon_pos + 1..];
                let value = value_part.trim();

                if !value.is_empty() && !value.starts_with('|') && !value.starts_with('>') {
                    tokenize_value(value, key, line_idx as u32, line, colon_pos, &mut tokens);
                }
            }
        } else if in_sql_block {
            // This line is part of a multiline SQL query
            tokenize_sql(line, line_idx as u32, 0, &mut tokens);
        }
    }

    // Convert raw tokens to delta-encoded semantic tokens
    encode_tokens(&tokens)
}

/// A raw token before delta encoding.
#[derive(Debug, Clone)]
struct RawToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

/// Check if a name is a top-level section.
fn is_top_level_section(name: &str) -> bool {
    matches!(
        name,
        "policies"
            | "queries"
            | "labels"
            | "controls"
            | "agent_options"
            | "software"
            | "webhook_settings"
            | "org_settings"
            | "team_settings"
    )
}

/// Find the colon that separates key from value in YAML (not inside quotes).
fn find_yaml_colon(line: &str) -> Option<usize> {
    let mut in_quotes = false;
    let mut quote_char = ' ';

    for (i, c) in line.char_indices() {
        if !in_quotes && (c == '"' || c == '\'') {
            in_quotes = true;
            quote_char = c;
        } else if in_quotes && c == quote_char {
            in_quotes = false;
        } else if !in_quotes && c == ':' {
            return Some(i);
        }
    }
    None
}

/// Tokenize a YAML value based on its key context.
fn tokenize_value(
    value: &str,
    key: &str,
    line: u32,
    full_line: &str,
    colon_pos: usize,
    tokens: &mut Vec<RawToken>,
) {
    let value_start = full_line[colon_pos + 1..]
        .find(|c: char| !c.is_whitespace())
        .map(|i| colon_pos + 1 + i)
        .unwrap_or(colon_pos + 1);

    // Remove quotes for analysis
    let clean_value = value.trim_matches('"').trim_matches('\'');

    // Check for environment variable references
    if value.starts_with('$') || value.contains("${") {
        tokens.push(RawToken {
            line,
            start: value_start as u32,
            length: value.len() as u32,
            token_type: token_type::VARIABLE,
            modifiers: 0,
        });
        return;
    }

    // Determine token type based on key
    let token_type = match key {
        "platform" | "logging" | "label_membership_type" => token_type::ENUM_MEMBER,
        "interval" | "timeout" => {
            if clean_value.parse::<i64>().is_ok() {
                token_type::NUMBER
            } else {
                token_type::STRING
            }
        }
        "critical" | "observer_can_run" | "automations_enabled" | "calendar_events_enabled" => {
            if clean_value == "true" || clean_value == "false" {
                token_type::KEYWORD
            } else {
                token_type::STRING
            }
        }
        "path" => token_type::STRING,
        _ => {
            // Check if it's a number
            if clean_value.parse::<f64>().is_ok() {
                token_type::NUMBER
            } else {
                token_type::STRING
            }
        }
    };

    tokens.push(RawToken {
        line,
        start: value_start as u32,
        length: value.len() as u32,
        token_type,
        modifiers: 0,
    });
}

/// Tokenize SQL content for syntax highlighting.
fn tokenize_sql(sql: &str, line: u32, offset: u32, tokens: &mut Vec<RawToken>) {
    // SQL keywords to highlight
    let sql_keywords = [
        "SELECT", "FROM", "WHERE", "AND", "OR", "NOT", "IN", "LIKE", "JOIN",
        "LEFT", "RIGHT", "INNER", "OUTER", "ON", "AS", "ORDER", "BY", "GROUP",
        "HAVING", "LIMIT", "OFFSET", "DISTINCT", "UNION", "EXCEPT", "INTERSECT",
        "NULL", "IS", "BETWEEN", "EXISTS", "CASE", "WHEN", "THEN", "ELSE", "END",
        "ASC", "DESC", "COUNT", "SUM", "AVG", "MIN", "MAX", "CAST", "COALESCE",
    ];

    let sql_upper = sql.to_uppercase();
    let mut pos = 0;

    while pos < sql.len() {
        let remaining = &sql[pos..];
        let remaining_upper = &sql_upper[pos..];

        // Skip whitespace
        if remaining.starts_with(char::is_whitespace) {
            pos += 1;
            continue;
        }

        // Check for SQL keywords
        let mut found_keyword = false;
        for keyword in &sql_keywords {
            if remaining_upper.starts_with(keyword) {
                // Make sure it's a whole word
                let keyword_len = keyword.len();
                let next_char = remaining.chars().nth(keyword_len);
                if next_char.map(|c| !c.is_alphanumeric() && c != '_').unwrap_or(true) {
                    tokens.push(RawToken {
                        line,
                        start: offset + pos as u32,
                        length: keyword_len as u32,
                        token_type: token_type::KEYWORD,
                        modifiers: 0,
                    });
                    pos += keyword_len;
                    found_keyword = true;
                    break;
                }
            }
        }

        if found_keyword {
            continue;
        }

        // Check for string literals
        if remaining.starts_with('\'') {
            if let Some(end) = remaining[1..].find('\'') {
                let length = end + 2; // Include quotes
                tokens.push(RawToken {
                    line,
                    start: offset + pos as u32,
                    length: length as u32,
                    token_type: token_type::STRING,
                    modifiers: 0,
                });
                pos += length;
                continue;
            }
        }

        // Check for numbers
        if remaining.starts_with(|c: char| c.is_ascii_digit()) {
            let num_len = remaining
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .count();
            tokens.push(RawToken {
                line,
                start: offset + pos as u32,
                length: num_len as u32,
                token_type: token_type::NUMBER,
                modifiers: 0,
            });
            pos += num_len;
            continue;
        }

        // Check for identifiers (potential table names)
        if remaining.starts_with(|c: char| c.is_alphabetic() || c == '_') {
            let ident_len = remaining
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .count();
            let ident = &remaining[..ident_len];

            // Check if this is an osquery table name
            if crate::linter::osquery::OSQUERY_TABLES.contains_key(ident) {
                tokens.push(RawToken {
                    line,
                    start: offset + pos as u32,
                    length: ident_len as u32,
                    token_type: token_type::FUNCTION,
                    modifiers: 0,
                });
            }
            // Otherwise it's just an identifier (column name, alias, etc.) - don't highlight

            pos += ident_len;
            continue;
        }

        // Skip other characters
        pos += 1;
    }
}

/// Encode raw tokens into LSP semantic token format (delta encoded).
fn encode_tokens(raw_tokens: &[RawToken]) -> SemanticTokens {
    let mut data: Vec<SemanticToken> = Vec::with_capacity(raw_tokens.len());

    let mut prev_line = 0u32;
    let mut prev_start = 0u32;

    for token in raw_tokens {
        let delta_line = token.line - prev_line;
        let delta_start = if delta_line == 0 {
            token.start - prev_start
        } else {
            token.start
        };

        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: token.length,
            token_type: token.token_type,
            token_modifiers_bitset: token.modifiers,
        });

        prev_line = token.line;
        prev_start = token.start;
    }

    SemanticTokens {
        result_id: None,
        data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_legend() {
        let legend = create_legend();
        assert!(!legend.token_types.is_empty());
        assert!(legend.token_types.contains(&SemanticTokenType::NAMESPACE));
        assert!(legend.token_types.contains(&SemanticTokenType::PROPERTY));
    }

    #[test]
    fn test_compute_semantic_tokens_basic() {
        let source = r#"policies:
  - name: Test Policy
    platform: darwin
    query: SELECT * FROM processes
"#;

        let tokens = compute_semantic_tokens(source);
        assert!(!tokens.data.is_empty());
    }

    #[test]
    fn test_find_yaml_colon() {
        assert_eq!(find_yaml_colon("name: test"), Some(4));
        assert_eq!(find_yaml_colon("  platform: darwin"), Some(10));
        assert_eq!(find_yaml_colon("query: \"test: value\""), Some(5));
    }

    #[test]
    fn test_is_top_level_section() {
        assert!(is_top_level_section("policies"));
        assert!(is_top_level_section("queries"));
        assert!(is_top_level_section("labels"));
        assert!(!is_top_level_section("name"));
        assert!(!is_top_level_section("query"));
    }

    #[test]
    fn test_tokenize_sql() {
        let mut tokens = Vec::new();
        tokenize_sql("SELECT * FROM processes WHERE name = 'test'", 0, 0, &mut tokens);

        // Should have tokens for SELECT, FROM, WHERE, and the string 'test'
        let token_types: Vec<u32> = tokens.iter().map(|t| t.token_type).collect();
        assert!(token_types.contains(&token_type::KEYWORD)); // SELECT, FROM, WHERE
        assert!(token_types.contains(&token_type::STRING)); // 'test'
        assert!(token_types.contains(&token_type::FUNCTION)); // processes (osquery table)
    }

    #[test]
    fn test_comments() {
        let source = "# This is a comment\npolicies:";
        let tokens = compute_semantic_tokens(source);

        // First token should be a comment
        assert!(!tokens.data.is_empty());
        assert_eq!(tokens.data[0].token_type, token_type::COMMENT);
    }
}
