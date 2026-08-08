//! Completion provider for Fleet GitOps YAML files.
//!
//! Provides context-aware autocompletion for field names, values, and osquery tables.

use std::path::Path;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, CompletionTextEdit, Documentation, InsertTextFormat,
    InsertTextMode, MarkupContent, MarkupKind, Position, Range, TextEdit,
};

use super::completion_data::{
    blocks_for_context, fields_for_context, globs_for_context, COMPLETION_DATA,
};
use super::schema::{get_field_doc, LOGGING_DOCS, PLATFORM_DOCS};
use flint_lint::osquery::OSQUERY_TABLES;

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
    /// Inside a labels[].criteria block (or nested and/or inside it)
    CriteriaField,
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
    /// Inside controls.*_settings section (suggests configuration_profiles, etc.)
    DeviceSettingsSection,
    /// Inside configuration_profiles array item (suggests path/paths)
    ConfigurationProfileField,
    /// Inside setup_experience (formerly macos_setup)
    SetupExperienceField,
    /// Inside controls.scripts array item
    ScriptField,
    /// Inside team_settings section
    TeamSettingsSection,
    /// Inside org_settings section
    OrgSettingsSection,
    /// Inside org_settings.fleet_desktop section
    OrgFleetDesktopSection,
    /// Inside org_settings.server_settings section
    OrgServerSettingsSection,
    /// Inside org_settings.sso_settings section
    OrgSsoSettingsSection,
    /// Inside org_settings.org_info section
    OrgInfoSection,
    /// Inside agent_options section
    AgentOptionsSection,
    /// After platform: key
    PlatformValue,
    /// After logging: key
    LoggingValue,
    /// After self_service: key
    #[expect(
        dead_code,
        reason = "variant reserved for future boolean completion support"
    )]
    BooleanValue,
    /// After path: key, completing file path value
    PathValue { context_type: PathContextType },
    /// After slug: key, suggesting FMA slugs
    SlugValue,
    /// After paths: key, suggesting glob patterns
    GlobValue { parent_context: String },
    /// Inside any `labels_{include,exclude}_{any,all}` list
    LabelValue,
    /// Inside categories list
    CategoryValue,
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
    complete_at_with_context(source, position, None, None, false)
}

/// Provide completion items with workspace context for file path completions.
///
/// When `future_names` is `true`, top-level completions suggest the new naming
/// convention (`reports` instead of `queries`, `settings` instead of `team_settings`).
pub fn complete_at_with_context(
    source: &str,
    position: Position,
    current_file: Option<&Path>,
    workspace_root: Option<&Path>,
    future_names: bool,
) -> Vec<CompletionItem> {
    let line_idx = position.line as usize;
    let col_idx = position.character as usize;

    // Get the line content (empty string if no line at that position)
    let line = source.lines().nth(line_idx).unwrap_or("");

    // Determine the context
    let context = determine_completion_context(source, line_idx, line, col_idx);

    match context {
        CompletionContext::TopLevel => complete_top_level_fields(future_names),
        CompletionContext::PolicyField => complete_policy_fields(line, col_idx),
        CompletionContext::QueryField => complete_query_fields(line, col_idx),
        CompletionContext::LabelField => complete_label_fields(line, col_idx),
        CompletionContext::CriteriaField => complete_criteria_fields(line, col_idx),
        CompletionContext::SoftwareSection => complete_software_section(),
        CompletionContext::SoftwarePackageField => complete_software_package_fields(line, col_idx),
        CompletionContext::AppStoreAppField => complete_app_store_app_fields(line, col_idx),
        CompletionContext::FleetMaintainedAppField => {
            complete_fleet_maintained_app_fields(line, col_idx)
        }
        CompletionContext::ControlsSection => complete_controls_section(),
        CompletionContext::MacOSCustomSettingField => complete_custom_setting_fields(line, col_idx),
        CompletionContext::WindowsCustomSettingField => {
            complete_custom_setting_fields(line, col_idx)
        }
        CompletionContext::SetupExperienceField => {
            completions_from_data("setup_experience", line, col_idx)
        }
        CompletionContext::DeviceSettingsSection => complete_device_settings_section(),
        CompletionContext::ConfigurationProfileField => complete_path_ref_fields(),
        CompletionContext::ScriptField => complete_script_fields(line, col_idx),
        CompletionContext::TeamSettingsSection => complete_team_settings_section(),
        CompletionContext::OrgSettingsSection => complete_org_settings_section(),
        CompletionContext::OrgFleetDesktopSection => complete_org_fleet_desktop_fields(),
        CompletionContext::OrgServerSettingsSection => complete_org_server_settings_fields(),
        CompletionContext::OrgSsoSettingsSection => complete_org_sso_settings_fields(),
        CompletionContext::OrgInfoSection => complete_org_info_fields(),
        CompletionContext::AgentOptionsSection => complete_agent_options_section(),
        CompletionContext::PlatformValue => complete_platform_values(),
        CompletionContext::LoggingValue => complete_logging_values(),
        CompletionContext::BooleanValue => complete_boolean_values(),
        CompletionContext::PathValue { context_type } => complete_file_paths(
            line,
            line_idx,
            col_idx,
            current_file,
            workspace_root,
            context_type,
        ),
        CompletionContext::SlugValue => complete_fma_slugs(),
        CompletionContext::GlobValue { parent_context } => {
            complete_glob_patterns(&parent_context, current_file, workspace_root)
        }
        CompletionContext::LabelValue => complete_common_labels(),
        CompletionContext::CategoryValue => complete_common_categories(),
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
            "slug" => return CompletionContext::SlugValue,
            "path" => {
                // path: → file lookup
                let parent = find_parent_context(source, line_idx);
                let context_type = match parent.as_deref() {
                    Some(p) if p.contains("software.packages") => PathContextType::SoftwarePackage,
                    Some(p) if p.contains("fleet_maintained_apps") => {
                        PathContextType::SoftwarePackage
                    }
                    Some(p) if p.contains("scripts") => PathContextType::Script,
                    Some(p) if p.contains("macos_settings") || p.contains("apple_settings") => {
                        PathContextType::MacOSProfile
                    }
                    Some(p) if p.contains("windows_settings") => PathContextType::WindowsProfile,
                    Some(p) if p.contains("android_settings") => PathContextType::Generic,
                    Some(p) if p == "policies" || p.ends_with(".policies") => {
                        PathContextType::Policy
                    }
                    Some(p) if p == "queries" || p.ends_with(".queries") => PathContextType::Query,
                    Some(p) if p == "reports" || p.ends_with(".reports") => PathContextType::Query,
                    Some(p) if p == "labels" || p.ends_with(".labels") => PathContextType::Label,
                    _ => PathContextType::Generic,
                };
                return CompletionContext::PathValue { context_type };
            }
            "paths" => {
                // paths: → suggest glob patterns based on context
                let parent = find_parent_context(source, line_idx);
                return CompletionContext::GlobValue {
                    parent_context: parent.unwrap_or_default(),
                };
            }
            _ => {}
        }
    }

    // Check if we're inside a labels or categories list (- item under labels_include_any etc.)
    if trimmed.starts_with('-') || trimmed.is_empty() {
        if let Some(parent_key) = find_immediate_parent_key(source, line_idx, line) {
            match parent_key.as_str() {
                "labels_include_any"
                | "labels_include_all"
                | "labels_exclude_any"
                | "labels_exclude_all" => {
                    return CompletionContext::LabelValue;
                }
                "categories" => {
                    return CompletionContext::CategoryValue;
                }
                _ => {}
            }
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
        if trimmed.ends_with(':') && !trimmed.starts_with('-') && !trimmed.starts_with("query:") {
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
        Some(p) if p == "labels.criteria" || p.ends_with(".criteria") => {
            CompletionContext::CriteriaField
        }
        Some(p) if p == "labels" || p.ends_with(".labels") => CompletionContext::LabelField,
        Some("software") => CompletionContext::SoftwareSection,
        Some(p) if p == "software.packages" || p.ends_with(".packages") => {
            CompletionContext::SoftwarePackageField
        }
        Some(p) if p.contains("app_store_apps") => CompletionContext::AppStoreAppField,
        Some(p) if p.contains("fleet_maintained_apps") => {
            CompletionContext::FleetMaintainedAppField
        }
        Some("controls") => CompletionContext::ControlsSection,
        Some(p) if p.contains("macos_settings.custom_settings") => {
            CompletionContext::MacOSCustomSettingField
        }
        Some(p) if p.contains("windows_settings.custom_settings") => {
            CompletionContext::WindowsCustomSettingField
        }
        Some(p) if p.contains("configuration_profiles") => {
            CompletionContext::ConfigurationProfileField
        }
        Some(p)
            if p.contains("apple_settings")
                || p.contains("windows_settings")
                || p.contains("android_settings")
                || p.contains("macos_settings") =>
        {
            // Inside a *_settings section but not yet in a sub-key
            // Only match if we're directly under the settings key, not deeper
            if !p.contains("custom_settings") && !p.contains("configuration_profiles") {
                CompletionContext::DeviceSettingsSection
            } else {
                CompletionContext::Unknown
            }
        }
        Some(p) if p.contains("setup_experience") || p.contains("macos_setup") => {
            CompletionContext::SetupExperienceField
        }
        Some(p) if p.contains("controls.scripts") => CompletionContext::ScriptField,
        Some(p) if p.starts_with("team_settings") => CompletionContext::TeamSettingsSection,
        Some(p) if p.starts_with("settings") => CompletionContext::TeamSettingsSection,
        Some(p) if p == "reports" || p.ends_with(".reports") => CompletionContext::QueryField,
        Some("org_settings") => CompletionContext::OrgSettingsSection,
        Some("org_settings.fleet_desktop") => CompletionContext::OrgFleetDesktopSection,
        Some("org_settings.server_settings") => CompletionContext::OrgServerSettingsSection,
        Some("org_settings.sso_settings") => CompletionContext::OrgSsoSettingsSection,
        Some("org_settings.org_info") => CompletionContext::OrgInfoSection,
        Some(p) if p.starts_with("org_settings") => CompletionContext::OrgSettingsSection,
        Some(p) if p.starts_with("agent_options") => CompletionContext::AgentOptionsSection,
        _ => CompletionContext::Unknown,
    }
}

/// Complete top-level field names.
///
/// When `future_names` is `true`, suggests new naming conventions:
/// - `reports` instead of `queries`
/// - `settings` instead of `team_settings` (not in default list, but swapped if present)
fn complete_top_level_fields(future_names: bool) -> Vec<CompletionItem> {
    let queries_entry = if future_names {
        (
            "reports",
            "List of osquery queries (future name for queries)",
        )
    } else {
        ("queries", "List of osquery queries")
    };

    let mut fields = vec![
        ("name", "Team or configuration name"),
        ("policies", "List of compliance policies"),
        queries_entry,
        ("labels", "List of host labels"),
        ("agent_options", "osquery agent configuration"),
        ("controls", "MDM controls and settings"),
        ("software", "Software packages to install"),
        (
            "org_settings",
            "Organization-wide settings (global config only)",
        ),
        ("team_settings", "Team-specific settings"),
        ("webhook_settings", "Webhook notification configuration"),
    ];

    if future_names {
        fields.push(("settings", "Team settings (future name for team_settings)"));
    }

    let mut items: Vec<CompletionItem> = fields
        .iter()
        .map(|(name, desc)| {
            let mut item = create_field_completion(name, desc, true);
            item.sort_text = Some(format!("1_{}", name));
            item
        })
        .collect();

    let queries_block = if future_names {
        ("reports", "Query/report list (block)", "reports:\n  - name: ${1:Query name}\n    query: ${2:SELECT 1}\n    interval: ${3:300}\n    platform: ${4:darwin}")
    } else {
        ("queries", "Query list (block)", "queries:\n  - name: ${1:Query name}\n    query: ${2:SELECT 1}\n    interval: ${3:300}\n    platform: ${4:darwin}")
    };

    let mut blocks = vec![
        (
            "policies",
            "Policy list (block)",
            "policies:\n  - name: ${1:Policy name}\n    query: ${2:SELECT 1}\n    platform: ${3:darwin}\n    critical: ${4:false}",
        ),
        queries_block,
        (
            "labels",
            "Label list (block)",
            "labels:\n  - name: ${1:Label name}\n    query: ${2:SELECT 1}\n    label_membership_type: ${3:dynamic}",
        ),
        (
            "controls",
            "MDM controls (block)",
            "controls:\n  enable_disk_encryption: ${1:true}",
        ),
        (
            "software",
            "Software section (block)",
            "software:\n  packages:\n    - path: ${1:../lib/package.yml}",
        ),
        (
            "agent_options",
            "Agent options (block)",
            "agent_options:\n  config:\n    options:\n      distributed_interval: ${1:10}",
        ),
        (
            "org_settings",
            "Organization settings (block)",
            "org_settings:\n  server_settings:\n    server_url: ${1:https://}\n  org_info:\n    org_name: ${2:Organization}",
        ),
        (
            "team_settings",
            "Team settings (block)",
            "team_settings:\n  features:\n    enable_host_users: ${1:true}\n    enable_software_inventory: ${2:true}",
        ),
    ];

    if future_names {
        blocks.push((
            "settings",
            "Team settings (block)",
            "settings:\n  features:\n    enable_host_users: ${1:true}\n    enable_software_inventory: ${2:true}",
        ));
    }

    for (name, desc, snippet) in blocks {
        items.push(create_block_completion(name, desc, snippet));
    }

    items
}

/// Complete policy field names.
fn complete_policy_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in value position
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "platform" => return complete_platform_values(),
            "type" => {
                return vec![
                    create_value_completion("dynamic", "Classic policy with an editable query"),
                    create_value_completion(
                        "patch",
                        "Patch policy tied to a Fleet-Maintained App (requires fleet_maintained_app_slug)",
                    ),
                ];
            }
            _ => {}
        }
    }

    // Policies can be either inline definitions OR path references.
    // Automations (run_script / install_software / calendar_events_enabled)
    // are only valid in fleet files per yaml-files.md:245 — still surfaced
    // here because the LSP can't reliably tell which file it's editing.
    let fields = [
        ("path", "Reference to external policy YAML file", false),
        (
            "paths",
            "Glob pattern for external policy YAML files",
            false,
        ),
        ("name", "Policy display name (for inline definitions)", true),
        ("description", "What this policy checks", false),
        ("query", "osquery SQL query (auto-generated when type: patch)", true),
        ("platform", "Target operating system", false),
        ("critical", "Whether policy is critical (Fleet Premium)", false),
        ("resolution", "How to fix policy failures", false),
        ("team", "Team this policy belongs to", false),
        (
            "type",
            "Policy type: dynamic (default) or patch",
            false,
        ),
        (
            "fleet_maintained_app_slug",
            "FMA slug for patch policies (e.g. zoom/darwin)",
            false,
        ),
        ("software_title_id", "ID of software to install on failure", false),
        ("script_id", "ID of script to run on failure", false),
        (
            "install_software",
            "Install a custom package or FMA on policy failure (fleet-only)",
            false,
        ),
        (
            "run_script",
            "Run a script on policy failure (fleet-only)",
            false,
        ),
        (
            "calendar_events_enabled",
            "Create calendar reminders (fleet-only)",
            false,
        ),
        (
            "conditional_access_enabled",
            "Gate resource access on policy pass (Fleet Premium)",
            false,
        ),
        (
            "conditional_access_bypass_enabled",
            "Allow conditional-access bypass (Fleet Premium)",
            false,
        ),
        // All four scopes are policy-valid (policies.go:646-649).
        // `labels_exclude_all` exists on no other GitOps context.
        ("labels_include_any", "Target hosts with any of these labels", false),
        (
            "labels_include_all",
            "Target hosts with all of these labels",
            false,
        ),
        ("labels_exclude_any", "Exclude hosts with any of these labels", false),
        (
            "labels_exclude_all",
            "Exclude hosts with all of these labels (policies only)",
            false,
        ),
        (
            "webhooks_and_tickets_enabled",
            "Add this policy to the failing-policies webhook (GitOps convenience)",
            false,
        ),
        ("team", "Fleet (team) this policy belongs to", false),
    ];

    let mut items: Vec<CompletionItem> = fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect();

    // Policies are the only context with BOTH an include pair and an exclude
    // pair, and the only one where include/exclude may be combined — the
    // starter blocks encode legal combinations so the shapes aren't guessed.
    for block in blocks_for_context("policies") {
        items.push(create_block_completion(
            &block.name,
            &block.description,
            &block.snippet,
        ));
    }

    items
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
        ("paths", "Glob pattern for external query YAML files", false),
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
                    create_value_completion(
                        "host_vitals",
                        "Membership via host vital criteria",
                    ),
                ];
            }
            _ => {}
        }
    }

    // Labels can be either inline definitions OR path references
    let fields = [
        ("path", "Reference to external label YAML file", false),
        ("paths", "Glob pattern for external label YAML files", false),
        ("name", "Label display name (for inline definitions)", true),
        ("description", "What hosts this label identifies", false),
        ("query", "osquery query for dynamic labels", false),
        ("platform", "Target operating system", false),
        (
            "label_membership_type",
            "dynamic, manual, or host_vitals",
            false,
        ),
        ("hosts", "List of hosts (manual labels)", false),
        ("criteria", "Host vital criteria (host_vitals labels)", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete host-vital criteria fields (inside labels[].criteria).
///
/// Per Fleet's REST API docs, a criteria is a single `{vital, value}` leaf.
/// `and`/`or` are in the Go struct but rejected at parse time, so we don't
/// suggest them here.
fn complete_criteria_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        if key.as_str() == "vital" {
            return vec![
                create_value_completion(
                    "end_user_idp_group",
                    "Host's IdP group (from end-user SSO)",
                ),
                create_value_completion(
                    "end_user_idp_department",
                    "Host's IdP department (from end-user SSO)",
                ),
            ];
        }
    }

    let fields = [
        (
            "vital",
            "Host vital identifier (end_user_idp_group or end_user_idp_department)",
            true,
        ),
        ("value", "Hosts whose vital matches this value join the label", true),
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
                    value: format!(
                        "**{}**\n\n{}\n\n**Platforms:** {}",
                        name, info.description, platforms
                    ),
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
        filter_text: Some(name.to_string()),
        sort_text: Some(format!("{}_{}", if required { "0" } else { "1" }, name)),
        ..Default::default()
    }
}

/// Create a completion item for a block snippet (expands to full YAML structure).
///
/// Uses `InsertTextMode::ADJUST_INDENTATION` so the editor automatically
/// adjusts leading whitespace on continuation lines to match the cursor's
/// indentation level. This works correctly in both VSCode and Zed.
fn create_block_completion(name: &str, description: &str, snippet: &str) -> CompletionItem {
    let documentation = get_field_doc(name).map(|doc| {
        Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc.to_markdown(),
        })
    });

    CompletionItem {
        label: format!("{} (block)", name),
        kind: Some(CompletionItemKind::SNIPPET),
        detail: Some(description.to_string()),
        documentation,
        insert_text: Some(snippet.to_string()),
        insert_text_format: Some(InsertTextFormat::SNIPPET),
        insert_text_mode: Some(InsertTextMode::ADJUST_INDENTATION),
        filter_text: Some(name.to_string()),
        sort_text: Some(format!("0_{}", name)),
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
        (
            "fleet_maintained_apps",
            "Fleet-managed apps with auto-updates",
            false,
        ),
    ];

    let mut items: Vec<CompletionItem> = fields
        .iter()
        .map(|(name, desc, required)| {
            let mut item = create_field_completion(name, desc, *required);
            item.sort_text = Some(format!("1_{}", name));
            item
        })
        .collect();

    let blocks = vec![
        (
            "packages",
            "Software packages (block)",
            "packages:\n\
  - path: ${1:../platforms/macos/software/app.yml}\n\
    self_service: ${2:true}\n\
    setup_experience: ${3:false}",
        ),
        (
            "app_store_apps",
            "App Store apps (block)",
            "app_store_apps:\n  - app_store_id: \"${1:id}\"\n    self_service: ${2:true}",
        ),
        (
            "fleet_maintained_apps",
            "Fleet-maintained apps (block)",
            "fleet_maintained_apps:\n\
  - slug: ${1:slack/darwin}\n\
    self_service: ${2:true}\n\
    setup_experience: ${3:false}\n\
    categories:\n\
      - ${4:Productivity}",
        ),
    ];

    for (name, desc, snippet) in blocks {
        items.push(create_block_completion(name, desc, snippet));
    }

    items
}

/// Complete software.packages array item fields (data-driven from completions.toml).
fn complete_software_package_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    completions_from_data("packages", line, col_idx)
}

/// Complete software.app_store_apps array item fields (data-driven from completions.toml).
fn complete_app_store_app_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    completions_from_data("app_store_apps", line, col_idx)
}

/// Complete software.fleet_maintained_apps array item fields (data-driven from completions.toml).
fn complete_fleet_maintained_app_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    completions_from_data("fleet_maintained_apps", line, col_idx)
}

/// Complete controls section keys.
///
/// Returns both bare field completions and block snippets that expand into
/// full YAML structures with tab stops for quick editing.
fn complete_controls_section() -> Vec<CompletionItem> {
    let fields = [
        (
            "enable_disk_encryption",
            "Require disk encryption on hosts",
            false,
        ),
        (
            "apple_settings",
            "Apple MDM configuration profiles (replaces macos_settings)",
            false,
        ),
        (
            "macos_settings",
            "macOS MDM configuration profiles (deprecated, use apple_settings)",
            false,
        ),
        (
            "setup_experience",
            "Setup experience settings (replaces macos_setup)",
            false,
        ),
        (
            "macos_setup",
            "macOS setup (deprecated, use setup_experience)",
            false,
        ),
        ("macos_updates", "macOS software update requirements", false),
        ("ios_updates", "iOS software update requirements", false),
        (
            "ipados_updates",
            "iPadOS software update requirements",
            false,
        ),
        ("macos_migration", "macOS migration settings", false),
        (
            "windows_settings",
            "Windows MDM configuration profiles",
            false,
        ),
        ("windows_updates", "Windows update requirements", false),
        (
            "android_settings",
            "Android MDM configuration profiles",
            false,
        ),
        ("scripts", "Management scripts to deploy", false),
        (
            "windows_enabled_and_configured",
            "Enable Windows MDM",
            false,
        ),
        (
            "enable_turn_on_windows_mdm_manually",
            "Require manual Windows MDM enrollment",
            false,
        ),
        (
            "windows_migration_enabled",
            "Enable Windows migration",
            false,
        ),
        (
            "windows_require_bitlocker_pin",
            "Require BitLocker PIN",
            false,
        ),
    ];

    let mut items: Vec<CompletionItem> = fields
        .iter()
        .map(|(name, desc, required)| {
            let mut item = create_field_completion(name, desc, *required);
            item.sort_text = Some(format!("1_{}", name));
            item
        })
        .collect();

    // Block snippets expand to full YAML structures with tab stops
    let blocks: Vec<(&str, &str, &str)> = vec![
        (
            "macos_updates",
            "macOS update requirements (block)",
            "macos_updates:\n  deadline: ${1:2024-12-31}\n  minimum_version: ${2:15.1}\n  update_new_hosts: ${3:true}",
        ),
        (
            "ios_updates",
            "iOS update requirements (block)",
            "ios_updates:\n  deadline: ${1:2024-12-31}\n  minimum_version: ${2:18.1}",
        ),
        (
            "ipados_updates",
            "iPadOS update requirements (block)",
            "ipados_updates:\n  deadline: ${1:2024-12-31}\n  minimum_version: ${2:18.1}",
        ),
        (
            "windows_updates",
            "Windows update requirements (block)",
            "windows_updates:\n  deadline_days: ${1:5}\n  grace_period_days: ${2:2}",
        ),
        (
            "macos_settings",
            "macOS configuration profiles (block)",
            "macos_settings:\n  custom_settings:\n    - path: ${1:../lib/profile.mobileconfig}",
        ),
        (
            "windows_settings",
            "Windows configuration profiles (block)",
            "windows_settings:\n  custom_settings:\n    - path: ${1:../lib/profile.xml}",
        ),
        (
            "macos_migration",
            "macOS migration settings (block)",
            "macos_migration:\n  enable: ${1:true}\n  mode: ${2:voluntary}\n  webhook_url: ${3:https://}",
        ),
        (
            "macos_setup",
            "macOS automatic enrollment (block)",
            "macos_setup:\n  enable_end_user_authentication: ${1:true}\n  macos_setup_assistant: ${2:../lib/dep-profile.json}\n  script: ${3:../lib/macos-setup-script.sh}",
        ),
        (
            "android_settings",
            "Android configuration profiles (block)",
            "android_settings:\n  custom_settings:\n    - path: ${1:../lib/profile.json}",
        ),
        (
            "scripts",
            "Management scripts (block)",
            "scripts:\n  - path: ${1:../lib/script.sh}",
        ),
        (
            "enable_disk_encryption",
            "Require disk encryption (block)",
            "enable_disk_encryption: ${1:true}",
        ),
        (
            "windows_enabled_and_configured",
            "Enable Windows MDM (block)",
            "windows_enabled_and_configured: ${1:true}",
        ),
        (
            "windows_migration_enabled",
            "Enable Windows migration (block)",
            "windows_migration_enabled: ${1:true}",
        ),
        (
            "windows_require_bitlocker_pin",
            "Require BitLocker PIN (block)",
            "windows_require_bitlocker_pin: ${1:true}",
        ),
        (
            "enable_turn_on_windows_mdm_manually",
            "Manual Windows MDM enrollment (block)",
            "enable_turn_on_windows_mdm_manually: ${1:false}",
        ),
    ];

    for (name, desc, snippet) in blocks {
        items.push(create_block_completion(name, desc, snippet));
    }

    items
}

/// Complete custom settings array item fields (macos/windows).
fn complete_custom_setting_fields(line: &str, col_idx: usize) -> Vec<CompletionItem> {
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        if key == "labels_include_any" || key == "labels_exclude_any" {
            return vec![]; // Let user type label names
        }
    }

    let fields = [
        ("path", "Path to configuration profile file", true),
        (
            "paths",
            "Glob pattern for configuration profile files",
            false,
        ),
        (
            "labels_include_any",
            "Only apply to hosts with these labels",
            false,
        ),
        (
            "labels_exclude_any",
            "Don't apply to hosts with these labels",
            false,
        ),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete script array item fields.
/// Complete device settings section (apple_settings, windows_settings, android_settings).
fn complete_device_settings_section() -> Vec<CompletionItem> {
    let fields = [
        (
            "configuration_profiles",
            "MDM configuration/declaration profiles to install",
            false,
        ),
        ("custom_settings", "Custom MDM profiles (legacy)", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Suggest common glob patterns based on the parent context.
///
/// When the user types `paths: ` and triggers completion, this offers
/// pre-built globs for the current section (profiles, scripts, policies, etc.).
/// Suggest Fleet Maintained App slugs for `slug:` values (data-driven from fma-registry.toml).
fn complete_fma_slugs() -> Vec<CompletionItem> {
    flint_lint::fma::APPS
        .iter()
        .flat_map(|app| {
            app.platforms.iter().map(move |platform| {
                let slug = format!("{}/{}", app.name, platform);
                let detail = format!(
                    "{} for {}",
                    app.name,
                    match platform.as_str() {
                        "darwin" => "macOS",
                        "windows" => "Windows",
                        _ => platform,
                    }
                );
                CompletionItem {
                    label: slug.clone(),
                    kind: Some(CompletionItemKind::VALUE),
                    detail: Some(detail),
                    filter_text: Some(app.name.clone()),
                    sort_text: Some(format!("0_{}", app.name)),
                    ..Default::default()
                }
            })
        })
        .collect()
}

/// Suggest glob patterns for `paths:` values (data-driven from completions.toml).
///
/// Resolves the `{base}` placeholder using the current file path and workspace
/// root to compute the correct relative path prefix (e.g., `../platforms`).
fn complete_glob_patterns(
    parent_context: &str,
    current_file: Option<&Path>,
    workspace_root: Option<&Path>,
) -> Vec<CompletionItem> {
    // Map parent_context to TOML context keys
    let context_key =
        if parent_context.contains("apple_settings") || parent_context.contains("macos_settings") {
            "apple_settings"
        } else if parent_context.contains("windows_settings") {
            "windows_settings"
        } else if parent_context.contains("android_settings") {
            "android_settings"
        } else if parent_context.contains("scripts") {
            "scripts"
        } else if parent_context.contains("policies") || parent_context.ends_with("policies") {
            "policies"
        } else if parent_context.contains("queries")
            || parent_context.contains("reports")
            || parent_context.ends_with("queries")
            || parent_context.ends_with("reports")
        {
            "reports"
        } else if parent_context.contains("labels") || parent_context.ends_with("labels") {
            "labels"
        } else {
            ""
        };

    let globs = globs_for_context(context_key);

    if globs.is_empty() {
        // Fallback for unknown contexts
        return vec![];
    }

    globs
        .iter()
        .enumerate()
        .map(|(i, glob)| {
            let pattern = match (current_file, workspace_root) {
                (Some(file), Some(root)) => {
                    super::completion_data::resolve_base(&glob.pattern, file, root)
                }
                _ => glob.pattern.replace("{base}", "../platforms"),
            };
            let mut item = CompletionItem {
                label: pattern.clone(),
                kind: Some(CompletionItemKind::VALUE),
                detail: Some(glob.description.clone()),
                sort_text: Some(format!("0_{}", i)),
                ..Default::default()
            };
            item.documentation = Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("**{}**\n\n`{}`", glob.description, pattern),
            }));
            item
        })
        .collect()
}

/// Build field + block completions from the TOML data for a given context.
///
/// This is the data-driven replacement for the per-context `complete_*_fields()`
/// functions. It reads fields and blocks from `completions.toml`.
fn completions_from_data(context: &str, line: &str, col_idx: usize) -> Vec<CompletionItem> {
    // Check if we're in a value position for boolean fields
    if let Some(key) = get_key_at_cursor(line, col_idx) {
        match key.as_str() {
            "self_service" | "setup_experience" | "auto_update_enabled" => {
                return complete_boolean_values();
            }
            _ => {}
        }
    }

    let mut items: Vec<CompletionItem> = Vec::new();

    // Fields from TOML
    for field in fields_for_context(context) {
        items.push(create_field_completion(
            &field.name,
            &field.description,
            field.required,
        ));
    }

    // Block snippets from TOML
    for block in blocks_for_context(context) {
        let snippet = block.snippet.strip_prefix('\n').unwrap_or(&block.snippet);
        items.push(create_block_completion(
            &block.name,
            &block.description,
            snippet,
        ));
    }

    items
}

/// Find the immediate parent key for the current line (the key whose list we're inside).
fn find_immediate_parent_key(source: &str, line_idx: usize, current_line: &str) -> Option<String> {
    let current_indent = current_line.len() - current_line.trim_start().len();
    let lines: Vec<&str> = source.lines().collect();

    for i in (0..line_idx).rev() {
        let line = lines.get(i).unwrap_or(&"");
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent < current_indent {
            // This is the parent — extract key
            if let Some(colon_pos) = trimmed.find(':') {
                let key = trimmed[..colon_pos].trim().trim_start_matches('-').trim();
                if !key.is_empty() {
                    return Some(key.to_string());
                }
            }
            break;
        }
    }
    None
}

/// Suggest common Fleet label names (data-driven from completions.toml).
fn complete_common_labels() -> Vec<CompletionItem> {
    COMPLETION_DATA
        .labels
        .iter()
        .enumerate()
        .map(|(i, label)| CompletionItem {
            label: label.name.clone(),
            kind: Some(CompletionItemKind::VALUE),
            detail: Some(label.description.clone()),
            insert_text: Some(format!("\"{}\"", label.name)),
            sort_text: Some(format!("0_{}", i)),
            ..Default::default()
        })
        .collect()
}

/// Suggest common Fleet software categories (data-driven from completions.toml).
fn complete_common_categories() -> Vec<CompletionItem> {
    COMPLETION_DATA
        .categories
        .values
        .iter()
        .enumerate()
        .map(|(i, name)| CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::VALUE),
            sort_text: Some(format!("0_{}", i)),
            ..Default::default()
        })
        .collect()
}

/// Complete path reference fields (used in configuration_profiles, policies path refs, etc.)
/// Complete configuration_profiles array item fields (data-driven from completions.toml).
fn complete_path_ref_fields() -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for field in fields_for_context("configuration_profiles") {
        items.push(create_field_completion(
            &field.name,
            &field.description,
            field.required,
        ));
    }
    // Label-targeting macros: correct starter combinations for semantics
    // that are easy to get wrong by hand (one labels_* key per item) and
    // expensive to discover at gitops-apply time.
    for block in blocks_for_context("configuration_profiles") {
        let snippet = block.snippet.strip_prefix('\n').unwrap_or(&block.snippet);
        items.push(create_block_completion(
            &block.name,
            &block.description,
            snippet,
        ));
    }
    items
}

/// Complete scripts array item fields (data-driven from completions.toml).
fn complete_script_fields(_line: &str, _col_idx: usize) -> Vec<CompletionItem> {
    let mut items = Vec::new();
    for field in fields_for_context("scripts") {
        items.push(create_field_completion(
            &field.name,
            &field.description,
            field.required,
        ));
    }
    for block in blocks_for_context("scripts") {
        let snippet = block.snippet.strip_prefix('\n').unwrap_or(&block.snippet);
        items.push(create_block_completion(
            &block.name,
            &block.description,
            snippet,
        ));
    }
    items
}

/// Complete team_settings section.
fn complete_team_settings_section() -> Vec<CompletionItem> {
    let fields = [
        (
            "webhook_settings",
            "Webhook configuration for team events",
            false,
        ),
        ("features", "Feature flags for this team", false),
        ("host_expiry_settings", "Auto-remove inactive hosts", false),
        ("secrets", "Enrollment secrets for this team", false),
        (
            "integrations",
            "Third-party integrations (calendar, ticketing)",
            false,
        ),
    ];

    let mut items: Vec<CompletionItem> = fields
        .iter()
        .map(|(name, desc, required)| {
            let mut item = create_field_completion(name, desc, *required);
            item.sort_text = Some(format!("1_{}", name));
            item
        })
        .collect();

    let blocks = vec![
        (
            "features",
            "Feature flags (block)",
            "features:\n  enable_host_users: ${1:true}\n  enable_software_inventory: ${2:true}",
        ),
        (
            "host_expiry_settings",
            "Host expiry settings (block)",
            "host_expiry_settings:\n  host_expiry_enabled: ${1:true}\n  host_expiry_window: ${2:10}",
        ),
        (
            "secrets",
            "Enrollment secrets (block)",
            "secrets:\n  - secret: ${1:\\$ENROLL_SECRET}",
        ),
        (
            "integrations",
            "Integrations (block)",
            "integrations:\n  google_calendar:\n    - api_key_json: ${1:\\$GOOGLE_CALENDAR_API_KEY_JSON}\n      domain: ${2:example.com}",
        ),
    ];

    for (name, desc, snippet) in blocks {
        items.push(create_block_completion(name, desc, snippet));
    }

    items
}

/// Complete org_settings section.
fn complete_org_settings_section() -> Vec<CompletionItem> {
    let fields = [
        ("features", "Feature flags for the organization", false),
        ("fleet_desktop", "Fleet Desktop configuration", false),
        ("host_expiry_settings", "Auto-remove inactive hosts", false),
        (
            "org_info",
            "Organization name, logo, and contact info",
            false,
        ),
        ("secrets", "Enrollment secrets", false),
        ("server_settings", "Server URL and global toggles", false),
        ("sso_settings", "Single sign-on configuration", false),
        (
            "integrations",
            "Third-party integrations (Jira, Zendesk, calendar)",
            false,
        ),
        ("certificate_authorities", "Custom CA certificates", false),
        ("gitops", "GitOps mode settings", false),
        ("mdm", "Apple Business Manager and VPP settings", false),
    ];

    let mut items: Vec<CompletionItem> = fields
        .iter()
        .map(|(name, desc, required)| {
            let mut item = create_field_completion(name, desc, *required);
            item.sort_text = Some(format!("1_{}", name));
            item
        })
        .collect();

    let blocks = vec![
        (
            "features",
            "Feature flags (block)",
            "features:\n  enable_host_users: ${1:true}\n  enable_software_inventory: ${2:true}",
        ),
        (
            "fleet_desktop",
            "Fleet Desktop (block)",
            "fleet_desktop:\n  transparency_url: ${1:https://fleetdm.com/transparency}",
        ),
        (
            "host_expiry_settings",
            "Host expiry settings (block)",
            "host_expiry_settings:\n  host_expiry_enabled: ${1:true}\n  host_expiry_window: ${2:10}",
        ),
        (
            "org_info",
            "Organization info (block)",
            // org_logo_url is deprecated (app.go:1368) — scaffold the
            // mode-aware name so the snippet never seeds a rename warning.
            "org_info:\n  org_name: ${1:Organization}\n  org_logo_url_dark_mode: ${2:https://}\n  contact_url: ${3:https://}",
        ),
        (
            "secrets",
            "Enrollment secrets (block)",
            "secrets:\n  - secret: ${1:\\$ENROLL_SECRET}",
        ),
        (
            "server_settings",
            "Server settings (block)",
            "server_settings:\n  server_url: ${1:https://}\n  enable_analytics: ${2:true}",
        ),
        (
            "sso_settings",
            "SSO settings (block)",
            "sso_settings:\n  enable_sso: ${1:true}\n  idp_name: ${2:Okta}\n  entity_id: ${3:https://}\n  metadata: ${4:\\$SSO_METADATA}",
        ),
        (
            "integrations",
            "Integrations (block)",
            "integrations:\n  jira:\n    - url: ${1:https://example.atlassian.net}\n      username: ${2:user}\n      api_token: ${3:\\$JIRA_API_TOKEN}\n      project_key: ${4:PRJ}",
        ),
        (
            "gitops",
            "GitOps settings (block)",
            "gitops:\n  gitops_mode_enabled: ${1:true}\n  repository_url: ${2:https://}",
        ),
        (
            "mdm",
            "MDM settings (block)",
            "mdm:\n  apple_business_manager:\n    - organization_name: ${1:Organization}\n      macos_team: ${2:Workstations}\n      ios_team: ${3:iPhones}\n      ipados_team: ${4:iPads}",
        ),
    ];

    for (name, desc, snippet) in blocks {
        items.push(create_block_completion(name, desc, snippet));
    }

    items
}

/// Complete org_settings.fleet_desktop fields.
fn complete_org_fleet_desktop_fields() -> Vec<CompletionItem> {
    let fields = [(
        "transparency_url",
        "URL shown in Fleet Desktop transparency page",
        false,
    )];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete org_settings.server_settings fields.
fn complete_org_server_settings_fields() -> Vec<CompletionItem> {
    let fields = [
        ("server_url", "Fleet server URL", false),
        ("enable_analytics", "Enable usage analytics", false),
        ("live_query_disabled", "Disable live queries", false),
        ("ai_features_disabled", "Disable AI features", false),
        ("query_reports_disabled", "Disable query reports", false),
        (
            "query_report_cap",
            "Maximum number of query report rows",
            false,
        ),
        ("scripts_disabled", "Disable script execution", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete org_settings.sso_settings fields.
fn complete_org_sso_settings_fields() -> Vec<CompletionItem> {
    let fields = [
        ("enable_sso", "Enable single sign-on", false),
        ("idp_name", "Identity provider name", false),
        ("idp_image_url", "Identity provider logo URL", false),
        ("entity_id", "SAML entity ID", false),
        ("metadata", "SAML metadata XML", false),
        ("metadata_url", "SAML metadata URL", false),
        (
            "enable_jit_provisioning",
            "Enable just-in-time user provisioning",
            false,
        ),
        ("enable_sso_idp_login", "Enable IdP-initiated login", false),
        ("sso_server_url", "SSO server URL override", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete org_settings.org_info fields.
fn complete_org_info_fields() -> Vec<CompletionItem> {
    let fields = [
        ("org_name", "Organization display name", false),
        // Mode-aware names only. `org_logo_url` and
        // `org_logo_url_light_background` are deprecated in Fleet
        // (app.go:1368-1371) and auto-migrated by NormalizeLogoFields —
        // offering them would hand the user a key flint then warns about.
        (
            "org_logo_url_dark_mode",
            "Logo URL shown in dark mode",
            false,
        ),
        (
            "org_logo_url_light_mode",
            "Logo URL shown in light mode",
            false,
        ),
        (
            "org_logo_path_dark_mode",
            "Local logo file for dark mode (mutually exclusive with the URL form)",
            false,
        ),
        (
            "org_logo_path_light_mode",
            "Local logo file for light mode (mutually exclusive with the URL form)",
            false,
        ),
        ("contact_url", "Support contact URL", false),
    ];

    fields
        .iter()
        .map(|(name, desc, required)| create_field_completion(name, desc, *required))
        .collect()
}

/// Complete agent_options section.
fn complete_agent_options_section() -> Vec<CompletionItem> {
    let fields = [
        (
            "path",
            "Reference to external agent options YAML file",
            false,
        ),
        ("config", "osquery configuration options", false),
        ("update_channels", "Fleet component update channels", false),
        ("command_line_flags", "osquery command-line flags", false),
        ("extensions", "osquery extensions to load", false),
    ];

    let mut items: Vec<CompletionItem> = fields
        .iter()
        .map(|(name, desc, required)| {
            let mut item = create_field_completion(name, desc, *required);
            item.sort_text = Some(format!("1_{}", name));
            item
        })
        .collect();

    let blocks = vec![(
        "config",
        "osquery config (block)",
        "config:\n  options:\n    distributed_interval: ${1:10}\n    logger_tls_period: ${2:10}",
    )];

    for (name, desc, snippet) in blocks {
        items.push(create_block_completion(name, desc, snippet));
    }

    items
}

/// Complete file paths for path: values.
fn complete_file_paths(
    line: &str,
    line_idx: usize,
    col_idx: usize,
    current_file: Option<&Path>,
    workspace_root: Option<&Path>,
    context_type: PathContextType,
) -> Vec<CompletionItem> {
    let mut completions = Vec::new();

    // Extract partial path already typed (text after "path: " or "paths: ")
    let partial = extract_partial_path(line, col_idx);

    // Find the start position of the value (after "path: " or "paths: ")
    // so we can create a TextEdit that replaces the entire value, not just appends
    let value_start_col = line.find(':').map(|c| {
        let after_colon = &line[c + 1..];
        let trimmed_len = after_colon.len() - after_colon.trim_start().len();
        c + 1 + trimmed_len
    });

    // Determine base directory for scanning
    let base_dir = match (workspace_root, current_file) {
        (Some(root), _) => root.to_path_buf(),
        (None, Some(file)) => file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        (None, None) => return completions,
    };

    // Scan all immediate subdirectories of the workspace root for matching files.
    // This handles any directory layout — lib/, platforms/, labels/, fleets/,
    // or custom directory names — without hardcoding.
    if let Ok(entries) = std::fs::read_dir(&base_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                // Skip hidden dirs, build artifacts, and node_modules
                if name.starts_with('.')
                    || name == "target"
                    || name == "node_modules"
                    || name == "dist"
                {
                    continue;
                }
                scan_directory_for_paths(
                    &path,
                    current_file,
                    &context_type,
                    &partial,
                    &base_dir,
                    &mut completions,
                    0,
                );
            }
        }
    }

    // Sort completions alphabetically
    completions.sort_by(|a, b| a.label.cmp(&b.label));

    // Add text_edit to each completion so it replaces the entire value after
    // the colon, instead of inserting at the cursor (which causes duplication
    // when the user has already typed part of the path).
    if let Some(start_col) = value_start_col {
        for item in &mut completions {
            item.text_edit = Some(CompletionTextEdit::Edit(TextEdit {
                range: Range {
                    start: Position {
                        line: line_idx as u32,
                        character: start_col as u32,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: col_idx as u32,
                    },
                },
                new_text: item.label.clone(),
            }));
            // filter_text ensures the completion still matches against the partial input
            item.filter_text = Some(item.label.clone());
        }
    }

    completions
}

/// Extract the partial path the user has typed after "path: ".
fn extract_partial_path(line: &str, col_idx: usize) -> String {
    let trimmed = line.trim().trim_start_matches('-').trim();

    if let Some(_colon_pos) = trimmed.find(':') {
        // Find where the value starts in the original line
        if let Some(line_colon_pos) = line.find(':') {
            let value_start = line_colon_pos + 1;
            // Get text from after colon to cursor position
            if col_idx > value_start {
                let value_portion = &line[value_start..col_idx.min(line.len())];
                return value_portion
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
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
            if partial.is_empty()
                || relative_path
                    .to_lowercase()
                    .contains(&partial.to_lowercase())
            {
                completions.push(create_path_completion(&relative_path, &path, context_type));
            }
        }
    }
}

/// Check if a file matches the expected context type based on extension.
fn matches_context_type(path: &Path, context_type: &PathContextType) -> bool {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

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
            value: format!("**{}**\n\nPath: `{}`", file_name, absolute_path.display()),
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
        let completions = complete_at(
            source,
            Position {
                line: 0,
                character: 0,
            },
        );
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"policies"));
        assert!(labels.contains(&"queries"));
        assert!(labels.contains(&"labels"));
        // future_names=false should NOT suggest new names
        assert!(!labels.contains(&"reports"));
        assert!(!labels.contains(&"settings"));
    }

    #[test]
    fn test_complete_top_level_future_names() {
        let source = "";
        let completions = complete_at_with_context(
            source,
            Position {
                line: 0,
                character: 0,
            },
            None,
            None,
            true,
        );
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        // future_names=true should suggest new names
        assert!(labels.contains(&"reports"));
        assert!(labels.contains(&"settings"));
        // Should NOT suggest old names that have been replaced
        assert!(!labels.contains(&"queries"));
        // policies is unchanged
        assert!(labels.contains(&"policies"));
    }

    #[test]
    fn test_complete_policy_fields() {
        let source = "policies:\n  - ";
        let completions = complete_at(
            source,
            Position {
                line: 1,
                character: 4,
            },
        );
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"name"));
        assert!(labels.contains(&"query"));
        assert!(labels.contains(&"platform"));
    }

    #[test]
    fn test_complete_platform_values() {
        let source = "policies:\n  - name: test\n    platform: ";
        let completions = complete_at(
            source,
            Position {
                line: 2,
                character: 15,
            },
        );

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        assert!(labels.contains(&"darwin"));
        assert!(labels.contains(&"windows"));
        assert!(labels.contains(&"linux"));
    }

    #[test]
    fn test_complete_osquery_tables() {
        let source = "policies:\n  - name: test\n    query: |\n      SELECT * FROM ";
        let completions = complete_at(
            source,
            Position {
                line: 3,
                character: 20,
            },
        );

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
        assert!(matches_context_type(
            Path::new("test.yml"),
            &PathContextType::SoftwarePackage
        ));
        assert!(matches_context_type(
            Path::new("test.yaml"),
            &PathContextType::SoftwarePackage
        ));
        assert!(!matches_context_type(
            Path::new("test.sh"),
            &PathContextType::SoftwarePackage
        ));

        // Script should match .sh, .ps1, .bat
        assert!(matches_context_type(
            Path::new("test.sh"),
            &PathContextType::Script
        ));
        assert!(matches_context_type(
            Path::new("test.ps1"),
            &PathContextType::Script
        ));
        assert!(matches_context_type(
            Path::new("test.bat"),
            &PathContextType::Script
        ));
        assert!(!matches_context_type(
            Path::new("test.yml"),
            &PathContextType::Script
        ));

        // MacOSProfile should match .mobileconfig and .plist
        assert!(matches_context_type(
            Path::new("test.mobileconfig"),
            &PathContextType::MacOSProfile
        ));
        assert!(matches_context_type(
            Path::new("test.plist"),
            &PathContextType::MacOSProfile
        ));
        assert!(!matches_context_type(
            Path::new("test.xml"),
            &PathContextType::MacOSProfile
        ));

        // WindowsProfile should match .xml
        assert!(matches_context_type(
            Path::new("test.xml"),
            &PathContextType::WindowsProfile
        ));
        assert!(!matches_context_type(
            Path::new("test.yml"),
            &PathContextType::WindowsProfile
        ));

        // Generic should match anything
        assert!(matches_context_type(
            Path::new("test.yml"),
            &PathContextType::Generic
        ));
        assert!(matches_context_type(
            Path::new("test.sh"),
            &PathContextType::Generic
        ));
        assert!(matches_context_type(
            Path::new("test.txt"),
            &PathContextType::Generic
        ));
    }

    #[test]
    fn test_complete_inside_reports_context() {
        let source = "reports:\n  - ";
        let completions = complete_at(
            source,
            Position {
                line: 1,
                character: 4,
            },
        );
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        // reports maps to QueryField, so should get query fields
        assert!(labels.contains(&"name"));
        assert!(labels.contains(&"query"));
    }

    #[test]
    fn test_complete_inside_settings_context() {
        let source = "settings:\n  ";
        let completions = complete_at(
            source,
            Position {
                line: 1,
                character: 2,
            },
        );
        assert!(!completions.is_empty());

        let labels: Vec<_> = completions.iter().map(|c| c.label.as_str()).collect();
        // settings maps to TeamSettingsSection
        assert!(labels.contains(&"features"));
    }

    #[test]
    fn test_path_context_detection() {
        // In software.packages, path: should give SoftwarePackage context
        let source = "software:\n  packages:\n    - path: ";
        let context = determine_completion_context(source, 2, "    - path: ", 12);
        assert_eq!(
            context,
            CompletionContext::PathValue {
                context_type: PathContextType::SoftwarePackage
            }
        );

        // In controls.scripts, path: should give Script context
        let source2 = "controls:\n  scripts:\n    - path: ";
        let context2 = determine_completion_context(source2, 2, "    - path: ", 12);
        assert_eq!(
            context2,
            CompletionContext::PathValue {
                context_type: PathContextType::Script
            }
        );

        // In macos_settings.custom_settings, path: should give MacOSProfile context
        let source3 = "controls:\n  macos_settings:\n    custom_settings:\n      - path: ";
        let context3 = determine_completion_context(source3, 3, "      - path: ", 14);
        assert_eq!(
            context3,
            CompletionContext::PathValue {
                context_type: PathContextType::MacOSProfile
            }
        );
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
            0,  // line_idx
            12, // col_idx
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

    #[test]
    fn test_controls_block_snippets() {
        let completions = complete_controls_section();
        assert!(!completions.is_empty());

        // Should have both field completions and block snippets
        let block_items: Vec<_> = completions
            .iter()
            .filter(|c| c.insert_text_format == Some(InsertTextFormat::SNIPPET))
            .collect();
        assert!(
            !block_items.is_empty(),
            "should have block snippet completions"
        );

        let block_labels: Vec<_> = block_items.iter().map(|c| c.label.as_str()).collect();
        assert!(block_labels.contains(&"macos_updates (block)"));
        assert!(block_labels.contains(&"ios_updates (block)"));
        assert!(block_labels.contains(&"ipados_updates (block)"));
        assert!(block_labels.contains(&"windows_updates (block)"));
        assert!(block_labels.contains(&"macos_settings (block)"));
        assert!(block_labels.contains(&"windows_settings (block)"));
        assert!(block_labels.contains(&"macos_migration (block)"));
        assert!(block_labels.contains(&"macos_setup (block)"));
        assert!(block_labels.contains(&"android_settings (block)"));
        assert!(block_labels.contains(&"scripts (block)"));
        assert!(block_labels.contains(&"enable_disk_encryption (block)"));
        assert!(block_labels.contains(&"windows_enabled_and_configured (block)"));
        assert!(block_labels.contains(&"windows_migration_enabled (block)"));
        assert!(block_labels.contains(&"windows_require_bitlocker_pin (block)"));
        assert!(block_labels.contains(&"enable_turn_on_windows_mdm_manually (block)"));

        // Field completions should still be present
        let field_labels: Vec<_> = completions
            .iter()
            .filter(|c| c.insert_text_format.is_none())
            .map(|c| c.label.as_str())
            .collect();
        assert!(field_labels.contains(&"macos_updates"));
        assert!(field_labels.contains(&"enable_disk_encryption"));
        assert!(field_labels.contains(&"scripts"));
        assert!(field_labels.contains(&"android_settings"));
        assert!(field_labels.contains(&"windows_enabled_and_configured"));
    }

    #[test]
    fn test_block_snippet_content() {
        let completions = complete_controls_section();

        // Check macos_updates block snippet content
        let macos_updates_block = completions
            .iter()
            .find(|c| c.label == "macos_updates (block)")
            .expect("macos_updates block snippet should exist");
        let snippet = macos_updates_block.insert_text.as_deref().unwrap();
        assert!(
            snippet.contains("deadline:"),
            "snippet should contain deadline"
        );
        assert!(
            snippet.contains("minimum_version:"),
            "snippet should contain minimum_version"
        );
        assert!(
            snippet.contains("update_new_hosts:"),
            "snippet should contain update_new_hosts"
        );
        assert!(snippet.contains("${1:"), "snippet should have tab stop $1");
        assert!(snippet.contains("${2:"), "snippet should have tab stop $2");
        assert!(snippet.contains("${3:"), "snippet should have tab stop $3");

        // Check windows_updates block snippet has different fields
        let windows_block = completions
            .iter()
            .find(|c| c.label == "windows_updates (block)")
            .expect("windows_updates block snippet should exist");
        let win_snippet = windows_block.insert_text.as_deref().unwrap();
        assert!(win_snippet.contains("deadline_days:"));
        assert!(win_snippet.contains("grace_period_days:"));

        // Check macos_setup has multiple tab stops
        let setup_block = completions
            .iter()
            .find(|c| c.label == "macos_setup (block)")
            .expect("macos_setup block snippet should exist");
        let setup_snippet = setup_block.insert_text.as_deref().unwrap();
        assert!(setup_snippet.contains("enable_end_user_authentication:"));
        assert!(setup_snippet.contains("macos_setup_assistant:"));
        assert!(setup_snippet.contains("script:"));
        assert!(
            setup_snippet.contains("${3:"),
            "macos_setup should have 3 tab stops"
        );
    }

    #[test]
    fn test_block_snippets_sort_before_fields() {
        let completions = complete_controls_section();

        // Block snippets should have sort_text starting with "0_"
        // Field completions should have sort_text starting with "1_"
        for item in &completions {
            if item.insert_text_format == Some(InsertTextFormat::SNIPPET) {
                assert!(
                    item.sort_text.as_ref().unwrap().starts_with("0_"),
                    "block snippet {} should sort first",
                    item.label
                );
            } else if item.sort_text.is_some() {
                assert!(
                    item.sort_text.as_ref().unwrap().starts_with("1_"),
                    "field {} should sort after snippets",
                    item.label
                );
            }
        }
    }

    #[test]
    fn test_top_level_block_snippets() {
        let completions = complete_top_level_fields(false);
        let block_labels: Vec<_> = completions
            .iter()
            .filter(|c| c.insert_text_format == Some(InsertTextFormat::SNIPPET))
            .map(|c| c.label.as_str())
            .collect();
        assert!(block_labels.contains(&"policies (block)"));
        assert!(block_labels.contains(&"queries (block)"));
        assert!(block_labels.contains(&"labels (block)"));
        assert!(block_labels.contains(&"controls (block)"));
        assert!(block_labels.contains(&"software (block)"));
        assert!(block_labels.contains(&"agent_options (block)"));
        assert!(block_labels.contains(&"org_settings (block)"));
        assert!(block_labels.contains(&"team_settings (block)"));

        // future_names=true should have reports instead of queries
        let future = complete_top_level_fields(true);
        let future_blocks: Vec<_> = future
            .iter()
            .filter(|c| c.insert_text_format == Some(InsertTextFormat::SNIPPET))
            .map(|c| c.label.as_str())
            .collect();
        assert!(future_blocks.contains(&"reports (block)"));
        assert!(future_blocks.contains(&"settings (block)"));
        assert!(!future_blocks.contains(&"queries (block)"));
    }

    #[test]
    fn test_software_section_block_snippets() {
        let completions = complete_software_section();
        let block_labels: Vec<_> = completions
            .iter()
            .filter(|c| c.insert_text_format == Some(InsertTextFormat::SNIPPET))
            .map(|c| c.label.as_str())
            .collect();
        assert!(block_labels.contains(&"packages (block)"));
        assert!(block_labels.contains(&"app_store_apps (block)"));
        assert!(block_labels.contains(&"fleet_maintained_apps (block)"));
    }

    #[test]
    fn test_org_settings_block_snippets() {
        let completions = complete_org_settings_section();
        let block_labels: Vec<_> = completions
            .iter()
            .filter(|c| c.insert_text_format == Some(InsertTextFormat::SNIPPET))
            .map(|c| c.label.as_str())
            .collect();
        assert!(block_labels.contains(&"features (block)"));
        assert!(block_labels.contains(&"fleet_desktop (block)"));
        assert!(block_labels.contains(&"host_expiry_settings (block)"));
        assert!(block_labels.contains(&"org_info (block)"));
        assert!(block_labels.contains(&"secrets (block)"));
        assert!(block_labels.contains(&"server_settings (block)"));
        assert!(block_labels.contains(&"sso_settings (block)"));
        assert!(block_labels.contains(&"integrations (block)"));
        assert!(block_labels.contains(&"gitops (block)"));
        assert!(block_labels.contains(&"mdm (block)"));

        // Verify sso_settings snippet has expected content
        let sso_block = completions
            .iter()
            .find(|c| c.label == "sso_settings (block)")
            .unwrap();
        let snippet = sso_block.insert_text.as_deref().unwrap();
        assert!(snippet.contains("enable_sso:"));
        assert!(snippet.contains("idp_name:"));
        assert!(snippet.contains("entity_id:"));
    }

    #[test]
    fn test_team_settings_block_snippets() {
        let completions = complete_team_settings_section();
        let block_labels: Vec<_> = completions
            .iter()
            .filter(|c| c.insert_text_format == Some(InsertTextFormat::SNIPPET))
            .map(|c| c.label.as_str())
            .collect();
        assert!(block_labels.contains(&"features (block)"));
        assert!(block_labels.contains(&"host_expiry_settings (block)"));
        assert!(block_labels.contains(&"secrets (block)"));
        assert!(block_labels.contains(&"integrations (block)"));
    }

    #[test]
    fn test_agent_options_block_snippets() {
        let completions = complete_agent_options_section();
        let block_labels: Vec<_> = completions
            .iter()
            .filter(|c| c.insert_text_format == Some(InsertTextFormat::SNIPPET))
            .map(|c| c.label.as_str())
            .collect();
        assert!(block_labels.contains(&"config (block)"));

        // Field completions should include path
        let field_labels: Vec<_> = completions
            .iter()
            .filter(|c| c.insert_text_format.is_none())
            .map(|c| c.label.as_str())
            .collect();
        assert!(field_labels.contains(&"path"));
        assert!(field_labels.contains(&"config"));
    }
}
