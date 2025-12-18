//! Completion provider for Fleet GitOps YAML files.
//!
//! Provides context-aware autocompletion for field names, values, and osquery tables.

use std::path::Path;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, Position,
};

use super::schema::{get_field_doc, LOGGING_DOCS, PLATFORM_DOCS};
use crate::linter::osquery::OSQUERY_TABLES;

/// Context types for completion.
#[derive(Debug, Clone, PartialEq)]
enum CompletionContext {
    /// At top level of document
    TopLevel,
    /// Inside a policies array item
    PolicyField,
    /// Inside a queries array item
    QueryField,
    /// Inside a labels array item
    LabelField,
    /// Inside software section (choosing packages/app_store_apps/fleet_maintained_apps)
    SoftwareSection,
    /// Inside software.packages array item
    SoftwarePackageField,
    /// Inside software.app_store_apps array item
    AppStoreAppField,
    /// Inside software.fleet_maintained_apps array item
    FleetMaintainedAppField,
    /// Inside controls section
    ControlsSection,
    /// Inside controls.macos_settings.custom_settings array item
    MacOSCustomSettingField,
    /// Inside controls.windows_settings.custom_settings array item
    WindowsCustomSettingField,
    /// Inside controls.scripts array item
    ScriptField,
    /// Inside team_settings section
    TeamSettingsSection,
    /// Inside agent_options section
    AgentOptionsSection,
    /// After platform: key
    PlatformValue,
    /// After logging: key
    LoggingValue,
    /// After self_service: key
    BooleanValue,
    /// After path: key, completing file path value
    PathValue { context_type: PathContextType },
    /// Inside an SQL query (for osquery tables)
    SqlContext { platform: Option<String> },
    /// Unknown context
    Unknown,
}

/// Type of path being completed, determines file filtering.
#[derive(Debug, Clone, PartialEq)]
enum PathContextType {
    /// Software package definitions (*.yml)
    SoftwarePackage,
    /// Scripts (*.sh, *.ps1)
    Script,
    /// macOS profiles (*.mobileconfig)
    MacOSProfile,
    /// Windows profiles (*.xml)
    WindowsProfile,
    /// Policy definitions (*.yml)
    Policy,
    /// Query definitions (*.yml)
    Query,
    /// Label definitions (*.yml)
    Label,
    /// Generic file reference
    Generic,
}

/// Provide completion items at a position in a Fleet YAML document.
/// For file path completions, use `complete_at_with_context` instead.
pub fn complete_at(source: &str, position: Position) -> Vec<CompletionItem> {
    complete_at_with_context(source, position, None, None)
}

/// Provide completion items with workspace context for file path completions.
pub fn complete_at_with_context(
    source: &str,
    position: Position,
    current_file: Option<&Path>,
    workspace_root: Option<&Path>,
) -> Vec<CompletionItem> {
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    // Get the line content (empty string if no line at that position)
    let line = source.lines().nth(line_idx).unwrap_or("");

    // Determine the context
    let context = determine_completion_context(source, line_idx, line, col_idx);

    match context {
        CompletionContext::TopLevel => complete_top_level_fields(),
        CompletionContext::PolicyField => complete_policy_fields(line, col_idx),
        CompletionContext::QueryField => complete_query_fields(line, col_idx),
        CompletionContext::LabelField => complete_label_fields(line, col_idx),
        CompletionContext::SoftwareSection => complete_software_section(),
        CompletionContext::SoftwarePackageField => complete_software_package_fields(line, col_idx),
        CompletionContext::AppStoreAppField => complete_app_store_app_fields(line, col_idx),
        CompletionContext::FleetMaintainedAppField => complete_fleet_maintained_app_fields(line, col_idx),
        CompletionContext::ControlsSection => complete_controls_section(),
        CompletionContext::MacOSCustomSettingField => complete_custom_setting_fields(line, col_idx),
        CompletionContext::WindowsCustomSettingField => complete_custom_setting_fields(line, col_idx),
        CompletionContext::ScriptField => complete_script_fields(line, col_idx),
        CompletionContext::TeamSettingsSection => complete_team_settings_section(),
        CompletionContext::AgentOptionsSection => complete_agent_options_section(),
        CompletionContext::PlatformValue => complete_platform_values(),
        CompletionContext::LoggingValue => complete_logging_values(),
        CompletionContext::BooleanValue => complete_boolean_values(),
        CompletionContext::PathValue { context_type } => {
            complete_file_paths(line, col_idx, current_file, workspace_root, context_type)
        }
        CompletionContext::SqlContext { platform } => complete_osquery_tables(platform.as_deref()),
        CompletionContext::Unknown => vec![],
    }
}

/// Determine the completion context based on cursor position and surrounding content.
fn determine_completion_context(
    source: &str,
    line_idx: usize,
    line: &str,
    col_idx: usize,
) -> CompletionContext {
    let trimmed = line.trim();

    // Empty document or at start - suggest top-level
    if source.trim().is_empty() || (line_idx == 0 && trimmed.is_empty()) {
        return CompletionContext::TopLevel;
    }

    // Check if we're after a specific key (value position)
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return CompletionContext::PlatformValue,
            "logging" => return CompletionContext::LoggingValue,
            "path" => {
                // Determine path context type based on parent context
                let parent = find_parent_context(source, line_idx);
                let context_type = match parent.as_deref() {
                    Some(p) if p.contains("software.packages") => PathContextType::SoftwarePackage,
                    Some(p) if p.contains("fleet_maintained_apps") => PathContextType::SoftwarePackage,
                    Some(p) if p.contains("scripts") => PathContextType::Script,
                    Some(p) if p.contains("macos_settings") => PathContextType::MacOSProfile,
                    Some(p) if p.contains("windows_settings") => PathContextType::WindowsProfile,
                    // Policies, queries, and labels can also use path references
                    Some(p) if p == "policies" || p.ends_with(".policies") => PathContextType::Policy,
                    Some(p) if p == "queries" || p.ends_with(".queries") => PathContextType::Query,
                    Some(p) if p == "labels" || p.ends_with(".labels") => PathContextType::Label,
                    _ => PathContextType::Generic,
                };
                return CompletionContext::PathValue { context_type };
            }
            _ => {}
        }
    }

    // Check if we're in SQL context (inside a query field)
    if is_in_sql_context(source, line_idx, line) {
        let platform = find_platform_in_context(source, line_idx);
        return CompletionContext::SqlContext { platform };
    }

    // Determine parent context by looking at indentation and surrounding lines
    let indent = line.len() - line.trim_start().len();

    // If indent is 0 or we're at a top-level key position, suggest top-level fields
    if indent == 0 && (trimmed.is_empty() || !trimmed.contains(':')) {
        return CompletionContext::TopLevel;
    }

    // Look for parent context using path-based detection
    let parent = find_parent_context(source, line_idx);
    let context = context_path_to_completion_context(parent.as_deref());

    if context != CompletionContext::Unknown {
        return context;
    }

    // Check if we're at a position that suggests array item fields
    if indent <= 2 && (trimmed.is_empty() || trimmed.starts_with('-')) {
        return find_array_parent(source, line_idx);
    }

    CompletionContext::Unknown
}

/// Get the key if cursor is in a value position (after colon).
fn get_key_at_cursor(line: &str, col_idx: usize) -> Option<String> {
    let trimmed = line.trim().trim_start_matches('-').trim();
    if let Some(colon_pos) = line.find(':') {
        // Cursor is after the colon
        if col_idx > colon_pos {
            let key = trimmed.split(':').next()?.trim();
            return Some(key.to_string());
        }
    }
    None
}

/// Check if we're in an SQL context.
fn is_in_sql_context(source: &str, line_idx: usize, current_line: &str) -> bool {
    // Check if current line is part of a multiline query
    if current_line.trim().starts_with("SELECT")
        || current_line.trim().starts_with("FROM")
        || current_line.trim().starts_with("WHERE")
        || current_line.trim().starts_with("JOIN")
    {
        return true;
    }

    let lines: Vec<&str> = source.lines().collect();

    // Look for query: | indicator above
    for i in (0..line_idx).rev() {
        let check_line = lines.get(i).unwrap_or(&"");
        let trimmed = check_line.trim();

        if trimmed.starts_with("query:") && trimmed.contains('|') {
            return true;
        }

        // Found another key at same or less indent - not in query
        if trimmed.ends_with(':')
            && !trimmed.starts_with('-')
            && !trimmed.starts_with("query:")
        {
            let current_indent = current_line.len() - current_line.trim_start().len();
            let check_indent = check_line.len() - check_line.trim_start().len();
            if check_indent <= current_indent {
                return false;
            }
        }
    }

    false
}

/// Find the platform value in the current context (for filtering osquery tables).
fn find_platform_in_context(source: &str, line_idx: usize) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let current_indent = lines
        .get(line_idx)
        .map(|l| l.len() - l.trim_start().len())
        .unwrap_or(0);

    // Look backwards for platform: field at same or parent level
    for i in (0..=line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim().trim_start_matches('-').trim();

        if trimmed.starts_with("platform:") {
            let indent = line.len() - line.trim_start().len();
            if indent <= current_indent {
                let value = trimmed.strip_prefix("platform:")?.trim();
                return Some(value.to_string());
            }
        }

        // If we hit a new array item at parent level, stop looking
        if line.trim().starts_with("- name:") {
            let indent = line.len() - line.trim_start().len();
            if indent < current_indent {
                break;
            }
        }
    }

    None
}

/// Find the parent array context with full path support.
fn find_parent_context(source: &str, line_idx: usize) -> Option<String> {
    let lines: Vec<&str> = source.lines().collect();
    let current_line = lines.get(line_idx).unwrap_or(&"");
    let current_indent = current_line.len() - current_line.trim_start().len();

    let mut context_stack: Vec<(usize, String)> = vec![];

    for i in (0..line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Only consider lines with less indentation (parent contexts)
        if indent < current_indent {
            // Check for key definitions (ending with :)
            if let Some(key) = trimmed.strip_suffix(':') {
                context_stack.push((indent, key.to_string()));
            } else if trimmed.contains(':') && !trimmed.starts_with('-') {
                let key = trimmed.split(':').next().unwrap_or("").trim();
                if !key.is_empty() {
                    context_stack.push((indent, key.to_string()));
                }
            }
        }

        // Stop at indent 0
        if indent == 0 && !trimmed.is_empty() {
            break;
        }
    }

    // Build context path from stack (reverse to get top-down order)
    context_stack.reverse();

    // Filter to keep only strictly increasing indents (proper nesting)
    let mut last_indent: i32 = -1;
    let path: Vec<String> = context_stack
        .into_iter()
        .filter(|(indent, _)| {
            let indent_i32 = *indent as i32;
            if indent_i32 > last_indent {
                last_indent = indent_i32;
                true
            } else {
                false
            }
        })
        .map(|(_, key)| key)
        .collect();

    if path.is_empty() {
        None
    } else {
        Some(path.join("."))
    }
}

/// Find the array parent for completing array item fields.
fn find_array_parent(source: &str, line_idx: usize) -> CompletionContext {
    let context = find_parent_context(source, line_idx);
    context_path_to_completion_context(context.as_deref())
}

/// Convert a context path string to a CompletionContext.
fn context_path_to_completion_context(path: Option<&str>) -> CompletionContext {
    match path {
        Some(p) if p == "policies" || p.ends_with(".policies") => CompletionContext::PolicyField,
        Some(p) if p == "queries" || p.ends_with(".queries") => CompletionContext::QueryField,
        Some(p) if p == "labels" || p.ends_with(".labels") => CompletionContext::LabelField,
        Some(p) if p == "software" => CompletionContext::SoftwareSection,
        Some(p) if p == "software.packages" || p.ends_with(".packages") => {
            CompletionContext::SoftwarePackageField
        }
        Some(p) if p == "software.app_store_apps" || p.contains("app_store_apps") => {
            CompletionContext::AppStoreAppField
        }
        Some(p) if p == "software.fleet_maintained_apps" || p.contains("fleet_maintained_apps") => {
            CompletionContext::FleetMaintainedAppField
        }
        Some(p) if p == "controls" => CompletionContext::ControlsSection,
        Some(p) if p.contains("macos_settings.custom_settings") => {
            CompletionContext::MacOSCustomSettingField
        }
        Some(p) if p.contains("windows_settings.custom_settings") => {
            CompletionContext::WindowsCustomSettingField
        }
        Some(p) if p.contains("controls.scripts") || (p == "controls" && p.contains("scripts")) => {
            CompletionContext::ScriptField
        }
        Some(p) if p == "team_settings" || p.starts_with("team_settings") => {
            CompletionContext::TeamSettingsSection
        }
        Some(p) if p == "agent_options" || p.starts_with("agent_options") => {
            CompletionContext::AgentOptionsSection
        }
        _ => CompletionContext::Unknown,
    }
}

/// Complete top-level field names.
fn complete_top_level_fields() -> Vec<CompletionItem> {
    let fields = [
        ("name", "Team or configuration name"),
        ("policies", "List of compliance policies"),
        ("queries", "List of osquery queries"),
        ("labels", "List of host labels"),
        ("agent_options", "osquery agent configuration"),
        ("controls", "MDM controls and settings"),
        ("software", "Software packages to install"),
        ("webhook_settings", "Webhook notification configuration"),
    ];

    fields
        .iter()
        .map(|(name, desc)| create_field_completion(name, desc, true))
        .collect()
}

/// Complete policy field names.
fn complete_policy_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in value position
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return complete_platform_values(),
            _ => {}
        }
    }

    // Policies can be either inline definitions OR path references
    let fields = [
        ("path", "Reference to external policy YAML file", false),
        ("name", "Policy display name (for inline definitions)", true),
        ("description", "What this policy checks", false),
        ("query", "osquery SQL query", true),
        ("platform", "Target operating system", false),
        ("critical", "Whether policy is critical", false),
        ("resolution", "How to fix policy failures", false),
        ("team", "Team this policy belongs to", false),
        ("calendar_events_enabled", "Create calendar reminders", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete query field names.
fn complete_query_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in value position
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return complete_platform_values(),
            "logging" => return complete_logging_values(),
            _ => {}
        }
    }

    // Queries can be either inline definitions OR path references
    let fields = [
        ("path", "Reference to external query YAML file", false),
        ("name", "Query display name (for inline definitions)", true),
        ("description", "What this query collects", false),
        ("query", "osquery SQL query", true),
        ("interval", "How often to run (seconds)", false),
        ("platform", "Target operating system", false),
        ("logging", "How results are logged", false),
        ("min_osquery_version", "Minimum osquery version", false),
        ("observer_can_run", "Allow observers to run", false),
        ("automations_enabled", "Enable automations", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete label field names.
fn complete_label_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in value position
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return complete_platform_values(),
            "label_membership_type" => {
                return vec![
                    create_value_completion("dynamic", "Membership via query"),
                    create_value_completion("manual", "Explicit host assignment"),
                ];
            }
            _ => {}
        }
    }

    // Labels can be either inline definitions OR path references
    let fields = [
        ("path", "Reference to external label YAML file", false),
        ("name", "Label display name (for inline definitions)", true),
        ("description", "What hosts this label identifies", false),
        ("query", "osquery query for dynamic labels", false),
        ("platform", "Target operating system", false),
        ("label_membership_type", "dynamic or manual", false),
        ("hosts", "List of hosts (manual labels)", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete platform values.
fn complete_platform_values() -> Vec<CompletionItem> {
    PLATFORM_DOCS
        .iter()
        .map(|(platform, desc)| create_value_completion(platform, desc))
        .collect()
}

/// Complete logging type values.
fn complete_logging_values() -> Vec<CompletionItem> {
    LOGGING_DOCS
        .iter()
        .map(|(logging, desc)| create_value_completion(logging, desc))
        .collect()
}

/// Complete osquery table names, optionally filtered by platform.
fn complete_osquery_tables(platform: Option<&str>) -> Vec<CompletionItem> {
    OSQUERY_TABLES
        .iter()
        .filter(|(_, info)| {
            platform
                .map(|p| p == "all" || info.platforms.contains(&p))
                .unwrap_or(true)
        })
        .map(|(name, info)| {
            let platforms = info.platforms.join(", ");
            CompletionItem {
                label: (*name).to_string(),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some(format!("osquery table ({})", platforms)),
                documentation: Some(Documentation::MarkupContent(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!("**{}**\n\n{}\n\n**Platforms:** {}", name, info.description, platforms),
                })),
                ..Default::default()
            }
        })
        .collect()
}

/// Create a completion item for a field name.
fn create_field_completion(name: &str, description: &str, required: bool) -> CompletionItem {
    let detail = if required {
        format!("{} (required)", description)
    } else {
        description.to_string()
    };

    // Get richer documentation from schema if available
    let documentation = get_field_doc(name).map(|doc| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc.to_markdown(),
        })
    });

    CompletionItem {
        label: name.to_string(),
        kind: Some(CompletionItemKind::FIELD),
        detail: Some(detail),
        documentation,
        insert_text: Some(format!("{}: ", name)),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..Default::default()
    }
}

/// Create a completion item for a value.
fn create_value_completion(value: &str, description: &str) -> CompletionItem {
    CompletionItem {
        label: value.to_string(),
        kind: Some(CompletionItemKind::ENUM_MEMBER),
        detail: Some(description.to_string()),
        ..Default::default()
    }
}

/// Complete boolean values.
fn complete_boolean_values() -> Vec<CompletionItem> {
    vec![
        create_value_completion("true", "Enable this option"),
        create_value_completion("false", "Disable this option"),
    ]
}

/// Complete software section keys.
fn complete_software_section() -> Vec<CompletionItem> {
    let fields = [
        ("packages", "Custom software packages to install", false),
        ("app_store_apps", "macOS App Store apps", false),
        ("fleet_maintained_apps", "Fleet-managed apps with auto-updates", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete software.packages array item fields.
fn complete_software_package_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in value position
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "self_service" | "setup_experience" => return complete_boolean_values(),
            _ => {}
        }
    }

    // Based on workstations.yml: path, self_service, setup_experience, categories, labels_include_any, display_name
    let fields = [
        ("path", "Path to software package YAML definition", true),
        ("self_service", "Allow users to install from Fleet Desktop", false),
        ("setup_experience", "Install during device setup (DEP)", false),
        ("categories", "App categories for organization", false),
        ("labels_include_any", "Only install on hosts with these labels", false),
        ("labels_exclude_any", "Don't install on hosts with these labels", false),
        ("display_name", "Custom display name in Fleet UI", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete software.app_store_apps array item fields.
fn complete_app_store_app_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "self_service" | "setup_experience" => return complete_boolean_values(),
            _ => {}
        }
    }

    let fields = [
        ("app_store_id", "Apple App Store app ID", true),
        ("self_service", "Allow users to install from Fleet Desktop", false),
        ("setup_experience", "Install during device setup (DEP)", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete software.fleet_maintained_apps array item fields.
fn complete_fleet_maintained_app_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "self_service" | "setup_experience" => return complete_boolean_values(),
            _ => {}
        }
    }

    // Based on workstations.yml
    let fields = [
        ("slug", "Fleet app identifier (e.g., slack/darwin)", true),
        ("self_service", "Allow users to install from Fleet Desktop", false),
        ("setup_experience", "Install during device setup (DEP)", false),
        ("labels_include_any", "Only install on hosts with these labels", false),
        ("labels_exclude_any", "Don't install on hosts with these labels", false),
        ("display_name", "Custom display name in Fleet UI", false),
        ("categories", "App categories for organization", false),
        ("post_install_script", "Script to run after installation", false),
        ("pre_install_query", "Query that must pass before installation", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete controls section keys.
fn complete_controls_section() -> Vec<CompletionItem> {
    let fields = [
        ("enable_disk_encryption", "Require disk encryption on hosts", false),
        ("macos_settings", "macOS MDM configuration profiles", false),
        ("macos_setup", "macOS automatic enrollment settings", false),
        ("macos_updates", "macOS software update requirements", false),
        ("windows_settings", "Windows MDM configuration profiles", false),
        ("windows_updates", "Windows update requirements", false),
        ("scripts", "Management scripts to deploy", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete custom settings array item fields (macos/windows).
fn complete_custom_setting_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        if key == "labels_include_any" || key == "labels_exclude_any" {
            return vec![];  // Let user type label names
        }
    }

    let fields = [
        ("path", "Path to configuration profile file", true),
        ("labels_include_any", "Only apply to hosts with these labels", false),
        ("labels_exclude_any", "Don't apply to hosts with these labels", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete script array item fields.
fn complete_script_fields(line: &str, _col_idx: usize) -> Vec<CompletionItem> {
    let fields = [
        ("path", "Path to script file", true),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete team_settings section.
fn complete_team_settings_section() -> Vec<CompletionItem> {
    let fields = [
        ("webhook_settings", "Webhook configuration for team events", false),
        ("features", "Feature flags for this team", false),
        ("host_expiry_settings", "Auto-remove inactive hosts", false),
        ("secrets", "Enrollment secrets for this team", false),
        ("integrations", "Third-party integrations (calendar, ticketing)", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete agent_options section.
fn complete_agent_options_section() -> Vec<CompletionItem> {
    let fields = [
        ("config", "osquery configuration options", false),
        ("update_channels", "Fleet component update channels", false),
        ("command_line_flags", "osquery command-line flags", false),
        ("extensions", "osquery extensions to load", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete file paths for path: values.
fn complete_file_paths(
    line: &str,
    col_idx: usize,
    current_file: Option<&Path>,
    workspace_root: Option<&Path>,
    context_type: PathContextType,
) -> Vec<CompletionItem> {
    let mut completions = Vec::new();

    // Extract partial path already typed (text after "path: ")
    let partial = extract_partial_path(line, col_idx);

    // Determine base directory for scanning
    let base_dir = match (workspace_root, current_file) {
        (Some(root), _) => root.to_path_buf(),
        (None, Some(file)) => file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        (None, None) => return completions,
    };

    // Scan lib/ directory for matching files
    let lib_dir = base_dir.join("lib");
    if lib_dir.exists() && lib_dir.is_dir() {
        scan_directory_for_paths(
            &lib_dir,
            current_file,
            &context_type,
            &partial,
            &base_dir,
            &mut completions,
            0,
        );
    }

    // Also scan teams/ directory for team-level completions
    let teams_dir = base_dir.join("teams");
    if teams_dir.exists() && teams_dir.is_dir() {
        scan_directory_for_paths(
            &teams_dir,
            current_file,
            &context_type,
            &partial,
            &base_dir,
            &mut completions,
            0,
        );
    }

    // Sort completions alphabetically
    completions.sort_by(|a, b| a.label.cmp(&b.label));

    completions
}

/// Extract the partial path the user has typed after "path: ".
fn extract_partial_path(line: &str, col_idx: usize) -> String {
    let trimmed = line.trim().trim_start_matches('-').trim();

    if let Some(colon_pos) = trimmed.find(':') {
        // Find where the value starts in the original line
        if let Some(line_colon_pos) = line.find(':') {
            let value_start = line_colon_pos + 1;
            // Get text from after colon to cursor position
            if col_idx > value_start {
                let value_portion = &line[value_start..col_idx.min(line.len())];
                return value_portion.trim().trim_matches('"').trim_matches('\'').to_string();
            }
        }
    }

    String::new()
}

/// Recursively scan a directory for files matching the context type.
fn scan_directory_for_paths(
    dir: &Path,
    current_file: Option<&Path>,
    context_type: &PathContextType,
    partial: &str,
    workspace_root: &Path,
    completions: &mut Vec<CompletionItem>,
    depth: usize,
) {
    // Limit recursion depth to avoid performance issues
    const MAX_DEPTH: usize = 5;
    if depth > MAX_DEPTH {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        if path.is_dir() {
            // Recursively scan subdirectories
            scan_directory_for_paths(
                &path,
                current_file,
                context_type,
                partial,
                workspace_root,
                completions,
                depth + 1,
            );
        } else if path.is_file() && matches_context_type(&path, context_type) {
            // Calculate relative path from current file or workspace root
            let relative_path = calculate_relative_path(&path, current_file, workspace_root);

            // Filter by partial input
            if partial.is_empty() || relative_path.to_lowercase().contains(&partial.to_lowercase()) {
                completions.push(create_path_completion(&relative_path, &path, context_type));
            }
        }
    }
}

/// Check if a file matches the expected context type based on extension.
fn matches_context_type(path: &Path, context_type: &PathContextType) -> bool {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();

    match context_type {
        PathContextType::SoftwarePackage => ext == "yml" || ext == "yaml",
        PathContextType::Script => ext == "sh" || ext == "ps1" || ext == "bat" || ext == "cmd",
        PathContextType::MacOSProfile => ext == "mobileconfig" || ext == "plist",
        PathContextType::WindowsProfile => ext == "xml",
        PathContextType::Policy => ext == "yml" || ext == "yaml",
        PathContextType::Query => ext == "yml" || ext == "yaml",
        PathContextType::Label => ext == "yml" || ext == "yaml",
        PathContextType::Generic => true,
    }
}

/// Calculate the relative path from the current file to the target file.
fn calculate_relative_path(
    target: &Path,
    current_file: Option<&Path>,
    workspace_root: &Path,
) -> String {
    // If we have a current file, calculate path relative to it
    if let Some(current) = current_file {
        if let Some(current_dir) = current.parent() {
            if let Some(relative) = pathdiff::diff_paths(target, current_dir) {
                return relative.to_string_lossy().to_string();
            }
        }
    }

    // Fall back to path relative to workspace root
    target
        .strip_prefix(workspace_root)
        .unwrap_or(target)
        .to_string_lossy()
        .to_string()
}

/// Create a completion item for a file path.
fn create_path_completion(
    relative_path: &str,
    absolute_path: &Path,
    context_type: &PathContextType,
) -> CompletionItem {
    let file_name = absolute_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let kind_desc = match context_type {
        PathContextType::SoftwarePackage => "Software package",
        PathContextType::Script => "Script",
        PathContextType::MacOSProfile => "macOS profile",
        PathContextType::WindowsProfile => "Windows profile",
        PathContextType::Policy => "Policy definition",
        PathContextType::Query => "Query definition",
        PathContextType::Label => "Label definition",
        PathContextType::Generic => "File",
    };

    CompletionItem {
        label: relative_path.to_string(),
        kind: Some(CompletionItemKind::FILE),
        detail: Some(format!("{}: {}", kind_desc, file_name)),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!(
                "**{}**\n\nPath: `{}`",
                file_name,
                absolute_path.display()
            ),
        })),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_top_level() {
        let source = "";
        let completions = complete_at(source, Position { line: 0, character: 0 });
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"policies"));
        assert!(labels.contains(&"queries"));
        assert!(labels.contains(&"labels"));
    }

    #[test]
    fn test_complete_policy_fields() {
        let source = "policies:\n  - ";
        let completions = complete_at(source, Position { line: 1, character: 4 });
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"name"));
        assert!(labels.contains(&"query"));
        assert!(labels.contains(&"platform"));
    }

    #[test]
    fn test_complete_platform_values() {
        let source = "policies:\n  - name: test\n    platform: ";
        let completions = complete_at(source, Position { line: 2, character: 15 });

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"darwin"));
        assert!(labels.contains(&"windows"));
        assert!(labels.contains(&"linux"));
    }

    #[test]
    fn test_complete_osquery_tables() {
        let source = "policies:\n  - name: test\n    query: |\n      SELECT * FROM ";
        let completions = complete_at(source, Position { line: 3, character: 20 });

        // Should have osquery tables
        assert!(!completions.is_empty());
        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"processes"));
    }

    #[test]
    fn test_get_key_at_cursor() {
        assert_eq!(
            get_key_at_cursor("    platform: darwin", 15),
            Some("platform".to_string())
        );
        assert_eq!(
            get_key_at_cursor("  - name: test", 10),
            Some("name".to_string())
        );
        assert_eq!(get_key_at_cursor("    platform: ", 5), None); // cursor before colon
    }

    #[test]
    fn test_find_platform_in_context() {
        let source = "policies:\n  - name: test\n    platform: darwin\n    query: |";
        assert_eq!(
            find_platform_in_context(source, 3),
            Some("darwin".to_string())
        );
    }

    #[test]
    fn test_extract_partial_path() {
        // Empty partial
        assert_eq!(extract_partial_path("    path: ", 10), "");

        // Partial typed
        assert_eq!(extract_partial_path("    path: ../lib/m", 18), "../lib/m");

        // With quotes
        assert_eq!(extract_partial_path("    path: \"../lib/m", 19), "../lib/m");

        // Array item format
        assert_eq!(extract_partial_path("  - path: ../lib/", 17), "../lib/");
    }

    #[test]
    fn test_matches_context_type() {
        // SoftwarePackage should match .yml and .yaml
        assert!(matches_context_type(Path::new("test.yml"), &PathContextType::SoftwarePackage));
        assert!(matches_context_type(Path::new("test.yaml"), &PathContextType::SoftwarePackage));
        assert!(!matches_context_type(Path::new("test.sh"), &PathContextType::SoftwarePackage));

        // Script should match .sh, .ps1, .bat
        assert!(matches_context_type(Path::new("test.sh"), &PathContextType::Script));
        assert!(matches_context_type(Path::new("test.ps1"), &PathContextType::Script));
        assert!(matches_context_type(Path::new("test.bat"), &PathContextType::Script));
        assert!(!matches_context_type(Path::new("test.yml"), &PathContextType::Script));

        // MacOSProfile should match .mobileconfig and .plist
        assert!(matches_context_type(Path::new("test.mobileconfig"), &PathContextType::MacOSProfile));
        assert!(matches_context_type(Path::new("test.plist"), &PathContextType::MacOSProfile));
        assert!(!matches_context_type(Path::new("test.xml"), &PathContextType::MacOSProfile));

        // WindowsProfile should match .xml
        assert!(matches_context_type(Path::new("test.xml"), &PathContextType::WindowsProfile));
        assert!(!matches_context_type(Path::new("test.yml"), &PathContextType::WindowsProfile));

        // Generic should match anything
        assert!(matches_context_type(Path::new("test.yml"), &PathContextType::Generic));
        assert!(matches_context_type(Path::new("test.sh"), &PathContextType::Generic));
        assert!(matches_context_type(Path::new("test.txt"), &PathContextType::Generic));
    }

    #[test]
    fn test_path_context_detection() {
        // In software.packages, path: should give SoftwarePackage context
        let source = "software:\n  packages:\n    - path: ";
        let context = determine_completion_context(source, 2, "    - path: ", 12);
        assert_eq!(context, CompletionContext::PathValue { context_type: PathContextType::SoftwarePackage });

        // In controls.scripts, path: should give Script context
        let source2 = "controls:\n  scripts:\n    - path: ";
        let context2 = determine_completion_context(source2, 2, "    - path: ", 12);
        assert_eq!(context2, CompletionContext::PathValue { context_type: PathContextType::Script });

        // In macos_settings.custom_settings, path: should give MacOSProfile context
        let source3 = "controls:\n  macos_settings:\n    custom_settings:\n      - path: ";
        let context3 = determine_completion_context(source3, 3, "      - path: ", 14);
        assert_eq!(context3, CompletionContext::PathValue { context_type: PathContextType::MacOSProfile });
    }

    #[test]
    fn test_complete_file_paths_with_workspace() {
        use std::fs;
        use tempfile::TempDir;

        // Create a temporary workspace
        let temp_dir = TempDir::new().unwrap();
        let workspace_root = temp_dir.path();

        // Create lib directory structure
        let lib_dir = workspace_root.join("lib");
        let macos_dir = lib_dir.join("macos").join("software");
        fs::create_dir_all(&macos_dir).unwrap();

        // Create some test files
        fs::write(macos_dir.join("firefox.yml"), "name: Firefox").unwrap();
        fs::write(macos_dir.join("chrome.yml"), "name: Chrome").unwrap();

        // Create teams directory
        let teams_dir = workspace_root.join("teams");
        fs::create_dir_all(&teams_dir).unwrap();
        let team_file = teams_dir.join("workstations.yml");
        fs::write(&team_file, "software:\n  packages:\n    - path: ").unwrap();

        // Test file path completion
        let completions = complete_file_paths(
            "    - path: ",
            12,
            Some(&team_file),
            Some(workspace_root),
            PathContextType::SoftwarePackage,
        );

        // Should find yml files
        assert!(!completions.is_empty());

        // All completions should be for yml files
        for item in &completions {
            assert!(item.label.ends_with(".yml") || item.label.ends_with(".yaml"));
        }
    }
}
