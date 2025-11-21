use regex::Regex;

/// Extract code blocks from markdown
pub fn extract_code_blocks(markdown: &str, language: Option<&str>) -> Vec<String> {
    let pattern = if let Some(lang) = language {
        format!(r"```{}\n(.*?)\n```", regex::escape(lang))
    } else {
        r"```(?:\w+)?\n(.*?)\n```".to_string()
    };

    let re = Regex::new(&pattern).unwrap();
    let mut blocks = Vec::new();

    for cap in re.captures_iter(markdown) {
        if let Some(code) = cap.get(1) {
            blocks.push(code.as_str().to_string());
        }
    }

    blocks
}

/// Extract headings from markdown
pub fn extract_headings(markdown: &str) -> Vec<(usize, String)> {
    let re = Regex::new(r"^(#{1,6})\s+(.+)$").unwrap();
    let mut headings = Vec::new();

    for line in markdown.lines() {
        if let Some(cap) = re.captures(line) {
            let level = cap.get(1).unwrap().as_str().len();
            let text = cap.get(2).unwrap().as_str().to_string();
            headings.push((level, text));
        }
    }

    headings
}

/// Extract tables from markdown
pub fn extract_tables(markdown: &str) -> Vec<Vec<Vec<String>>> {
    let mut tables = Vec::new();
    let mut current_table = Vec::new();
    let mut in_table = false;

    for line in markdown.lines() {
        let trimmed = line.trim();

        // Check if line is a table row
        if trimmed.starts_with('|') && trimmed.ends_with('|') {
            // Skip separator rows
            if trimmed.contains("---") {
                continue;
            }

            let cells: Vec<String> = trimmed
                .trim_matches('|')
                .split('|')
                .map(|s| s.trim().to_string())
                .collect();

            current_table.push(cells);
            in_table = true;
        } else if in_table {
            // End of table
            tables.push(current_table.clone());
            current_table.clear();
            in_table = false;
        }
    }

    // Add last table if exists
    if !current_table.is_empty() {
        tables.push(current_table);
    }

    tables
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_blocks() {
        let md = r#"
Some text

```yaml
key: value
```

More text

```json
{"key": "value"}
```
"#;

        let yaml_blocks = extract_code_blocks(md, Some("yaml"));
        assert_eq!(yaml_blocks.len(), 1);
        assert_eq!(yaml_blocks[0], "key: value");

        let all_blocks = extract_code_blocks(md, None);
        assert_eq!(all_blocks.len(), 2);
    }

    #[test]
    fn test_extract_headings() {
        let md = r#"
# Heading 1
Some text
## Heading 2
More text
### Heading 3
"#;

        let headings = extract_headings(md);
        assert_eq!(headings.len(), 3);
        assert_eq!(headings[0], (1, "Heading 1".to_string()));
        assert_eq!(headings[1], (2, "Heading 2".to_string()));
        assert_eq!(headings[2], (3, "Heading 3".to_string()));
    }
}
