//! Document symbols provider for Fleet GitOps YAML files.
//!
//! Provides document outline showing policies, queries, and labels.

use tower_lsp::lsp_types::{DocumentSymbol, Position, Range, SymbolKind};

/// Generate document symbols for a Fleet YAML document.
pub fn document_symbols(source: &str) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    // Parse the source to find policies, queries, labels
    let lines: Vec<&str> = source.lines().collect();

    let mut current_section: Option<&str> = None;
    let mut section_start_line = 0;
    let mut items: Vec<(String, usize, &str)> = Vec::new(); // (name, line, section)

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        // Detect top-level sections
        if trimmed == "policies:" || trimmed.starts_with("policies:") {
            if let Some(section) = current_section {
                add_section_symbol(&mut symbols, section, section_start_line, idx, &items, &lines);
            }
            current_section = Some("policies");
            section_start_line = idx;
            items.clear();
        } else if trimmed == "queries:" || trimmed.starts_with("queries:") {
            if let Some(section) = current_section {
                add_section_symbol(&mut symbols, section, section_start_line, idx, &items, &lines);
            }
            current_section = Some("queries");
            section_start_line = idx;
            items.clear();
        } else if trimmed == "labels:" || trimmed.starts_with("labels:") {
            if let Some(section) = current_section {
                add_section_symbol(&mut symbols, section, section_start_line, idx, &items, &lines);
            }
            current_section = Some("labels");
            section_start_line = idx;
            items.clear();
        } else if trimmed == "controls:" || trimmed.starts_with("controls:") {
            if let Some(section) = current_section {
                add_section_symbol(&mut symbols, section, section_start_line, idx, &items, &lines);
            }
            current_section = Some("controls");
            section_start_line = idx;
            items.clear();
        } else if trimmed == "agent_options:" || trimmed.starts_with("agent_options:") {
            if let Some(section) = current_section {
                add_section_symbol(&mut symbols, section, section_start_line, idx, &items, &lines);
            }
            current_section = Some("agent_options");
            section_start_line = idx;
            items.clear();
        } else if trimmed == "software:" || trimmed.starts_with("software:") {
            if let Some(section) = current_section {
                add_section_symbol(&mut symbols, section, section_start_line, idx, &items, &lines);
            }
            current_section = Some("software");
            section_start_line = idx;
            items.clear();
        }

        // Detect item names within sections
        if current_section.is_some() {
            if let Some(name) = extract_name_from_line(trimmed) {
                if let Some(section) = current_section {
                    items.push((name, idx, section));
                }
            }
        }
    }

    // Handle final section
    if let Some(section) = current_section {
        add_section_symbol(&mut symbols, section, section_start_line, lines.len(), &items, &lines);
    }

    symbols
}

/// Extract a name value from a line like "- name: Foo" or "name: Foo"
fn extract_name_from_line(line: &str) -> Option<String> {
    let trimmed = line.trim().trim_start_matches('-').trim();

    if trimmed.starts_with("name:") {
        let value = trimmed.strip_prefix("name:")?.trim();
        // Remove surrounding quotes if present
        let value = value.trim_matches('"').trim_matches('\'');
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }

    // Also check for "- path:" references
    if trimmed.starts_with("path:") {
        let value = trimmed.strip_prefix("path:")?.trim();
        let value = value.trim_matches('"').trim_matches('\'');
        if !value.is_empty() {
            return Some(format!("→ {}", value));
        }
    }

    None
}

/// Add a section symbol with its children
fn add_section_symbol(
    symbols: &mut Vec<DocumentSymbol>,
    section: &str,
    start_line: usize,
    end_line: usize,
    items: &[(String, usize, &str)],
    lines: &[&str],
) {
    let (symbol_kind, detail) = match section {
        "policies" => (SymbolKind::NAMESPACE, "Compliance policies"),
        "queries" => (SymbolKind::NAMESPACE, "osquery queries"),
        "labels" => (SymbolKind::NAMESPACE, "Host labels"),
        "controls" => (SymbolKind::NAMESPACE, "MDM controls"),
        "agent_options" => (SymbolKind::NAMESPACE, "Agent configuration"),
        "software" => (SymbolKind::NAMESPACE, "Software packages"),
        _ => (SymbolKind::NAMESPACE, ""),
    };

    // Build children for items in this section
    let children: Vec<DocumentSymbol> = items
        .iter()
        .filter(|(_, _, s)| *s == section)
        .map(|(name, line, _)| {
            let item_kind = match section {
                "policies" => SymbolKind::FUNCTION,
                "queries" => SymbolKind::FUNCTION,
                "labels" => SymbolKind::CONSTANT,
                _ => SymbolKind::PROPERTY,
            };

            // Calculate item range (approximate - from name line to next item or section end)
            let item_end = find_item_end(lines, *line, end_line);

            #[allow(deprecated)]
            DocumentSymbol {
                name: name.clone(),
                detail: None,
                kind: item_kind,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position {
                        line: *line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: item_end as u32,
                        character: lines.get(item_end).map(|l| l.len()).unwrap_or(0) as u32,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: *line as u32,
                        character: 0,
                    },
                    end: Position {
                        line: *line as u32,
                        character: lines.get(*line).map(|l| l.len()).unwrap_or(0) as u32,
                    },
                },
                children: None,
            }
        })
        .collect();

    let end_line = end_line.saturating_sub(1);

    #[allow(deprecated)]
    let section_symbol = DocumentSymbol {
        name: section.to_string(),
        detail: Some(detail.to_string()),
        kind: symbol_kind,
        tags: None,
        deprecated: None,
        range: Range {
            start: Position {
                line: start_line as u32,
                character: 0,
            },
            end: Position {
                line: end_line as u32,
                character: lines.get(end_line).map(|l| l.len()).unwrap_or(0) as u32,
            },
        },
        selection_range: Range {
            start: Position {
                line: start_line as u32,
                character: 0,
            },
            end: Position {
                line: start_line as u32,
                character: lines.get(start_line).map(|l| l.len()).unwrap_or(0) as u32,
            },
        },
        children: if children.is_empty() {
            None
        } else {
            Some(children)
        },
    };

    symbols.push(section_symbol);
}

/// Find where an item ends (line before next item or section end)
fn find_item_end(lines: &[&str], item_start: usize, section_end: usize) -> usize {
    let item_indent = lines
        .get(item_start)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    for idx in (item_start + 1)..section_end {
        let line = lines.get(idx).unwrap_or(&"");
        let trimmed = line.trim();

        if trimmed.is_empty() {
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        // Found a sibling item (same indent and starts with -)
        if indent <= item_indent && trimmed.starts_with('-') {
            return idx.saturating_sub(1);
        }

        // Found a new section
        if indent == 0 && trimmed.contains(':') {
            return idx.saturating_sub(1);
        }
    }

    section_end.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_symbols_basic() {
        let source = r#"policies:
  - name: Disk Encryption
    query: SELECT 1 FROM disk_encryption
  - name: Firewall Enabled
    query: SELECT 1 FROM alf

queries:
  - name: Running Processes
    query: SELECT * FROM processes
"#;

        let symbols = document_symbols(source);

        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "policies");
        assert_eq!(symbols[1].name, "queries");

        let policy_children = symbols[0].children.as_ref().unwrap();
        assert_eq!(policy_children.len(), 2);
        assert_eq!(policy_children[0].name, "Disk Encryption");
        assert_eq!(policy_children[1].name, "Firewall Enabled");

        let query_children = symbols[1].children.as_ref().unwrap();
        assert_eq!(query_children.len(), 1);
        assert_eq!(query_children[0].name, "Running Processes");
    }

    #[test]
    fn test_extract_name_from_line() {
        assert_eq!(
            extract_name_from_line("  - name: Test Policy"),
            Some("Test Policy".to_string())
        );
        assert_eq!(
            extract_name_from_line("    name: Another Name"),
            Some("Another Name".to_string())
        );
        assert_eq!(
            extract_name_from_line("  - path: lib/policies.yml"),
            Some("→ lib/policies.yml".to_string())
        );
        assert_eq!(extract_name_from_line("  query: SELECT 1"), None);
    }

    #[test]
    fn test_document_symbols_with_path_refs() {
        let source = r#"policies:
  - name: Local Policy
    query: SELECT 1
  - path: lib/policies.yml
"#;

        let symbols = document_symbols(source);
        assert_eq!(symbols.len(), 1);

        let children = symbols[0].children.as_ref().unwrap();
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].name, "Local Policy");
        assert_eq!(children[1].name, "→ lib/policies.yml");
    }
}
