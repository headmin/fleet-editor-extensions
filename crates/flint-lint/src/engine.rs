//! Linting engine — orchestrates rule execution and file traversal.
//!
//! The `Linter` struct is the main entry point. It loads configuration,
//! runs rules in parallel (via rayon), applies suppressions, and produces
//! a `LintReport` per file.

use super::config::FleetLintConfig;
use super::error::{LintError, LintReport, Severity};
use super::fleet_config::{
    FleetConfig, Label, LabelOrPath, Policy, PolicyOrPath, Query, QueryOrPath,
};
use super::rules::RuleSet;
use super::version_gate::VersionContext;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Linter {
    rules: RuleSet,
    config: FleetLintConfig,
    /// Directory containing the loaded `.fleetlint.toml`, when known.
    /// `[files]` include/exclude globs are resolved relative to this.
    config_root: Option<PathBuf>,
    /// Shared handle the rules read for wiring knowledge; filled once per
    /// directory lint. Empty for single-file lints, where wiring is unknowable.
    referenced: super::rules::ReferencedPaths,
}

/// Build the standard ruleset parameterized by a config (version context for
/// deprecations, thresholds for interval/query rules).
fn rules_for(config: &FleetLintConfig, version_ctx: VersionContext) -> RuleSet {
    rules_for_with_snapshot(config, version_ctx, None)
}

/// Build the ruleset with an optional server snapshot attached.
///
/// Separate from `rules_for` because the snapshot lives next to the config
/// file, so only the config-discovering constructors can find one — and
/// `Linter::new()`/`with_rules()` must stay snapshot-free so a bare linter
/// behaves identically with or without a snapshot on disk.
fn rules_for_with_snapshot(
    config: &FleetLintConfig,
    version_ctx: VersionContext,
    snapshot: Option<std::sync::Arc<super::snapshot::LoadedSnapshot>>,
) -> RuleSet {
    rules_for_full(config, version_ctx, snapshot, None)
}

fn rules_for_full(
    config: &FleetLintConfig,
    version_ctx: VersionContext,
    snapshot: Option<std::sync::Arc<super::snapshot::LoadedSnapshot>>,
    referenced: Option<super::rules::ReferencedPaths>,
) -> RuleSet {
    RuleSet::standard(super::rules::RuleOptions {
        version: version_ctx,
        thresholds: config.thresholds.clone(),
        snapshot,
        placeholders: config.placeholders.clone(),
        referenced: referenced.unwrap_or_default(),
    })
}

/// Discover a snapshot beside the config root, if one exists.
fn discover_snapshot_with_age(
    root: &Path,
    max_age_days: u64,
) -> Option<std::sync::Arc<super::snapshot::LoadedSnapshot>> {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    super::snapshot::LoadedSnapshot::discover(root, max_age_days, now_unix)
        .map(std::sync::Arc::new)
}

#[allow(dead_code)]
fn discover_snapshot(root: &Path) -> Option<std::sync::Arc<super::snapshot::LoadedSnapshot>> {
    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    super::snapshot::LoadedSnapshot::discover(
        root,
        super::snapshot::DEFAULT_MAX_AGE_DAYS,
        now_unix,
    )
    .map(std::sync::Arc::new)
}

impl Linter {
    /// Default rules, default config — deliberately NO config discovery
    /// (pinned by `test_new_does_not_load_config`); use [`Linter::from_path`]
    /// to pick up a repo's `.fleetlint.toml`.
    pub fn new() -> Self {
        Self {
            rules: RuleSet::default_rules(),
            config: FleetLintConfig::default(),
            config_root: None,

            referenced: Default::default(),
        }
    }

    pub fn with_rules(rules: RuleSet) -> Self {
        Self {
            rules,
            config: FleetLintConfig::default(),
            config_root: None,

            referenced: Default::default(),
        }
    }

    /// Create a linter with configuration.
    pub fn with_config(config: FleetLintConfig) -> Self {
        let version_ctx = VersionContext::resolve(
            Some(&config.deprecations.fleet_version),
            config.deprecations.future_names,
        );
        Self {
            rules: rules_for(&config, version_ctx),
            config,
            config_root: None,

            referenced: Default::default(),
        }
    }

    /// Create a linter by searching for configuration from a path.
    pub fn from_path(start_path: &Path) -> Self {
        match FleetLintConfig::find_and_load(start_path) {
            Some((config_path, config)) => {
                let config_root = config_path.parent().map(|root| {
                    root.canonicalize().unwrap_or_else(|_| root.to_path_buf())
                });
                let version_ctx = VersionContext::resolve(
                    Some(&config.deprecations.fleet_version),
                    config.deprecations.future_names,
                );
                let max_age = config
                    .fleet
                    .snapshot_max_age_days
                    .unwrap_or(super::snapshot::DEFAULT_MAX_AGE_DAYS);
                let snapshot = config_root
                    .as_deref()
                    .and_then(|r| discover_snapshot_with_age(r, max_age));
                let referenced: super::rules::ReferencedPaths = Default::default();
                Self {
                    rules: rules_for_full(&config, version_ctx, snapshot, Some(referenced.clone())),
                    referenced,
                    config,
                    config_root,
                }
            }
            None => {
                let config = FleetLintConfig::default();
                Self {
                    rules: rules_for(&config, VersionContext::latest()),
                    config,
                    config_root: None,

                    referenced: Default::default(),
                }
            }
        }
    }

    /// Get the current configuration.
    pub fn config(&self) -> &FleetLintConfig {
        &self.config
    }

    /// Set the configuration (rebuilds the threshold-parameterized rules).
    pub fn set_config(&mut self, config: FleetLintConfig) {
        let version_ctx = VersionContext::resolve(
            Some(&config.deprecations.fleet_version),
            config.deprecations.future_names,
        );
        self.rules = rules_for(&config, version_ctx);
        self.config = config;
    }

    /// Set the directory that `[files]` include/exclude globs resolve
    /// against (the directory holding `.fleetlint.toml`). Needed when the
    /// config was not discovered via [`Linter::from_path`], e.g. the LSP
    /// receiving a workspace root (issue #15).
    pub fn set_config_root(&mut self, root: PathBuf) {
        self.config_root = Some(root.canonicalize().unwrap_or(root));
    }

    /// Whether a file passes the config's `[files]` include/exclude globs.
    ///
    /// Absolute paths are first made relative to the config root so that
    /// narrowing `include` patterns written relative to the repo (e.g.
    /// `fleets/*.yml`) still match (issue #15).
    pub fn should_lint(&self, file_path: &Path) -> bool {
        let candidate = self
            .config_root
            .as_ref()
            .and_then(|root| {
                let abs = file_path
                    .canonicalize()
                    .unwrap_or_else(|_| file_path.to_path_buf());
                abs.strip_prefix(root).ok().map(Path::to_path_buf)
            })
            .unwrap_or_else(|| file_path.to_path_buf());

        self.config.should_lint_file(&candidate)
    }

    /// Whether `[files]` scoping puts this path out of bounds, resolved the
    /// same way `should_lint` resolves its candidate.
    ///
    /// Used to scope WORKSPACE-rule findings. Those attach to files the YAML
    /// walk never sees (scripts, profiles, payloads), so `should_lint` is the
    /// wrong question for them — it is false for every non-YAML path.
    pub fn is_out_of_scope(&self, file_path: &Path) -> bool {
        let candidate = self
            .config_root
            .as_ref()
            .and_then(|root| {
                let abs = file_path
                    .canonicalize()
                    .unwrap_or_else(|_| file_path.to_path_buf());
                abs.strip_prefix(root).ok().map(Path::to_path_buf)
            })
            .unwrap_or_else(|| file_path.to_path_buf());

        self.config.is_out_of_scope_file(&candidate)
    }

    /// Lint a single file
    pub fn lint_file(&self, file_path: &Path) -> Result<LintReport> {
        // Read file
        let source = fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read file: {}", file_path.display()))?;

        self.lint_content(&source, file_path)
    }

    /// Lint content directly (for LSP - content already in memory).
    ///
    /// This method is useful when the file content is already available,
    /// such as in an LSP server where the client sends document content.
    pub fn lint_content(&self, content: &str, file_path: &Path) -> Result<LintReport> {
        // Run basic YAML hygiene checks first (before parsing)
        let mut report = LintReport::new();
        check_yaml_hygiene(content, file_path, &mut report);

        // Use file path to determine the expected type, then parse accordingly.
        // This prevents labels from being misidentified as policies, software files
        // from triggering policy checks, etc.
        let file_type = detect_file_type(file_path);

        // Agent-options lib files and opaque non-YAML assets are not fleet
        // configs and nothing in the ruleset applies — return early with just
        // hygiene checks.
        if matches!(file_type, FileType::AgentOptions | FileType::NonYaml) {
            return Ok(report);
        }

        // Software lib files aren't fleet configs either, but a few rules still
        // apply (they parse the file directly, e.g. `software-url`). Run only
        // those — see SOFTWARE_RULES and the filter in the rule loop below.
        let software_only = matches!(file_type, FileType::Software);

        let fleet_config: FleetConfig = match file_type {
            FileType::Software => FleetConfig::default(),
            FileType::Labels => {
                // lib/*/labels/*.yml — parse as label array
                if let Ok(labels) = serde_yaml::from_str::<Vec<Label>>(content) {
                    FleetConfig {
                        labels: Some(labels.into_iter().map(LabelOrPath::Label).collect()),
                        ..Default::default()
                    }
                } else {
                    FleetConfig::default()
                }
            }
            FileType::AgentOptions | FileType::NonYaml => {
                unreachable!("handled by early return above")
            }
            FileType::Policies => {
                // lib/*/policies/*.yml — parse as policy array
                if let Ok(policies) = serde_yaml::from_str::<Vec<Policy>>(content) {
                    FleetConfig {
                        policies: Some(policies.into_iter().map(PolicyOrPath::Policy).collect()),
                        ..Default::default()
                    }
                } else {
                    FleetConfig::default()
                }
            }
            FileType::Queries => {
                // lib/*/queries/*.yml or lib/*/reports/*.yml — parse as query array
                if let Ok(queries) = serde_yaml::from_str::<Vec<Query>>(content) {
                    FleetConfig {
                        queries: Some(queries.into_iter().map(QueryOrPath::Query).collect()),
                        ..Default::default()
                    }
                } else {
                    FleetConfig::default()
                }
            }
            FileType::FleetConfig => {
                // default.yml, fleets/*.yml, teams/*.yml — full fleet config
                match serde_yaml::from_str(content) {
                    Ok(config) => config,
                    Err(_) => {
                        // Last resort: try parsing as generic YAML for a parse error
                        match serde_yaml::from_str::<serde_yaml::Value>(content) {
                            Ok(_) => FleetConfig::default(),
                            Err(e) => {
                                let err_msg = e.to_string();

                                // Fleet's Go YAML parser accepts duplicate keys
                                // (e.g. multiple `path:` under `packages:` or
                                // `configuration_profiles:`). serde_yaml rejects
                                // them but this is valid Fleet GitOps YAML — skip.
                                if err_msg.contains("duplicate entry") {
                                    FleetConfig::default()
                                } else {
                                    let mut err = LintError::error(
                                        format!("YAML parse error: {}", e),
                                        file_path,
                                    )
                                    .with_rule_code(crate::codes::YAML_SYNTAX);

                                    if let Some(location) = e.location() {
                                        err = err.with_location(location.line(), location.column());
                                    }

                                    report.add(err);
                                    return Ok(report);
                                }
                            }
                        }
                    }
                }
            }
        };

        // Run all rules
        // (report was initialized earlier with YAML hygiene checks)

        // Get disabled and warning rules from config
        let mut disabled_rules = self.config.disabled_rules();
        let warning_rules = self.config.warning_rules();

        // If allow_unknown_fields is enabled, disable structural validation
        if self.config.schema.allow_unknown_fields {
            disabled_rules.insert("structural-validation");
        }

        // Collect all errors first (for suppression filtering)
        let mut all_errors = Vec::new();

        // Rules that apply to standalone software lib files (which are not
        // fleet configs). These parse the file directly rather than relying on
        // the typed FleetConfig, so they work with the empty config above.
        const SOFTWARE_RULES: &[&str] =
            &["software-url", "software-source", "structural-validation"];

        for rule in self.rules.rules() {
            // Skip disabled rules
            if disabled_rules.contains(rule.name()) {
                continue;
            }

            // On software lib files, run only the software-applicable subset.
            if software_only && !SOFTWARE_RULES.contains(&rule.name()) {
                continue;
            }

            let errors = rule.check(&fleet_config, file_path, content);

            // Downgrade to warnings if configured
            let should_warn = warning_rules.contains(rule.name());

            for mut error in errors {
                if should_warn && error.severity == Severity::Error {
                    error.severity = Severity::Warning;
                }
                // Tag each error with its originating rule code
                if error.rule_code.is_none() {
                    error.rule_code = Some(rule.name());
                }
                all_errors.push(error);
            }
        }

        // Apply inline suppressions (# fleet-lint: ignore [rule-code]) and
        // configured downgrades through the shared control pass.
        let suppressions = parse_suppressions(content);
        apply_error_controls(&mut all_errors, &suppressions, &warning_rules);

        for mut error in all_errors {
            stamp_doc_url(&mut error);
            report.add(error);
        }

        Ok(report)
    }

    /// Lint multiple files. Uses rayon for parallel processing when > 3 files.
    pub fn lint_files(&self, files: &[&Path]) -> Result<Vec<(PathBuf, LintReport)>> {
        use rayon::prelude::*;

        let lint_one = |file: &&Path| -> (PathBuf, LintReport) {
            match self.lint_file(file) {
                Ok(report) => (file.to_path_buf(), report),
                Err(e) => {
                    let mut report = LintReport::new();
                    report.add(LintError::error(
                        format!("Failed to lint file: {}", e),
                        *file,
                    ));
                    (file.to_path_buf(), report)
                }
            }
        };

        let results = if files.len() > 3 {
            files.par_iter().map(lint_one).collect()
        } else {
            files.iter().map(lint_one).collect()
        };

        Ok(results)
    }

    /// Lint a directory recursively. Parallelized via rayon for large repos.
    pub fn lint_directory(
        &self,
        dir: &Path,
        pattern: Option<&str>,
    ) -> Result<Vec<(PathBuf, LintReport)>> {
        use rayon::prelude::*;

        let pattern = pattern.unwrap_or("**/*.{yml,yaml}");

        // Find all YAML files, filtered through the config's include/exclude
        // globs (.fleetlint.toml `files`) so excluded folders are skipped by
        // the per-file rules AND the cross-file graph pass below.
        let mut yaml_files = find_yaml_files(dir, pattern)?;
        yaml_files.retain(|p| self.should_lint(p));

        // Read each file ONCE; both the per-file rules and the cross-file
        // pass work from these sources (previously the cross-file pass
        // re-read every file from disk).
        let sources: Vec<(PathBuf, String)> = yaml_files
            .into_iter()
            .filter_map(|path| {
                let source = fs::read_to_string(&path).ok()?;
                Some((path, source))
            })
            .collect();

        // Fill the wiring handle BEFORE per-file rules run.
        //
        // software-source may only escalate an unresolved hash to an ERROR for
        // a file some config actually references: `fleetctl gitops` reads a
        // software file only through a `path:`/`paths:` reference, so an
        // unreferenced file with a bad hash cannot fail an apply. Claiming
        // otherwise is the same overreach as the glob-zero rule.
        //
        // Parsed here rather than reusing the cross-file pass's Workspace
        // because that pass runs AFTER per-file rules. One extra parse of the
        // already-read sources, not a second disk walk.
        {
            let parsed: Vec<super::cross_reference::ParsedFile> = sources
                .iter()
                .filter_map(|(path, source)| {
                    Some(super::cross_reference::ParsedFile {
                        path: path.clone(),
                        source: source.clone(),
                        yaml: serde_yaml::from_str(source).ok()?,
                    })
                })
                .collect();
            let ws = super::workspace::Workspace::build(dir, &parsed);
            let mut refs = std::collections::HashSet::new();
            for pf in ws.parsed {
                refs.extend(super::workspace::referenced_by(pf, &ws));
            }
            let _ = self.referenced.set(refs);
        }

        let lint_one = |(path, source): &(PathBuf, String)| -> (PathBuf, LintReport) {
            match self.lint_content(source, path) {
                Ok(report) => (path.clone(), report),
                Err(e) => {
                    let mut report = LintReport::new();
                    report.add(LintError::error(
                        format!("Failed to lint file: {}", e),
                        path.as_path(),
                    ));
                    (path.clone(), report)
                }
            }
        };
        let mut results: Vec<(PathBuf, LintReport)> = if sources.len() > 3 {
            sources.par_iter().map(lint_one).collect()
        } else {
            sources.iter().map(lint_one).collect()
        };

        // Cross-file graph pass: only meaningful with a whole repo in view, so
        // it runs here (directory lint) rather than per-file. Build the index
        // once, then check every file's references against it.
        self.run_cross_reference_pass(dir, &sources, &mut results);

        Ok(results)
    }

    /// The cross-file graph pass ONLY — no per-file rules. The LSP's
    /// save-time workspace pass uses this: the saved document's per-file
    /// diagnostics already come from `lint_content`, so re-running every
    /// rule over every repo file on each save would be pure waste. Returns
    /// only files that received findings.
    pub fn cross_file_findings(&self, dir: &Path) -> Result<Vec<(PathBuf, LintReport)>> {
        let mut yaml_files = find_yaml_files(dir, "**/*.{yml,yaml}")?;
        yaml_files.retain(|p| self.should_lint(p));
        let sources: Vec<(PathBuf, String)> = yaml_files
            .into_iter()
            .filter_map(|path| {
                let source = fs::read_to_string(&path).ok()?;
                Some((path, source))
            })
            .collect();
        let mut results: Vec<(PathBuf, LintReport)> = sources
            .iter()
            .map(|(p, _)| (p.clone(), LintReport::new()))
            .collect();
        self.run_cross_reference_pass(dir, &sources, &mut results);
        results.retain(|(_, r)| r.total_issues() > 0);
        Ok(results)
    }

    /// Build the repo-wide index and append cross-file reference findings
    /// (undefined labels, unresolved `install_software` hashes) to each file's
    /// report. Findings respect the same inline `# fleet-lint: ignore` and
    /// `.fleetlint.toml` controls as per-file rules.
    fn run_cross_reference_pass(
        &self,
        root: &Path,
        sources: &[(PathBuf, String)],
        results: &mut Vec<(PathBuf, LintReport)>,
    ) {
        use super::cross_reference::{
            check_app_store_vpp, check_package_id_match, check_references, check_team_membership,
            ParsedFile, RepoIndex,
        };

        // Respect disabling the cross-file rules via config. The code list
        // lives in the registry (`codes::CROSS_FILE`) — no hardcoded copies.
        let disabled = self.config.disabled_rules();
        if super::codes::CROSS_FILE
            .iter()
            .all(|code| disabled.contains(*code))
        {
            return;
        }

        // Parse each already-read source once (skip unparseable — per-file
        // linting already reported their syntax errors).
        let parsed: Vec<ParsedFile> = sources
            .iter()
            .filter_map(|(path, source)| {
                let yaml = serde_yaml::from_str(source).ok()?;
                Some(ParsedFile {
                    path: path.clone(),
                    source: source.clone(),
                    yaml,
                })
            })
            .collect();

        // Load the server snapshot once for the whole pass. Absent is the
        // normal case and simply leaves label findings at warning severity.
        let now_unix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let snapshot = crate::snapshot::LoadedSnapshot::discover(
            self.config_root.as_deref().unwrap_or(root),
            self.config
                .fleet
                .snapshot_max_age_days
                .unwrap_or(crate::snapshot::DEFAULT_MAX_AGE_DAYS),
            now_unix,
        );

        let index = RepoIndex::build(&parsed);
        let warning_rules = self.config.warning_rules();

        // Suppressions parsed once per file; findings attached via an index
        // map instead of a linear scan per finding (was O(files²)).
        let suppressions_by_path: HashMap<&Path, HashMap<usize, Vec<String>>> = parsed
            .iter()
            .map(|pf| (pf.path.as_path(), parse_suppressions(&pf.source)))
            .collect();
        // Owned keys so the map doesn't hold a borrow of `results` while we
        // mutate its reports.
        let report_idx: HashMap<PathBuf, usize> = results
            .iter()
            .enumerate()
            .map(|(i, (p, _))| (p.clone(), i))
            .collect();
        let empty_suppressions = HashMap::new();

        for pf in &parsed {
            let mut findings =
                check_references(&index, &pf.path, &pf.source, &pf.yaml, snapshot.as_ref());
            findings.retain(|e| !disabled.contains(e.rule_code.unwrap_or("")));
            if findings.is_empty() {
                continue;
            }

            let suppressions = suppressions_by_path
                .get(pf.path.as_path())
                .unwrap_or(&empty_suppressions);
            apply_error_controls(&mut findings, suppressions, &warning_rules);

            if let Some(&i) = report_idx.get(&pf.path) {
                for mut f in findings {
                    stamp_doc_url(&mut f);
                    results[i].1.add(f);
                }
            }
        }

        // Whole-repo passes that attach findings to a specific file (not the
        // file being scanned). Merge each through the same control pass.
        // Findings may attach to files that weren't linted (payloads,
        // scripts — e.g. case-collision between two .mobileconfig files);
        // those get a fresh report entry instead of being dropped.
        let mut report_idx = report_idx;
        let mut merge = |findings: Vec<(PathBuf, LintError)>| {
            for (path, err) in findings {
                // Scope workspace findings by `[files].exclude`.
                //
                // The Workspace file set deliberately stays COMPLETE — every
                // file under the root, excluded ones included — because it is
                // what reference resolution runs against. Dropping excluded
                // files from that set would turn each surviving reference to
                // one into a phantom `broken-reference`. So the exclusion is
                // applied to the finding's SUBJECT here instead: an excluded
                // file is never reported ON, but is still found BY others.
                if self.is_out_of_scope(&path) {
                    continue;
                }
                let suppressions = suppressions_by_path
                    .get(path.as_path())
                    .unwrap_or(&empty_suppressions);
                let mut one = vec![err];
                apply_error_controls(&mut one, suppressions, &warning_rules);
                let Some(mut err) = one.pop() else { continue };
                stamp_doc_url(&mut err);
                let i = *report_idx.entry(path.clone()).or_insert_with(|| {
                    results.push((path.clone(), LintReport::new()));
                    results.len() - 1
                });
                results[i].1.add(err);
            }
        };

        // A policy's install_software references a package the fleet doesn't include.
        if !disabled.contains(super::codes::INSTALL_SOFTWARE_TEAM) {
            merge(check_team_membership(&parsed));
        }
        // A policy's query checks a different id than the package it installs.
        if !disabled.contains(super::codes::INSTALL_SOFTWARE_ID) {
            merge(check_package_id_match(&parsed));
        }
        // app_store_apps declared but VPP not configured anywhere.
        if !disabled.contains(super::codes::APP_STORE_VPP) {
            merge(check_app_store_vpp(&parsed));
        }

        // Workspace rules (ADR-010 Phase 1): one file-set walk, then each
        // rule is index lookups over it. Declarative [[patterns]] (Phase 2)
        // share the same Workspace.
        let ws_rules: Vec<_> = super::workspace::workspace_rules()
            .into_iter()
            .filter(|r| !disabled.contains(r.code()))
            .collect();
        let has_patterns = !self.config.patterns.is_empty();
        if !ws_rules.is_empty() || has_patterns {
            let ws = super::workspace::Workspace::build(root, &parsed);
            for rule in ws_rules {
                merge(rule.check(&ws));
            }
            if has_patterns {
                let mut findings =
                    super::patterns::check_patterns(&self.config.patterns, root, &ws);
                findings.retain(|(_, e)| !disabled.contains(e.rule_code.unwrap_or("")));
                merge(findings);
            }
        }
    }
}

impl Default for Linter {
    fn default() -> Self {
        Self::new()
    }
}

/// File type classification based on path.
///
/// Used to determine how to parse a YAML file before attempting deserialization.
/// This prevents misidentification (e.g., labels parsed as policies).
#[derive(Debug, PartialEq)]
pub(crate) enum FileType {
    FleetConfig,  // default.yml, fleets/*.yml, teams/*.yml, unassigned.yml
    Policies,     // */policies/*.yml
    Queries,      // */queries/*.yml, */reports/*.yml
    Labels,       // */labels/*.yml, *.labels.yml
    Software,     // */software/*.yml
    AgentOptions, // agent-options*.yml
    NonYaml,      // profiles, scripts, icons, declarations, commands — not YAML to lint
}

/// Detect file type from path using directory names and file name patterns.
pub(crate) fn detect_file_type(path: &Path) -> FileType {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Agent options files
    if file_name.starts_with("agent-options") || file_name.starts_with("agent_options") {
        return FileType::AgentOptions;
    }

    if let Some(parent) = path.parent() {
        // v4.83 non-YAML asset directories — matched on the immediate parent
        // (these are leaf dirs holding profiles/scripts/icons, never nested).
        let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if matches!(
            parent_name,
            "configuration-profiles"
                | "declaration-profiles"
                | "enrollment-profiles"
                | "commands"
                | "scripts"
                | "icons"
                | "managed-app-configurations"
        ) {
            return FileType::NonYaml;
        }

        // Content directories (labels/software/policies/queries) may be nested
        // at ANY depth — e.g. labels/dynamic/*.yml, labels/api/*.yml,
        // lib/macos/software/*.yml. The nearest matching ancestor wins, so a
        // top-level `labels:` block in a fleet file isn't mistaken for one.
        for comp in parent.components().rev() {
            match comp.as_os_str().to_str() {
                Some("labels") => return FileType::Labels,
                Some("software") => return FileType::Software,
                Some("policies") => return FileType::Policies,
                Some("queries") | Some("reports") => return FileType::Queries,
                _ => {}
            }
        }
    }

    // File name patterns
    if file_name.contains(".labels.") {
        return FileType::Labels;
    }

    // Everything else is a fleet config (default.yml, fleets/*.yml, teams/*.yml, etc.)
    FileType::FleetConfig
}

/// Basic YAML hygiene checks that run before parsing.
///
/// These catch issues that serde_yaml would either silently accept or report
/// as opaque parse errors. Running them first gives clear, actionable diagnostics.
fn check_yaml_hygiene(content: &str, file: &Path, report: &mut LintReport) {
    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;

        // Tab indentation — YAML spec allows tabs but they cause subtle bugs
        if line.starts_with('\t') || (line.starts_with(' ') && line.contains('\t')) {
            let col = line.find('\t').unwrap_or(0) + 1;
            report.add(
                LintError::warning(
                    "Tab character found — use spaces for YAML indentation",
                    file,
                )
                .with_location(line_num, col)
                .with_rule_code(crate::codes::YAML_TABS)
                .with_help("YAML indentation must use spaces, not tabs"),
            );
        }

        // Trailing whitespace
        if line.len() > 1 && line != line.trim_end() && !line.trim().is_empty() {
            report.add(
                LintError::info("Trailing whitespace", file)
                    .with_location(line_num, line.trim_end().len() + 1)
                    .with_rule_code(crate::codes::YAML_TRAILING_WHITESPACE),
            );
        }
    }

    // Duplicate top-level keys (YAML spec says last wins, but it's almost always a mistake)
    let mut seen_keys: HashMap<String, usize> = HashMap::new();
    for (idx, line) in content.lines().enumerate() {
        let line_num = idx + 1;
        let trimmed = line.trim();

        // Only check top-level keys (no leading whitespace, not a comment, not a list item)
        if !line.starts_with(' ')
            && !line.starts_with('\t')
            && !trimmed.starts_with('#')
            && !trimmed.starts_with('-')
            && !trimmed.is_empty()
        {
            if let Some(key) = trimmed.split(':').next() {
                let key = key.trim().to_string();
                if !key.is_empty() {
                    if let Some(prev_line) = seen_keys.get(&key) {
                        report.add(
                            LintError::error(
                                format!("Duplicate top-level key '{}' (first seen at line {})", key, prev_line),
                                file,
                            )
                            .with_location(line_num, 1)
                            .with_rule_code(crate::codes::YAML_DUPLICATE_KEY)
                            .with_help("YAML uses the last occurrence of duplicate keys — the first one is silently ignored")
                        );
                    } else {
                        seen_keys.insert(key, line_num);
                    }
                }
            }
        }
    }
}

/// Find YAML files in directory
fn find_yaml_files(dir: &Path, _pattern: &str) -> Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();

    // Simple recursive search for YAML files
    fn visit_dirs(dir: &Path, files: &mut Vec<std::path::PathBuf>) -> Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    // Skip hidden directories and common ignores
                    if let Some(name) = path.file_name() {
                        let name_str = name.to_string_lossy();
                        if name_str.starts_with('.')
                            || name_str == "node_modules"
                            || name_str == "target"
                            || name_str == "dist"
                        {
                            continue;
                        }
                    }
                    visit_dirs(&path, files)?;
                } else if let Some(ext) = path.extension() {
                    if ext == "yml" || ext == "yaml" {
                        // Skip CI config files and other non-Fleet YAML
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if name.starts_with('.')
                                || name == "docker-compose.yml"
                                || name == "docker-compose.yaml"
                                || name == "action.yml"
                                || name == "action.yaml"
                            {
                                continue;
                            }
                        }
                        files.push(path);
                    }
                }
            }
        }
        Ok(())
    }

    visit_dirs(dir, &mut files)?;
    Ok(files)
}

// ============================================================================
// Inline Suppression Support
// ============================================================================

/// Fill `doc_url` from the code registry for any error that has a rule code
/// but no documentation link yet. The single stamping point — rules never
/// carry URL knowledge themselves.
fn stamp_doc_url(error: &mut LintError) {
    if error.doc_url.is_none() {
        if let Some(code) = error.rule_code {
            error.doc_url = super::codes::doc_url(code);
        }
    }
}

/// Parse inline suppression comments from YAML source.
///
/// Supports two forms:
/// - `# fleet-lint: ignore` — suppress all rules on this line
/// - `# fleet-lint: ignore rule-code` — suppress a specific rule
/// - `# fleet-lint: ignore rule-a, rule-b` — suppress multiple rules
///
/// Returns a map of 1-indexed line numbers to suppressed rule codes.
/// An empty Vec means "suppress all rules on this line".
/// The one error-control pass shared by per-file linting and the cross-file
/// pass: drop suppressed findings, then downgrade errors whose code is listed
/// in `[rules] warn`. (Suppression and downgrade are independent — order
/// doesn't matter.)
fn apply_error_controls(
    errors: &mut Vec<LintError>,
    suppressions: &HashMap<usize, Vec<String>>,
    warning_rules: &std::collections::HashSet<&str>,
) {
    if !suppressions.is_empty() {
        errors.retain(|e| !is_suppressed(e, suppressions));
    }
    for e in errors.iter_mut() {
        if e.severity == Severity::Error && warning_rules.contains(e.rule_code.unwrap_or_default())
        {
            e.severity = Severity::Warning;
        }
    }
}

fn parse_suppressions(source: &str) -> HashMap<usize, Vec<String>> {
    let mut suppressions = HashMap::new();

    for (idx, line) in source.lines().enumerate() {
        let line_num = idx + 1; // 1-indexed to match LintError.line

        if let Some(comment_start) = line.find('#') {
            let comment = line[comment_start + 1..].trim();

            if let Some(rest) = comment.strip_prefix("fleet-lint:") {
                let rest = rest.trim();
                if let Some(codes) = rest.strip_prefix("ignore") {
                    let codes = codes.trim();
                    if codes.is_empty() {
                        // Ignore all rules
                        suppressions.insert(line_num, Vec::new());
                    } else {
                        // Ignore specific rule(s)
                        let rule_codes: Vec<String> = codes
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        suppressions.insert(line_num, rule_codes);
                    }
                }
            }
        }
    }

    suppressions
}

/// Check if a lint error is suppressed by an inline comment.
///
/// An error is suppressed if:
/// - Its line has a same-line suppression comment matching the rule code
/// - The line immediately before it has a standalone suppression comment matching the rule code
fn is_suppressed(error: &LintError, suppressions: &HashMap<usize, Vec<String>>) -> bool {
    let line = match error.line() {
        Some(l) => l,
        None => return false, // Can't suppress errors without line info
    };

    // Check same-line suppression
    if let Some(codes) = suppressions.get(&line) {
        if matches_suppression(error, codes) {
            return true;
        }
    }

    // Check previous-line suppression
    if line > 1 {
        if let Some(codes) = suppressions.get(&(line - 1)) {
            if matches_suppression(error, codes) {
                return true;
            }
        }
    }

    false
}

/// Check if an error matches a suppression rule list.
/// Empty list means "suppress all". Otherwise, the error's rule_code must be in the list.
fn matches_suppression(error: &LintError, codes: &[String]) -> bool {
    if codes.is_empty() {
        return true; // Suppress all rules
    }
    if let Some(rule_code) = &error.rule_code {
        codes.iter().any(|c| c == rule_code)
    } else {
        false
    }
}

#[cfg(test)]
mod suppression_tests {
    use super::*;

    #[test]
    fn test_parse_suppression_ignore_all() {
        let source = "platform: macos  # fleet-lint: ignore\n";
        let s = parse_suppressions(source);
        assert_eq!(s.len(), 1);
        assert!(s.get(&1).unwrap().is_empty()); // empty = all rules
    }

    #[test]
    fn test_parse_suppression_specific_rule() {
        let source = "platform: macos  # fleet-lint: ignore type-validation\n";
        let s = parse_suppressions(source);
        assert_eq!(s.get(&1).unwrap(), &vec!["type-validation".to_string()]);
    }

    #[test]
    fn test_parse_suppression_multiple_rules() {
        let source = "query: bad  # fleet-lint: ignore query-syntax, type-validation\n";
        let s = parse_suppressions(source);
        let codes = s.get(&1).unwrap();
        assert_eq!(codes.len(), 2);
        assert!(codes.contains(&"query-syntax".to_string()));
        assert!(codes.contains(&"type-validation".to_string()));
    }

    #[test]
    fn test_parse_suppression_standalone_line() {
        let source = "# fleet-lint: ignore type-validation\nplatform: macos\n";
        let s = parse_suppressions(source);
        assert!(s.contains_key(&1)); // suppression on line 1
        assert!(!s.contains_key(&2)); // no suppression on line 2
    }

    #[test]
    fn test_is_suppressed_same_line() {
        let mut suppressions = HashMap::new();
        suppressions.insert(5, vec!["type-validation".to_string()]);

        let error = LintError::error("test", "test.yml")
            .with_location(5, 1)
            .with_rule_code(crate::codes::TYPE_VALIDATION);
        assert!(is_suppressed(&error, &suppressions));

        let error2 = LintError::error("test", "test.yml")
            .with_location(5, 1)
            .with_rule_code("other-rule");
        assert!(!is_suppressed(&error2, &suppressions));
    }

    #[test]
    fn test_is_suppressed_previous_line() {
        let mut suppressions = HashMap::new();
        suppressions.insert(4, vec!["type-validation".to_string()]);

        let error = LintError::error("test", "test.yml")
            .with_location(5, 1)
            .with_rule_code(crate::codes::TYPE_VALIDATION);
        assert!(is_suppressed(&error, &suppressions));
    }

    #[test]
    fn test_is_suppressed_all_rules() {
        let mut suppressions = HashMap::new();
        suppressions.insert(5, Vec::new()); // empty = all rules

        let error = LintError::error("test", "test.yml")
            .with_location(5, 1)
            .with_rule_code("any-rule");
        assert!(is_suppressed(&error, &suppressions));
    }

    #[test]
    fn test_no_suppression_without_line() {
        let mut suppressions = HashMap::new();
        suppressions.insert(5, Vec::new());

        let error = LintError::error("test", "test.yml"); // no line info
        assert!(!is_suppressed(&error, &suppressions));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_lint_valid_config() {
        let yaml = r#"
policies:
  - name: "Test Policy"
    query: "SELECT 1 FROM users;"
    platform: darwin
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(file.path()).unwrap();

        assert!(!report.has_errors());
    }

    #[test]
    fn test_lint_missing_required_field() {
        let yaml = r#"
policies:
  - name: "Test Policy"
    # Missing query field
    platform: darwin
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(file.path()).unwrap();

        assert!(report.has_errors());
        assert!(report
            .errors
            .iter()
            .any(|e| e.message.contains("missing required field 'query'")));
    }

    #[test]
    fn test_lint_invalid_platform() {
        let yaml = r#"
policies:
  - name: "Test Policy"
    query: "SELECT 1;"
    platform: macos  # Should be 'darwin'
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(file.path()).unwrap();

        assert!(report.has_errors());
        assert!(report
            .errors
            .iter()
            .any(|e| e.message.contains("invalid platform")));
    }

    #[test]
    fn test_platform_compatibility() {
        let yaml = r#"
policies:
  - name: "Windows Firewall"
    query: "SELECT * FROM alf;"  # alf is macOS-only
    platform: windows
"#;

        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(file.path()).unwrap();

        assert!(report.has_errors());
        assert!(report
            .errors
            .iter()
            .any(|e| e.message.contains("not available on platform")));
    }

    // ── File type detection tests ────────────────────────────────

    #[test]
    fn test_detect_file_type_fleet_config() {
        assert_eq!(
            detect_file_type(Path::new("default.yml")),
            FileType::FleetConfig
        );
        assert_eq!(
            detect_file_type(Path::new("fleets/engineering.yml")),
            FileType::FleetConfig
        );
        assert_eq!(
            detect_file_type(Path::new("teams/ops.yml")),
            FileType::FleetConfig
        );
    }

    #[test]
    fn test_detect_file_type_labels() {
        assert_eq!(
            detect_file_type(Path::new("labels/macos.yml")),
            FileType::Labels
        );
        assert_eq!(
            detect_file_type(Path::new("lib/all/labels/hosts.yml")),
            FileType::Labels
        );
        assert_eq!(
            detect_file_type(Path::new("my.labels.yml")),
            FileType::Labels
        );
        // Nested under labels/ at any depth (Fleet repos organize labels into
        // labels/dynamic/, labels/api/, …) — must still classify as Labels.
        assert_eq!(
            detect_file_type(Path::new("labels/dynamic/macos-27-hosts.yml")),
            FileType::Labels
        );
        assert_eq!(
            detect_file_type(Path::new("labels/api/debug-profiles.yml")),
            FileType::Labels
        );
    }

    #[test]
    fn test_detect_file_type_nested_content_dirs() {
        assert_eq!(
            detect_file_type(Path::new("software/macos/L1/slack.yml")),
            FileType::Software
        );
        assert_eq!(
            detect_file_type(Path::new("policies/autoinstalls/gp.yml")),
            FileType::Policies
        );
        // A fleet config is unaffected — no content-dir ancestor.
        assert_eq!(
            detect_file_type(Path::new("fleets/FDN-ALPHA.yml")),
            FileType::FleetConfig
        );
    }

    #[test]
    fn test_detect_file_type_software() {
        assert_eq!(
            detect_file_type(Path::new("lib/macos/software/slack.yml")),
            FileType::Software
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/macos/software/chrome.yml")),
            FileType::Software
        );
    }

    #[test]
    fn test_detect_file_type_policies() {
        assert_eq!(
            detect_file_type(Path::new("lib/macos/policies/filevault.yml")),
            FileType::Policies
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/macos/policies/security.yml")),
            FileType::Policies
        );
    }

    #[test]
    fn test_detect_file_type_queries() {
        assert_eq!(
            detect_file_type(Path::new("lib/macos/queries/uptime.yml")),
            FileType::Queries
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/all/reports/compliance.yml")),
            FileType::Queries
        );
    }

    #[test]
    fn test_detect_file_type_agent_options() {
        assert_eq!(
            detect_file_type(Path::new("lib/agent-options.yml")),
            FileType::AgentOptions
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/all/agent-options.yml")),
            FileType::AgentOptions
        );
    }

    #[test]
    fn test_detect_file_type_non_yaml() {
        assert_eq!(
            detect_file_type(Path::new(
                "platforms/macos/configuration-profiles/wifi.mobileconfig"
            )),
            FileType::NonYaml
        );
        assert_eq!(
            detect_file_type(Path::new(
                "platforms/macos/declaration-profiles/activation.json"
            )),
            FileType::NonYaml
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/macos/scripts/setup.sh")),
            FileType::NonYaml
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/macos/commands/restart.plist")),
            FileType::NonYaml
        );
        assert_eq!(
            detect_file_type(Path::new("platforms/all/icons/slack.png")),
            FileType::NonYaml
        );
    }

    // ── Label linting tests ─────────────────────────────────────

    #[test]
    fn test_label_dynamic_with_query_no_error() {
        let yaml = r#"
- name: macOS Hosts
  description: All macOS hosts
  platform: darwin
  label_membership_type: dynamic
  query: "SELECT 1 FROM os_version WHERE name = 'macOS';"
"#;
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(yaml.as_bytes()).unwrap();
        // Rename to labels dir to trigger label parsing
        let label_dir = tempfile::tempdir().unwrap();
        let label_path = label_dir.path().join("labels");
        std::fs::create_dir_all(&label_path).unwrap();
        let label_file = label_path.join("macos.yml");
        std::fs::write(&label_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&label_file).unwrap();
        assert!(
            !report.has_errors(),
            "Dynamic label with query should have no errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_label_manual_no_error() {
        let yaml = r#"
- name: VIP Hosts
  description: Manually managed VIP hosts
  label_membership_type: manual
  hosts:
    - host1.example.com
    - host2.example.com
"#;
        let label_dir = tempfile::tempdir().unwrap();
        let label_path = label_dir.path().join("labels");
        std::fs::create_dir_all(&label_path).unwrap();
        let label_file = label_path.join("vip.yml");
        std::fs::write(&label_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&label_file).unwrap();
        assert!(
            !report.has_errors(),
            "Manual label should have no errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_label_host_vitals_no_error() {
        // As of the Fleet version this code tracks, parseHostVitalCriteria only
        // registers `end_user_idp_group` and `end_user_idp_department`, and
        // rejects and/or composites outright. Keep this test aligned with what
        // Fleet actually parses today.
        let yaml = r#"
- name: Engineering
  description: Hosts assigned to the Engineering IdP group
  label_membership_type: host_vitals
  criteria:
    vital: end_user_idp_department
    value: Engineering
"#;
        let label_dir = tempfile::tempdir().unwrap();
        let label_path = label_dir.path().join("labels");
        std::fs::create_dir_all(&label_path).unwrap();
        let label_file = label_path.join("sequoia.yml");
        std::fs::write(&label_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&label_file).unwrap();
        assert!(
            !report.has_errors(),
            "host_vitals label should have no errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_label_criteria_no_error() {
        let yaml = r#"
- name: Engineering IdP group
  description: Hosts whose end user is in the Engineering IdP group
  label_membership_type: host_vitals
  criteria:
    vital: end_user_idp_group
    value: Engineering
"#;
        let label_dir = tempfile::tempdir().unwrap();
        let label_path = label_dir.path().join("labels");
        std::fs::create_dir_all(&label_path).unwrap();
        let label_file = label_path.join("macos15.yml");
        std::fs::write(&label_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&label_file).unwrap();
        assert!(
            !report.has_errors(),
            "criteria label should have no errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_software_file_skips_rules() {
        let yaml = r#"
hash_sha256: abc123def456
"#;
        let sw_dir = tempfile::tempdir().unwrap();
        let sw_path = sw_dir.path().join("software");
        std::fs::create_dir_all(&sw_path).unwrap();
        let sw_file = sw_path.join("slack.yml");
        std::fs::write(&sw_file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&sw_file).unwrap();
        // Software files skip structural rules — only hygiene checks
        assert!(
            !report.has_errors(),
            "Software file should skip structural validation: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_agent_options_file_skips_rules() {
        let yaml = r#"
config:
  decorators:
    load:
      - SELECT uuid AS host_uuid FROM system_info;
"#;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("agent-options.yml");
        std::fs::write(&file, yaml).unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&file).unwrap();
        assert!(
            !report.has_errors(),
            "Agent options file should skip structural validation: {:?}",
            report.errors
        );
    }

    // -- Issue #5: .fleetlint.toml is loaded by Linter::from_path --

    #[test]
    fn test_from_path_loads_fleetlint_toml() {
        // Without config a missing-query policy yields a `required-fields`
        // error. With `[rules.required-fields] enabled = false` the rule
        // should be silenced. Regression for issue #5.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".fleetlint.toml"),
            "[rules]\ndisabled = [\"required-fields\"]\n",
        )
        .unwrap();
        let yaml_file = dir.path().join("default.yml");
        std::fs::write(&yaml_file, "policies:\n  - name: missing query\n").unwrap();

        let linter = Linter::from_path(&yaml_file);
        let report = linter.lint_file(&yaml_file).unwrap();
        let has_required_fields_error = report
            .errors
            .iter()
            .any(|e| e.rule_code == Some("required-fields"));
        assert!(
            !has_required_fields_error,
            "required-fields rule should be disabled by .fleetlint.toml; got errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn test_out_of_scope_covers_non_yaml_under_both_scoping_styles() {
        use super::super::config::FilesConfig;

        // Workspace rules (orphaned-file, duplicate-content, case-collision)
        // report on scripts and profiles, which `should_lint` rejects outright
        // because it defaults to YAML-only. Scoping those findings therefore
        // needs `is_out_of_scope`, and it has to honor BOTH styles: a denylist
        // repo writes `exclude`, an allowlist repo writes `include`.
        let script = Path::new("tools-scripts/runscripts/scripts/install-rosetta.sh");
        let kept = Path::new("platforms/macos/L1/scripts/real.sh");

        // Denylist style.
        let mut cfg = FleetLintConfig::default();
        cfg.files = FilesConfig {
            include: vec![],
            exclude: vec!["tools-scripts/**".to_string()],
            ..Default::default()
        };
        let linter = Linter::with_config(cfg);
        assert!(linter.is_out_of_scope(script), "exclude must scope a .sh");
        assert!(!linter.is_out_of_scope(kept));
        // The YAML-only default is exactly why should_lint is the wrong test.
        assert!(!linter.should_lint(kept), "should_lint is false for any .sh");

        // Allowlist style — the same script is out of scope by omission.
        let mut cfg = FleetLintConfig::default();
        cfg.files = FilesConfig {
            include: vec![
                "default.yml".to_string(),
                "fleets/**".to_string(),
                "platforms/**".to_string(),
            ],
            exclude: vec!["platforms/_retired/**".to_string()],
            ..Default::default()
        };
        let linter = Linter::with_config(cfg);
        assert!(
            linter.is_out_of_scope(script),
            "a non-empty include is authoritative — omission means out of scope"
        );
        assert!(!linter.is_out_of_scope(kept));
        // exclude still wins over include.
        assert!(linter.is_out_of_scope(Path::new("platforms/_retired/old.mobileconfig")));
    }

    #[test]
    fn test_should_lint_resolves_absolute_paths_against_config_root() {
        // Issue #15 (LSP): the editor hands the linter ABSOLUTE paths, but
        // narrowing [files].include globs are written repo-relative. Without
        // config_root the absolute path matches no include and every file is
        // silently skipped.
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("fleets")).unwrap();
        std::fs::create_dir_all(dir.path().join("vendor")).unwrap();
        let in_scope = dir.path().join("fleets/prod.yml");
        let out_of_scope = dir.path().join("vendor/c.yml");
        std::fs::write(&in_scope, "name: prod\n").unwrap();
        std::fs::write(&out_of_scope, "name: vendor\n").unwrap();

        let mut config = FleetLintConfig::default();
        config.files.include = vec!["fleets/*.yml".to_string()];
        let mut linter = Linter::with_config(config);

        // Without a config root, the absolute path matches nothing.
        assert!(!linter.should_lint(&in_scope));

        linter.set_config_root(dir.path().to_path_buf());
        assert!(linter.should_lint(&in_scope));
        assert!(!linter.should_lint(&out_of_scope));

        // Relative paths keep working unchanged.
        assert!(linter.should_lint(Path::new("fleets/prod.yml")));
    }

    #[test]
    fn test_from_path_sets_config_root() {
        // from_path discovers .fleetlint.toml and must anchor [files] globs
        // to its directory, so absolute paths from any caller stay in scope.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".fleetlint.toml"),
            "[files]\ninclude = [\"fleets/*.yml\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("fleets")).unwrap();
        let in_scope = dir.path().join("fleets/prod.yml");
        std::fs::write(&in_scope, "name: prod\n").unwrap();

        let linter = Linter::from_path(dir.path());
        assert!(linter.should_lint(&in_scope));
        assert!(!linter.should_lint(&dir.path().join("other.yml")));
    }

    #[test]
    fn test_new_does_not_load_config() {
        // Documents the historical behavior of Linter::new(): it does NOT
        // load .fleetlint.toml. CLI now uses from_path() (issue #5 fix);
        // keep this test so any future "make new() load config" change is
        // a deliberate decision.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".fleetlint.toml"),
            "[rules]\ndisabled = [\"required-fields\"]\n",
        )
        .unwrap();
        let yaml_file = dir.path().join("default.yml");
        std::fs::write(&yaml_file, "policies:\n  - name: missing query\n").unwrap();

        let linter = Linter::new();
        let report = linter.lint_file(&yaml_file).unwrap();
        let has_required_fields_error = report
            .errors
            .iter()
            .any(|e| e.rule_code == Some("required-fields"));
        assert!(
            has_required_fields_error,
            "Linter::new() ignores .fleetlint.toml — required-fields should still fire"
        );
    }
}
