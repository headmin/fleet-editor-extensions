//! Infer the target platform(s) of a SQL query from the osquery tables it
//! references — used by `flint query` to fill in `platform:` automatically.

use super::osquery::OSQUERY_TABLES;

/// Table names referenced after `FROM`/`JOIN` in a SQL query (lowercased,
/// deduped). SQL keywords that can follow (e.g. a subquery's `select`) are
/// dropped.
pub fn referenced_tables(sql: &str) -> Vec<String> {
    const KEYWORDS: &[&str] = &[
        "select", "where", "on", "using", "group", "order", "limit", "as", "lateral", "cross",
        "inner", "outer", "left", "right", "natural",
    ];
    let words: Vec<String> = sql
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect();

    let mut tables = Vec::new();
    for i in 0..words.len() {
        if words[i] == "from" || words[i] == "join" {
            if let Some(next) = words.get(i + 1) {
                if !KEYWORDS.contains(&next.as_str()) && !tables.contains(next) {
                    tables.push(next.clone());
                }
            }
        }
    }
    tables
}

/// Infer the platform(s) a query targets: the intersection of the platforms of
/// every *known* osquery table it references. Empty when no known tables are
/// used or the platforms don't intersect.
pub fn infer_platforms(sql: &str) -> Vec<String> {
    let mut result: Option<Vec<String>> = None;
    for table in referenced_tables(sql) {
        if let Some(t) = OSQUERY_TABLES.get(table.as_str()) {
            let set: Vec<String> = t.platforms.iter().map(|s| s.to_string()).collect();
            result = Some(match result {
                None => set,
                Some(prev) => prev.into_iter().filter(|p| set.contains(p)).collect(),
            });
        }
    }
    result.unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_referenced_tables() {
        let t = referenced_tables("SELECT * FROM osquery_info JOIN users USING (uid)");
        assert!(t.contains(&"osquery_info".to_string()));
        assert!(t.contains(&"users".to_string()));
        assert!(!t.iter().any(|x| x == "using"));
    }

    #[test]
    fn test_referenced_tables_skips_subquery_keyword() {
        let t = referenced_tables("SELECT * FROM (SELECT 1) sub");
        assert!(!t.iter().any(|x| x == "select"));
    }

    #[test]
    fn test_infer_platform_single() {
        // `alf` is macOS-only in the schema.
        assert_eq!(infer_platforms("SELECT * FROM alf"), vec!["darwin".to_string()]);
    }

    #[test]
    fn test_infer_platform_intersection() {
        // acpi_tables is darwin+linux; alf is darwin-only → intersection darwin.
        let p = infer_platforms("SELECT * FROM acpi_tables JOIN alf");
        assert_eq!(p, vec!["darwin".to_string()]);
    }

    #[test]
    fn test_infer_platform_unknown_table_empty() {
        assert!(infer_platforms("SELECT 1 FROM not_a_real_table").is_empty());
    }
}
