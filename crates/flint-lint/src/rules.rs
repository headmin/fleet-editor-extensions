//! Rule trait and built-in rule implementations.
//!
//! The `Rule` trait defines the interface for all lint rules. `RuleSet` is
//! the ordered collection that the engine iterates. Rules are stateless —
//! they receive config, file path, and source, and return diagnostics.

use super::error::{LintError, Severity};
use super::fleet_config::FleetConfig;
use std::path::Path;

/// Trait for linting rules
pub trait Rule: Send + Sync {
    /// Name of the rule (e.g., "required-fields", "osquery-syntax")
    fn name(&self) -> &'static str;

    /// Description of what this rule checks
    fn description(&self) -> &'static str;

    /// Check the Fleet config and return any lint errors
    fn check(&self, config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError>;

    /// Rule category for grouping and selection
    fn category(&self) -> &'static str {
        "general"
    }

    /// URL to documentation for this rule
    fn docs_url(&self) -> Option<&'static str> {
        None
    }

    /// Whether this rule can produce auto-fixable suggestions
    fn is_fixable(&self) -> bool {
        false
    }

    /// Whether this rule is experimental (requires --preview to enable)
    fn is_preview(&self) -> bool {
        false
    }

    /// Default severity level for this rule's diagnostics
    fn default_severity(&self) -> Severity {
        Severity::Error
    }
}

/// Collection of linting rules
pub struct RuleSet {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleSet {
    pub fn new() -> Self {
        Self { rules: Vec::new() }
    }

    /// Add a rule to the set
    pub fn add_rule(&mut self, rule: Box<dyn Rule>) {
        self.rules.push(rule);
    }

    /// Get all rules
    pub fn rules(&self) -> &[Box<dyn Rule>] {
        &self.rules
    }

    /// Create default ruleset with all built-in rules
    pub fn default_rules() -> Self {
        let mut set = Self::new();

        set.add_rule(Box::new(RequiredFieldsRule));
        set.add_rule(Box::new(PlatformCompatibilityRule));
        set.add_rule(Box::new(TypeValidationRule));
        set.add_rule(Box::new(SecurityRule));
        set.add_rule(Box::new(IntervalValidationRule));
        set.add_rule(Box::new(DuplicateNamesRule));
        set.add_rule(Box::new(QuerySyntaxRule));
        set.add_rule(Box::new(super::structural::StructuralValidationRule));
        set.add_rule(Box::new(super::self_reference::SelfReferenceRule));
        set.add_rule(Box::new(super::deprecation_rule::DeprecationRule::dormant()));

        // Semantic rules
        set.add_rule(Box::new(super::semantic::LabelTargetingRule));
        set.add_rule(Box::new(super::semantic::LabelMembershipRule));
        set.add_rule(Box::new(super::semantic::PatchPolicyRule));
        set.add_rule(Box::new(super::semantic::PolicyAutomationLocationRule));
        set.add_rule(Box::new(super::semantic::DateFormatRule));
        set.add_rule(Box::new(super::semantic::HashFormatRule));
        set.add_rule(Box::new(super::semantic::CategoriesRule));
        set.add_rule(Box::new(super::semantic::FileExtensionRule));
        set.add_rule(Box::new(super::semantic::SecretHygieneRule));
        set.add_rule(Box::new(super::semantic::PathReferenceRule));
        set.add_rule(Box::new(super::semantic::ShebangSyntaxRule));
        set.add_rule(Box::new(super::semantic::WebhookEndpointRule));
        set.add_rule(Box::new(super::semantic::CalendarEventCoercionRule));

        // YAML hygiene rules (ADR-008)
        set.add_rule(Box::new(super::yaml_lint::YamlIndentationRule));
        set.add_rule(Box::new(super::yaml_lint::YamlColonsRule));
        set.add_rule(Box::new(super::yaml_lint::YamlEmptyValuesRule));

        set
    }

    /// Create default ruleset with a specific version context for deprecation checking.
    pub fn default_rules_with_version(version_ctx: super::version_gate::VersionContext) -> Self {
        let mut set = Self::new();

        set.add_rule(Box::new(RequiredFieldsRule));
        set.add_rule(Box::new(PlatformCompatibilityRule));
        set.add_rule(Box::new(TypeValidationRule));
        set.add_rule(Box::new(SecurityRule));
        set.add_rule(Box::new(IntervalValidationRule));
        set.add_rule(Box::new(DuplicateNamesRule));
        set.add_rule(Box::new(QuerySyntaxRule));
        set.add_rule(Box::new(super::structural::StructuralValidationRule));
        set.add_rule(Box::new(super::self_reference::SelfReferenceRule));
        set.add_rule(Box::new(super::deprecation_rule::DeprecationRule::new(
            version_ctx,
        )));

        // Semantic rules
        set.add_rule(Box::new(super::semantic::LabelTargetingRule));
        set.add_rule(Box::new(super::semantic::LabelMembershipRule));
        set.add_rule(Box::new(super::semantic::PatchPolicyRule));
        set.add_rule(Box::new(super::semantic::PolicyAutomationLocationRule));
        set.add_rule(Box::new(super::semantic::DateFormatRule));
        set.add_rule(Box::new(super::semantic::HashFormatRule));
        set.add_rule(Box::new(super::semantic::CategoriesRule));
        set.add_rule(Box::new(super::semantic::FileExtensionRule));
        set.add_rule(Box::new(super::semantic::SecretHygieneRule));
        set.add_rule(Box::new(super::semantic::PathReferenceRule));
        set.add_rule(Box::new(super::semantic::ShebangSyntaxRule));
        set.add_rule(Box::new(super::semantic::WebhookEndpointRule));
        set.add_rule(Box::new(super::semantic::CalendarEventCoercionRule));

        // YAML hygiene rules (ADR-008)
        set.add_rule(Box::new(super::yaml_lint::YamlIndentationRule));
        set.add_rule(Box::new(super::yaml_lint::YamlColonsRule));
        set.add_rule(Box::new(super::yaml_lint::YamlEmptyValuesRule));

        set
    }
}

impl Default for RuleSet {
    fn default() -> Self {
        Self::default_rules()
    }
}

// ============================================================================
// Built-in Rules
// ============================================================================

/// Check that required fields are present
pub struct RequiredFieldsRule;

impl Rule for RequiredFieldsRule {
    fn name(&self) -> &'static str {
        "required-fields"
    }
    fn description(&self) -> &'static str {
        "Ensures all required fields are present"
    }
    fn category(&self) -> &'static str {
        "structural"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://fleetdm.com/docs/configuration/yaml-files#gitops")
    }

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        // Check policies
        if let Some(policies) = &config.policies {
            for (idx, policy_or_path) in policies.iter().enumerate() {
                match policy_or_path {
                    super::fleet_config::PolicyOrPath::Path { .. }
                    | super::fleet_config::PolicyOrPath::Paths { .. } => {
                        // Path/glob references are valid, skip validation
                    }
                    super::fleet_config::PolicyOrPath::Policy(policy) => {
                        if policy.name.is_none() || policy.name.as_ref().unwrap().is_empty() {
                            errors.push(
                                LintError::error(
                                    format!("Policy #{} is missing required field 'name'", idx + 1),
                                    file,
                                )
                                .with_help("Policies must have a name field"),
                            );
                        }

                        // Patch policies (`type: patch`) auto-generate the query
                        // from the Fleet-Maintained App metadata — don't require
                        // the user to provide one (yaml-files.md:147).
                        let is_patch = policy.policy_type.as_deref() == Some("patch");
                        if !is_patch
                            && (policy.query.is_none() || policy.query.as_ref().unwrap().is_empty())
                        {
                            errors.push(
                                LintError::error(
                                    format!(
                                        "Policy '{}' is missing required field 'query'",
                                        policy.name.as_deref().unwrap_or("unnamed")
                                    ),
                                    file,
                                )
                                .with_help("Policies must have a query field with osquery SQL (or set type: patch for Fleet Maintained App patch policies)")
                                .with_suggestion("query: \"SELECT 1 FROM ...;\""),
                            );
                        }
                    }
                }
            }
        }

        // Check queries
        if let Some(queries) = &config.queries {
            for (idx, query_or_path) in queries.iter().enumerate() {
                match query_or_path {
                    super::fleet_config::QueryOrPath::Path { .. }
                    | super::fleet_config::QueryOrPath::Paths { .. } => {
                        // Path/glob references are valid, skip validation
                    }
                    super::fleet_config::QueryOrPath::Query(query) => {
                        if query.name.is_none() || query.name.as_ref().unwrap().is_empty() {
                            errors.push(
                                LintError::error(
                                    format!("Query #{} is missing required field 'name'", idx + 1),
                                    file,
                                )
                                .with_help("Queries must have a name field"),
                            );
                        }

                        if query.query.is_none() || query.query.as_ref().unwrap().is_empty() {
                            errors.push(
                                LintError::error(
                                    format!(
                                        "Query '{}' is missing required field 'query'",
                                        query.name.as_deref().unwrap_or("unnamed")
                                    ),
                                    file,
                                )
                                .with_help("Queries must have a query field with osquery SQL"),
                            );
                        }
                    }
                }
            }
        }

        // Check labels
        if let Some(labels) = &config.labels {
            for (idx, label_or_path) in labels.iter().enumerate() {
                match label_or_path {
                    super::fleet_config::LabelOrPath::Path { .. }
                    | super::fleet_config::LabelOrPath::Paths { .. } => {
                        // Path/glob references are valid, skip validation
                    }
                    super::fleet_config::LabelOrPath::Label(label) => {
                        if label.name.is_none() || label.name.as_ref().unwrap().is_empty() {
                            errors.push(LintError::error(
                                format!("Label #{} is missing required field 'name'", idx + 1),
                                file,
                            ));
                        }

                        // Label membership consistency is checked by LabelMembershipRule
                    }
                }
            }
        }

        errors
    }
}

/// Check platform compatibility
pub struct PlatformCompatibilityRule;

impl Rule for PlatformCompatibilityRule {
    fn name(&self) -> &'static str {
        "platform-compatibility"
    }
    fn description(&self) -> &'static str {
        "Validates osquery tables are compatible with specified platforms"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://fleetdm.com/docs/configuration/yaml-files#policies")
    }

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        // Check policies
        if let Some(policies) = &config.policies {
            for policy_or_path in policies {
                if let super::fleet_config::PolicyOrPath::Policy(policy) = policy_or_path {
                    if let (Some(platform), Some(query)) = (&policy.platform, &policy.query) {
                        errors.extend(check_query_platform_compat(
                            query,
                            platform,
                            &format!("Policy '{}'", policy.name.as_deref().unwrap_or("unnamed")),
                            file,
                        ));
                    }
                }
            }
        }

        // Check queries
        if let Some(queries) = &config.queries {
            for query_or_path in queries {
                if let super::fleet_config::QueryOrPath::Query(query) = query_or_path {
                    if let (Some(platform), Some(query_sql)) = (&query.platform, &query.query) {
                        errors.extend(check_query_platform_compat(
                            query_sql,
                            platform,
                            &format!("Query '{}'", query.name.as_deref().unwrap_or("unnamed")),
                            file,
                        ));
                    }
                }
            }
        }

        errors
    }
}

/// Check type correctness
pub struct TypeValidationRule;

impl Rule for TypeValidationRule {
    fn name(&self) -> &'static str {
        "type-validation"
    }

    fn description(&self) -> &'static str {
        "Validates field types match expected values"
    }
    fn category(&self) -> &'static str {
        "structural"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://fleetdm.com/docs/configuration/yaml-files#policies")
    }

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        // Check policies
        if let Some(policies) = &config.policies {
            for policy_or_path in policies {
                if let super::fleet_config::PolicyOrPath::Policy(policy) = policy_or_path {
                    // Platform may be a single value or comma-separated (e.g. "darwin,linux"),
                    // and an empty string means "all platforms". Validate each component.
                    if let Some(platform) = &policy.platform {
                        const VALID: &[&str] = &[
                            "darwin", "windows", "linux", "chrome", "ios", "ipados", "android",
                        ];
                        for component in platform.split(',').map(str::trim).filter(|p| !p.is_empty())
                        {
                            if !VALID.contains(&component) {
                                let mut err = LintError::error(
                                    format!(
                                        "Policy '{}' has invalid platform '{}'",
                                        policy.name.as_deref().unwrap_or("unnamed"),
                                        component
                                    ),
                                    file,
                                )
                                .with_help("Valid platforms: darwin, windows, linux, chrome, ios, ipados, android")
                                .with_suggestion(find_similar_platform(component))
                                .with_fix_safety(super::error::FixSafety::Safe)
                                .with_context(component.to_string());

                                // Find line number in source for --fix support
                                if let Some(line) =
                                    super::yaml_utils::find_key_line(_source, "platform", 0)
                                {
                                    err = err.with_location(line, 1);
                                }
                                errors.push(err);
                            }
                        }
                    }
                }
            }
        }

        // Check queries
        if let Some(queries) = &config.queries {
            for query_or_path in queries {
                if let super::fleet_config::QueryOrPath::Query(query) = query_or_path {
                    // Interval must be positive integer
                    if let Some(interval) = query.interval {
                        if interval <= 0 {
                            errors.push(
                                LintError::error(
                                    format!(
                                        "Query '{}' has invalid interval {}",
                                        query.name.as_deref().unwrap_or("unnamed"),
                                        interval
                                    ),
                                    file,
                                )
                                .with_help("Interval must be a positive integer (seconds)"),
                            );
                        }
                    }

                    // Logging must be valid enum
                    if let Some(logging) = &query.logging {
                        if !["snapshot", "differential", "differential_ignore_removals"]
                            .contains(&logging.as_str())
                        {
                            errors.push(
                                LintError::error(
                                    format!(
                                        "Query '{}' has invalid logging type '{}'",
                                        query.name.as_deref().unwrap_or("unnamed"),
                                        logging
                                    ),
                                    file,
                                )
                                .with_help("Valid logging types: snapshot, differential, differential_ignore_removals")
                                .with_suggestion(find_similar_logging(logging))
                                .with_fix_safety(super::error::FixSafety::Safe)
                            );
                        }
                    }
                }
            }
        }

        errors
    }
}

/// Check for security issues
pub struct SecurityRule;

impl Rule for SecurityRule {
    fn name(&self) -> &'static str {
        "security"
    }

    fn description(&self) -> &'static str {
        "Detects potential security issues like hardcoded secrets"
    }
    fn category(&self) -> &'static str {
        "security"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn default_severity(&self) -> Severity {
        Severity::Warning
    }

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        // Check webhook URLs for tokens
        if let Some(webhook) = &config.webhook_settings {
            if let Some(url) = &webhook.url {
                if url.contains("token=") || url.contains("api_key=") || url.contains("secret=") {
                    errors.push(
                        LintError::warning(
                            "Webhook URL appears to contain a token or API key",
                            file,
                        )
                        .with_help("Use environment variables for secrets: $WEBHOOK_URL")
                        .with_suggestion("webhook_settings:\n  url: $WEBHOOK_URL"),
                    );
                }
            }
        }

        errors
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn check_query_platform_compat(
    query: &str,
    platform: &str,
    item_name: &str,
    file: &Path,
) -> Vec<LintError> {
    use super::osquery::OSQUERY_TABLES;

    let mut errors = Vec::new();
    let query_lower = query.to_lowercase();

    // Fleet's `platform` field is comma-separated (e.g. "darwin,linux")
    // meaning the query targets *all* listed platforms. Split before
    // comparing so we don't emit a literal-string false positive.
    // Empty string means "all platforms" — skip the check entirely.
    let trimmed = platform.trim();
    if trimmed.is_empty() {
        return errors;
    }
    let target_platforms: Vec<&str> = trimmed
        .split(',')
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();

    // Extract table names from query (simple regex for FROM clauses)
    let re = regex::Regex::new(r"\bfrom\s+(\w+)").unwrap();
    for cap in re.captures_iter(&query_lower) {
        let table = &cap[1];

        if let Some(table_info) = OSQUERY_TABLES.get(table) {
            // Flag each individual platform that the table doesn't support.
            // A query targeting `darwin,linux` against a darwin-only table
            // should report an error for `linux`, not for the joined string.
            for p in &target_platforms {
                if !table_info.platforms.contains(p) {
                    errors.push(
                        LintError::error(
                            format!(
                                "{} uses table '{}' which is not available on platform '{}'",
                                item_name, table, p
                            ),
                            file,
                        )
                        .with_help(format!(
                            "Table '{}' is only available on: {}",
                            table,
                            table_info.platforms.join(", ")
                        )),
                    );
                }
            }
        }
    }

    errors
}

/// Strip SQL comments from a query string.
///
/// Removes `/* ... */` block comments and `-- ...` line comments so that
/// English text inside comments (e.g., apostrophes in "organization's")
/// doesn't trigger false positives in quote balancing or keyword checks.
fn strip_sql_comments(query: &str) -> String {
    let mut result = String::with_capacity(query.len());
    let bytes = query.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
            // Block comment — skip until */ (or end of string if unterminated)
            i += 2;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2; // skip */
            } else {
                i = len; // unterminated comment — consume rest of input
            }
            result.push(' '); // replace comment with space to preserve token boundaries
        } else if i + 1 < len && bytes[i] == b'-' && bytes[i + 1] == b'-' {
            // Line comment — skip until newline
            i += 2;
            while i < len && bytes[i] != b'\n' {
                i += 1;
            }
        } else {
            result.push(bytes[i] as char);
            i += 1;
        }
    }

    result
}

/// Strip single-quoted string literals from SQL so keywords inside strings
/// (e.g., `'%Drop Box%'`) don't trigger false positives.
fn strip_sql_string_literals(sql: &str) -> String {
    let mut result = String::with_capacity(sql.len());
    let mut in_string = false;

    for ch in sql.chars() {
        if ch == '\'' {
            in_string = !in_string;
            result.push(ch);
        } else if in_string {
            // Replace string content with spaces to preserve positions
            result.push(' ');
        } else {
            result.push(ch);
        }
    }

    result
}

/// Find the most similar valid logging type for a suggestion.
fn find_similar_logging(input: &str) -> String {
    let input_lower = input.to_lowercase();

    if input_lower.contains("diff") {
        if input_lower.contains("ignore") {
            return "differential_ignore_removals".to_string();
        }
        return "differential".to_string();
    }
    if input_lower.contains("snap") {
        return "snapshot".to_string();
    }

    // Default to snapshot
    "snapshot".to_string()
}

/// Find the most similar valid platform for a suggestion.
/// Returns the platform name itself (not a message) for use in code actions.
fn find_similar_platform(input: &str) -> String {
    let platforms = [
        "darwin", "windows", "linux", "chrome", "ios", "ipados", "android",
    ];
    let input_lower = input.to_lowercase();

    // Check for common typos and variations
    for platform in &platforms {
        if platform.starts_with(&input_lower) || input_lower.starts_with(platform) {
            return (*platform).to_string();
        }
    }

    // Check for common aliases
    match input_lower.as_str() {
        "macos" | "mac" | "osx" | "apple" => "darwin".to_string(),
        "win" | "win32" | "win64" => "windows".to_string(),
        "ubuntu" | "debian" | "centos" | "redhat" | "fedora" => "linux".to_string(),
        "chromeos" | "chromebook" => "chrome".to_string(),
        "iphone" => "ios".to_string(),
        "ipad" => "ipados".to_string(),
        _ => "darwin".to_string(), // Default suggestion
    }
}

// ============================================================================
// Additional Rules
// ============================================================================

/// Check query interval values for sensible ranges
pub struct IntervalValidationRule;

impl Rule for IntervalValidationRule {
    fn name(&self) -> &'static str {
        "interval-validation"
    }

    fn description(&self) -> &'static str {
        "Validates query intervals are within sensible ranges"
    }
    fn category(&self) -> &'static str {
        "style"
    }
    fn is_fixable(&self) -> bool {
        true
    }
    fn default_severity(&self) -> Severity {
        Severity::Warning
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://fleetdm.com/docs/configuration/yaml-files#reports")
    }

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        if let Some(queries) = &config.queries {
            for query_or_path in queries {
                if let super::fleet_config::QueryOrPath::Query(query) = query_or_path {
                    if let Some(interval) = query.interval {
                        let name = query.name.as_deref().unwrap_or("unnamed");

                        if interval < 60 {
                            errors.push(
                                LintError::warning(
                                    format!(
                                        "Query '{}' has very short interval ({} seconds). This may cause high resource usage.",
                                        name, interval
                                    ),
                                    file,
                                )
                                .with_help("Consider using an interval of at least 60 seconds")
                                .with_suggestion("interval: 60")
                            );
                        } else if interval > 86400 {
                            errors.push(
                                LintError::info(
                                    format!(
                                        "Query '{}' has interval > 24 hours ({} seconds). Events may be missed.",
                                        name, interval
                                    ),
                                    file,
                                )
                                .with_help("Consider using a shorter interval for time-sensitive data")
                            );
                        }
                    }
                }
            }
        }

        errors
    }
}

/// Check for duplicate names across policies, queries, and labels
pub struct DuplicateNamesRule;

impl Rule for DuplicateNamesRule {
    fn name(&self) -> &'static str {
        "duplicate-names"
    }

    fn description(&self) -> &'static str {
        "Detects duplicate names within policies, queries, or labels"
    }
    fn category(&self) -> &'static str {
        "structural"
    }

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        use std::collections::HashSet;
        let mut errors = Vec::new();

        // Check policies
        if let Some(policies) = &config.policies {
            let mut seen = HashSet::new();
            for policy_or_path in policies {
                if let super::fleet_config::PolicyOrPath::Policy(policy) = policy_or_path {
                    if let Some(name) = &policy.name {
                        if !seen.insert(name.clone()) {
                            errors.push(
                                LintError::error(
                                    format!("Duplicate policy name: '{}'", name),
                                    file,
                                )
                                .with_help("Policy names must be unique within the organization"),
                            );
                        }
                    }
                }
            }
        }

        // Check queries
        if let Some(queries) = &config.queries {
            let mut seen = HashSet::new();
            for query_or_path in queries {
                if let super::fleet_config::QueryOrPath::Query(query) = query_or_path {
                    if let Some(name) = &query.name {
                        if !seen.insert(name.clone()) {
                            errors.push(
                                LintError::error(format!("Duplicate query name: '{}'", name), file)
                                    .with_help(
                                        "Query names must be unique within the organization",
                                    ),
                            );
                        }
                    }
                }
            }
        }

        // Check labels
        if let Some(labels) = &config.labels {
            let mut seen = HashSet::new();
            for label_or_path in labels {
                if let super::fleet_config::LabelOrPath::Label(label) = label_or_path {
                    if let Some(name) = &label.name {
                        if !seen.insert(name.clone()) {
                            errors.push(
                                LintError::error(format!("Duplicate label name: '{}'", name), file)
                                    .with_help(
                                        "Label names must be unique within the organization",
                                    ),
                            );
                        }
                    }
                }
            }
        }

        errors
    }
}

/// Check SQL query syntax for common issues
pub struct QuerySyntaxRule;

impl Rule for QuerySyntaxRule {
    fn name(&self) -> &'static str {
        "query-syntax"
    }

    fn description(&self) -> &'static str {
        "Validates basic SQL query syntax"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }
    fn docs_url(&self) -> Option<&'static str> {
        Some("https://fleetdm.com/docs/configuration/yaml-files#reports")
    }

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        // Check policies
        if let Some(policies) = &config.policies {
            for policy_or_path in policies {
                if let super::fleet_config::PolicyOrPath::Policy(policy) = policy_or_path {
                    if let Some(query) = &policy.query {
                        let name = policy.name.as_deref().unwrap_or("unnamed");
                        errors.extend(check_query_syntax(
                            query,
                            &format!("Policy '{}'", name),
                            file,
                        ));
                    }
                }
            }
        }

        // Check queries
        if let Some(queries) = &config.queries {
            for query_or_path in queries {
                if let super::fleet_config::QueryOrPath::Query(query) = query_or_path {
                    if let Some(query_sql) = &query.query {
                        let name = query.name.as_deref().unwrap_or("unnamed");
                        errors.extend(check_query_syntax(
                            query_sql,
                            &format!("Query '{}'", name),
                            file,
                        ));
                    }
                }
            }
        }

        // Check labels
        if let Some(labels) = &config.labels {
            for label_or_path in labels {
                if let super::fleet_config::LabelOrPath::Label(label) = label_or_path {
                    if let Some(query) = &label.query {
                        let name = label.name.as_deref().unwrap_or("unnamed");
                        errors.extend(check_query_syntax(
                            query,
                            &format!("Label '{}'", name),
                            file,
                        ));
                    }
                }
            }
        }

        errors
    }
}

fn check_query_syntax(query: &str, item_name: &str, file: &Path) -> Vec<LintError> {
    let mut errors = Vec::new();

    // Strip SQL comments before analysis to avoid false positives from
    // apostrophes in English text (e.g., "organization's") or keywords
    // in comment blocks.
    let query_stripped = strip_sql_comments(query);
    let query_upper = query_stripped.to_uppercase();

    // Check for SELECT keyword
    if !query_upper.contains("SELECT") {
        errors.push(
            LintError::error(
                format!("{} query does not contain SELECT statement", item_name),
                file,
            )
            .with_help("osquery queries must be SELECT statements"),
        );
    }

    // Warn about SELECT * (performance concern)
    let select_star_pattern = regex::Regex::new(r"(?i)SELECT\s+\*\s+FROM").unwrap();
    if select_star_pattern.is_match(query) {
        errors.push(
            LintError::info(
                format!(
                    "{} uses SELECT * which may return unnecessary data",
                    item_name
                ),
                file,
            )
            .with_help("Consider selecting only the columns you need for better performance"),
        );
    }

    // Check for unbalanced parentheses
    let open_parens = query.matches('(').count();
    let close_parens = query.matches(')').count();
    if open_parens != close_parens {
        errors.push(
            LintError::error(
                format!(
                    "{} has unbalanced parentheses ({} open, {} close)",
                    item_name, open_parens, close_parens
                ),
                file,
            )
            .with_help("Check that all parentheses are properly matched"),
        );
    }

    // Check for unbalanced quotes (on comment-stripped query)
    let single_quotes = query_stripped.matches('\'').count();
    if !single_quotes.is_multiple_of(2) {
        errors.push(
            LintError::error(format!("{} has unbalanced single quotes", item_name), file)
                .with_help("Check that all string literals are properly quoted"),
        );
    }

    // Check for common dangerous patterns (word-boundary aware to avoid false positives
    // like "software_update" matching "UPDATE").
    // First strip string literals so keywords inside quotes (e.g., '%Drop Box%')
    // don't trigger false positives.
    let query_no_strings = strip_sql_string_literals(&query_upper);
    let is_dangerous_sql = |q: &str, keyword: &str| -> bool {
        for (i, _) in q.match_indices(keyword) {
            // Check character before — must be start-of-string or non-alphanumeric/underscore
            let before_ok = i == 0 || {
                let c = q.as_bytes()[i - 1];
                !(c.is_ascii_alphanumeric() || c == b'_')
            };
            if before_ok {
                return true;
            }
        }
        false
    };
    if is_dangerous_sql(&query_no_strings, "DROP ")
        || is_dangerous_sql(&query_no_strings, "DELETE ")
        || is_dangerous_sql(&query_no_strings, "INSERT ")
        || is_dangerous_sql(&query_no_strings, "UPDATE ")
    {
        errors.push(
            LintError::error(
                format!("{} contains non-SELECT SQL statement", item_name),
                file,
            )
            .with_help("osquery only supports SELECT queries"),
        );
    }

    // Note: Trailing semicolons in queries are common and OK - don't warn about them

    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_comments_block() {
        let sql = "SELECT 1 /* comment */ FROM t";
        assert_eq!(strip_sql_comments(sql), "SELECT 1   FROM t");
    }

    #[test]
    fn strip_comments_line() {
        let sql = "SELECT 1 -- comment\nFROM t";
        assert_eq!(strip_sql_comments(sql), "SELECT 1 \nFROM t");
    }

    #[test]
    fn strip_comments_unterminated_block() {
        // Unterminated block comment — should not panic, consumes rest of input
        let sql = "SELECT 1 /* no closing";
        let result = strip_sql_comments(sql);
        assert_eq!(result, "SELECT 1  ");
        assert!(!result.contains("no closing"));
    }

    #[test]
    fn strip_comments_apostrophe_in_comment() {
        // The apostrophe in "organization's" should be stripped with the comment
        let sql = "SELECT 1 /*organization's decision*/";
        let result = strip_sql_comments(sql);
        assert!(
            !result.contains('\''),
            "Apostrophe should be stripped: {}",
            result
        );
    }

    #[test]
    fn strip_string_literals_preserves_keywords() {
        let sql = "SELECT 1 WHERE name = 'DROP TABLE'";
        let result = strip_sql_string_literals(sql);
        assert!(
            !result.contains("DROP TABLE"),
            "Keywords inside strings should be blanked"
        );
        assert!(result.contains("SELECT"));
    }

    #[test]
    fn strip_string_literals_drop_box() {
        let sql = "SELECT 1 WHERE path NOT LIKE '%Drop Box%'";
        let result = strip_sql_string_literals(sql);
        assert!(
            !result.contains("Drop"),
            "Drop inside string literal should be blanked"
        );
    }

    // -- Issue #4: comma-separated platform handling --

    #[test]
    fn platform_compat_single_platform_supported() {
        // usb_devices is on darwin — should be clean.
        let errs = check_query_platform_compat(
            "SELECT 1 FROM usb_devices",
            "darwin",
            "Query 'q'",
            std::path::Path::new("test.yml"),
        );
        assert!(errs.is_empty(), "got: {:?}", errs);
    }

    #[test]
    fn platform_compat_comma_separated_all_supported() {
        // usb_devices is on darwin+linux — `darwin,linux` must NOT trigger
        // (was Bug A in issue #4: literal "darwin,linux" never matched).
        let errs = check_query_platform_compat(
            "SELECT 1 FROM usb_devices",
            "darwin,linux",
            "Query 'q'",
            std::path::Path::new("test.yml"),
        );
        assert!(errs.is_empty(), "got: {:?}", errs);
    }

    #[test]
    fn platform_compat_comma_separated_with_unsupported() {
        // Targeting darwin,windows but usb_devices isn't on Windows.
        // Expect a single error pinpointing `windows`, not the joined string.
        let errs = check_query_platform_compat(
            "SELECT 1 FROM usb_devices",
            "darwin,windows",
            "Query 'q'",
            std::path::Path::new("test.yml"),
        );
        assert_eq!(errs.len(), 1);
        assert!(
            errs[0].message.contains("'windows'"),
            "expected error to name 'windows', got: {}",
            errs[0].message
        );
    }

    #[test]
    fn platform_compat_empty_platform_skips() {
        // Empty platform = "all platforms" — flint shouldn't second-guess.
        let errs = check_query_platform_compat(
            "SELECT 1 FROM usb_devices",
            "",
            "Query 'q'",
            std::path::Path::new("test.yml"),
        );
        assert!(errs.is_empty());
    }

    #[test]
    fn platform_compat_handles_whitespace_in_list() {
        let errs = check_query_platform_compat(
            "SELECT 1 FROM usb_devices",
            "darwin , linux",
            "Query 'q'",
            std::path::Path::new("test.yml"),
        );
        assert!(errs.is_empty(), "whitespace tolerance: got {:?}", errs);
    }

    #[test]
    fn platform_compat_usb_devices_rejects_windows() {
        // Issue #4 Bug B regression: usb_devices is darwin+linux only,
        // NOT windows. Source: fleetdm/fleet schema/osquery_fleet_schema.json.
        let errs = check_query_platform_compat(
            "SELECT 1 FROM usb_devices",
            "windows",
            "Query 'q'",
            std::path::Path::new("test.yml"),
        );
        assert!(
            errs.iter().any(|e| e.message.contains("usb_devices")),
            "windows should be unsupported for usb_devices: {:?}",
            errs
        );
    }
}
