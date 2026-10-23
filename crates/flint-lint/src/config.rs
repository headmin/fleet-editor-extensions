//! Configuration file support for Fleet linter.
//!
//! Supports `.fleetlint.toml` configuration files that allow teams
//! to customize linting behavior and share settings via version control.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Config file name written by `flint init`, and the preferred spelling.
///
/// VISIBLE by default. The hidden form is still read (see [`CONFIG_FILE_NAMES`])
/// so existing repos keep working, but a new file is visible because this one
/// is load-bearing and easy to lose: in one real session a repo's
/// `.fleetlint.toml` was untracked while silently suppressing 18 errors, and
/// it was left out of a commit that included the tooling it configured.
/// Modern linters have made the same move (ruff.toml, biome.json).
pub(crate) const CONFIG_FILE_NAME: &str = "fleetlint.toml";

/// Every accepted config filename, in discovery order.
///
/// Visible first so it wins when both exist — a repo that has added the
/// visible form has opted into it, and silently preferring the hidden one
/// would make the migration look broken.
pub(crate) const CONFIG_FILE_NAMES: &[&str] = &["fleetlint.toml", ".fleetlint.toml"];

/// Resolve the user-level config path from the process environment.
///
/// Search order: `$XDG_CONFIG_HOME/flint/config.toml`, then
/// `$HOME/.config/flint/config.toml`. Returns `None` if neither env var
/// is set (minimal CI containers, daemonized contexts).
pub(crate) fn user_config_path() -> Option<PathBuf> {
    resolve_user_config_path(
        std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
        std::env::var("HOME").ok().as_deref(),
    )
}

/// Pure resolver — testable without env mutation. Production callers go
/// through [`user_config_path`]; tests construct env values inline.
///
/// An empty `XDG_CONFIG_HOME` is treated as unset (matches XDG basedir
/// spec: "if XDG_CONFIG_HOME is either not set or empty, a default of
/// $HOME/.config should be used").
fn resolve_user_config_path(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = xdg
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("flint").join("config.toml"))
}

/// Fleet linter configuration loaded from `.fleetlint.toml`.
#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FleetLintConfig {
    /// Rule configuration.
    pub rules: RulesConfig,

    /// Validation thresholds.
    pub thresholds: ThresholdsConfig,

    /// File patterns to include/exclude.
    pub files: FilesConfig,

    /// Schema validation options.
    pub schema: SchemaConfig,

    /// Deprecation settings.
    pub deprecations: DeprecationsConfig,

    /// Fleet server connection settings.
    pub fleet: FleetConnectionConfig,

    /// LSP-specific settings (debounce, etc).
    pub lsp: LspConfig,

    /// Values a repo deliberately leaves unresolved.
    ///
    /// Distinct from Fleet's own `$VAR` interpolation, which flint recognizes
    /// unconditionally because it is Fleet SEMANTICS — the server resolves it,
    /// so flagging it would be wrong in every repo. This list is repo
    /// CONVENTION: scaffolding markers a team agrees to write, which flint
    /// cannot guess.
    pub placeholders: PlaceholdersConfig,

    /// Files the repo keeps on purpose without wiring them into a fleet.
    #[serde(default)]
    pub orphans: OrphansConfig,

    /// Declarative repo-convention patterns (ADR-010 Phase 2). Top-level
    /// `[[patterns]]` array.
    pub patterns: Vec<PatternConfig>,

    /// Base config this one overlays (path relative to this file). Tables
    /// merge key by key; any other value, arrays included, is replaced.
    pub extend: Option<String>,

    /// Per-path rule levels, applied over `[rules]` in order (later wins).
    pub overrides: Vec<OverrideConfig>,

    /// Compiled include/exclude glob sets (built on first use; skipped by
    /// serde and reset on clone — see the manual `Clone` impl).
    #[serde(skip)]
    compiled: once_cell::sync::OnceCell<CompiledGlobs>,

    /// Directory the config was loaded from, i.e. the repo root the
    /// `[files]` globs are written against.
    ///
    /// `include = ["fleets/**"]` is relative to the config file, but callers
    /// hand us whatever path the user typed — and an editor or CI job hands
    /// us an ABSOLUTE one. Without a base to strip, an absolute path matches
    /// no relative glob, the file is silently judged out of scope, and
    /// `flint check /abs/path/to/fleets/x.yml` reports nothing for a file
    /// that is plainly in scope. Set on every load-from-disk path; `None`
    /// for a config parsed from a string, where there is nothing to be
    /// relative to.
    #[serde(skip)]
    base_dir: Option<PathBuf>,
}

/// Clone resets the compiled-glob cache: clones are routinely mutated
/// (e.g. the CLI merges `--exclude` into a cloned config), and a carried
/// cache would silently keep matching against the OLD pattern lists.
impl Clone for FleetLintConfig {
    fn clone(&self) -> Self {
        Self {
            rules: self.rules.clone(),
            thresholds: self.thresholds.clone(),
            files: self.files.clone(),
            schema: self.schema.clone(),
            deprecations: self.deprecations.clone(),
            fleet: self.fleet.clone(),
            lsp: self.lsp.clone(),
            patterns: self.patterns.clone(),
            placeholders: self.placeholders.clone(),
            orphans: self.orphans.clone(),
            extend: self.extend.clone(),
            overrides: self.overrides.clone(),
            compiled: once_cell::sync::OnceCell::new(),
            // Carried, unlike `compiled`: the base directory describes where
            // the globs are anchored, and a clone that forgot it would stop
            // matching absolute paths.
            base_dir: self.base_dir.clone(),
        }
    }
}

/// Artifacts deliberately kept unreferenced.
///
/// `orphaned-file` is right to report an unwired payload — it is never applied,
/// so it is either a mistake or dead weight. But some are neither: a reference
/// copy, a template, a profile parked before its rollout. Without a way to say
/// so, those are reported for the life of the repo and teach people to skim
/// past the rule.
///
/// Declaring one is deliberately more work than deleting it: an entry here is
/// a claim, in review, that the file is kept on purpose.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OrphansConfig {
    /// Globs, relative to the repo root, matched against the artifact's path.
    /// A match silences `orphaned-file` for that artifact and nothing else —
    /// unlike `[files] exclude`, which would take the file out of scope for
    /// every rule, including the ones that check its contents.
    pub allow: Vec<String>,
}

impl OrphansConfig {
    /// Whether `rel` (repo-root-relative, POSIX separators) is declared kept.
    pub fn allows(&self, rel: &str) -> bool {
        self.allow
            .iter()
            .any(|g| super::config::compile_glob(g).is_some_and(|m| m.is_match(rel)))
    }
}

/// Repo-defined placeholder markers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaceholdersConfig {
    /// Globs matched against a value that would otherwise be reported as
    /// unresolved. A match downgrades the finding to a warning and says the
    /// value is intentionally parked — it NEVER gates.
    pub patterns: Vec<String>,
}

impl Default for PlaceholdersConfig {
    fn default() -> Self {
        // Conservative defaults: the shapes `flint gen` scaffolds emit and the
        // near-universal TODO spelling. Anything else a team invents goes in
        // their own config — guessing more would silently excuse typos.
        Self {
            patterns: vec![
                "PLACEHOLDER*".to_string(),
                "TODO*".to_string(),
                "<*>".to_string(),
            ],
        }
    }
}

impl PlaceholdersConfig {
    /// Whether `value` is a declared placeholder.
    pub fn is_placeholder(&self, value: &str) -> bool {
        let v = value.trim();
        self.patterns.iter().any(|p| {
            super::config::compile_glob(p).is_some_and(|m| m.is_match(v))
        })
    }
}

/// Whether a value is Fleet's own interpolation and must NOT be judged.
///
/// `$VAR`, `${VAR}`, `$FLEET_SECRET_*` and `$FLEET_VAR_*` are resolved by
/// Fleet (ExpandEnvBytes) or at apply time on the server. Their literal text
/// is never what reaches Fleet, so flint has nothing to check and must stay
/// silent — reporting one would be a false positive in every repo, which is
/// why this is built in rather than configurable.
pub(crate) fn is_fleet_variable(value: &str) -> bool {
    let v = value.trim();
    v.starts_with('$') || v.contains("${")
}

/// LSP-specific settings.
///
/// One declarative repo-convention pattern (ADR-010 Phase 2): a file glob,
/// an assertion, and a REQUIRED `why` — an unjustified pattern is a guess,
/// and guesses become the noise that gets linters disabled. Findings carry
/// code `pattern/<assert>` and participate in `[rules]` disable/downgrade
/// like built-ins.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct PatternConfig {
    /// Glob selecting the files (for `required-structure`: directories)
    /// this pattern applies to, relative to the repo root.
    pub files: String,
    /// Assertion kind: `name-matches-filename`, `filename`,
    /// `content-must-match`, `content-must-not-match`, `token-consistency`,
    /// `must-be-referenced`, `unique-content-within`, `required-structure`,
    /// `forbid-file`.
    pub assert: String,
    /// `error`, `warn`, or `info`. Default: `warn`.
    pub severity: String,
    /// REQUIRED: the reason this convention exists — ideally citing the
    /// commit that taught it. Shown with every finding.
    pub why: String,
    /// `name-matches-filename`: the YAML key to compare (default `name`).
    pub key: String,
    /// `filename` / `content-must[-not]-match`: the regex.
    pub regex: String,
    /// `token-consistency`: 0-based path segment holding the token that
    /// must appear in the filename.
    pub segment: Option<usize>,
    /// `must-be-referenced`: glob of the config files that must reference
    /// the matched files.
    pub by: String,
    /// `must-be-referenced`: `any` (default — at least one) or `all`.
    pub quantifier: String,
    /// `required-structure`: entries each matched directory must contain.
    pub entries: Vec<String>,
}

/// The assertion kinds `PatternConfig.assert` accepts.
pub(crate) const PATTERN_ASSERTS: &[&str] = &[
    "name-matches-filename",
    "filename",
    "content-must-match",
    "content-must-not-match",
    "token-consistency",
    "must-be-referenced",
    "unique-content-within",
    "required-structure",
    "forbid-file",
];

impl PatternConfig {
    /// Reject malformed patterns at load time — loud and immediate beats
    /// silently checking nothing.
    pub fn validate(&self) -> Result<(), String> {
        if !PATTERN_ASSERTS.contains(&self.assert.as_str()) {
            return Err(format!(
                "unknown assert '{}' (expected one of: {})",
                self.assert,
                PATTERN_ASSERTS.join(", ")
            ));
        }
        if self.why.trim().is_empty() {
            return Err(format!(
                "assert '{}' has no `why` — every pattern must say what taught it",
                self.assert
            ));
        }
        if self.files.trim().is_empty() {
            return Err(format!("assert '{}' has no `files` glob", self.assert));
        }
        match self.assert.as_str() {
            "filename" | "content-must-match" | "content-must-not-match" => {
                if self.regex.is_empty() {
                    return Err(format!("assert '{}' requires `regex`", self.assert));
                }
                regex::Regex::new(&self.regex)
                    .map_err(|e| format!("assert '{}': invalid regex: {e}", self.assert))?;
            }
            "token-consistency" if self.segment.is_none() => {
                return Err("assert 'token-consistency' requires `segment`".into());
            }
            "must-be-referenced" if self.by.is_empty() && self.quantifier == "all" => {
                return Err("assert 'must-be-referenced' with quantifier 'all' requires `by`".into());
            }
            "required-structure" if self.entries.is_empty() => {
                return Err("assert 'required-structure' requires `entries`".into());
            }
            _ => {}
        }
        if !self.severity.is_empty() && !["error", "warn", "info"].contains(&self.severity.as_str())
        {
            return Err(format!(
                "assert '{}': severity must be error|warn|info, got '{}'",
                self.assert, self.severity
            ));
        }
        Ok(())
    }
}

/// Controls behavior that's only meaningful in the language-server context
/// (not the CLI). Today this is just the keystroke debounce window, but the
/// `[lsp]` section is the natural home for any future LSP-only knobs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LspConfig {
    /// Milliseconds to wait after the last keystroke before linting.
    /// Default 150ms (matches rust-analyzer's typing-debounce default —
    /// imperceptible while typing, fast enough to feel responsive on pause).
    /// Set to 0 to disable debouncing entirely (every keystroke re-lints).
    pub lint_debounce_ms: u32,
}

impl Default for LspConfig {
    fn default() -> Self {
        // 150ms is the well-known sweet spot for typing-debounce. Lower
        // values waste CPU on doomed lint runs; higher values are
        // noticeable as input lag.
        Self {
            lint_debounce_ms: 150,
        }
    }
}

/// Rule enable/disable configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct RulesConfig {
    /// Rules to disable (by name).
    /// Example: `disabled = ["query-syntax", "interval-validation"]`
    #[serde(default)]
    pub disabled: Vec<String>,

    /// Same as `disabled` (ruff/ty spelling).
    #[serde(default)]
    pub ignore: Vec<String>,

    /// Rules to set as warnings instead of errors.
    /// Example: `warn = ["duplicate-names"]`
    #[serde(default)]
    pub warn: Vec<String>,

    /// Rules whose warnings and infos become errors.
    /// Example: `error = ["orphaned-file"]`
    #[serde(default)]
    pub error: Vec<String>,
}

/// A rule's configured level. `None` from [`RulesConfig::level`] = the
/// severity the rule chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleLevel {
    Ignore,
    Warn,
    Error,
}

impl RulesConfig {
    /// This layer's level for `code`; ignore beats warn beats error.
    pub fn level(&self, code: &str) -> Option<RuleLevel> {
        let has = |list: &[String]| list.iter().any(|c| c == code);
        if has(&self.disabled) || has(&self.ignore) {
            Some(RuleLevel::Ignore)
        } else if has(&self.warn) {
            Some(RuleLevel::Warn)
        } else if has(&self.error) {
            Some(RuleLevel::Error)
        } else {
            None
        }
    }

    fn codes(&self) -> impl Iterator<Item = &String> {
        self.disabled.iter().chain(&self.ignore).chain(&self.warn).chain(&self.error)
    }
}

/// `[[overrides]]`: rule levels for the files matching `files` (globs
/// relative to the config file, like `[files]`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct OverrideConfig {
    pub files: Vec<String>,
    pub rules: RulesConfig,
}

/// Threshold configuration for various checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThresholdsConfig {
    /// Minimum allowed interval in seconds (default: 60).
    pub min_interval: i64,

    /// Maximum allowed interval in seconds (default: 86400).
    pub max_interval: i64,

    /// Maximum query length in characters (default: 10000).
    pub max_query_length: usize,

    /// Whether to warn on SELECT * queries (default: true).
    pub warn_select_star: bool,

}

impl Default for ThresholdsConfig {
    fn default() -> Self {
        Self {
            min_interval: 60,
            max_interval: 86400,
            warn_select_star: true,
            max_query_length: 10000,
        }
    }
}

/// File include/exclude patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesConfig {
    /// Glob patterns to include. Default: EMPTY, meaning "not narrowed".
    ///
    /// This must stay empty by default. A non-empty `include` is
    /// authoritative — see [`FleetLintConfig::is_out_of_scope_file`] — and it
    /// scopes the cross-file rules too. Those report on scripts, profiles and
    /// payloads, so defaulting to the extension globs `["**/*.yml",
    /// "**/*.yaml"]` silently put every non-YAML path out of scope and
    /// disabled orphaned-file, duplicate-content, case-collision and
    /// unregistered-script for every repo, whether or not it had a config.
    ///
    /// Empty is behavior-preserving for the YAML walk: `should_lint_file`
    /// already falls back to "YAML files only" when no `include` is set, so
    /// the extension defaults were doing no work there in the first place.
    #[serde(default)]
    pub include: Vec<String>,

    /// Glob patterns to exclude.
    /// Default: `["**/node_modules/**", "**/target/**", "**/.git/**"]`
    #[serde(default = "default_exclude_patterns")]
    pub exclude: Vec<String>,
}

impl Default for FilesConfig {
    fn default() -> Self {
        Self {
            // Empty on purpose — see the `include` field docs. A non-empty
            // default would act as an authoritative allowlist for the
            // cross-file rules and silently switch them off on non-YAML files.
            include: Vec::new(),
            exclude: default_exclude_patterns(),
        }
    }
}


fn default_exclude_patterns() -> Vec<String> {
    vec![
        "**/node_modules/**".to_string(),
        "**/target/**".to_string(),
        "**/.git/**".to_string(),
        "**/dist/**".to_string(),
    ]
}

/// Schema validation options.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SchemaConfig {
    /// Whether to allow unknown fields (default: false).
    /// Set to `true` to disable structural validation.
    pub allow_unknown_fields: bool,
}

/// Deprecation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DeprecationsConfig {
    /// Target Fleet version for deprecation phase calculation.
    /// Accepts a semver string (e.g. `"4.80.0"`) or `"latest"`.
    pub fleet_version: String,

    /// Opt in to future naming conventions before they become mandatory.
    /// When `true`, completions suggest new names (`reports`, `settings`, `fleets/`)
    /// and deprecation warnings fire on old names (`queries`, `team_settings`, `teams/`).
    pub future_names: bool,
}

impl Default for DeprecationsConfig {
    fn default() -> Self {
        Self {
            fleet_version: "latest".to_string(),
            future_names: false,
        }
    }
}

/// Fleet server connection configuration.
///
/// Credentials are resolved in order:
/// 1. Fields in `.fleetlint.toml` (`url`, `token`)
/// 2. Environment variables (`FLEET_URL`, `FLEET_API_TOKEN`)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct FleetConnectionConfig {
    /// Enable gitops dry-run validation on save (default: false).
    pub gitops_validation: bool,

    /// Enable live completions from Fleet instance (default: false).
    pub live_completions: bool,

    /// Fleet server URL. Falls back to `FLEET_URL` env var.
    #[serde(default)]
    pub url: String,

    /// Fleet API token. Falls back to `FLEET_API_TOKEN` env var.
    /// Avoid committing tokens — use env vars in shared repos.
    #[serde(default)]
    pub token: String,

    /// Path to fleetctl binary. Falls back to `fleetctl` on PATH.
    #[serde(default)]
    pub fleetctl_path: String,

    /// Extra environment variables to pass to fleetctl.
    /// Values support `op://` references for 1Password secrets.
    /// These are needed when gitops YAML references `$VAR` placeholders.
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,

    /// Days a `.fleet-snapshot.json` may gate before degrading to warnings.
    ///
    /// A snapshot is a claim about a moment. Past this age it stops being
    /// authoritative — a label deleted last week would otherwise produce a
    /// confident, wrong block — so findings fall back to warnings rather
    /// than to a different answer.
    ///
    /// `0` means "never gate": keeps a snapshot's silencing effect (it can
    /// still prove a label or hash EXISTS and suppress a finding) while
    /// never letting it block a commit. Raising it trades safety for
    /// strictness. Default: 30 (`snapshot::DEFAULT_MAX_AGE_DAYS`).
    #[serde(default)]
    pub snapshot_max_age_days: Option<u64>,

    /// Re-read server state into `.fleet-snapshot.json` before `flint dry-run`,
    /// as if `--refresh-snapshot` had been passed.
    ///
    /// A snapshot proves PRESENCE soundly; absence only ever means "absent at
    /// capture time". Upload a package and the next dry-run still blocks on it
    /// — with a snapshot the freshness check considers perfectly current, so
    /// `snapshot_max_age_days` cannot help. Only re-reading the server can.
    ///
    /// Off by default: dry-run is otherwise offline and deterministic, which
    /// is what makes it safe in CI. Turn it on in a repo whose authors are
    /// uploading installers as they work, and leave it off where the run must
    /// not depend on the network.
    #[serde(default)]
    pub refresh_snapshot: bool,
}

impl FleetConnectionConfig {
    /// Resolve Fleet URL: config field first, then env var.
    /// Supports `op://` references resolved via 1Password CLI.
    pub fn resolved_url(&self) -> Option<String> {
        if !self.url.is_empty() {
            return Some(resolve_secret(&self.url));
        }
        std::env::var("FLEET_URL").ok().filter(|s| !s.is_empty())
    }

    /// Resolve Fleet API token: config field first, then env var.
    /// Supports `op://` references resolved via 1Password CLI.
    pub fn resolved_token(&self) -> Option<String> {
        if !self.token.is_empty() {
            return Some(resolve_secret(&self.token));
        }
        std::env::var("FLEET_API_TOKEN")
            .ok()
            .filter(|s| !s.is_empty())
    }

    /// Resolve fleetctl binary path: config field first, then PATH lookup.
    pub fn resolved_fleetctl(&self) -> String {
        if !self.fleetctl_path.is_empty() {
            self.fleetctl_path.clone()
        } else {
            "fleetctl".to_string()
        }
    }

    /// Resolve all extra env vars (with `op://` support) into key-value pairs.
    pub fn resolved_env(&self) -> Vec<(String, String)> {
        self.env
            .iter()
            .map(|(k, v)| (k.clone(), resolve_secret(v)))
            .collect()
    }

    /// Check if any Fleet features are enabled and credentials are available.
    pub fn is_active(&self) -> bool {
        (self.gitops_validation || self.live_completions)
            && self.resolved_url().is_some()
            && self.resolved_token().is_some()
    }
}

/// Resolve a config value that may be a 1Password secret reference.
///
/// If the value starts with `op://`, runs `op read <ref>` to fetch the secret.
/// Otherwise returns the value as-is.
#[expect(
    clippy::print_stderr,
    reason = "reports a failed `op read` to the operator; should surface as an error the caller renders"
)]
fn resolve_secret(value: &str) -> String {
    if value.starts_with("op://") {
        match std::process::Command::new("op")
            .args(["read", value])
            .output()
        {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            }
            Ok(output) => {
                eprintln!(
                    "op read failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                );
                value.to_string()
            }
            Err(e) => {
                eprintln!("Failed to run `op`: {e} — is 1Password CLI installed?");
                value.to_string()
            }
        }
    } else {
        value.to_string()
    }
}

impl FleetLintConfig {
    /// Load configuration from a file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| ConfigError::ReadError(path.to_path_buf(), e.to_string()))?;

        let mut config = Self::from_table(load_table(path, &content, 0)?)?;
        // The `[files]` globs are written relative to this directory; remember
        // it so an absolute path can be matched against them.
        config.base_dir = path.parent().map(|p| p.to_path_buf());
        Ok(config)
    }

    /// Parse configuration from a TOML string.
    pub fn parse(content: &str) -> Result<Self, ConfigError> {
        let config: Self =
            toml::from_str(content).map_err(|e| ConfigError::ParseError(e.to_string()))?;
        config.validated()
    }

    fn from_table(table: toml::Table) -> Result<Self, ConfigError> {
        let config: Self = table.try_into().map_err(|e: toml::de::Error| ConfigError::ParseError(e.to_string()))?;
        config.validated()
    }

    fn validated(self) -> Result<Self, ConfigError> {
        let config = self;
        for p in &config.patterns {
            p.validate()
                .map_err(|e| ConfigError::ParseError(format!("[[patterns]]: {e}")))?;
        }
        Ok(config)
    }

    /// Find and load configuration, walking up from `start_path` and
    /// then falling back to the user's XDG config directory.
    ///
    /// Search order (first match wins):
    /// 1. `.fleetlint.toml` in `start_path` or any ancestor directory.
    /// 2. `$XDG_CONFIG_HOME/flint/config.toml` (or `$HOME/.config/flint/config.toml`).
    ///
    /// The user-level fallback lets contributors share lint preferences
    /// across multiple Fleet GitOps checkouts without committing a config
    /// to each repo — matches fleet-plan's `~/.config/fleet-plan.json`
    /// convention. Repo-local config always wins, so a team file can
    /// override a personal one without surprises.
    pub fn find_and_load(start_path: &Path) -> Option<(PathBuf, Self)> {
        if let Some(hit) = Self::find_in_ancestors(start_path) {
            return Some(hit);
        }
        Self::find_in_user_config()
    }

    /// The config file discovery would read from `start_path`, if it does
    /// not parse — with the parse error. `check` and `dry-run` exit 3 on it
    /// rather than lint with defaults: a CI run must not pass because its
    /// config was silently dropped.
    pub fn find_unparseable(start_path: &Path) -> Option<(PathBuf, String)> {
        let mut current = if start_path.is_file() {
            start_path.parent()?.to_path_buf()
        } else {
            start_path.to_path_buf()
        };
        loop {
            for name in CONFIG_FILE_NAMES {
                let path = current.join(name);
                if path.exists() {
                    return Self::from_file(&path).err().map(|e| (path, e.to_string()));
                }
            }
            current = current.parent()?.to_path_buf();
        }
    }

    /// Walk up from `start_path` looking for `.fleetlint.toml` (legacy
    /// behavior, preserved for backward compatibility). Returns the
    /// repo-local config if found and parseable.
    fn find_in_ancestors(start_path: &Path) -> Option<(PathBuf, Self)> {
        let mut current = if start_path.is_file() {
            start_path.parent()?.to_path_buf()
        } else {
            start_path.to_path_buf()
        };

        loop {
            // Both spellings are checked at EACH level before moving up, so a
            // nested visible config still beats a hidden one further out —
            // proximity wins over spelling, which is what "the config next to
            // my files" means to a reader.
            for name in CONFIG_FILE_NAMES {
                let config_path = current.join(name);
                if config_path.exists() {
                    match Self::from_file(&config_path) {
                        Ok(config) => {
                            warn_ignored_settings(&config_path);
                            return Some((config_path, config));
                        }
                        // The workspace denies printing from the library, and
                        // rightly so. This one site is the exception: the
                        // failure has to reach a human, there is no error
                        // channel out of an `Option`-returning discovery walk,
                        // and staying silent is what caused the bug — a config
                        // that fails to parse looks exactly like no config, so
                        // scoping vanishes without a word. Plumbing the error
                        // up to every caller is the cleaner fix; this is the
                        // one that stops shipping the hazard today.
                        #[allow(clippy::print_stderr)]
                        Err(e) => {
                            // A config that fails to parse used to be
                            // discarded silently, which is indistinguishable
                            // from having no config at all: `[files]` stops
                            // narrowing, and flint quietly lints the whole
                            // repo — including the files the author wrote the
                            // config to keep out. A typo'd section name is
                            // enough to trigger it. Say so loudly and carry on
                            // with defaults, because the alternative (failing
                            // the run) would break every hook and editor on a
                            // one-character mistake.
                            eprintln!(
                                "warning: {} could not be parsed, so it is being IGNORED.",
                                config_path.display()
                            );
                            eprintln!("  {e}");
                            eprintln!(
                                "  No [files] scoping and no rule settings are in effect — \
flint is linting with defaults. Fix the file or move it aside."
                            );
                            return None;
                        }
                    }
                }
            }

            current = current.parent()?.to_path_buf();
        }
    }

    /// Look up the user-level config at `$XDG_CONFIG_HOME/flint/config.toml`
    /// (or `$HOME/.config/flint/config.toml` if XDG_CONFIG_HOME isn't set).
    /// Returns `None` if no home directory can be resolved, the file
    /// doesn't exist, or it fails to parse.
    /// Load `[fleet]` connection settings with the user-level config as a
    /// FALLBACK LAYER, not an either/or.
    ///
    /// `find_and_load` returns the repo config when one exists and only tries
    /// the user config otherwise. That is right for lint settings — a repo's
    /// rules should not silently inherit a developer's personal file — but
    /// wrong for credentials: a repo `.fleetlint.toml` carrying only
    /// `[files]` would shadow `~/.config/flint/config.toml` entirely, and
    /// `flint fleet` would report "no Fleet URL" while the user was looking
    /// at a config file that plainly had one.
    ///
    /// Credentials belong OUTSIDE the repo, so here the layering inverts:
    /// take the repo's `[fleet]` when it sets anything, otherwise fall back
    /// to the user config, field by field.
    pub fn resolve_fleet_connection(start_path: &Path) -> FleetConnectionConfig {
        let repo = Self::find_in_ancestors(start_path).map(|(_, c)| c.fleet);
        let user = Self::find_in_user_config().map(|(_, c)| c.fleet);

        match (repo, user) {
            (Some(mut r), Some(u)) => {
                if r.url.trim().is_empty() {
                    r.url = u.url;
                }
                if r.token.trim().is_empty() {
                    r.token = u.token;
                }
                r
            }
            (Some(r), None) => r,
            (None, Some(u)) => u,
            (None, None) => FleetConnectionConfig::default(),
        }
    }

    fn find_in_user_config() -> Option<(PathBuf, Self)> {
        let path = user_config_path()?;
        if !path.exists() {
            return None;
        }
        match Self::from_file(&path) {
            Ok(config) => {
                warn_ignored_settings(&path);
                Some((path, config))
            }
            Err(_) => None,
        }
    }

    /// Check if a rule is disabled.
    pub fn is_rule_disabled(&self, rule_name: &str) -> bool {
        self.rules.level(rule_name) == Some(RuleLevel::Ignore)
    }

    /// Check if a rule should be downgraded to warning.
    pub fn is_rule_warning(&self, rule_name: &str) -> bool {
        self.rules.level(rule_name) == Some(RuleLevel::Warn)
    }

    /// Rules ignored for every file, so they need not run at all: ignored in
    /// `[rules]` (or by `allow_unknown_fields`) and set to another level by
    /// no override.
    pub fn disabled_rules(&self) -> HashSet<&str> {
        let mut set: HashSet<&str> = self.rules.disabled.iter().chain(&self.rules.ignore).map(|s| s.as_str()).collect();
        if self.schema.allow_unknown_fields {
            set.insert(crate::codes::STRUCTURAL_VALIDATION);
        }
        for o in &self.overrides {
            for code in o.rules.codes() {
                if o.rules.level(code) != Some(RuleLevel::Ignore) {
                    set.remove(code.as_str());
                }
            }
        }
        set
    }

    /// The level for `code` on `file`: `[rules]`, then each matching
    /// `[[overrides]]` in order (CLI `--error/--warn/--ignore` are the last).
    pub fn rule_level(&self, code: &str, file: &Path) -> Option<RuleLevel> {
        let mut level = self.rules.level(code);
        if level.is_none() && self.schema.allow_unknown_fields && code == crate::codes::STRUCTURAL_VALIDATION {
            level = Some(RuleLevel::Ignore);
        }
        if self.overrides.is_empty() {
            return level;
        }
        let rel = self.relative(file);
        for (o, globs) in self.overrides.iter().zip(&self.compiled().overrides) {
            if globs.is_match(rel.as_ref()) {
                level = o.rules.level(code).or(level);
            }
        }
        level
    }

    /// `file` as the config-relative string the globs are written against.
    fn relative<'a>(&self, file: &'a Path) -> std::borrow::Cow<'a, str> {
        let raw = match self.base_dir.as_deref().filter(|_| file.is_absolute()).and_then(|b| file.strip_prefix(b).ok()) {
            Some(rel) => rel.to_string_lossy(),
            None => file.to_string_lossy(),
        };
        match raw {
            std::borrow::Cow::Borrowed(s) => std::borrow::Cow::Borrowed(s.strip_prefix("./").unwrap_or(s)),
            std::borrow::Cow::Owned(s) => std::borrow::Cow::Owned(s.strip_prefix("./").unwrap_or(&s).to_string()),
        }
    }

    /// The compiled glob sets, built once per config instance.
    fn compiled(&self) -> &CompiledGlobs {
        self.compiled.get_or_init(|| CompiledGlobs {
            include: build_globset(&self.files.include),
            exclude: build_globset(&expand_bare_names(&self.files.exclude)),
            overrides: self.overrides.iter().map(|o| build_globset(&expand_bare_names(&o.files))).collect(),
        })
    }

    /// Check if a file path should be linted based on include/exclude patterns.
    pub fn should_lint_file(&self, file_path: &Path) -> bool {
        // An absolute path matches no relative glob, so re-express it against
        // the directory the config came from. Editors and CI hand us absolute
        // paths; without this a scoped `include` silently rejects every one of
        // them. Falls through unchanged when the file is outside the config's
        // tree, which is a genuine out-of-scope answer rather than a miss.
        // A leading "./" is dropped too, so anchored globs (e.g.
        // "fleets/**/*.yml") match paths from a `.` directory walk.
        let rel = self.relative(file_path);
        let path_str = rel.as_ref();
        let globs = self.compiled();

        // Exclusions take priority.
        if globs.exclude.is_match(path_str) {
            return false;
        }

        // When an `include` list is configured it is authoritative: a file
        // matching none of its globs is NOT linted. Previously a non-match fell
        // through to "any YAML", so `include` could only ever ADD files, never
        // narrow the set — making a scoping `include` a silent no-op.
        if !self.files.include.is_empty() {
            return globs.include.is_match(path_str);
        }

        // No `include` configured: default to including YAML files.
        matches!(
            file_path.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        )
    }

    /// Whether `[files]` scoping puts this path out of bounds.
    ///
    /// Applies the same two scoping mechanisms as `should_lint_file` —
    /// `exclude` wins, then a non-empty `include` is authoritative — but NOT
    /// its YAML-only default. Workspace rules report on files the YAML walk
    /// never sees (scripts, profiles, payloads), so reusing `should_lint_file`
    /// would silently drop every `orphaned-file` finding on a `.sh`.
    ///
    /// Both scoping styles must be honored here: a denylist repo writes
    /// `exclude = ["tools-scripts/**"]`, an allowlist repo writes
    /// `include = ["fleets/**", "platforms/**", …]`. Checking only one style
    /// would leave the other silently unscoped for cross-file rules.
    pub fn is_out_of_scope_file(&self, file_path: &Path) -> bool {
        let raw = file_path.to_string_lossy();
        let path_str = raw.strip_prefix("./").unwrap_or(&raw);
        let globs = self.compiled();

        if globs.exclude.is_match(path_str) {
            return true;
        }
        if !self.files.include.is_empty() {
            return !globs.include.is_match(path_str);
        }
        false
    }

    /// JSON Schema for `fleetlint.toml` (editors via SchemaStore / taplo).
    ///
    /// Derived from the defaults as serialized, so it cannot drift from the
    /// config types: every key the loader reads, typed by its default, and
    /// no other (a typo is flagged as you type). Rule lists only take codes
    /// from the registry.
    pub fn json_schema() -> serde_json::Value {
        use serde_json::{json, Value};
        fn infer(v: &Value) -> Value {
            match v {
                Value::Bool(_) => json!({ "type": "boolean" }),
                Value::Number(n) if n.is_f64() => json!({ "type": "number" }),
                Value::Number(_) => json!({ "type": "integer" }),
                Value::String(_) => json!({ "type": "string" }),
                // Every list that defaults empty is a list of strings, except
                // [[patterns]] and [[overrides]], which are seeded below.
                Value::Array(a) => json!({ "type": "array", "items": a.first().map_or(json!({ "type": "string" }), infer) }),
                Value::Object(o) => json!({
                    "type": "object",
                    "additionalProperties": false,
                    "properties": o.iter().map(|(k, v)| (k.clone(), infer(v))).collect::<serde_json::Map<_, _>>(),
                }),
                Value::Null => json!({}),
            }
        }
        let mut root = serde_json::to_value(Self::default()).unwrap_or_default();
        root["patterns"] = json!([PatternConfig::default()]);
        root["overrides"] = json!([OverrideConfig::default()]);
        let mut schema = infer(&root);

        let codes: Vec<&str> = super::codes::REGISTRY.iter().map(|m| m.code).collect();
        let rule_lists = |rules: &mut Value| {
            for list in ["disabled", "ignore", "warn", "error"] {
                rules["properties"][list]["items"] = json!({ "enum": codes });
            }
        };
        rule_lists(&mut schema["properties"]["rules"]);
        rule_lists(&mut schema["properties"]["overrides"]["items"]["properties"]["rules"]);
        schema["properties"]["extend"] = json!({ "type": "string", "description": "Base config this file overlays, relative to this file" });
        schema["properties"]["fleet"]["properties"]["env"]["additionalProperties"] = json!({ "type": "string" });
        schema["$schema"] = json!("http://json-schema.org/draft-07/schema#");
        schema["$id"] = json!("https://flint.macadmin.me/schemas/fleetlint.json");
        schema["title"] = json!("flint configuration (fleetlint.toml)");
        schema
    }

    /// Write a default configuration to a path.
    pub fn write_default(path: &Path) -> Result<(), ConfigError> {
        let default_config = Self::default_with_comments();
        std::fs::write(path, default_config)
            .map_err(|e| ConfigError::WriteError(path.to_path_buf(), e.to_string()))
    }

    /// Generate default configuration with explanatory comments.
    pub fn default_with_comments() -> String {
        r#"#:schema https://flint.macadmin.me/schemas/fleetlint.json
# Fleet Linter Configuration
# Place this file at the root of your GitOps repository as `fleetlint.toml`
# All rule codes: `flint list-rules`

# Rule Configuration
[rules]
# Rules to disable entirely (by name)
# Available rules:
#   - required-fields: Ensures required fields are present
#   - platform-compatibility: Validates osquery tables work on specified platform
#   - type-validation: Validates field types
#   - security: Detects hardcoded secrets
#   - interval-validation: Warns about extreme interval values
#   - duplicate-names: Detects duplicate policy/query/label names
#   - query-syntax: Validates SQL query syntax
#   - structural-validation: Validates YAML structure (unknown/misplaced keys)
disabled = []

# Rules to downgrade from error to warning
warn = []

# Rules to promote from warning to error
error = []

# Per-path levels (later wins):
# [[overrides]]
# files = ["fleets/legacy/**"]
# rules.warn = ["structural-validation"]

# Threshold Configuration
[thresholds]
# Minimum query interval in seconds (default: 60)
min_interval = 60

# Maximum query interval in seconds (default: 86400 = 24 hours)
max_interval = 86400

# Maximum query length in characters (default: 10000)
max_query_length = 10000

# Warn when using SELECT * (default: true)
warn_select_star = true

# File Patterns
[files]
# Scope. `include` is left UNSET on purpose: a non-empty include is
# authoritative, and it also governs the cross-file rules (orphaned-file,
# duplicate-content, case-collision), which report on scripts, profiles and
# payloads rather than YAML. A YAML-only include would silently disable all
# of them on non-YAML files. Unset = everything not excluded.
#
# To narrow, list DIRECTORIES rather than extensions:
#   include = ["default.yml", "fleets/**", "labels/**", "platforms/**"]

# Glob patterns to exclude
exclude = [
    "**/node_modules/**",
    "**/target/**",
    "**/.git/**",
    "**/dist/**",
]

# Schema Validation
[schema]
# Allow unknown/extra fields (default: false)
# Set to true to disable structural validation of YAML keys
allow_unknown_fields = false

# Deprecation Settings
[deprecations]
# Target Fleet version for deprecation warnings (default: "latest")
# Set to a specific version like "4.80.0" to see deprecations for that version
fleet_version = "latest"

# Opt in to future naming conventions (default: false)
# When true, completions suggest new names and old names get deprecation warnings:
#   queries -> reports, team_settings -> settings, teams/ -> fleets/
future_names = false

# Fleet Server Connection
[fleet]
# Enable gitops validation on save (default: false)
gitops_validation = false

# Enable live completions from Fleet instance (default: false)
live_completions = false

# Fleet server URL (falls back to FLEET_URL env var)
# url = "https://fleet.example.com"

# Fleet API token (falls back to FLEET_API_TOKEN env var)
# Supports 1Password references: op://vault/item/field
# token = ""
# token = "op://Work/Fleet/api-token"

# Path to fleetctl binary (falls back to "fleetctl" on PATH)
# fleetctl_path = "/usr/local/bin/fleetctl"

# Extra env vars passed to fleetctl (for $VAR references in gitops YAML)
# Values support op:// references for 1Password secrets
# [fleet.env]
# FLEET_GLOBAL_ENROLL_SECRET = "op://Vault/Item/field"

# ── Repo-convention patterns ────────────────────────────────────────────
# Declarative rules for conventions THIS repo owns — naming schemes, file
# placement, fan-out completeness. Each entry needs `files` (a glob),
# `assert` (one of: name-matches-filename, filename, content-must-match,
# content-must-not-match, token-consistency, must-be-referenced,
# unique-content-within, required-structure, forbid-file), and a REQUIRED
# `why` — ideally citing the commit that taught you the rule. Findings get
# code `pattern/<assert>` and can be disabled/downgraded via [rules].
#
# [[patterns]]
# files = "fleets/*.yml"
# assert = "name-matches-filename"   # `name:` must equal the file stem
# severity = "warn"                  # error | warn (default) | info
# why = "fleet names drifted from filenames after renames (commit abc123)"
#
# [[patterns]]
# files = "**/.DS_Store"
# assert = "forbid-file"
# why = "Finder droppings keep sneaking into commits"
"#
        .to_string()
    }
}

/// Compile one glob with flint's semantics: `*` does not cross `/`
/// (`literal_separator`), `**` does, braces work. Invalid patterns yield
/// `None` — they match nothing rather than panicking.
pub(crate) fn compile_glob(pattern: &str) -> Option<globset::GlobMatcher> {
    globset::GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .ok()
        .map(|g| g.compile_matcher())
}

/// One-off glob match (compiles per call — prefer [`compile_glob`] or the
/// config's compiled sets in loops).
#[cfg(test)]
pub(crate) fn matches_glob(pattern: &str, path: &str) -> bool {
    let pattern = pattern.strip_prefix("./").unwrap_or(pattern);
    let path = path.strip_prefix("./").unwrap_or(path);
    compile_glob(pattern).is_some_and(|m| m.is_match(path))
}

/// Compile a pattern list into a `GlobSet`; invalid patterns are skipped.
///
/// A leading `./` on a pattern is stripped so that configs written as
/// `include = ["./fleets/*.yml"]` match the normalized (also `./`-stripped)
/// paths checked by `should_lint_file` (issue #15).
/// Settings a config file contains that have no effect: keys no config field
/// reads (a typo'd key or section is dropped by the lenient parse), and rule
/// codes in `[rules]` that no rule emits. Found by round-tripping the parsed
/// config back to TOML, so the config structs stay the only list of keys.
pub(crate) fn ignored_settings(content: &str) -> Vec<String> {
    let (Ok(raw), Ok(cfg)) = (content.parse::<toml::Table>(), FleetLintConfig::parse(content)) else {
        return Vec::new();
    };
    let Ok(toml::Value::Table(kept)) = toml::Value::try_from(&cfg) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    diff_keys(&raw, &kept, "", &mut out);
    for code in cfg.rules.codes().chain(cfg.overrides.iter().flat_map(|o| o.rules.codes())) {
        if super::codes::meta(code).is_none() {
            let codes = super::codes::REGISTRY.iter().map(|m| m.code);
            out.push(format!("[rules] `{code}` is not a rule code{}", did_you_mean(code, codes)));
        }
    }
    out
}

/// `path`'s TOML with its `extend` chain merged underneath.
fn load_table(path: &Path, content: &str, depth: usize) -> Result<toml::Table, ConfigError> {
    let mut table: toml::Table = content.parse().map_err(|e: toml::de::Error| ConfigError::ParseError(e.to_string()))?;
    let Some(toml::Value::String(base)) = table.get("extend") else { return Ok(table) };
    if depth >= 8 {
        return Err(ConfigError::ParseError(format!("`extend` chain is too deep (cycle?) at {}", path.display())));
    }
    let base_path = path.parent().unwrap_or(Path::new(".")).join(base);
    let base_content = std::fs::read_to_string(&base_path)
        .map_err(|e| ConfigError::ParseError(format!("extend = \"{base}\": {}: {e}", base_path.display())))?;
    let mut merged = load_table(&base_path, &base_content, depth + 1)?;
    table.remove("extend");
    merge_tables(&mut merged, table);
    Ok(merged)
}

fn merge_tables(base: &mut toml::Table, over: toml::Table) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(toml::Value::Table(b)), toml::Value::Table(o)) => merge_tables(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

/// Settings older configs (and `flint init` before 0.3.4) carry that no code
/// ever read.
const RETIRED: &[&str] = &["schema.validate", "schema.require_platform", "files.root"];

fn diff_keys(raw: &toml::Table, kept: &toml::Table, prefix: &str, out: &mut Vec<String>) {
    for (key, value) in raw {
        let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        match (value, kept.get(key)) {
            (_, None) if RETIRED.contains(&path.as_str()) => {
                out.push(format!("`{path}` never had an effect and is ignored — delete it"))
            }
            (_, None) => out.push(format!(
                "`{path}` is not a setting and is ignored{}",
                did_you_mean(key, kept.keys().map(String::as_str))
            )),
            (toml::Value::Table(r), Some(toml::Value::Table(k))) => diff_keys(r, k, &path, out),
            (toml::Value::Array(r), Some(toml::Value::Array(k))) => {
                for (r, k) in r.iter().zip(k) {
                    if let (toml::Value::Table(r), toml::Value::Table(k)) = (r, k) {
                        diff_keys(r, k, &format!("{path}[]"), out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn did_you_mean<'a>(word: &str, candidates: impl Iterator<Item = &'a str>) -> String {
    candidates
        .map(|c| (super::util::levenshtein_distance(word, c), c))
        .filter(|(d, c)| *d <= c.len().max(word.len()) / 3)
        .min_by_key(|(d, _)| *d)
        .map(|(_, c)| format!(" — did you mean `{c}`?"))
        .unwrap_or_default()
}

/// Print [`ignored_settings`] for a config file that parsed. Same exception
/// as the parse-failure warning: a setting with no effect looks exactly like
/// one that works, so it has to reach a human.
#[allow(clippy::print_stderr)]
fn warn_ignored_settings(path: &Path) {
    let Ok(content) = std::fs::read_to_string(path) else { return };
    for (line, msg) in ignored_settings_located(&content) {
        match line {
            Some(n) => eprintln!("warning: {}:{n}: {msg}", path.display()),
            None => eprintln!("warning: {}: {msg}", path.display()),
        }
    }
}

/// [`ignored_settings`] with the 1-based line each one is written on.
/// ponytail: textual lookup (section header, `key =` line, or the quoted
/// rule code), first match; switch to toml spans if duplicates mislead.
fn ignored_settings_located(content: &str) -> Vec<(Option<usize>, String)> {
    ignored_settings(content)
        .into_iter()
        .map(|msg| {
            let token = msg.split('`').nth(1).unwrap_or_default();
            let key = token.rsplit('.').next().unwrap_or(token).trim_end_matches("[]");
            let line = content.lines().position(|l| {
                let l = l.trim();
                l == format!("[{token}]")
                    || l == format!("[[{token}]]")
                    || l.strip_prefix(key).is_some_and(|r| r.trim_start().starts_with('='))
                    || l.contains(&format!("\"{token}\""))
            });
            (line.map(|i| i + 1), msg)
        })
        .collect()
}

/// `exclude = ["node_modules"]` means what it does in `.gitignore`: that
/// name at any depth. A bare name (no `/`, no glob characters) becomes
/// `**/<name>` and `**/<name>/**`; anything else is a glob as written.
/// Only for `exclude` — an `include` of `"default.yml"` means the root file.
fn expand_bare_names(patterns: &[String]) -> Vec<String> {
    patterns
        .iter()
        .flat_map(|p| {
            let bare = !p.contains('/') && !p.contains(['*', '?', '[', '{']);
            if bare {
                vec![format!("**/{p}"), format!("**/{p}/**")]
            } else {
                vec![p.clone()]
            }
        })
        .collect()
}

fn build_globset(patterns: &[String]) -> globset::GlobSet {
    let mut builder = globset::GlobSetBuilder::new();
    for p in patterns {
        let p = p.strip_prefix("./").unwrap_or(p);
        if let Ok(g) = globset::GlobBuilder::new(p).literal_separator(true).build() {
            builder.add(g);
        }
    }
    builder.build().unwrap_or_else(|_| globset::GlobSet::empty())
}

/// The lazily compiled include/exclude sets for [`FleetLintConfig::should_lint_file`].
#[derive(Debug, Clone)]
pub(crate) struct CompiledGlobs {
    include: globset::GlobSet,
    exclude: globset::GlobSet,
    overrides: Vec<globset::GlobSet>,
}

/// Configuration error types.
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// Failed to read config file.
    ReadError(PathBuf, String),
    /// Failed to parse TOML.
    ParseError(String),
    /// Failed to write config file.
    WriteError(PathBuf, String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::ReadError(path, msg) => {
                write!(f, "Failed to read config file {}: {}", path.display(), msg)
            }
            ConfigError::ParseError(msg) => {
                write!(f, "Failed to parse config: {}", msg)
            }
            ConfigError::WriteError(path, msg) => {
                write!(f, "Failed to write config file {}: {}", path.display(), msg)
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default `include` MUST stay empty.
    ///
    /// It used to default to `["**/*.yml", "**/*.yaml"]`, which was harmless
    /// while `include` only narrowed the YAML walk. Once scoping was extended
    /// to the cross-file rules, that default became an authoritative allowlist
    /// that put every script, profile and payload out of scope — silently
    /// disabling orphaned-file, duplicate-content, case-collision and
    /// unregistered-script for EVERY repo, including repos with no config at
    /// all. On the reference repo that was 13 orphaned-file and 2
    /// duplicate-content findings reduced to zero, with nothing reported to
    /// explain the difference.
    ///
    /// Empty is also behavior-preserving for the YAML walk: `should_lint_file`
    /// falls back to "YAML only" when no include is set.
    /// Both spellings are discovered, visible wins, and proximity beats
    /// spelling.
    ///
    /// The visible name is new; the hidden one is what every existing repo
    /// has. Breaking either would be a silent downgrade — a repo whose
    /// config stops being found does not error, it just quietly lints with
    /// defaults, which is how `platforms/_retired` went unexcluded for weeks.
    #[test]
    fn both_config_spellings_are_discovered() {
        use std::fs;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let nested = root.join("a/b");
        fs::create_dir_all(&nested).unwrap();

        // Hidden only, at the root: found from a nested dir.
        fs::write(root.join(".fleetlint.toml"), "[files]\n").unwrap();
        let (p, _) = FleetLintConfig::find_and_load(&nested).expect("hidden form");
        assert_eq!(p.file_name().unwrap(), ".fleetlint.toml");

        // Visible alongside it wins — a repo that added it opted in.
        fs::write(root.join("fleetlint.toml"), "[files]\n").unwrap();
        let (p, _) = FleetLintConfig::find_and_load(&nested).expect("visible form");
        assert_eq!(p.file_name().unwrap(), "fleetlint.toml");

        // A nearer HIDDEN config still beats a further VISIBLE one:
        // proximity is what "the config next to my files" means.
        fs::write(nested.join(".fleetlint.toml"), "[files]\n").unwrap();
        let (p, _) = FleetLintConfig::find_and_load(&nested).expect("nearest");
        assert_eq!(p.parent().unwrap(), nested);
    }

    #[test]
    fn files_config_default_include_is_empty() {
        assert!(
            FilesConfig::default().include.is_empty(),
            "a non-empty default include acts as an allowlist for the \
             cross-file rules and silently disables them on non-YAML files"
        );
        // And the whole-config default must agree — that is the path a repo
        // with no .fleetlint.toml takes.
        assert!(FleetLintConfig::default().files.include.is_empty());
        assert!(
            !FleetLintConfig::default()
                .is_out_of_scope_file(Path::new("platforms/macos/site/scripts/x.sh")),
            "with no config, a script must be in scope for the workspace rules"
        );
    }

    #[test]
    fn test_default_config() {
        let config = FleetLintConfig::default();
        assert!(config.rules.disabled.is_empty());
        assert_eq!(config.thresholds.min_interval, 60);
        assert_eq!(config.thresholds.max_interval, 86400);
        assert!(config.thresholds.warn_select_star);
    }

    #[test]
    fn test_parse_config() {
        let toml = r#"
[rules]
disabled = ["query-syntax"]
warn = ["interval-validation"]

[thresholds]
min_interval = 30
max_interval = 3600

[files]
exclude = ["**/test/**"]
"#;

        let config = FleetLintConfig::parse(toml).unwrap();
        assert_eq!(config.rules.disabled, vec!["query-syntax"]);
        assert_eq!(config.rules.warn, vec!["interval-validation"]);
        assert_eq!(config.thresholds.min_interval, 30);
        assert_eq!(config.thresholds.max_interval, 3600);
        assert!(config.files.exclude.contains(&"**/test/**".to_string()));
    }

    #[test]
    fn test_is_rule_disabled() {
        let toml = r#"
[rules]
disabled = ["query-syntax", "security"]
"#;
        let config = FleetLintConfig::parse(toml).unwrap();

        assert!(config.is_rule_disabled("query-syntax"));
        assert!(config.is_rule_disabled("security"));
        assert!(!config.is_rule_disabled("required-fields"));
    }

    #[test]
    fn test_is_rule_warning() {
        let toml = r#"
[rules]
warn = ["duplicate-names"]
"#;
        let config = FleetLintConfig::parse(toml).unwrap();

        assert!(config.is_rule_warning("duplicate-names"));
        assert!(!config.is_rule_warning("query-syntax"));
    }

    #[test]
    fn test_matches_glob() {
        // ** pattern
        assert!(matches_glob("**/*.yml", "lib/policies.yml"));
        assert!(matches_glob("**/*.yml", "teams/engineering/default.yml"));
        assert!(!matches_glob("**/*.yml", "lib/policies.yaml"));

        // Simple * pattern
        assert!(matches_glob("*.yml", "default.yml"));
        assert!(!matches_glob("*.yml", "lib/default.yml"));

        // Exclude patterns
        assert!(matches_glob(
            "**/node_modules/**",
            "node_modules/foo/bar.yml"
        ));
        assert!(matches_glob("**/target/**", "some/target/debug/test.yml"));
    }

    #[test]
    fn test_should_lint_file() {
        let config = FleetLintConfig::default();

        assert!(config.should_lint_file(Path::new("default.yml")));
        assert!(config.should_lint_file(Path::new("lib/policies.yaml")));
        assert!(!config.should_lint_file(Path::new("node_modules/foo.yml")));
        assert!(!config.should_lint_file(Path::new("target/test.yml")));
        assert!(!config.should_lint_file(Path::new("script.js")));
    }

    #[test]
    fn include_narrows_the_linted_set() {
        // A scoping `include` must actually restrict — a file matching none of
        // its globs is skipped even though it is valid YAML. (Regression: the
        // include list used to only add, never narrow.)
        let mut config = FleetLintConfig::default();
        config.files.include = vec!["fleets/**/*.yml".to_string()];

        assert!(config.should_lint_file(Path::new("fleets/a.yml")));
        assert!(config.should_lint_file(Path::new("fleets/sub/b.yml")));
        // Leading "./" (from a `.` directory walk) must still match.
        assert!(config.should_lint_file(Path::new("./fleets/a.yml")));
        // Outside the include scope → not linted, despite being YAML.
        assert!(!config.should_lint_file(Path::new("vendor/c.yml")));
        assert!(!config.should_lint_file(Path::new("./vendor/c.yml")));
    }

    #[test]
    fn test_should_lint_file_custom_include_exclude() {
        // Ported from pre-merge main (issue #15): a real user config with
        // "./"-prefixed includes must narrow, and excludes must win.
        let toml = r#"
[files]
include = ["./fleets/*.yml", "./lib/**/*.yaml"]
exclude = [
    "**/node_modules/**",
    "**/target/**",
    "**/.git/**",
    "**/dist/**",
    "cspell.config.yaml"
]
"#;
        let config = FleetLintConfig::parse(toml).unwrap();

        // Explicitly excluded root-level file
        assert!(!config.should_lint_file(Path::new("cspell.config.yaml")));
        assert!(!config.should_lint_file(Path::new("./cspell.config.yaml")));

        // Files matching the includes
        assert!(config.should_lint_file(Path::new("fleets/prod.yml")));
        assert!(config.should_lint_file(Path::new("./fleets/prod.yml")));
        assert!(config.should_lint_file(Path::new("lib/macos/policies.yaml")));

        // YAML files NOT in the include list must be skipped
        assert!(!config.should_lint_file(Path::new("random.yml")));
        assert!(!config.should_lint_file(Path::new("docs/mkdocs.yaml")));
        assert!(!config.should_lint_file(Path::new("fleets/nested/deep.yml")));
    }

    #[test]
    fn test_matches_glob_dot_slash_prefix() {
        assert!(matches_glob("./fleets/*.yml", "fleets/prod.yml"));
        assert!(matches_glob("fleets/*.yml", "./fleets/prod.yml"));
        assert!(matches_glob("./lib/**/*.yaml", "lib/a/b/c.yaml"));
        assert!(!matches_glob("./fleets/*.yml", "other/prod.yml"));
    }

    #[test]
    fn test_should_lint_file_empty_include_falls_back_to_yaml() {
        let toml = r#"
[files]
include = []
exclude = ["**/node_modules/**"]
"#;
        let config = FleetLintConfig::parse(toml).unwrap();

        assert!(config.should_lint_file(Path::new("anything.yml")));
        assert!(config.should_lint_file(Path::new("deep/nested/file.yaml")));
        assert!(!config.should_lint_file(Path::new("script.js")));
        assert!(!config.should_lint_file(Path::new("node_modules/a.yml")));
    }

    #[test]
    fn json_schema_covers_every_setting_and_only_those() {
        let schema = FleetLintConfig::json_schema();
        // Every key the docs' full example and the init template write is
        // in the schema; a key the loader ignores is not.
        fn walk(v: &toml::Value, schema: &serde_json::Value, path: &str, out: &mut Vec<String>) {
            if let toml::Value::Table(t) = v {
                for (k, v) in t {
                    let sub = &schema["properties"][k];
                    let open = schema["additionalProperties"].is_object();
                    if sub.is_null() && !open {
                        out.push(format!("{path}{k}"));
                    }
                    walk(v, sub, &format!("{path}{k}."), out);
                }
            }
        }
        let docs = include_str!("../../../docs/docs/configuration.md");
        let example = docs.split("```toml").nth(1).unwrap().split("```").next().unwrap();
        for (name, src) in [("configuration.md", example.to_string()), ("template", FleetLintConfig::default_with_comments())] {
            let mut missing = Vec::new();
            walk(&src.parse::<toml::Value>().unwrap(), &schema, "", &mut missing);
            assert!(missing.is_empty(), "{name} uses keys the schema (and the loader) don't know: {missing:?}");
        }
        assert!(schema["properties"]["rules"]["properties"]["warn"]["items"]["enum"].as_array().unwrap().len() == crate::codes::REGISTRY.len());
        assert!(schema["properties"]["schema"]["properties"].get("validate").is_none());
    }

    #[test]
    fn ignored_settings_reports_typos_and_unknown_codes() {
        let msgs = ignored_settings(
            "[rules]\ndisabeld = [\"x\"]\nwarn = [\"structural-validaton\"]\n[rule]\n[fleet.env]\nANY_NAME = \"v\"\n[[patterns]]\nfiles = \"*.txt\"\nassert = \"forbid-file\"\nwhy = \"w\"\nfils = \"x\"\n",
        );
        let joined = msgs.join("\n");
        assert!(joined.contains("`rules.disabeld` is not a setting") && joined.contains("did you mean `disabled`"), "{joined}");
        assert!(joined.contains("`rule` is not a setting") && joined.contains("did you mean `rules`"), "{joined}");
        assert!(joined.contains("`structural-validaton` is not a rule code — did you mean `structural-validation`"), "{joined}");
        assert!(joined.contains("`patterns[].fils`"), "{joined}");
        assert!(!joined.contains("ANY_NAME"), "[fleet.env] names are free-form: {joined}");
        let retired = ignored_settings("[schema]\nvalidate = true\nrequire_platform = true\n").join("\n");
        assert!(retired.contains("`schema.validate` never had an effect") && retired.contains("`schema.require_platform` never"), "{retired}");
        assert_eq!(msgs.len(), 4, "{joined}");
        let located = ignored_settings_located("[rules]\ndisabled = [\"orphan-file\"]\n\n[thresolds]\nmin_interval = 1\n");
        assert_eq!(located.iter().map(|(l, _)| *l).collect::<Vec<_>>(), [Some(4), Some(2)], "{located:?}");
        // a valid config is silent
        assert!(ignored_settings("[rules]\ndisabled = [\"orphaned-file\"]\n[thresholds]\nmin_interval = 30\n").is_empty());
    }

    #[test]
    fn exclude_bare_name_matches_at_any_depth() {
        // The form docs/configuration.md has always shown.
        let cfg: FleetLintConfig =
            toml::from_str("[files]\nexclude = [\"node_modules\", \"scratch\"]\n").unwrap();
        assert!(!cfg.should_lint_file(Path::new("scratch/b.yml")));
        assert!(!cfg.should_lint_file(Path::new("./a/node_modules/x/y.yml")));
        assert!(cfg.should_lint_file(Path::new("fleets/scratchpad.yml")));
        // include keeps exact semantics
        let cfg: FleetLintConfig = toml::from_str("[files]\ninclude = [\"default.yml\"]\n").unwrap();
        assert!(cfg.should_lint_file(Path::new("default.yml")));
        assert!(!cfg.should_lint_file(Path::new("fleets/default.yml")));
    }

    #[test]
    fn exclude_wins_over_include() {
        let mut config = FleetLintConfig::default();
        config.files.include = vec!["**/*.yml".to_string()];
        config.files.exclude = vec!["**/vendor/**".to_string()];

        assert!(config.should_lint_file(Path::new("fleets/a.yml")));
        assert!(!config.should_lint_file(Path::new("vendor/c.yml")));
        assert!(!config.should_lint_file(Path::new("./vendor/c.yml")));
    }

    #[test]
    fn brace_patterns_work() {
        // Under the old hand-rolled matcher, `{yml,yaml}` braces were escaped
        // as literals and silently never matched. globset handles them.
        let mut config = FleetLintConfig::default();
        config.files.include = vec!["fleets/**/*.{yml,yaml}".to_string()];

        assert!(config.should_lint_file(Path::new("fleets/a.yml")));
        assert!(config.should_lint_file(Path::new("fleets/sub/b.yaml")));
        assert!(!config.should_lint_file(Path::new("vendor/c.yml")));
    }

    #[test]
    fn invalid_glob_does_not_panic() {
        // A malformed user pattern must not take the linter down — it simply
        // matches nothing.
        let mut config = FleetLintConfig::default();
        config.files.include = vec!["fleets/[".to_string()];
        assert!(!config.should_lint_file(Path::new("fleets/a.yml")));

        let mut config2 = FleetLintConfig::default();
        config2.files.exclude = vec!["fleets/[".to_string()];
        // Invalid exclude excludes nothing; default include still applies.
        assert!(config2.should_lint_file(Path::new("fleets/a.yml")));
    }

    #[test]
    fn clone_resets_compiled_globs() {
        // Clones are mutated (the CLI merges --exclude into a cloned config);
        // a carried glob cache would keep matching the old lists.
        let config = FleetLintConfig::default();
        assert!(config.should_lint_file(Path::new("fleets/a.yml"))); // warms cache

        let mut clone = config.clone();
        clone.files.exclude.push("fleets/**".to_string());
        assert!(!clone.should_lint_file(Path::new("fleets/a.yml")));
    }

    /// An absolute path must be matched against the relative `[files]` globs,
    /// which are written against the config's own directory. Editors and CI
    /// hand flint absolute paths; before this, every one of them matched no
    /// `include` glob, so a scoped repo judged the file out of scope and
    /// `flint check /abs/path/fleets/x.yml` printed nothing for a file that is
    /// plainly in scope — and the CLI then panicked indexing the empty result.
    #[test]
    fn absolute_paths_match_relative_include_globs() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("fleetlint.toml"),
            "[files]\ninclude = [\"fleets/**\"]\nexclude = [\"tools/**\"]\n",
        )
        .unwrap();
        let config = FleetLintConfig::from_file(&root.join("fleetlint.toml")).unwrap();

        // In scope, named absolutely — the case that used to fail.
        assert!(config.should_lint_file(&root.join("fleets/a.yml")));
        // Excluded, named absolutely: still excluded, and for the right reason.
        assert!(!config.should_lint_file(&root.join("tools/t.yml")));
        // Outside `include`, named absolutely.
        assert!(!config.should_lint_file(&root.join("other/o.yml")));
        // The relative forms keep behaving exactly as before.
        assert!(config.should_lint_file(Path::new("fleets/a.yml")));
        assert!(config.should_lint_file(Path::new("./fleets/a.yml")));
        assert!(!config.should_lint_file(Path::new("tools/t.yml")));
    }

    /// A path outside the config's tree cannot be relativized. It should fall
    /// through as out of scope rather than being silently accepted.
    #[test]
    fn absolute_path_outside_the_config_tree_is_out_of_scope() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path();
        std::fs::write(
            root.join("fleetlint.toml"),
            "[files]\ninclude = [\"fleets/**\"]\n",
        )
        .unwrap();
        let config = FleetLintConfig::from_file(&root.join("fleetlint.toml")).unwrap();

        let elsewhere = dir.path().parent().unwrap().join("somewhere-else/fleets/a.yml");
        assert!(!config.should_lint_file(&elsewhere));
    }

    #[test]
    fn test_default_with_comments() {
        let content = FleetLintConfig::default_with_comments();
        assert!(content.contains("[rules]"));
        assert!(content.contains("[thresholds]"));
        assert!(content.contains("[files]"));
        assert!(content.contains("[schema]"));
        assert!(content.contains("disabled = []"));
    }

    #[test]
    fn test_partial_config() {
        // Only specify some fields, rest should use defaults
        let toml = r#"
[thresholds]
min_interval = 120
"#;

        let config = FleetLintConfig::parse(toml).unwrap();
        assert_eq!(config.thresholds.min_interval, 120);
        // Other thresholds should be default
        assert_eq!(config.thresholds.max_interval, 86400);
        assert!(config.thresholds.warn_select_star);
        // Rules should be empty
        assert!(config.rules.disabled.is_empty());
    }

    // LSP config — debounce knob. Default 150ms, settable via [lsp]
    // section in .fleetlint.toml. Zero disables debouncing entirely.

    #[test]
    fn lsp_config_defaults_to_150ms() {
        let config = FleetLintConfig::default();
        assert_eq!(config.lsp.lint_debounce_ms, 150);
    }

    #[test]
    fn lsp_config_parses_explicit_value() {
        let toml = r#"
[lsp]
lint_debounce_ms = 300
"#;
        let config = FleetLintConfig::parse(toml).unwrap();
        assert_eq!(config.lsp.lint_debounce_ms, 300);
    }

    #[test]
    fn lsp_config_zero_means_no_debounce() {
        // Sentinel for "lint immediately on every keystroke" — useful for
        // tests and for users isolating a perf bug to confirm it's not
        // the debounce hiding the issue.
        let toml = r#"
[lsp]
lint_debounce_ms = 0
"#;
        let config = FleetLintConfig::parse(toml).unwrap();
        assert_eq!(config.lsp.lint_debounce_ms, 0);
    }

    #[test]
    fn lsp_config_absent_section_uses_default() {
        // A config file that doesn't mention [lsp] at all should still
        // produce the 150ms default, not crash. This is the path for
        // existing repos that don't know about the new section yet.
        let toml = r#"
[rules]
disabled = []
"#;
        let config = FleetLintConfig::parse(toml).unwrap();
        assert_eq!(config.lsp.lint_debounce_ms, 150);
    }

    // User-level config fallback — XDG path resolution. The fs-touching
    // outer `find_in_user_config` is not tested here (would need tempdir
    // fixtures); the pure resolver covers all the branching logic.

    #[test]
    fn user_config_prefers_xdg_when_set() {
        let p = resolve_user_config_path(Some("/custom/xdg"), Some("/home/alice")).unwrap();
        assert_eq!(p, PathBuf::from("/custom/xdg/flint/config.toml"));
    }

    #[test]
    fn user_config_falls_back_to_home_dot_config() {
        let p = resolve_user_config_path(None, Some("/home/alice")).unwrap();
        assert_eq!(p, PathBuf::from("/home/alice/.config/flint/config.toml"));
    }

    #[test]
    fn user_config_empty_xdg_is_treated_as_unset() {
        // XDG basedir spec: empty value falls back to $HOME/.config rather
        // than resolving to a bare `flint/config.toml` in the cwd.
        let p = resolve_user_config_path(Some(""), Some("/home/alice")).unwrap();
        assert_eq!(p, PathBuf::from("/home/alice/.config/flint/config.toml"));
    }

    #[test]
    fn user_config_returns_none_when_no_env_available() {
        // Daemonized / minimal-container case — no usable path, caller
        // must skip the user-level fallback entirely.
        assert!(resolve_user_config_path(None, None).is_none());
        assert!(resolve_user_config_path(Some(""), None).is_none());
    }
}
