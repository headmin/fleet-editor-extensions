//! `flint fleet` — read-only views of a Fleet instance.
//!
//! Read-only is structural: [`FleetClient`] exposes `get` and nothing else —
//! there is no code path that can mutate the instance. Connection settings
//! come from the shared `[fleet]` config (repo `.fleetlint.toml` →
//! `~/.config/flint/config.toml`) with `FLEET_URL`/`FLEET_API_TOKEN` env
//! fallback (a `./.env` file is loaded into the process env first) and
//! `op://` secret references resolved via the 1Password CLI. The token is
//! never printed.

use crate::args::FleetKind;
use anyhow::{Context, Result};
use colored::Colorize;
use flint_lint::FleetLintConfig;

pub(crate) fn run(what: FleetKind) -> Result<()> {
    // Doctor runs BEFORE the client is built: constructing it is one of the
    // steps that can hang, so it has to be timed rather than awaited.
    if matches!(what, FleetKind::Doctor) {
        return doctor();
    }
    let client = FleetClient::from_environment()?;
    match what {
        FleetKind::Doctor => unreachable!("handled above, before the client exists"),
        FleetKind::Status => status(&client),
        FleetKind::Software { team, available, json } => software(&client, team, available, json),
        FleetKind::Fma { json } => fma(&client, json),
        FleetKind::Labels { json } => labels(&client, json),
        FleetKind::Teams { json } => teams(&client, json),
        FleetKind::Snapshot { out, stdout } => snapshot(&client, out, stdout),
    }
}

/// GET-only Fleet API client. The absence of any other verb is the
/// read-only guarantee.
pub(crate) struct FleetClient {
    base: String,
    token: String,
}

impl FleetClient {
    /// Bare hostname of the configured server.
    ///
    /// Snapshots record this rather than `base`, because the configured URL
    /// can carry a port, a path, or credentials, and the snapshot is a file
    /// meant to be committed. A hostname is enough to tell two instances
    /// apart and carries nothing sensitive.
    fn host(&self) -> String {
        self.base
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .split('/')
            .next()
            .unwrap_or("")
            .split('@')          // strip any user:pass@ prefix
            .next_back()
            .unwrap_or("")
            .to_string()
    }

    /// Resolve connection settings: `.env` → shared `[fleet]` config
    /// (repo, then user level) → `FLEET_URL`/`FLEET_API_TOKEN` env vars.
    pub(crate) fn from_environment() -> Result<Self> {
        load_dotenv();

        let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
        // Layered: repo [fleet] wins per field, user config fills the rest.
        // A repo .fleetlint.toml with only [files] must not shadow the
        // user-level credentials file.
        let fleet_cfg = FleetLintConfig::resolve_fleet_connection(&cwd);

        let base = fleet_cfg
            .resolved_url()
            .context("no Fleet URL — set [fleet] url in .fleetlint.toml (or ~/.config/flint/config.toml), or FLEET_URL (env / ./.env)")?
            .trim_end_matches('/')
            .to_string();
        let token = fleet_cfg
            .resolved_token()
            .context("no Fleet API token — set [fleet] token (op:// supported), or FLEET_API_TOKEN (env / ./.env)")?;
        Ok(Self { base, token })
    }

    /// The one HTTP verb. Returns the parsed JSON body; auth and API errors
    /// surface with status + Fleet's error message, token redacted.
    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base, path);
        let mut req = ureq::get(&url)
            .set("Authorization", &format!("Bearer {}", self.token))
            .timeout(std::time::Duration::from_secs(20));
        for (k, v) in query {
            req = req.query(k, v);
        }
        match req.call() {
            Ok(resp) => resp
                .into_json()
                .with_context(|| format!("invalid JSON from {path}")),
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                let msg = serde_json::from_str::<serde_json::Value>(&body)
                    .ok()
                    .and_then(|v| v["message"].as_str().map(str::to_string))
                    .unwrap_or_else(|| body.chars().take(200).collect());
                anyhow::bail!("GET {path} → HTTP {code}: {msg}");
            }
            Err(e) => anyhow::bail!("GET {path} failed: {e}"),
        }
    }
}

/// Load `./.env` (KEY=VALUE lines, `#` comments, optional quotes) into the
/// process environment — only keys not already set, so real env wins.
fn load_dotenv() {
    let Ok(content) = std::fs::read_to_string(".env") else {
        return;
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim().trim_start_matches("export ").trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if !key.is_empty() && std::env::var(key).is_err() {
            std::env::set_var(key, value);
        }
    }
}

fn status(client: &FleetClient) -> Result<()> {
    let v = client.get("/api/v1/fleet/version", &[])?;
    println!(
        "{} connected to {}",
        "✓".green(),
        client.base.bold()
    );
    if let Some(ver) = v["version"].as_str() {
        println!("  version: {ver}");
    }
    if let Some(branch) = v["branch"].as_str() {
        println!("  branch:  {branch}");
    }
    Ok(())
}

fn software(client: &FleetClient, team: Option<u32>, available: bool, json: bool) -> Result<()> {
    let mut query: Vec<(&str, String)> = vec![("per_page", "400".into())];
    if let Some(id) = team {
        query.push(("team_id", id.to_string()));
    }
    if available {
        query.push(("available_for_install", "true".into()));
    }
    let v = client.get("/api/v1/fleet/software/titles", &query)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    let titles = v["software_titles"].as_array().cloned().unwrap_or_default();
    for t in &titles {
        let name = t["name"].as_str().unwrap_or("?");
        let version = t["latest_version"]
            .as_str()
            .or_else(|| t["versions"][0]["version"].as_str())
            .unwrap_or("-");
        let pkg = t["software_package"]["name"]
            .as_str()
            .map(|p| format!("  [{p}]"))
            .unwrap_or_default();
        println!("{name}  {}{}", version.dimmed(), pkg.dimmed());
    }
    println!("{}", format!("{} title(s)", titles.len()).dimmed());
    Ok(())
}

fn fma(client: &FleetClient, json: bool) -> Result<()> {
    let v = client.get("/api/v1/fleet/software/fleet_maintained_apps", &[])?;
    if json {
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    let apps = v["fleet_maintained_apps"].as_array().cloned().unwrap_or_default();
    for a in &apps {
        let name = a["name"].as_str().unwrap_or("?");
        let slug = a["slug"].as_str().unwrap_or("-");
        let version = a["version"].as_str().unwrap_or("-");
        println!("{slug}  {}  {}", name.dimmed(), version.dimmed());
    }
    println!("{}", format!("{} app(s) on this instance", apps.len()).dimmed());
    Ok(())
}

fn labels(client: &FleetClient, json: bool) -> Result<()> {
    let v = client.get("/api/v1/fleet/labels", &[])?;
    if json {
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    let labels = v["labels"].as_array().cloned().unwrap_or_default();
    for l in &labels {
        let name = l["name"].as_str().unwrap_or("?");
        let kind = l["label_type"].as_str().unwrap_or("");
        println!("{name}  {}", kind.dimmed());
    }
    println!("{}", format!("{} label(s)", labels.len()).dimmed());
    Ok(())
}

fn teams(client: &FleetClient, json: bool) -> Result<()> {
    let v = client.get("/api/v1/fleet/teams", &[])?;
    if json {
        println!("{}", serde_json::to_string_pretty(&v)?);
        return Ok(());
    }
    let teams = v["teams"].as_array().cloned().unwrap_or_default();
    for t in &teams {
        let name = t["name"].as_str().unwrap_or("?");
        let id = t["id"].as_u64().unwrap_or(0);
        let hosts = t["host_count"].as_u64().unwrap_or(0);
        println!("{id:>4}  {name}  {}", format!("{hosts} host(s)").dimmed());
    }
    println!("{}", format!("{} team(s)", teams.len()).dimmed());
    Ok(())
}

/// Write a `.fleet-snapshot.json` capturing server state the repo cannot know.
///
/// Today this records labels. That single fact converts `label-reference` from
/// a rule that can only ever warn — because a label may legitimately exist
/// server-side and never appear in the repo — into one that can gate.
///
/// Deliberately excluded: anything secret. The file is meant to be committed,
/// so it carries label NAMES, a bare hostname, and a timestamp. No token, no
/// full URL, no host data.
pub(crate) fn snapshot(
    client: &FleetClient,
    out: Option<std::path::PathBuf>,
    to_stdout: bool,
) -> Result<()> {
    use flint_lint::snapshot::{
        Capabilities, FleetSnapshot, Labels, Provenance, Software,
        SNAPSHOT_FILE_NAME,
    };

    // Version is provenance, not a gate — an older server still yields a
    // usable label list, so a failure here must not sink the snapshot.
    let fleet_version = client
        .get("/api/v1/fleet/version", &[])
        .ok()
        .and_then(|v| v["version"].as_str().map(str::to_string))
        .unwrap_or_default();

    let v = client
        .get("/api/v1/fleet/labels", &[])
        .context("fetching labels")?;
    let raw = v["labels"].as_array().cloned().unwrap_or_default();

    let mut builtin = Vec::new();
    let mut custom = Vec::new();
    for l in &raw {
        let Some(name) = l["name"].as_str() else {
            continue;
        };
        // Fleet marks its own labels `builtin`; everything else is the org's.
        // Splitting them keeps a snapshot diff to what humans actually changed.
        if l["label_type"].as_str() == Some("builtin") {
            builtin.push(name.to_string());
        } else {
            custom.push(name.to_string());
        }
    }
    builtin.sort();
    custom.sort();
    // Sorted so re-running produces a byte-identical file when nothing moved:
    // a snapshot that churns on every fetch is one nobody will commit.

    // Capabilities are BEST EFFORT: each endpoint is admin-scoped and may 403
    // on a reader token, or 404 on a server without MDM. A snapshot without
    // them is still useful for labels, so a failure here degrades the file
    // rather than failing the command.
    let cfg = client.get("/api/v1/fleet/config", &[]).ok();
    let mut caps = Capabilities {
        premium: cfg
            .as_ref()
            .and_then(|v| v["license"]["tier"].as_str().map(|t| t == "premium"))
            .unwrap_or(false),
        mdm_enabled: cfg
            .as_ref()
            .and_then(|v| v["mdm"]["enabled_and_configured"].as_bool())
            .unwrap_or(false),
        ..Default::default()
    };
    if let Ok(v) = client.get("/api/latest/fleet/vpp_tokens", &[]) {
        if let Some(arr) = v["vpp_tokens"].as_array() {
            caps.vpp_locations = arr
                .iter()
                .filter_map(|t| t["location"].as_str().map(str::to_string))
                .collect();
            caps.vpp_locations.sort();
        }
    }
    if let Ok(v) = client.get("/api/latest/fleet/abm_tokens", &[]) {
        if let Some(arr) = v["abm_tokens"].as_array() {
            caps.abm_org_names = arr
                .iter()
                .filter_map(|t| t["org_name"].as_str().map(str::to_string))
                .collect();
            caps.abm_org_names.sort();
        }
    }

    // Software + scripts, best effort.
    //
    // Installer hashes are TEAM-SCOPED and only returned for titles
    // `available_for_install`: a global query returns zero. And the hash sits
    // at the TOP LEVEL of the title (SoftwareTitleListResult.HashSHA256,
    // `omitempty`), NOT inside `software_package` — which is where the first
    // version of this looked, so it silently collected nothing against a real
    // instance while looking perfectly reasonable.
    let mut software = Software::default();
    let team_ids: Vec<u64> = client
        .get("/api/v1/fleet/teams", &[])
        .ok()
        .and_then(|v| v["teams"].as_array().cloned())
        .map(|ts| ts.iter().filter_map(|t| t["id"].as_u64()).collect())
        .unwrap_or_default();

    // Team 0 = "no team"; the rest come from the instance.
    for team in std::iter::once(0u64).chain(team_ids) {
        let q = [
            ("team_id", team.to_string()),
            ("available_for_install", "true".to_string()),
            ("per_page", "500".to_string()),
        ];
        let Ok(v) = client.get("/api/v1/fleet/software/titles", &q) else {
            continue;
        };
        let Some(arr) = v["software_titles"].as_array() else {
            continue;
        };
        for t in arr {
            if let Some(n) = t["name"].as_str() {
                software.titles.push(n.to_string());
            }
            if let Some(h) = t["hash_sha256"].as_str() {
                software.hashes.push(h.to_ascii_lowercase());
            }
        }
    }
    if let Ok(v) = client.get("/api/v1/fleet/software/fleet_maintained_apps", &[]) {
        if let Some(arr) = v["fleet_maintained_apps"].as_array() {
            software.fma_slugs = arr
                .iter()
                .filter_map(|a| a["slug"].as_str().map(str::to_string))
                .collect();
        }
    }
    software.titles.sort();
    software.titles.dedup();
    software.hashes.sort();
    software.hashes.dedup();
    software.fma_slugs.sort();
    software.fma_slugs.dedup();

    let snap = FleetSnapshot {
        schema: 1,
        provenance: Provenance {
            fetched_at: utc_now_rfc3339(),
            fleet_version,
            server: client.host(),
            fetched_by: format!("flint fleet snapshot {}", env!("CARGO_PKG_VERSION")),
        },
        labels: Labels { builtin, custom },
        capabilities: caps,
        software,
    };

    let json = serde_json::to_string_pretty(&snap)? + "\n";
    if to_stdout {
        print!("{json}");
        return Ok(());
    }

    let path = out.unwrap_or_else(|| std::path::PathBuf::from(SNAPSHOT_FILE_NAME));
    std::fs::write(&path, &json)
        .with_context(|| format!("writing {}", path.display()))?;

    println!(
        "{} Wrote {} ({} built-in, {} custom label(s))",
        "✓".green().bold(),
        path.display().to_string().bold(),
        snap.labels.builtin.len(),
        snap.labels.custom.len()
    );
    if !snap.software.hashes.is_empty() {
        println!(
            "  {}",
            format!(
                "{} installer hash(es), {} title(s), {} FMA slug(s)",
                snap.software.hashes.len(),
                snap.software.titles.len(),
                snap.software.fma_slugs.len()
            )
            .dimmed()
        );
    }
    if !snap.capabilities.vpp_locations.is_empty() || !snap.capabilities.abm_org_names.is_empty() {
        println!(
            "  {}",
            format!(
                "{} VPP location(s), {} ABM org(s)",
                snap.capabilities.vpp_locations.len(),
                snap.capabilities.abm_org_names.len()
            )
            .dimmed()
        );
    }
    // Deliberately NOT "commit it".
    //
    // The file carries no credentials, but it does carry an inventory of the
    // organization: every label name, software title and VPP organization
    // unit on the instance. Whether that belongs in a repo is the owner's
    // call, not the tool's — a shared GitOps repo and a public one are very
    // different answers, and flint cannot tell which it is looking at.
    //
    // Both modes work identically. Committed, the whole team and CI gate on
    // the same server facts. Gitignored, the developer who fetched it gets
    // the precision locally and everyone else keeps today's warnings. The
    // ONLY thing that changes is who benefits.
    println!(
        "  {}",
        "Keep it local (add to .gitignore) or commit it to share the \
         precision with your team and CI — both work. flint gates while it \
         is fresh and degrades to warnings once stale."
            .dimmed()
    );
    println!(
        "  {}",
        "Contains label/software/VPP names from the instance — no \
         credentials, but it is an inventory: review before sharing."
            .dimmed()
    );
    Ok(())
}

/// Current UTC time as RFC3339, without pulling in a date-time crate.
///
/// Only whole seconds are needed — the consumer asks "how many days ago".
fn utc_now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    // Civil-from-days (Howard Hinnant, public domain) — exact on leap years.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Step-by-step connection diagnosis.
///
/// Exists because a hang gives you nothing to go on. The language server does
/// this same sequence while starting, and when one step blocks the editor just
/// sits at `initialize` with no message. Printing each step as it finishes
/// turns "it hangs" into "it hangs on step 4", without asking anyone to attach
/// a sampler.
///
/// Every line is flushed immediately — buffered output would defeat the whole
/// purpose, since the interesting case is the step that never returns.
fn doctor() -> Result<()> {
    use colored::Colorize;
    use std::io::Write;
    use std::time::Instant;

    fn step(n: u8, what: &str) -> Instant {
        print!("  {n}. {what} … ");
        let _ = std::io::stdout().flush();
        Instant::now()
    }
    fn done(t: Instant, outcome: &str) {
        println!("{outcome} {}", format!("({} ms)", t.elapsed().as_millis()).dimmed());
        let _ = std::io::stdout().flush();
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    println!("{} connection diagnosis — {}\n", "🩺".blue(), cwd.display());

    let t = step(1, "locating a config file");
    match FleetLintConfig::find_and_load(&cwd) {
        Some((path, _)) => done(t, &format!("{} {}", "found".green(), path.display())),
        None => done(
            t,
            &format!(
                "{} — using defaults (if a config exists here it failed to parse; the \
                 warning above says why)",
                "none".yellow()
            ),
        ),
    }

    let t = step(2, "reading [fleet] settings");
    let fleet_cfg = FleetLintConfig::resolve_fleet_connection(&cwd);
    done(
        t,
        &format!(
            "gitops_validation={} live_completions={}",
            fleet_cfg.gitops_validation, fleet_cfg.live_completions
        ),
    );

    if !fleet_cfg.gitops_validation && !fleet_cfg.live_completions {
        println!(
            "\n{} both server-backed features are off, so the language server \
             never opens a connection. Nothing further to check.",
            "✓".green()
        );
        return Ok(());
    }

    // Steps 3 and 4 are the ones that shell out to `op` for an `op://` value.
    // A slow result here is the answer: the same call runs during LSP startup.
    let t = step(3, "resolving the Fleet URL");
    let url = fleet_cfg.resolved_url();
    match &url {
        Some(u) => done(t, &format!("{} {u}", "ok".green())),
        None => {
            done(t, &format!("{}", "not set".red()));
            println!(
                "\n{} no Fleet URL. Set `[fleet] url`, or FLEET_URL in the environment.",
                "✗".red()
            );
            return Ok(());
        }
    }

    let t = step(4, "resolving the API token");
    let token = fleet_cfg.resolved_token();
    match &token {
        Some(_) => done(t, &format!("{} (value not shown)", "ok".green())),
        None => {
            done(t, &format!("{}", "not set".red()));
            println!(
                "\n{} no API token. Set `[fleet] token`, or FLEET_API_TOKEN.",
                "✗".red()
            );
            return Ok(());
        }
    }

    let t = step(5, "contacting the server");
    match FleetClient::from_environment() {
        Ok(client) => match client.get("/api/v1/fleet/version", &[]) {
            Ok(v) => {
                let ver = v["version"].as_str().unwrap_or("unknown");
                done(t, &format!("{} Fleet {ver}", "ok".green()));
                println!("\n{} all steps completed.", "✓".green());
            }
            Err(e) => {
                done(t, &format!("{}", "failed".red()));
                println!("\n{} reachable but the request failed: {e}", "✗".red());
            }
        },
        Err(e) => {
            done(t, &format!("{}", "failed".red()));
            println!("\n{} could not build a client: {e}", "✗".red());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotenv_parser_handles_quotes_comments_and_export() {
        let dir = std::env::temp_dir().join(format!("flint-dotenv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(".env"),
            "# comment\nexport FLINT_TEST_DOTENV_A=\"quoted value\"\nFLINT_TEST_DOTENV_B=plain\n\nnot a pair\n",
        )
        .unwrap();
        let old = std::env::current_dir().unwrap();
        std::env::set_current_dir(&dir).unwrap();
        load_dotenv();
        std::env::set_current_dir(old).unwrap();

        assert_eq!(std::env::var("FLINT_TEST_DOTENV_A").unwrap(), "quoted value");
        assert_eq!(std::env::var("FLINT_TEST_DOTENV_B").unwrap(), "plain");
        std::env::remove_var("FLINT_TEST_DOTENV_A");
        std::env::remove_var("FLINT_TEST_DOTENV_B");
        std::fs::remove_dir_all(&dir).ok();
    }
}
