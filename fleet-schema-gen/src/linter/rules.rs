use super::error::LintError;
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

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        // Check policies
        if let Some(policies) = &config.policies {
            for (idx, policy_or_path) in policies.iter().enumerate() {
                match policy_or_path {
                    super::fleet_config::PolicyOrPath::Path { .. } => {
                        // Path references are valid, skip validation
                    }
                    super::fleet_config::PolicyOrPath::Policy(policy) => {
                        if policy.name.is_none() || policy.name.as_ref().unwrap().is_empty() {
                            errors.push(
                                LintError::error(
                                    format!("Policy #{} is missing required field 'name'", idx + 1),
                                    file,
                                )
                                .with_help("Policies must have a name field")
                            );
                        }

                        if policy.query.is_none() || policy.query.as_ref().unwrap().is_empty() {
                            errors.push(
                                LintError::error(
                                    format!("Policy '{}' is missing required field 'query'",
                                        policy.name.as_deref().unwrap_or("unnamed")),
                                    file,
                                )
                                .with_help("Policies must have a query field with osquery SQL")
                                .with_suggestion("query: \"SELECT 1 FROM ...;\"")
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
                    super::fleet_config::QueryOrPath::Path { .. } => {
                        // Path references are valid, skip validation
                    }
                    super::fleet_config::QueryOrPath::Query(query) => {
                        if query.name.is_none() || query.name.as_ref().unwrap().is_empty() {
                            errors.push(
                                LintError::error(
                                    format!("Query #{} is missing required field 'name'", idx + 1),
                                    file,
                                )
                                .with_help("Queries must have a name field")
                            );
                        }

                        if query.query.is_none() || query.query.as_ref().unwrap().is_empty() {
                            errors.push(
                                LintError::error(
                                    format!("Query '{}' is missing required field 'query'",
                                        query.name.as_deref().unwrap_or("unnamed")),
                                    file,
                                )
                                .with_help("Queries must have a query field with osquery SQL")
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
                    super::fleet_config::LabelOrPath::Path { .. } => {
                        // Path references are valid, skip validation
                    }
                    super::fleet_config::LabelOrPath::Label(label) => {
                        if label.name.is_none() || label.name.as_ref().unwrap().is_empty() {
                            errors.push(
                                LintError::error(
                                    format!("Label #{} is missing required field 'name'", idx + 1),
                                    file,
                                )
                            );
                        }

                        // Dynamic labels require a query
                        if label.label_membership_type.as_deref() == Some("dynamic") {
                            if label.query.is_none() || label.query.as_ref().unwrap().is_empty() {
                                errors.push(
                                    LintError::error(
                                        format!("Dynamic label '{}' is missing required field 'query'",
                                            label.name.as_deref().unwrap_or("unnamed")),
                                        file,
                                    )
                                    .with_help("Dynamic labels must have a query field")
                                );
                            }
                        }
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

    fn check(&self, config: &FleetConfig, file: &Path, _source: &str) -> Vec<LintError> {
        let mut errors = Vec::new();

        // Check policies
        if let Some(policies) = &config.policies {
            for policy_or_path in policies {
                if let super::fleet_config::PolicyOrPath::Policy(policy) = policy_or_path {
                    // Platform must be valid enum
                    if let Some(platform) = &policy.platform {
                        if !["darwin", "windows", "linux", "chrome"].contains(&platform.as_str()) {
                            errors.push(
                                LintError::error(
                                    format!(
                                        "Policy '{}' has invalid platform '{}'",
                                        policy.name.as_deref().unwrap_or("unnamed"),
                                        platform
                                    ),
                                    file,
                                )
                                .with_help("Valid platforms: darwin, windows, linux, chrome")
                                .with_suggestion(find_similar_platform(platform))
                            );
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
                                .with_help("Interval must be a positive integer (seconds)")
                            );
                        }
                    }

                    // Logging must be valid enum
                    if let Some(logging) = &query.logging {
                        if !["snapshot", "differential", "differential_ignore_removals"].contains(&logging.as_str()) {
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
                        .with_suggestion("webhook_settings:\n  url: $WEBHOOK_URL")
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

    // Extract table names from query (simple regex for FROM clauses)
    let re = regex::Regex::new(r"\bfrom\s+(\w+)").unwrap();
    for cap in re.captures_iter(&query_lower) {
        let table = &cap[1];

        // Check if table exists for this platform
        if let Some(table_info) = OSQUERY_TABLES.get(table) {
            if !table_info.platforms.contains(&platform) {
                errors.push(
                    LintError::error(
                        format!(
                            "{} uses table '{}' which is not available on platform '{}'",
                            item_name, table, platform
                        ),
                        file,
                    )
                    .with_help(format!(
                        "Table '{}' is only available on: {}",
                        table,
                        table_info.platforms.join(", ")
                    ))
                );
            }
        }
    }

    errors
}

fn find_similar_platform(input: &str) -> String {
    let platforms = ["darwin", "windows", "linux", "chrome"];
    let input_lower = input.to_lowercase();

    for platform in &platforms {
        if platform.starts_with(&input_lower) || input_lower.starts_with(platform) {
            return format!("Did you mean '{}'?", platform);
        }
    }

    "Use one of: darwin, windows, linux, chrome".to_string()
}
