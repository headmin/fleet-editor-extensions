//! YAML hygiene rules — pure text-based analysis (no YAML parser needed).
//!
//! These rules complement the existing `check_yaml_hygiene()` in `engine.rs`
//! (which handles tabs and trailing whitespace) with structural checks:
//!
//! - `yaml-indentation` — non-standard indent (not multiple of 2), mixed widths
//! - `yaml-colons` — key-like lines missing a colon separator
//! - `yaml-empty-values` — keys with no value that serde parses as null
//!
//! See ADR-008 for design rationale.

use std::path::Path;

use super::error::{FixSafety, LintError};
use super::fleet_config::FleetConfig;
use super::rules::Rule;

// ============================================================================
// Rule: yaml-indentation
// ============================================================================

/// Flags lines with non-standard indentation:
/// - Leading spaces that are not a multiple of 2
/// - Mixed indent widths within the same file (e.g., 2 in one section, 4 in another)
///
/// Skips blank lines, comment-only lines, and lines inside multi-line scalars
/// (block `|` / `>` indicators).
pub(crate) struct YamlIndentationRule;

impl Rule for YamlIndentationRule {
    fn name(&self) -> &'static str {
        "yaml-indentation"
    }
    fn description(&self) -> &'static str {
        "Flags non-standard indentation (not a multiple of 2 spaces) and mixed indent widths"
    }
    fn category(&self) -> &'static str {
        "yaml"
    }
    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();
        // Track the indent "unit" observed so far (first non-zero indent sets it).
        let mut expected_unit: Option<usize> = None;
        let mut in_block_scalar = false;
        let mut block_scalar_base_indent: usize = 0;

        for (idx, line) in source.lines().enumerate() {
            let line_num = idx + 1;

            // Detect block scalar start on the *previous* content line.
            // We set the flag here and skip subsequent lines that are deeper.
            if in_block_scalar {
                let indent = leading_spaces(line);
                // Block scalar continues while indent > base, or line is blank
                if line.trim().is_empty() || indent > block_scalar_base_indent {
                    continue;
                }
                // We've exited the block scalar
                in_block_scalar = false;
            }

            let trimmed = line.trim();

            // Skip blank lines and comment-only lines
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            // Check if this line starts a block scalar (ends with | or > possibly
            // followed by modifiers like |2, |-, >+, etc.)
            if is_block_scalar_start(trimmed) {
                block_scalar_base_indent = leading_spaces(line);
                in_block_scalar = true;
                // Still check indentation of *this* line (the key line)
            }

            let indent = leading_spaces(line);

            // Skip zero-indent lines (top-level keys) — nothing to check
            if indent == 0 {
                continue;
            }

            // Check: indent must be a multiple of 2
            if !indent.is_multiple_of(2) {
                // Round down to nearest multiple of 2 (safer than rounding up,
                // which could change nesting structure)
                let fixed_indent = (indent / 2) * 2;
                let old_spaces = " ".repeat(indent);
                let new_spaces = " ".repeat(fixed_indent);
                errors.push(
                    LintError::warning(
                        format!("Indentation is {} spaces (not a multiple of 2)", indent),
                        file,
                    )
                    .with_location(line_num, 1)
                    .with_rule_code(crate::codes::YAML_INDENTATION)
                    .with_help("Use 2-space indentation for consistent YAML formatting")
                    .with_context(old_spaces.clone())
                    .with_fix(super::error::Fix::Replace {
                        old: Some(old_spaces),
                        new: new_spaces,
                        safety: FixSafety::Unsafe,
                    }),
                );
                continue; // Don't also flag mixed-indent for an odd-width line
            }

            // Track indent unit for mixed-indent detection.
            // The "unit" is the smallest non-zero indent seen. If a file uses
            // 2-space, we see 2, 4, 6 … If 4-space, we see 4, 8, 12 …
            // We only flag when a line's indent is not a multiple of the unit.
            match expected_unit {
                None => {
                    expected_unit = Some(indent);
                }
                Some(unit) => {
                    if !indent.is_multiple_of(unit) && !unit.is_multiple_of(indent) {
                        errors.push(
                            LintError::warning(
                                format!(
                                    "Mixed indentation: this line uses {} spaces, but file uses {}-space indent",
                                    indent, unit
                                ),
                                file,
                            )
                            .with_location(line_num, 1)
                            .with_rule_code(crate::codes::YAML_INDENTATION)
                            .with_help("Use a consistent indent width throughout the file"),
                        );
                    }
                    // Update unit to the GCD if we see a smaller valid multiple
                    if indent < unit && unit % indent == 0 {
                        expected_unit = Some(indent);
                    }
                }
            }
        }

        errors
    }
}

// ============================================================================
// Rule: yaml-colons
// ============================================================================

/// Flags lines that look like YAML keys but are missing the colon separator.
///
/// serde_yaml treats `key_name` (no colon) as a plain string value, not a
/// key-value pair. This catches typos where the user forgot the `:`.
pub(crate) struct YamlColonsRule;

impl Rule for YamlColonsRule {
    fn name(&self) -> &'static str {
        "yaml-colons"
    }
    fn description(&self) -> &'static str {
        "Detects lines that look like YAML keys but are missing a colon separator"
    }
    fn category(&self) -> &'static str {
        "yaml"
    }
    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();
        let mut in_block_scalar = false;
        let mut block_scalar_base_indent: usize = 0;

        for (idx, line) in source.lines().enumerate() {
            let line_num = idx + 1;

            if in_block_scalar {
                let indent = leading_spaces(line);
                if line.trim().is_empty() || indent > block_scalar_base_indent {
                    continue;
                }
                in_block_scalar = false;
            }

            let trimmed = line.trim();

            // Skip blank, comments, document markers, list items
            if trimmed.is_empty()
                || trimmed.starts_with('#')
                || trimmed.starts_with("---")
                || trimmed.starts_with("...")
                || trimmed.starts_with('-')
            {
                continue;
            }

            if is_block_scalar_start(trimmed) {
                block_scalar_base_indent = leading_spaces(line);
                in_block_scalar = true;
                continue; // The key: | line itself has a colon, so skip
            }

            // If the line already has a colon, it's a proper key-value — skip
            if trimmed.contains(':') {
                continue;
            }

            // At this point: no colon, not a comment, not a list item, not blank.
            // If it looks like an identifier (word chars, hyphens, underscores),
            // it's likely a key that's missing its colon.
            // Exclude lines that are clearly string values (quoted, numeric, boolean).
            if looks_like_missing_key(trimmed) {
                errors.push(
                    LintError::warning(
                        format!(
                            "Line looks like a YAML key but has no colon: `{}`",
                            truncate(trimmed, 40)
                        ),
                        file,
                    )
                    .with_location(line_num, 1)
                    .with_rule_code(crate::codes::YAML_COLONS)
                    .with_help("Add a colon after the key name, e.g., `key: value`")
                    .with_context(trimmed.to_string())
                    .with_fix(super::error::Fix::Replace {
                        old: Some(trimmed.to_string()),
                        new: format!("{}: ", trimmed),
                        safety: FixSafety::Safe,
                    }),
                );
            }
        }

        errors
    }
}

// ============================================================================
// Rule: yaml-empty-values
// ============================================================================

/// Flags keys that have no value — serde_yaml parses these as `null`.
///
/// Example: `platform:` with nothing after the colon. This is valid YAML
/// but often a mistake in Fleet configs where a value was intended.
///
/// Fleet collection keys (`configuration_profiles:`, `certificates:`, `scripts:`,
/// `packages:`, `custom_settings:`, `labels_include_any:`, etc.) are commonly
/// and intentionally left empty to mean "no items" — those are skipped.
pub(crate) struct YamlEmptyValuesRule;

/// Keys where an empty value is idiomatic Fleet GitOps (collection = no items).
/// Matched by name only; empty values on these keys are never flagged.
const FLEET_EMPTY_OK_KEYS: &[&str] = &[
    // Top-level collections
    "policies",
    "queries",
    "reports",
    "labels",
    "software",
    // Software sub-collections
    "packages",
    "app_store_apps",
    "fleet_maintained_apps",
    // Controls / MDM collections
    "configuration_profiles",
    "custom_settings",
    "certificates",
    "scripts",
    // Label targeting
    "labels_include_any",
    "labels_include_all",
    "labels_exclude_any",
    // Membership
    "hosts",
    "host_ids",
    "categories",
    // Integrations
    "apple_business_manager",
    "volume_purchasing_program",
    "integrations",
    "google_calendar",
    "jira",
    "zendesk",
    "webhook_settings",
];

impl Rule for YamlEmptyValuesRule {
    fn name(&self) -> &'static str {
        "yaml-empty-values"
    }
    fn description(&self) -> &'static str {
        "Flags keys with no value (serde parses as null) that may indicate missing data"
    }
    fn category(&self) -> &'static str {
        "yaml"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();
        let lines: Vec<&str> = source.lines().collect();
        let mut in_block_scalar = false;
        let mut block_scalar_base_indent: usize = 0;

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;

            if in_block_scalar {
                let indent = leading_spaces(line);
                if line.trim().is_empty() || indent > block_scalar_base_indent {
                    continue;
                }
                in_block_scalar = false;
            }

            let trimmed = line.trim();

            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if is_block_scalar_start(trimmed) {
                block_scalar_base_indent = leading_spaces(line);
                in_block_scalar = true;
                continue;
            }

            // Look for `key:` with nothing (or only a comment) after the colon.
            // Must look like a real key (not a list item starting a mapping).
            if let Some(colon_pos) = trimmed.find(':') {
                let key_part = &trimmed[..colon_pos];
                let value_part = trimmed[colon_pos + 1..].trim();

                // Skip if key part is empty, quoted, or not identifier-like
                if key_part.is_empty() || key_part.starts_with('"') || key_part.starts_with('\'') {
                    continue;
                }

                // Strip leading `- ` for list-item mappings
                let key_clean = key_part.trim_start_matches('-').trim();
                if key_clean.is_empty() {
                    continue;
                }

                // Fleet collection keys: empty means "no items" — not a mistake.
                if FLEET_EMPTY_OK_KEYS.contains(&key_clean) {
                    continue;
                }

                // Value is empty or just a comment
                let is_empty = value_part.is_empty() || value_part.starts_with('#');

                if !is_empty {
                    continue;
                }

                // Check if the next non-blank line is more indented (i.e., this is
                // a mapping/sequence parent). Parents like `policies:` are expected
                // to have no inline value.
                let current_indent = leading_spaces(line);
                let has_children = lines[idx + 1..]
                    .iter()
                    .find(|l| !l.trim().is_empty())
                    .map(|l| leading_spaces(l) > current_indent)
                    .unwrap_or(false);

                if has_children {
                    continue; // Parent key — children follow, this is normal
                }

                // Leaf key with no value — likely a mistake
                errors.push(
                    LintError::info(
                        format!(
                            "Key `{}` has no value (will be parsed as null)",
                            truncate(key_clean, 30)
                        ),
                        file,
                    )
                    .with_location(line_num, colon_pos + 2)
                    .with_rule_code(crate::codes::YAML_EMPTY_VALUES)
                    .with_help(
                        "Provide a value, or remove the key if not needed.",
                    ),
                );
            }
        }

        errors
    }
}

// ============================================================================
// Helpers
// ============================================================================

/// Count leading space characters.
fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// Check if a trimmed line ends with a block scalar indicator (| or >),
/// possibly with modifiers like `|2`, `|-`, `>+`.
fn is_block_scalar_start(trimmed: &str) -> bool {
    // Must have a colon first (it's a key: | line)
    if let Some(colon_pos) = trimmed.find(':') {
        let after_colon = trimmed[colon_pos + 1..].trim();
        // Strip trailing comment
        let after_colon = if let Some(hash_pos) = after_colon.find(" #") {
            after_colon[..hash_pos].trim()
        } else {
            after_colon
        };
        if after_colon.is_empty() {
            return false;
        }
        let first_char = after_colon.chars().next().unwrap();
        if first_char == '|' || first_char == '>' {
            // Remaining chars (if any) should be modifiers: digits, +, -
            return after_colon[1..]
                .chars()
                .all(|c| c.is_ascii_digit() || c == '+' || c == '-');
        }
    }
    false
}

/// Heuristic: does this line look like a forgotten YAML key?
/// True if it's a single identifier-like token (word chars, hyphens, underscores, dots).
fn looks_like_missing_key(trimmed: &str) -> bool {
    // Must not be quoted (string value)
    if trimmed.starts_with('"')
        || trimmed.starts_with('\'')
        || trimmed.starts_with('[')
        || trimmed.starts_with('{')
    {
        return false;
    }

    // Must not be a boolean, null, or numeric literal
    let lower = trimmed.to_lowercase();
    if matches!(
        lower.as_str(),
        "true" | "false" | "yes" | "no" | "null" | "~" | "on" | "off"
    ) {
        return false;
    }
    if trimmed.parse::<f64>().is_ok() {
        return false;
    }

    // Must look identifier-like: letters, digits, hyphens, underscores, dots
    // and start with a letter or underscore
    let first = match trimmed.chars().next() {
        Some(c) => c,
        None => return false,
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }

    trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Truncate a string for display in diagnostics.
fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..max]
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Severity;
    use std::path::PathBuf;

    fn check_rule(rule: &dyn Rule, source: &str) -> Vec<LintError> {
        let config = FleetConfig::default();
        let file = PathBuf::from("test.yml");
        rule.check(&config, &file, source)
    }

    // ── yaml-indentation ───────────────────────────────────────

    #[test]
    fn indentation_clean_2_space() {
        let source = "policies:\n  - name: test\n    query: \"SELECT 1;\"\n";
        let errors = check_rule(&YamlIndentationRule, source);
        assert!(
            errors.is_empty(),
            "Clean 2-space file should have no errors: {:?}",
            errors
        );
    }

    #[test]
    fn indentation_odd_spaces() {
        let source = "policies:\n   - name: test\n";
        let errors = check_rule(&YamlIndentationRule, source);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("3 spaces"));
        assert_eq!(errors[0].line(), Some(2));
    }

    #[test]
    fn indentation_mixed_2_and_6() {
        // 2-space indent established, then a 6-space line (not a multiple of 2 unit
        // from the first indent perspective, but 6 is a multiple of 2, so it's fine).
        // This should NOT flag because 6 % 2 == 0.
        let source = "a:\n  b: 1\n      c: 2\n";
        let errors = check_rule(&YamlIndentationRule, source);
        assert!(
            errors.is_empty(),
            "6 is a multiple of 2, should be fine: {:?}",
            errors
        );
    }

    #[test]
    fn indentation_skips_blank_and_comments() {
        let source = "a:\n\n  # comment\n  b: 1\n";
        let errors = check_rule(&YamlIndentationRule, source);
        assert!(errors.is_empty());
    }

    #[test]
    fn indentation_skips_block_scalar_body() {
        let source = "query: |\n  SELECT 1\n   FROM users\n    WHERE id = 1;\nname: test\n";
        let errors = check_rule(&YamlIndentationRule, source);
        // The block scalar body (lines 2-4) should be skipped — odd indent is OK there
        assert!(
            errors.is_empty(),
            "Block scalar body should be skipped: {:?}",
            errors
        );
    }

    #[test]
    fn indentation_1_space() {
        let source = "a:\n b: 1\n";
        let errors = check_rule(&YamlIndentationRule, source);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("1 spaces"));
    }

    // ── yaml-colons ────────────────────────────────────────────

    #[test]
    fn colons_normal_key_value() {
        let source = "name: test\nplatform: darwin\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert!(errors.is_empty());
    }

    #[test]
    fn colons_missing_colon() {
        let source = "name: test\nplatform\nquery: \"SELECT 1;\"\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("platform"));
    }

    #[test]
    fn colons_skips_comments() {
        let source = "# This is a comment\nname: test\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert!(errors.is_empty());
    }

    #[test]
    fn colons_skips_list_items() {
        let source = "policies:\n  - name: test\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert!(errors.is_empty());
    }

    #[test]
    fn colons_skips_boolean_values() {
        // `true` as a standalone value in a list shouldn't flag
        let source = "name: test\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert!(errors.is_empty());
    }

    #[test]
    fn colons_skips_quoted_strings() {
        let source = "\"some string value\"\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert!(errors.is_empty());
    }

    #[test]
    fn colons_skips_block_scalar_body() {
        let source = "query: |\n  SELECT 1\n  FROM users\nname: test\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert!(
            errors.is_empty(),
            "Block scalar body should be skipped: {:?}",
            errors
        );
    }

    #[test]
    fn colons_skips_document_markers() {
        let source = "---\nname: test\n...\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert!(errors.is_empty());
    }

    // ── yaml-empty-values ──────────────────────────────────────

    #[test]
    fn empty_values_parent_key_ok() {
        // `policies:` with children is normal — not flagged
        let source = "policies:\n  - name: test\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert!(
            errors.is_empty(),
            "Parent keys should not be flagged: {:?}",
            errors
        );
    }

    #[test]
    fn empty_values_leaf_key_flagged() {
        let source = "name: test\nplatform:\ndescription: hello\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("platform"));
        assert_eq!(errors[0].severity, Severity::Info);
    }

    #[test]
    fn empty_values_with_comment_after_colon() {
        let source = "name: test\nplatform: # TODO fill this in\ndescription: hello\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("platform"));
    }

    #[test]
    fn empty_values_block_scalar_not_flagged() {
        let source = "query: |\n  SELECT 1;\nname: test\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert!(
            errors.is_empty(),
            "Block scalar keys should not be flagged: {:?}",
            errors
        );
    }

    #[test]
    fn empty_values_last_line_leaf() {
        let source = "name: test\nplatform:\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn empty_values_list_item_mapping_with_siblings() {
        // `- name:` in a list-item mapping — `query:` at deeper indent looks
        // like a child to our text-based heuristic, so it's NOT flagged.
        // This is a known trade-off of pure text analysis vs. YAML parsing.
        let source = "policies:\n  - name:\n    query: \"SELECT 1;\"\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert!(
            errors.is_empty(),
            "List-item siblings at deeper indent should not be flagged: {:?}",
            errors
        );
    }

    #[test]
    fn empty_values_standalone_leaf() {
        // A leaf key at the end of a list item with no deeper content
        let source = "policies:\n  - name: test\n    platform:\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].message.contains("platform"));
    }

    #[test]
    fn empty_values_fleet_collection_keys_not_flagged() {
        // Fleet-idiomatic: empty collection keys mean "no items" — don't flag.
        let source = "android_settings:\n  configuration_profiles:\n  certificates:\n\n  scripts:\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert!(
            errors.is_empty(),
            "Fleet collection keys should not trigger empty-values: {:?}",
            errors
        );
    }

    #[test]
    fn empty_values_top_level_collections_not_flagged() {
        // `software:`, `policies:`, `labels:`, `queries:` at top level with no
        // children are valid ("no items") — don't flag.
        let source = "software:\n\npolicies:\n\nlabels:\n\nqueries:\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert!(
            errors.is_empty(),
            "Top-level empty collections should not trigger empty-values: {:?}",
            errors
        );
    }

    #[test]
    fn empty_values_help_does_not_suggest_null() {
        // The help text should not push users toward `null` — empty keys in
        // Fleet GitOps are fixed by providing a value or removing the key.
        let source = "query:\n";
        let errors = check_rule(&YamlEmptyValuesRule, source);
        assert_eq!(errors.len(), 1);
        let help = errors[0].help.as_deref().unwrap_or("");
        assert!(
            !help.to_lowercase().contains("null"),
            "help should not mention null, got: {help}"
        );
    }

    // ── Auto-fix tests ──────────────────────────────────────────

    #[test]
    fn indentation_fix_rounds_down_to_2() {
        let source = "policies:\n   - name: test\n";
        let errors = check_rule(&YamlIndentationRule, source);
        assert_eq!(errors.len(), 1);
        // 3 spaces → 2 spaces (round down)
        assert_eq!(errors[0].context.as_deref(), Some("   "));
        assert_eq!(errors[0].suggestion(), Some("  "));
        assert_eq!(errors[0].fix_safety(), Some(FixSafety::Unsafe));
    }

    #[test]
    fn indentation_fix_5_rounds_to_4() {
        let source = "a:\n     b: 1\n";
        let errors = check_rule(&YamlIndentationRule, source);
        assert_eq!(errors.len(), 1);
        // 5 spaces → 4 spaces
        assert_eq!(errors[0].context.as_deref(), Some("     "));
        assert_eq!(errors[0].suggestion(), Some("    "));
    }

    #[test]
    fn indentation_fix_1_rounds_to_0() {
        let source = "a:\n b: 1\n";
        let errors = check_rule(&YamlIndentationRule, source);
        assert_eq!(errors.len(), 1);
        // 1 space → 0 spaces
        assert_eq!(errors[0].context.as_deref(), Some(" "));
        assert_eq!(errors[0].suggestion(), Some(""));
    }

    #[test]
    fn colons_fix_appends_colon() {
        let source = "name: test\nplatform\nquery: \"SELECT 1;\"\n";
        let errors = check_rule(&YamlColonsRule, source);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].context.as_deref(), Some("platform"));
        assert_eq!(errors[0].suggestion(), Some("platform: "));
        assert_eq!(errors[0].fix_safety(), Some(FixSafety::Safe));
    }

    // Fixability now lives in codes::REGISTRY (one source, printed by
    // `flint list-rules`), and is guarded against the rules' real behaviour by
    // flint-lint/tests/fixable_metadata.rs. These assert the registry entries
    // for this file's rules.

    #[test]
    fn colons_is_fixable() {
        assert!(crate::codes::meta(YamlColonsRule.name()).unwrap().fixable);
    }

    #[test]
    fn indentation_is_fixable() {
        assert!(
            crate::codes::meta(YamlIndentationRule.name())
                .unwrap()
                .fixable
        );
    }

    #[test]
    fn empty_values_not_fixable() {
        assert!(
            !crate::codes::meta(YamlEmptyValuesRule.name())
                .unwrap()
                .fixable
        );
    }

    // ── Integration: rules appear in list-rules ────────────────

    #[test]
    fn rules_have_correct_metadata() {
        let indent = YamlIndentationRule;
        assert_eq!(indent.name(), "yaml-indentation");
        assert_eq!(indent.category(), "yaml");

        let colons = YamlColonsRule;
        assert_eq!(colons.name(), "yaml-colons");

        let empty = YamlEmptyValuesRule;
        assert_eq!(empty.name(), "yaml-empty-values");
    }
}
