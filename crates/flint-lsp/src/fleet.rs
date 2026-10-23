//! Fleet server integration for live validation and completions.
//!
//! Wraps `fleetctl` CLI to provide:
//! - GitOps dry-run validation (Layer 2 diagnostics)
//! - Live resource fetching for completions (labels, teams, queries)

use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tempfile::TempDir;

use flint_lint::fleet_config::Label;

/// Wrapper for fleetctl CLI operations.
///
/// Manages a temporary config file with credentials and provides
/// methods for gitops validation and resource fetching.
pub struct FleetConnection {
    #[expect(
        dead_code,
        reason = "stored for future use in connection status/display"
    )]
    url: String,
    /// Keep the temp directory alive for the lifetime of the connection
    _config_dir: TempDir,
    config_path: PathBuf,
    /// Path to the fleetctl binary.
    fleetctl_bin: String,
    /// Extra environment variables to pass to fleetctl (e.g. FLEET_URL for $VAR expansion in YAML).
    extra_env: Vec<(String, String)>,
}

/// A single error from gitops dry-run output.
#[derive(Debug, Clone)]
pub struct GitOpsError {
    pub message: String,
    /// Actionable hint explaining the likely cause and how to fix it.
    pub hint: Option<String>,
    /// Whether this is a known false-positive from fleetctl.
    pub noise: bool,
}

/// Result of a gitops dry-run validation.
#[derive(Debug, Default)]
pub struct GitOpsReport {
    pub success: bool,
    pub errors: Vec<GitOpsError>,
    pub summary: String,
}

/// Cached resource names from a Fleet instance for completions.
#[derive(Debug, Clone)]
pub struct ResourceCache {
    pub labels: Vec<String>,
    /// Full label structs from the Fleet server, used for block snippet completions.
    pub label_details: Vec<Label>,
    pub fleets: Vec<String>,
    pub reports: Vec<String>,
    pub last_fetched: Instant,
    pub refresh_interval: Duration,
}

impl Default for ResourceCache {
    fn default() -> Self {
        Self {
            labels: Vec::new(),
            label_details: Vec::new(),
            fleets: Vec::new(),
            reports: Vec::new(),
            last_fetched: Instant::now(),
            refresh_interval: Duration::from_secs(300), // 5 minutes
        }
    }
}

impl ResourceCache {
    /// Check if the cache is stale and needs refresh.
    pub fn is_stale(&self) -> bool {
        self.last_fetched.elapsed() > self.refresh_interval
    }
}

impl FleetConnection {
    /// Create a new FleetConnection with the given credentials.
    ///
    /// Creates a temporary fleetctl config file (mode 0o600) that
    /// lives as long as this connection.
    pub fn new(url: &str, token: &str) -> Result<Self> {
        Self::with_options(url, token, "fleetctl", Vec::new())
    }

    /// Create a new FleetConnection with a custom fleetctl binary path.
    pub fn with_fleetctl(url: &str, token: &str, fleetctl_bin: &str) -> Result<Self> {
        Self::with_options(url, token, fleetctl_bin, Vec::new())
    }

    /// Create a new FleetConnection with full options.
    ///
    /// `extra_env` is passed to every fleetctl invocation — use it for
    /// variables referenced in gitops YAML (e.g. `$FLEET_URL`).
    pub fn with_options(
        url: &str,
        token: &str,
        fleetctl_bin: &str,
        extra_env: Vec<(String, String)>,
    ) -> Result<Self> {
        let config_dir = TempDir::new().context("Failed to create temp config directory")?;
        let config_path = config_dir.path().join("config");

        let config_content = format!(
            r#"contexts:
  default:
    address: {url}
    token: {token}
    tls-skip-verify: false
"#
        );

        {
            let mut file =
                std::fs::File::create(&config_path).context("Failed to create fleetctl config")?;
            file.write_all(config_content.as_bytes())
                .context("Failed to write fleetctl config")?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = file.metadata()?.permissions();
                perms.set_mode(0o600);
                file.set_permissions(perms)?;
            }
        }

        Ok(Self {
            url: url.to_string(),
            _config_dir: config_dir,
            config_path,
            fleetctl_bin: fleetctl_bin.to_string(),
            extra_env,
        })
    }

    /// Run a fleetctl command and return stdout.
    fn run_command(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new(&self.fleetctl_bin);
        cmd.env("CONFIG", &self.config_path);
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        let output = cmd
            .args(args)
            .output()
            .context("Failed to run fleetctl — is it installed?")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            anyhow::bail!("fleetctl {} failed:\n{stderr}", args.join(" "));
        }

        Ok(stdout)
    }

    /// Test the connection to the Fleet server.
    pub fn test_connection(&self) -> Result<()> {
        self.run_command(&["get", "config"])
            .context("Failed to connect to Fleet server")?;
        Ok(())
    }

    /// Run gitops dry-run validation against a file.
    ///
    /// This is a blocking call — use `spawn_blocking` when calling from async context.
    pub fn gitops_dry_run(&self, file: &Path) -> Result<GitOpsReport> {
        let mut cmd = Command::new(&self.fleetctl_bin);
        cmd.env("CONFIG", &self.config_path);
        for (k, v) in &self.extra_env {
            cmd.env(k, v);
        }
        let output = cmd
            .args(["gitops", "--dry-run", "-f"])
            .arg(file)
            .output()
            .context("Failed to run fleetctl gitops --dry-run")?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let combined = format!("{stdout}\n{stderr}");

        Ok(GitOpsReport::from_output(
            &combined,
            output.status.success(),
        ))
    }

    /// Fetch label names from the Fleet instance.
    pub fn get_labels(&self) -> Result<Vec<String>> {
        let output = self.run_command(&["get", "labels", "--yaml"])?;
        Ok(parse_names_from_yaml(&output))
    }

    /// Fetch full label data from the Fleet instance.
    ///
    /// # Errors
    ///
    /// Returns an error if `fleetctl get labels --yaml` fails (e.g. connection
    /// refused, auth failure, binary not found).
    pub fn get_label_details(&self) -> Result<Vec<Label>> {
        let output = self.run_command(&["get", "labels", "--yaml"])?;
        Ok(parse_labels_from_yaml(&output))
    }

    /// Fetch fleet (team) names from the Fleet instance.
    pub fn get_fleets(&self) -> Result<Vec<String>> {
        // Server API still uses "teams" endpoint
        let output = self.run_command(&["get", "teams", "--yaml"])?;
        Ok(parse_names_from_yaml(&output))
    }

    /// Fetch report (query) names from the Fleet instance.
    ///
    /// Uses the `get queries` fleetctl endpoint (Fleet API still uses the old name).
    pub fn get_reports(&self) -> Result<Vec<String>> {
        let output = self.run_command(&["get", "queries", "--yaml"])?;
        Ok(parse_names_from_yaml(&output))
    }

    /// Run `fleetctl generate-gitops` to export server config into a gitops repo structure.
    pub fn generate_gitops(
        &self,
        dir: &Path,
        team: Option<&str>,
        force: bool,
        print: bool,
    ) -> Result<String> {
        let dir_str = dir.to_string_lossy().to_string();
        let mut args: Vec<&str> = vec!["generate-gitops", "--dir", &dir_str];

        let team_owned: String;
        if let Some(team_name) = team {
            team_owned = team_name.to_string();
            args.push("--team");
            args.push(&team_owned);
        }

        if force {
            args.push("--force");
        }

        if print {
            args.push("--print");
        }

        self.run_command(&args)
    }

    /// Refresh the resource cache by fetching all resources.
    ///
    /// Returns a new cache. Errors in individual fetches are logged
    /// but don't fail the whole refresh — partial data is better than none.
    pub fn refresh_cache(&self) -> ResourceCache {
        let label_details = self.get_label_details().unwrap_or_default();
        let labels: Vec<String> = label_details
            .iter()
            .filter_map(|l| l.name.clone())
            .collect();
        let fleets = self.get_fleets().unwrap_or_default();
        let reports = self.get_reports().unwrap_or_default();

        ResourceCache {
            labels,
            label_details,
            fleets,
            reports,
            last_fetched: Instant::now(),
            refresh_interval: Duration::from_secs(300),
        }
    }
}

/// Match common fleetctl error patterns and return an actionable hint.
fn hint_for_error(message: &str) -> Option<String> {
    let lower = message.to_lowercase();

    // Permission / auth errors
    if lower.contains("403") || lower.contains("forbidden") {
        return Some(
            "The API token lacks permission for this operation. \
             Check that the token has the correct role (GitOps or Admin) \
             and hasn't expired."
                .to_string(),
        );
    }
    if lower.contains("401") || lower.contains("unauthorized") {
        return Some(
            "Authentication failed. The API token may be invalid or expired. \
             Regenerate it in Fleet → Settings → API tokens."
                .to_string(),
        );
    }

    // Connection errors
    if lower.contains("connection refused") || lower.contains("no such host") {
        return Some(
            "Cannot reach the Fleet server. Check the URL in [fleet] and \
             that the server is running."
                .to_string(),
        );
    }
    if lower.contains("tls") || lower.contains("certificate") {
        return Some(
            "TLS/certificate error. If using a self-signed cert, \
             set `tls-skip-verify: true` in fleetctl config."
                .to_string(),
        );
    }
    if lower.contains("timeout") {
        return Some(
            "Request timed out. The Fleet server may be overloaded or unreachable.".to_string(),
        );
    }

    // Duplicate / conflict errors
    if lower.contains("duplicate") {
        return Some(
            "A resource with this name already exists. \
             Rename it or remove the duplicate definition."
                .to_string(),
        );
    }

    // Unknown team/resource
    if lower.contains("unknown team") || lower.contains("team not found") {
        return Some(
            "This team doesn't exist on the Fleet server. \
             Create it first or check the spelling."
                .to_string(),
        );
    }

    // YAML parse errors
    if lower.contains("yaml") && (lower.contains("unmarshal") || lower.contains("parse")) {
        return Some(
            "The YAML couldn't be parsed by the Fleet server. \
             Check for syntax errors or unsupported fields."
                .to_string(),
        );
    }

    // Missing env var references
    if lower.contains("environment variable") || lower.contains("env var") {
        return Some(
            "A referenced environment variable is not set. \
             Add it to [fleet.env] in .fleetlint.toml."
                .to_string(),
        );
    }

    None
}

/// Known false-positive patterns from fleetctl that aren't actual config errors.
///
/// These are fleetctl bugs or endpoint issues that fire even when the gitops
/// YAML is correct. We still show them as warnings but don't count them as errors.
fn is_noise(message: &str) -> bool {
    let lower = message.to_lowercase();

    // fleetctl tries to manage EULA even when none is configured
    if lower.contains("eula") && (lower.contains("403") || lower.contains("404")) {
        return true;
    }

    // fleetctl tries to manage setup experience on servers without premium
    if lower.contains("setup_experience") && lower.contains("403") {
        return true;
    }

    false
}

impl GitOpsReport {
    /// Parse fleetctl gitops output into a report.
    pub fn from_output(output: &str, success: bool) -> Self {
        let mut report = Self {
            success,
            ..Default::default()
        };

        let mut changes: Vec<String> = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Collect change summary lines
            if line.starts_with("[+]") || line.starts_with("[-]") || line.starts_with("[~]") {
                changes.push(line.to_string());
            }

            // Collect error lines
            if line.to_lowercase().contains("error") && !line.starts_with("[") {
                report.errors.push(GitOpsError {
                    noise: is_noise(line),
                    hint: hint_for_error(line),
                    message: line.to_string(),
                });
            }
        }

        // If the command failed and we found no explicit error lines,
        // treat the entire output as the error
        if !success && report.errors.is_empty() {
            let trimmed = output.trim();
            if !trimmed.is_empty() {
                report.errors.push(GitOpsError {
                    noise: is_noise(trimmed),
                    hint: hint_for_error(trimmed),
                    message: trimmed.to_string(),
                });
            }
        }

        // If all errors are noise, treat the run as successful
        let real_errors: Vec<_> = report.errors.iter().filter(|e| !e.noise).collect();
        let noise_count = report.errors.iter().filter(|e| e.noise).count();

        if !success && real_errors.is_empty() && noise_count > 0 {
            report.success = true;
        }

        // Build summary
        if report.success && changes.is_empty() {
            if noise_count > 0 {
                report.summary =
                    format!("Passed ({} known fleetctl issue(s) ignored)", noise_count);
            } else {
                report.summary = "No changes detected".to_string();
            }
        } else if report.success {
            report.summary = format!("{} change(s) would be applied", changes.len());
        } else {
            report.summary = format!("{} error(s)", real_errors.len());
        }

        report
    }
}

/// Wrapper for a single `fleetctl get labels --yaml` document.
///
/// Each document has `apiVersion`, `kind`, and a `spec` that maps to our `Label`.
#[derive(Debug, Deserialize)]
struct FleetctlDocument {
    spec: Label,
}

/// Parse full label data from `fleetctl get labels --yaml` output.
///
/// The output is multi-document YAML (separated by `---`). Each document
/// has the shape `{ apiVersion, kind, spec: { name, description, ... } }`.
///
/// Documents that fail to deserialize (e.g. unexpected schema) are silently
/// skipped so that partial results are still usable.
fn parse_labels_from_yaml(yaml: &str) -> Vec<Label> {
    let mut labels = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(yaml) {
        if let Ok(wrapper) = FleetctlDocument::deserialize(doc) {
            labels.push(wrapper.spec);
        }
    }
    labels
}

/// Parse resource names from multi-document YAML output.
///
/// Looks for `name:` fields in the YAML documents returned by fleetctl.
fn parse_names_from_yaml(yaml: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name:") {
            if let Some(name) = trimmed.strip_prefix("name:") {
                let name = name.trim().trim_matches('"').trim_matches('\'');
                if !name.is_empty() {
                    names.push(name.to_string());
                }
            }
        }
    }
    names
}

/// Thread-safe handle to an optional Fleet connection + resource cache.
///
/// Used by the LSP backend to share the connection across async handlers.
pub type SharedFleetConnection = Arc<RwLock<Option<FleetConnection>>>;
pub type SharedResourceCache = Arc<RwLock<Option<ResourceCache>>>;

/// Find the gitops root file (default.yml or the team's config file)
/// starting from a given file path and walking up.
pub fn find_gitops_root(file_path: &Path) -> Option<PathBuf> {
    let mut current = if file_path.is_file() {
        file_path.parent()?.to_path_buf()
    } else {
        file_path.to_path_buf()
    };

    loop {
        // Check for default.yml (standard gitops root)
        let default_yml = current.join("default.yml");
        if default_yml.exists() {
            return Some(default_yml);
        }

        // Check for any .yml file that looks like a team config
        if let Ok(entries) = std::fs::read_dir(&current) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "yml").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let first_lines: String =
                            content.lines().take(5).collect::<Vec<_>>().join("\n");
                        if first_lines.contains("name:")
                            && (first_lines.contains("policies:")
                                || first_lines.contains("queries:")
                                || first_lines.contains("reports:")
                                || first_lines.contains("controls:"))
                        {
                            return Some(path);
                        }
                    }
                }
            }
        }

        current = current.parent()?.to_path_buf();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gitops_report_success() {
        let output = r#"[+] would create label "New Label"
[~] would update query "Existing Query"
"#;
        let report = GitOpsReport::from_output(output, true);
        assert!(report.success);
        assert!(report.errors.is_empty());
        assert!(report.summary.contains("2 change(s)"));
    }

    #[test]
    fn test_gitops_report_with_errors() {
        let output = r#"Error: duplicate policy name "Disk Encryption"
Error: unknown team "nonexistent"
"#;
        let report = GitOpsReport::from_output(output, false);
        assert!(!report.success);
        assert_eq!(report.errors.len(), 2);
        assert!(report.errors[0].message.contains("duplicate policy"));
        assert!(report.errors[0].hint.as_ref().unwrap().contains("Rename"));
        assert!(report.errors[1].message.contains("unknown team"));
        assert!(report.errors[1]
            .hint
            .as_ref()
            .unwrap()
            .contains("doesn't exist"));
    }

    #[test]
    fn test_gitops_error_hints() {
        // 401 unauthorized → expired token
        let output = "Error: 401 unauthorized";
        let report = GitOpsReport::from_output(output, false);
        assert!(report.errors[0].hint.as_ref().unwrap().contains("expired"));
        assert!(!report.errors[0].noise);

        // Connection refused
        let output = "Error: connection refused";
        let report = GitOpsReport::from_output(output, false);
        assert!(report.errors[0].hint.as_ref().unwrap().contains("reach"));

        // No hint for generic error
        let output = "Error: something unexpected happened";
        let report = GitOpsReport::from_output(output, false);
        assert!(report.errors[0].hint.is_none());
    }

    #[test]
    fn test_gitops_noise_filtered() {
        // EULA 403 is noise — report should pass
        let output = "Error: error deleting EULA: getting eula metadata: GET /api/latest/fleet/setup_experience/eula/metadata received status 403 forbidden: forbidden";
        let report = GitOpsReport::from_output(output, false);
        assert!(report.success, "EULA 403 should be treated as noise");
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].noise);
        assert!(report.summary.contains("ignored"));

        // Real error mixed with noise — report should fail
        let output = "Error: error deleting EULA: received status 403 forbidden\nError: duplicate policy name \"test\"";
        let report = GitOpsReport::from_output(output, false);
        assert!(!report.success, "Real error should still fail");
        assert_eq!(report.errors.len(), 2);
        assert!(report.errors[0].noise);
        assert!(!report.errors[1].noise);
    }

    #[test]
    fn test_gitops_report_empty_success() {
        let report = GitOpsReport::from_output("", true);
        assert!(report.success);
        assert!(report.errors.is_empty());
        assert_eq!(report.summary, "No changes detected");
    }

    #[test]
    fn test_gitops_report_failure_no_error_keyword() {
        let output = "invalid YAML in file default.yml";
        let report = GitOpsReport::from_output(output, false);
        assert!(!report.success);
        assert_eq!(report.errors.len(), 1);
    }

    #[test]
    fn test_parse_names_from_yaml() {
        let yaml = r#"---
apiVersion: v1
kind: label
spec:
  name: "Production"
---
apiVersion: v1
kind: label
spec:
  name: "Staging"
"#;
        let names = parse_names_from_yaml(yaml);
        assert_eq!(names, vec!["Production", "Staging"]);
    }

    #[test]
    fn test_parse_labels_from_yaml_full() {
        let yaml = r#"---
apiVersion: v1
kind: label
spec:
  name: "Production"
  description: "Production hosts"
  query: "SELECT 1 FROM os_version WHERE major >= 14;"
  platform: "darwin"
  label_membership_type: "dynamic"
---
apiVersion: v1
kind: label
spec:
  name: "Staging"
  description: "Staging environment"
  query: "SELECT 1;"
  platform: ""
  label_membership_type: "manual"
"#;
        let labels = parse_labels_from_yaml(yaml);
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].name.as_deref(), Some("Production"));
        assert_eq!(labels[0].description.as_deref(), Some("Production hosts"));
        assert_eq!(
            labels[0].query.as_deref(),
            Some("SELECT 1 FROM os_version WHERE major >= 14;")
        );
        assert_eq!(labels[0].platform.as_deref(), Some("darwin"));
        assert_eq!(labels[0].label_membership_type.as_deref(), Some("dynamic"));
        assert_eq!(labels[1].name.as_deref(), Some("Staging"));
        assert_eq!(labels[1].label_membership_type.as_deref(), Some("manual"));
    }

    #[test]
    fn test_parse_labels_from_yaml_partial_fields() {
        // Label with only name — other fields should be None
        let yaml = r#"---
apiVersion: v1
kind: label
spec:
  name: "Minimal"
"#;
        let labels = parse_labels_from_yaml(yaml);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].name.as_deref(), Some("Minimal"));
        assert!(labels[0].description.is_none());
        assert!(labels[0].query.is_none());
    }

    #[test]
    fn test_parse_labels_from_yaml_empty() {
        let labels = parse_labels_from_yaml("");
        assert!(labels.is_empty());
    }

    #[test]
    fn test_resource_cache_staleness() {
        let mut cache = ResourceCache::default();
        assert!(!cache.is_stale());

        // Simulate stale cache
        cache.last_fetched = Instant::now() - Duration::from_secs(600);
        assert!(cache.is_stale());
    }

    #[test]
    fn test_find_gitops_root() {
        use tempfile::TempDir;

        let temp = TempDir::new().unwrap();
        let fleets_dir = temp.path().join("fleets").join("engineering");
        std::fs::create_dir_all(&fleets_dir).unwrap();

        // Create a default.yml at root
        std::fs::write(
            temp.path().join("default.yml"),
            "name: Global\npolicies:\n  - name: test",
        )
        .unwrap();

        // Create a fleet file
        let fleet_file = fleets_dir.join("workstations.yml");
        std::fs::write(&fleet_file, "software:\n  packages:\n    - path: foo.yml").unwrap();

        // From a file inside fleets/, should find default.yml at root
        let root = find_gitops_root(&fleet_file);
        assert!(root.is_some());
    }
}
