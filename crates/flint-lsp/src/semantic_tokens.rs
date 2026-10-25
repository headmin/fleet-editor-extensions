//! Semantic tokens provider for Fleet GitOps YAML files.
//!
//! Provides enhanced syntax highlighting through semantic tokens,
//! allowing editors to distinguish between different element types.

use tower_lsp::lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens, SemanticTokensLegend,
};

/// Token types used by Fleet GitOps files.
/// The order here defines the token type index.
pub const TOKEN_TYPES: &[SemanticTokenType] = &[
    SemanticTokenType::NAMESPACE, // 0: Top-level sections (policies, queries, labels)
    SemanticTokenType::PROPERTY,  // 1: Field names (name, query, platform, etc.)
    SemanticTokenType::STRING,    // 2: String values
    SemanticTokenType::NUMBER,    // 3: Numeric values (interval, etc.)
    SemanticTokenType::KEYWORD,   // 4: SQL keywords (SELECT, FROM, WHERE, etc.)
    SemanticTokenType::ENUM_MEMBER, // 5: Enum values (darwin, windows, snapshot, etc.)
    SemanticTokenType::FUNCTION,  // 6: osquery table names
    SemanticTokenType::VARIABLE,  // 7: Environment variable references ($VAR)
    SemanticTokenType::COMMENT,   // 8: YAML comments
    // 9: A key the schema does not have where it is. Custom type; the VS
    // Code extension maps it to `invalid.illegal` (package.json).
    SemanticTokenType::new("invalidKey"),
];

/// Token modifiers (optional enhancements).
pub const TOKEN_MODIFIERS: &[SemanticTokenModifier] = &[
    SemanticTokenModifier::DECLARATION, // 0: Declaring a new item
    SemanticTokenModifier::DEFINITION,  // 1: Definition
    SemanticTokenModifier::READONLY,    // 2: Read-only value
    SemanticTokenModifier::DEPRECATED,  // 3: Deprecated field
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
    pub(super) const NAMESPACE: u32 = 0;
    pub(super) const PROPERTY: u32 = 1;
    pub(super) const STRING: u32 = 2;
    pub(super) const NUMBER: u32 = 3;
    pub(super) const KEYWORD: u32 = 4;
    pub(super) const ENUM_MEMBER: u32 = 5;
    pub(super) const FUNCTION: u32 = 6;
    pub(super) const VARIABLE: u32 = 7;
    pub(super) const COMMENT: u32 = 8;
    pub(super) const INVALID_KEY: u32 = 9;
}

/// Generate semantic tokens for a Fleet YAML document.
/// Keys are coloured by where they are, per the same schema the lint uses:
/// a top-level key as a section (NAMESPACE), a nested one as a PROPERTY, and
/// a key the schema does not have there as INVALID_KEY (typo or misplaced).
/// Free-form content and the children of an invalid key are not judged.
pub fn compute_semantic_tokens(source: &str, file: Option<&std::path::Path>) -> SemanticTokens {
    let mut tokens: Vec<RawToken> = Vec::new();
    // (column, key) of the keys enclosing the current line.
    let mut parents: Vec<(usize, String)> = Vec::new();
    // Inside a `|`/`>` block scalar owned by the key at this column; `true`
    // when it is SQL.
    let mut block: Option<(usize, bool)> = None;

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();

        if let Some((owner, sql)) = block {
            if indent > owner {
                if sql {
                    tokenize_sql(line, line_idx as u32, 0, &mut tokens);
                }
                continue;
            }
            block = None;
        }

        if trimmed.starts_with('#') {
            tokens.push(RawToken {
                line: line_idx as u32,
                start: indent as u32,
                length: (line.len() - indent) as u32,
                token_type: token_type::COMMENT,
                modifiers: 0,
            });
            continue;
        }

        let Some(colon_pos) = find_yaml_colon(line) else { continue };
        let key = line[..colon_pos].trim().trim_start_matches('-').trim();
        if key.is_empty() {
            continue;
        }
        let key_col = line[..colon_pos].find(key).unwrap_or(indent);
        while parents.last().is_some_and(|(col, _)| *col >= key_col) {
            parents.pop();
        }
        let path: Vec<&str> = parents.iter().map(|(_, k)| k.as_str()).collect();
        let token_type = match flint_lint::key_fit(file, &path, key) {
            flint_lint::KeyFit::Invalid => token_type::INVALID_KEY,
            flint_lint::KeyFit::Known if path.is_empty() => token_type::NAMESPACE,
            _ => token_type::PROPERTY,
        };
        tokens.push(RawToken {
            line: line_idx as u32,
            start: key_col as u32,
            length: key.len() as u32,
            token_type,
            modifiers: 0,
        });
        parents.push((key_col, key.to_string()));

        let value = line[colon_pos + 1..].trim();
        if value.starts_with('|') || value.starts_with('>') {
            block = Some((key_col, key == "query"));
        } else if key == "query" && !value.is_empty() {
            tokenize_sql(&line[colon_pos + 1..], line_idx as u32, (colon_pos + 1) as u32, &mut tokens);
        } else if !value.is_empty() {
            tokenize_value(value, key, line_idx as u32, line, colon_pos, &mut tokens);
        }
    }

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
        "SELECT",
        "FROM",
        "WHERE",
        "AND",
        "OR",
        "NOT",
        "IN",
        "LIKE",
        "JOIN",
        "LEFT",
        "RIGHT",
        "INNER",
        "OUTER",
        "ON",
        "AS",
        "ORDER",
        "BY",
        "GROUP",
        "HAVING",
        "LIMIT",
        "OFFSET",
        "DISTINCT",
        "UNION",
        "EXCEPT",
        "INTERSECT",
        "NULL",
        "IS",
        "BETWEEN",
        "EXISTS",
        "CASE",
        "WHEN",
        "THEN",
        "ELSE",
        "END",
        "ASC",
        "DESC",
        "COUNT",
        "SUM",
        "AVG",
        "MIN",
        "MAX",
        "CAST",
        "COALESCE",
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
                if next_char
                    .map(|c| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(true)
                {
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
        if let Some(stripped) = remaining.strip_prefix('\'') {
            if let Some(end) = stripped.find('\'') {
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
            if flint_lint::osquery::OSQUERY_TABLES.contains_key(ident) {
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

        let tokens = compute_semantic_tokens(source, None);
        assert!(!tokens.data.is_empty());
    }

    #[test]
    fn test_find_yaml_colon() {
        assert_eq!(find_yaml_colon("name: test"), Some(4));
        assert_eq!(find_yaml_colon("  platform: darwin"), Some(10));
        assert_eq!(find_yaml_colon("query: \"test: value\""), Some(5));
    }

    #[test]
    fn keys_are_coloured_by_where_they_are() {
        let src = "name: A\ncontrols:\n  enable_disk_encryption: true\n  bogus: 1\napple_settings:\n    configuration_profiles:\npolicies:\n  - name: P\n    description: |\n      Note: not a key\n";
        let types: Vec<(u32, u32)> = raw(src).into_iter().map(|t| (t.line, t.token_type)).filter(|(_, t)| ![token_type::STRING, token_type::KEYWORD, token_type::NUMBER].contains(t)).collect();
        use token_type::*;
        assert_eq!(types, vec![
            (0, NAMESPACE), (1, NAMESPACE), (2, PROPERTY), (3, INVALID_KEY),
            (4, INVALID_KEY), (5, PROPERTY), // a misplaced section's child is not judged
            (6, NAMESPACE), (7, PROPERTY), (8, PROPERTY), // block scalar text is not a key
        ]);
        // `- paths:` followed by indented items: they are under a value-only key.
        let shifted: Vec<(u32, u32)> = raw("name: A\nreports:\n- paths:\n  - path: x.yml\n").into_iter().map(|t| (t.line, t.token_type)).filter(|(_, t)| *t != token_type::STRING).collect();
        assert_eq!(shifted, vec![(0, NAMESPACE), (1, NAMESPACE), (2, PROPERTY), (3, INVALID_KEY)]);
    }

    fn raw(src: &str) -> Vec<RawToken> {
        let tokens = compute_semantic_tokens(src, Some(std::path::Path::new("fleets/a.yml")));
        let (mut line, mut out) = (0, Vec::new());
        for t in tokens.data {
            line += t.delta_line;
            out.push(RawToken { line, start: 0, length: t.length, token_type: t.token_type, modifiers: 0 });
        }
        out
    }

    #[test]
    fn test_tokenize_sql() {
        let mut tokens = Vec::new();
        tokenize_sql(
            "SELECT * FROM processes WHERE name = 'test'",
            0,
            0,
            &mut tokens,
        );

        // Should have tokens for SELECT, FROM, WHERE, and the string 'test'
        let token_types: Vec<u32> = tokens.iter().map(|t| t.token_type).collect();
        assert!(token_types.contains(&token_type::KEYWORD)); // SELECT, FROM, WHERE
        assert!(token_types.contains(&token_type::STRING)); // 'test'
        assert!(token_types.contains(&token_type::FUNCTION)); // processes (osquery table)
    }

    #[test]
    fn test_comments() {
        let source = "# This is a comment\npolicies:";
        let tokens = compute_semantic_tokens(source, None);

        // First token should be a comment
        assert!(!tokens.data.is_empty());
        assert_eq!(tokens.data[0].token_type, token_type::COMMENT);
    }
}
