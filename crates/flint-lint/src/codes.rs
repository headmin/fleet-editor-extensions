//! The rule-code registry — the single source of truth for every diagnostic
//! code flint emits.
//!
//! Historically codes lived in two disjoint namespaces: registered rules
//! identified by `Rule::name()`, and inline string literals for the
//! cross-file graph pass and the engine's YAML-hygiene checks (which the
//! engine then re-hardcoded to gate). This module merges them: every code is
//! a `const`, every code has a [`RuleMeta`] entry, and the engine stamps
//! `doc_url` onto diagnostics from here — so the LSP no longer maintains a
//! parallel code→URL table.
//!
//! Note: `RuleMeta.doc_url` holds the FULL documentation URL (not an anchor)
//! because one rule (`structural-validation`) links the page without an
//! anchor; full static URLs keep the type uniform.

/// Base of Fleet's YAML reference documentation.
const DOCS: &str = "https://fleetdm.com/docs/configuration/yaml-files";

/// Full doc URL from an anchor literal (compile-time concatenation).
macro_rules! concat_url {
    ($anchor:literal) => {
        concat!("https://fleetdm.com/docs/configuration/yaml-files", $anchor)
    };
}

// --- Core rules (registered via `RuleSet::standard`) -----------------------
pub const REQUIRED_FIELDS: &str = "required-fields";
pub const PLATFORM_COMPATIBILITY: &str = "platform-compatibility";
pub const TYPE_VALIDATION: &str = "type-validation";
pub const SECURITY: &str = "security";
pub const INTERVAL_VALIDATION: &str = "interval-validation";
pub const DUPLICATE_NAMES: &str = "duplicate-names";
pub const QUERY_SYNTAX: &str = "query-syntax";
/// Emitted by `query-syntax` when a query exceeds `[thresholds] max_query_length`.
pub const QUERY_LENGTH: &str = "query-length";
pub const FMA_SLUG: &str = "fma-slug";
pub const STRUCTURAL_VALIDATION: &str = "structural-validation";
pub const LABEL_TARGETING: &str = "label-targeting";
pub const LABEL_MEMBERSHIP: &str = "label-membership";
pub const DATE_FORMAT: &str = "date-format";
pub const PATCH_POLICY: &str = "patch-policy";
pub const POLICY_AUTOMATION_LOCATION: &str = "policy-automation-location";
pub const HASH_FORMAT: &str = "hash-format";
pub const CATEGORIES: &str = "categories";
pub const FILE_EXTENSION: &str = "file-extension";
pub const SECRET_HYGIENE: &str = "secret-hygiene";
pub const PATH_REFERENCE: &str = "path-reference";
pub const SHEBANG_SYNTAX: &str = "shebang-syntax";
pub const WEBHOOK_ENDPOINT_VALID: &str = "webhook-endpoint-valid";
pub const SOFTWARE_URL: &str = "software-url";
pub const SOFTWARE_SOURCE: &str = "software-source";
pub const CALENDAR_EVENT_COERCION: &str = "calendar-event-coercion";
pub const PATH_EXISTS: &str = "path-exists";
pub const SELF_REFERENCE: &str = "self-reference";
pub const DEPRECATED_KEYS: &str = "deprecated-keys";
pub const YAML_INDENTATION: &str = "yaml-indentation";
pub const YAML_COLONS: &str = "yaml-colons";
pub const YAML_EMPTY_VALUES: &str = "yaml-empty-values";
pub const DUPLICATE_PAYLOAD_UUID: &str = "duplicate-payload-uuid";

// --- Extra codes emitted by PathExistsRule beyond its own name -------------
pub const PATH_CASE: &str = "path-case";
pub const PATH_EMPTY: &str = "path-empty";
pub const PATH_IS_FILE: &str = "path-is-file";

// --- Cross-file graph pass (directory lint only) ----------------------------
pub const LABEL_REFERENCE: &str = "label-reference";
pub const INSTALL_SOFTWARE_HASH: &str = "install-software-hash";
pub const INSTALL_SOFTWARE_TEAM: &str = "install-software-team";
pub const INSTALL_SOFTWARE_ID: &str = "install-software-id";
pub const APP_STORE_VPP: &str = "app-store-vpp";

// --- Workspace rules (ADR-010 Phase 1; directory lint only) -----------------
/// A `paths:` glob resolves to zero files (typically after a folder rename).
pub const BROKEN_REFERENCE: &str = "broken-reference";
/// Two repo paths differ only by case (breaks case-sensitive checkouts/CI).
pub const CASE_COLLISION: &str = "case-collision";
/// A policy runs a script that no fleet registers under `controls.scripts`.
pub const UNREGISTERED_SCRIPT: &str = "unregistered-script";
/// Byte-identical payload committed at two paths.
pub const DUPLICATE_CONTENT: &str = "duplicate-content";
/// Payload/script file referenced by no fleet/config file.
pub const ORPHANED_FILE: &str = "orphaned-file";
/// Same PayloadIdentifier in two profiles of one fleet with different content.
pub const DUPLICATE_IDENTIFIER: &str = "duplicate-identifier";

// --- Declarative patterns (ADR-010 Phase 2; [[patterns]] in .fleetlint.toml)
pub const PATTERN_NAME_MATCHES_FILENAME: &str = "pattern/name-matches-filename";
pub const PATTERN_FILENAME: &str = "pattern/filename";
pub const PATTERN_CONTENT_MUST_MATCH: &str = "pattern/content-must-match";
pub const PATTERN_CONTENT_MUST_NOT_MATCH: &str = "pattern/content-must-not-match";
pub const PATTERN_TOKEN_CONSISTENCY: &str = "pattern/token-consistency";
pub const PATTERN_MUST_BE_REFERENCED: &str = "pattern/must-be-referenced";
pub const PATTERN_UNIQUE_CONTENT_WITHIN: &str = "pattern/unique-content-within";
pub const PATTERN_REQUIRED_STRUCTURE: &str = "pattern/required-structure";
pub const PATTERN_FORBID_FILE: &str = "pattern/forbid-file";

/// `pattern/<assert>` code for an assert kind (codes are static; the set of
/// asserts is closed — config validation rejects unknown ones).
pub fn pattern_code(assert: &str) -> &'static str {
    match assert {
        "name-matches-filename" => PATTERN_NAME_MATCHES_FILENAME,
        "filename" => PATTERN_FILENAME,
        "content-must-match" => PATTERN_CONTENT_MUST_MATCH,
        "content-must-not-match" => PATTERN_CONTENT_MUST_NOT_MATCH,
        "token-consistency" => PATTERN_TOKEN_CONSISTENCY,
        "must-be-referenced" => PATTERN_MUST_BE_REFERENCED,
        "unique-content-within" => PATTERN_UNIQUE_CONTENT_WITHIN,
        "required-structure" => PATTERN_REQUIRED_STRUCTURE,
        _ => PATTERN_FORBID_FILE,
    }
}

// --- Engine YAML-hygiene checks ---------------------------------------------
pub const YAML_TABS: &str = "yaml-tabs";
pub const YAML_TRAILING_WHITESPACE: &str = "yaml-trailing-whitespace";
pub const YAML_DUPLICATE_KEY: &str = "yaml-duplicate-key";
pub const YAML_SYNTAX: &str = "yaml-syntax";

/// The cross-file codes the engine gates in `run_cross_reference_pass`.
pub const CROSS_FILE: &[&str] = &[
    LABEL_REFERENCE,
    INSTALL_SOFTWARE_HASH,
    INSTALL_SOFTWARE_TEAM,
    INSTALL_SOFTWARE_ID,
    APP_STORE_VPP,
    BROKEN_REFERENCE,
    CASE_COLLISION,
    UNREGISTERED_SCRIPT,
    DUPLICATE_CONTENT,
    ORPHANED_FILE,
    DUPLICATE_IDENTIFIER,
];

/// Registry metadata for one diagnostic code.
pub struct RuleMeta {
    pub code: &'static str,
    pub category: &'static str,
    /// Full documentation URL, when the code has one.
    pub doc_url: Option<&'static str>,
    pub fixable: bool,
    /// Drives the LSP `DiagnosticTag::DEPRECATED` strikethrough.
    pub is_deprecation: bool,
}

macro_rules! meta {
    ($code:expr, $cat:expr, $doc:expr, $fix:expr, $dep:expr) => {
        RuleMeta {
            code: $code,
            category: $cat,
            doc_url: $doc,
            fixable: $fix,
            is_deprecation: $dep,
        }
    };
}

/// One entry per code flint can emit. Guarded by `registry_is_complete`.
pub static REGISTRY: &[RuleMeta] = &[
    // Core rules — doc URLs merged from the rules' own docs_url() impls and
    // the LSP's former doc_url_for_code table (rule's own wins on conflict).
    meta!(REQUIRED_FIELDS, "structural", Some(concat_url!("#gitops")), true, false),
    meta!(PLATFORM_COMPATIBILITY, "semantic", Some(concat_url!("#policies")), false, false),
    meta!(TYPE_VALIDATION, "structural", Some(concat_url!("#policies")), true, false),
    meta!(SECURITY, "security", Some(concat_url!("#policies")), true, false),
    meta!(INTERVAL_VALIDATION, "style", Some(concat_url!("#reports")), true, false),
    meta!(DUPLICATE_NAMES, "structural", Some(concat_url!("#gitops")), false, false),
    meta!(QUERY_SYNTAX, "semantic", Some(concat_url!("#reports")), false, false),
    meta!(QUERY_LENGTH, "semantic", Some(concat_url!("#reports")), false, false),
    meta!(FMA_SLUG, "semantic", None, true, false),
    meta!(STRUCTURAL_VALIDATION, "structural", Some(DOCS), true, false),
    meta!(LABEL_TARGETING, "semantic", Some(concat_url!("#policies")), false, false),
    meta!(LABEL_MEMBERSHIP, "semantic", Some(concat_url!("#labels")), false, false),
    meta!(DATE_FORMAT, "semantic", Some(concat_url!("#macos_updates")), true, false),
    meta!(PATCH_POLICY, "semantic", Some(concat_url!("#patch-policy")), false, false),
    meta!(POLICY_AUTOMATION_LOCATION, "semantic", Some(concat_url!("#policies")), false, false),
    meta!(HASH_FORMAT, "semantic", Some(concat_url!("#packages")), true, false),
    meta!(
        CATEGORIES,
        "semantic",
        Some(concat_url!("#self_service-labels-categories-and-setup_experience")),
        true,
        false
    ),
    meta!(FILE_EXTENSION, "semantic", Some(concat_url!("#controls")), false, false),
    meta!(SECRET_HYGIENE, "security", Some(concat_url!("#policies")), true, false),
    meta!(PATH_REFERENCE, "semantic", None, false, false),
    meta!(SHEBANG_SYNTAX, "semantic", None, false, false),
    meta!(WEBHOOK_ENDPOINT_VALID, "semantic", None, false, false),
    meta!(SOFTWARE_URL, "semantic", Some(concat_url!("#packages")), false, false),
    meta!(SOFTWARE_SOURCE, "semantic", Some(concat_url!("#packages")), false, false),
    meta!(CALENDAR_EVENT_COERCION, "semantic", None, false, false),
    meta!(PATH_EXISTS, "structural", None, true, false),
    meta!(SELF_REFERENCE, "structural", Some(concat_url!("#gitops")), false, false),
    meta!(DEPRECATED_KEYS, "deprecation", Some(concat_url!("#gitops")), true, true),
    meta!(YAML_INDENTATION, "yaml", None, true, false),
    meta!(YAML_COLONS, "yaml", None, true, false),
    meta!(YAML_EMPTY_VALUES, "yaml", None, false, false),
    meta!(DUPLICATE_PAYLOAD_UUID, "semantic", None, false, false),
    // PathExistsRule's extra codes.
    meta!(PATH_CASE, "structural", None, true, false),
    meta!(PATH_EMPTY, "structural", None, false, false),
    meta!(PATH_IS_FILE, "structural", None, true, false),
    // Cross-file graph pass.
    meta!(LABEL_REFERENCE, "cross-file", None, false, false),
    meta!(INSTALL_SOFTWARE_HASH, "cross-file", None, false, false),
    meta!(INSTALL_SOFTWARE_TEAM, "cross-file", None, false, false),
    meta!(INSTALL_SOFTWARE_ID, "cross-file", None, false, false),
    meta!(APP_STORE_VPP, "cross-file", None, false, false),
    // Workspace rules (ADR-010 Phase 1).
    meta!(BROKEN_REFERENCE, "cross-file", None, false, false),
    meta!(CASE_COLLISION, "cross-file", None, false, false),
    meta!(UNREGISTERED_SCRIPT, "cross-file", Some(concat_url!("#run-script")), false, false),
    meta!(DUPLICATE_CONTENT, "cross-file", None, false, false),
    meta!(ORPHANED_FILE, "cross-file", None, false, false),
    meta!(DUPLICATE_IDENTIFIER, "cross-file", None, false, false),
    meta!(PATTERN_NAME_MATCHES_FILENAME, "pattern", None, false, false),
    meta!(PATTERN_FILENAME, "pattern", None, false, false),
    meta!(PATTERN_CONTENT_MUST_MATCH, "pattern", None, false, false),
    meta!(PATTERN_CONTENT_MUST_NOT_MATCH, "pattern", None, false, false),
    meta!(PATTERN_TOKEN_CONSISTENCY, "pattern", None, false, false),
    meta!(PATTERN_MUST_BE_REFERENCED, "pattern", None, false, false),
    meta!(PATTERN_UNIQUE_CONTENT_WITHIN, "pattern", None, false, false),
    meta!(PATTERN_REQUIRED_STRUCTURE, "pattern", None, false, false),
    meta!(PATTERN_FORBID_FILE, "pattern", None, false, false),
    // Engine hygiene checks.
    meta!(YAML_TABS, "yaml", Some(concat_url!("#gitops")), false, false),
    meta!(YAML_TRAILING_WHITESPACE, "yaml", None, false, false),
    meta!(YAML_DUPLICATE_KEY, "yaml", Some(concat_url!("#gitops")), false, false),
    meta!(YAML_SYNTAX, "yaml", Some(concat_url!("#gitops")), false, false),
];

/// Look up a code's registry entry.
pub fn meta(code: &str) -> Option<&'static RuleMeta> {
    REGISTRY.iter().find(|m| m.code == code)
}

/// The full documentation URL for a code, if it has one.
pub fn doc_url(code: &str) -> Option<&'static str> {
    meta(code).and_then(|m| m.doc_url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSet;

    /// Every registered rule's name and every extra/cross-file/hygiene code
    /// must have a REGISTRY entry — the guard against the old two-namespace
    /// drift where cross-file codes were invisible to tooling.
    #[test]
    fn registry_is_complete() {
        for rule in RuleSet::default_rules().rules() {
            assert!(
                meta(rule.name()).is_some(),
                "rule '{}' missing from codes::REGISTRY",
                rule.name()
            );
        }
        let extra = [
            PATH_CASE,
            PATH_EMPTY,
            PATH_IS_FILE,
            QUERY_LENGTH,
            LABEL_REFERENCE,
            INSTALL_SOFTWARE_HASH,
            INSTALL_SOFTWARE_TEAM,
            INSTALL_SOFTWARE_ID,
            APP_STORE_VPP,
            BROKEN_REFERENCE,
            CASE_COLLISION,
            UNREGISTERED_SCRIPT,
            DUPLICATE_CONTENT,
    ORPHANED_FILE,
    DUPLICATE_IDENTIFIER,
            PATTERN_NAME_MATCHES_FILENAME,
            PATTERN_FILENAME,
            PATTERN_CONTENT_MUST_MATCH,
            PATTERN_CONTENT_MUST_NOT_MATCH,
            PATTERN_TOKEN_CONSISTENCY,
            PATTERN_MUST_BE_REFERENCED,
            PATTERN_UNIQUE_CONTENT_WITHIN,
            PATTERN_REQUIRED_STRUCTURE,
            PATTERN_FORBID_FILE,
            YAML_TABS,
            YAML_TRAILING_WHITESPACE,
            YAML_DUPLICATE_KEY,
            YAML_SYNTAX,
        ];
        for code in extra {
            assert!(meta(code).is_some(), "'{code}' missing from codes::REGISTRY");
        }
    }

    #[test]
    fn registry_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for m in REGISTRY {
            assert!(seen.insert(m.code), "duplicate REGISTRY entry: {}", m.code);
        }
    }

    #[test]
    fn only_deprecated_keys_is_deprecation() {
        for m in REGISTRY {
            assert_eq!(
                m.is_deprecation,
                m.code == DEPRECATED_KEYS,
                "is_deprecation wrong for {}",
                m.code
            );
        }
    }
}
