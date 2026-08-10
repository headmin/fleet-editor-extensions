//! Fleet Maintained Apps (FMA) — slug registry, search, and the `fma-slug`
//! lint rule.
//!
//! The registry is the single source of truth for FMA slug knowledge across
//! every flint face: the lint rule validates `slug:` /
//! `fleet_maintained_app_slug:` values offline, the LSP builds completions
//! from it, and `flint fma` searches it. A bundled snapshot
//! (`data/fma-registry.toml`) ships with the binary; a cache file written by
//! `flint fma refresh` (from the fmalibrary.com feed) overlays newer entries
//! — this module only ever READS that file, keeping the engine network-free.

use super::error::{Fix, FixSafety, LintError, Span};
use super::fleet_config::FleetConfig;
use super::rules::Rule;
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::path::{Path, PathBuf};

const REGISTRY_TOML: &str = include_str!("../data/fma-registry.toml");

/// One Fleet Maintained App: a name plus the platforms Fleet builds it for.
/// Slug form is `{name}/{platform}` (e.g. `slack/darwin`).
#[derive(Debug, Clone, Deserialize)]
pub struct FmaApp {
    pub name: String,
    pub platforms: Vec<String>,
    /// Latest version seen (cache overlay only — the bundled snapshot has none).
    #[serde(default)]
    pub latest_version: Option<String>,
    /// Installer URL for the latest version (cache overlay only).
    #[serde(default)]
    pub installer_url: Option<String>,
    /// RFC-2822 date of the last feed update (cache overlay only).
    #[serde(default)]
    pub updated: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[serde(default)]
    fma: Vec<FmaApp>,
}

/// Path of the refreshable cache written by `flint fma refresh`:
/// `$XDG_CACHE_HOME/flint/fma-cache.toml`, else `~/.cache/flint/fma-cache.toml`.
pub fn cache_path() -> Option<PathBuf> {
    let base = std::env::var("XDG_CACHE_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".cache"))
        })?;
    Some(base.join("flint").join("fma-cache.toml"))
}

/// The effective registry: bundled snapshot with the cache overlay merged in
/// (cache entries win by app name — they are fresher). Sorted by name.
pub static APPS: Lazy<Vec<FmaApp>> = Lazy::new(|| {
    let bundled: RegistryFile =
        toml::from_str(REGISTRY_TOML).expect("bundled fma-registry.toml is valid");
    let mut apps = bundled.fma;

    if let Some(overlay) = cache_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| toml::from_str::<RegistryFile>(&s).ok())
    {
        for fresh in overlay.fma {
            match apps.iter_mut().find(|a| a.name == fresh.name) {
                Some(existing) => {
                    // Feed entries carry one platform per update — UNION with
                    // the bundled platforms so the other platform's slug stays
                    // valid; version/url/date come from the fresher source.
                    for p in fresh.platforms {
                        if !existing.platforms.contains(&p) {
                            existing.platforms.push(p);
                        }
                    }
                    existing.latest_version = fresh.latest_version.or(existing.latest_version.take());
                    existing.installer_url = fresh.installer_url.or(existing.installer_url.take());
                    existing.updated = fresh.updated.or(existing.updated.take());
                }
                None => apps.push(fresh),
            }
        }
    }

    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps
});

/// All valid slugs (`name/platform`) in the effective registry.
pub fn all_slugs() -> Vec<String> {
    APPS.iter()
        .flat_map(|app| app.platforms.iter().map(move |p| format!("{}/{}", app.name, p)))
        .collect()
}

/// Whether `slug` names a known app/platform pair.
pub fn is_valid_slug(slug: &str) -> bool {
    match slug.rsplit_once('/') {
        Some((name, platform)) => APPS
            .iter()
            .any(|app| app.name == name && app.platforms.iter().any(|p| p == platform)),
        None => false,
    }
}

/// Closest matching slug for typo suggestions: exact (case-insensitive),
/// then prefix, then substring, then Levenshtein on the name part
/// (`slck/darwin` → `slack/darwin`).
pub fn find_similar_slug(input: &str) -> Option<String> {
    let input_lower = input.to_lowercase();
    let all = all_slugs();

    if let Some(s) = all.iter().find(|s| s.to_lowercase() == input_lower) {
        return Some(s.clone());
    }
    if let Some(s) = all
        .iter()
        .find(|s| s.to_lowercase().starts_with(&input_lower))
    {
        return Some(s.clone());
    }
    if let Some(s) = all
        .iter()
        .find(|s| s.to_lowercase().contains(&input_lower))
    {
        return Some(s.clone());
    }

    // Typo tier: edit distance on the app-name part, platform preserved
    // when the input carries one. Distance cap matches structural.rs's
    // did-you-mean heuristic (3 or half the name, whichever is larger).
    let (name_in, platform_in) = match input_lower.rsplit_once('/') {
        Some((n, p)) => (n.to_string(), Some(p.to_string())),
        None => (input_lower.clone(), None),
    };
    let max_dist = 3.max(name_in.len() / 2);
    let best = APPS
        .iter()
        .map(|app| (super::util::levenshtein_distance(&name_in, &app.name), app))
        .filter(|(d, _)| *d <= max_dist)
        .min_by_key(|(d, _)| *d)?
        .1;
    let platform = platform_in
        .filter(|p| best.platforms.iter().any(|bp| bp == p))
        .or_else(|| best.platforms.first().cloned())?;
    Some(format!("{}/{}", best.name, platform))
}

/// Substring search over app names and slugs (for `flint fma search`).
pub fn search(term: &str) -> Vec<&'static FmaApp> {
    let term = term.to_lowercase();
    APPS.iter().filter(|app| app.name.contains(&term)).collect()
}

// ============================================================================
// Rule: fma-slug
// ============================================================================

/// Validates `slug:` and `fleet_maintained_app_slug:` values against the FMA
/// registry — offline, so unknown slugs fail in CI, not first at `fleetctl
/// gitops` time. Warning severity with an Unsafe did-you-mean fix: the
/// bundled snapshot can lag Fleet's own additions, so a plain `--fix` never
/// rewrites a slug automatically (run `flint fma refresh` to update).
pub struct FmaSlugRule;

/// Keys whose string value is an FMA slug.
const SLUG_KEYS: [&str; 2] = ["slug", "fleet_maintained_app_slug"];

impl Rule for FmaSlugRule {
    fn name(&self) -> &'static str {
        super::codes::FMA_SLUG
    }
    fn description(&self) -> &'static str {
        "Validates Fleet Maintained App slugs (slug:, fleet_maintained_app_slug:) against the app registry"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }
    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let Some(yaml) = super::yaml_utils::parse_yaml(source) else {
            return Vec::new();
        };

        let mut errors = Vec::new();
        super::yaml_utils::walk_mappings(&yaml, &mut |map| {
            for key in SLUG_KEYS {
                let Some(serde_yaml::Value::String(value)) =
                    map.get(serde_yaml::Value::String(key.to_string()))
                else {
                    continue;
                };
                let value = value.trim();
                // Env-var indirection and empty values are not checkable.
                if value.is_empty() || value.starts_with('$') {
                    continue;
                }
                if is_valid_slug(value) {
                    continue;
                }

                let suggestion = find_similar_slug(value);
                let mut err = LintError::warning(
                    match &suggestion {
                        Some(s) => format!(
                            "Unknown Fleet Maintained App slug '{value}'. Did you mean '{s}'?"
                        ),
                        None => format!(
                            "Unknown Fleet Maintained App slug '{value}'. Run `flint fma search <name>` to find it, or `flint fma refresh` if it was added recently."
                        ),
                    },
                    file,
                )
                .with_context(value.to_string());
                if let Some(s) = suggestion {
                    err = err.with_fix(Fix::Replace {
                        old: Some(value.to_string()),
                        new: s,
                        // The registry snapshot can lag Fleet — never rewrite
                        // a slug without the user opting in.
                        safety: FixSafety::Unsafe,
                    });
                }
                if let Some(span) = find_slug_span(source, key, value) {
                    err = err.with_span(span);
                }
                errors.push(err);
            }
        });
        errors
    }
}

/// Locate `key: value` in the source, spanning the value.
fn find_slug_span(source: &str, key: &str, value: &str) -> Option<Span> {
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim().trim_start_matches('-').trim_start();
        if let Some(rest) = trimmed.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(rest) = rest.strip_prefix(':') {
                let unquoted = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches('"')
                    .trim_matches('\'');
                if unquoted == value {
                    if let Some(col) = line.find(value) {
                        return Some(Span::token(idx + 1, col + 1, value.len()));
                    }
                    return Some(Span::line(idx + 1));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn check(yaml: &str) -> Vec<LintError> {
        FmaSlugRule.check(&FleetConfig::default(), &PathBuf::from("fleets/t.yml"), yaml)
    }

    #[test]
    fn bundled_registry_loads_and_is_populated() {
        assert!(APPS.len() >= 200, "bundled registry looks truncated");
        assert!(is_valid_slug("slack/darwin"));
        assert!(is_valid_slug("1password/darwin"));
        assert!(!is_valid_slug("slack"));
        assert!(!is_valid_slug("not-a-real-app/darwin"));
    }

    #[test]
    fn registry_has_no_duplicate_names_and_valid_platforms() {
        let mut seen = std::collections::HashSet::new();
        for app in APPS.iter() {
            assert!(seen.insert(&app.name), "Duplicate FMA entry: {}", app.name);
            for platform in &app.platforms {
                assert!(
                    ["darwin", "windows"].contains(&platform.as_str()),
                    "FMA '{}' has invalid platform '{}'",
                    app.name,
                    platform
                );
            }
        }
    }

    #[test]
    fn valid_slugs_pass_both_keys() {
        let yaml = "software:\n  fleet_maintained_apps:\n    - slug: slack/darwin\npolicies:\n  - name: p\n    fleet_maintained_app_slug: 1password/darwin\n";
        assert!(check(yaml).is_empty(), "got: {:?}", check(yaml));
    }

    #[test]
    fn unknown_slug_warns_with_unsafe_suggestion() {
        let yaml = "software:\n  fleet_maintained_apps:\n    - slug: slck/darwin\n";
        let errs = check(yaml);
        assert_eq!(errs.len(), 1);
        let e = &errs[0];
        assert!(e.message.contains("Did you mean 'slack/darwin'"));
        assert_eq!(e.fix_safety(), Some(FixSafety::Unsafe));
        assert_eq!(e.line(), Some(3));
    }

    #[test]
    fn env_vars_and_empty_are_skipped() {
        let yaml = "software:\n  fleet_maintained_apps:\n    - slug: $FMA_SLUG\n    - slug: \"\"\n";
        assert!(check(yaml).is_empty());
    }

    #[test]
    fn inline_comment_does_not_break_matching() {
        // Regression (ported from the LSP scanner this rule replaced): an
        // inline `# comment` after the slug must not break validation, and
        // setup_experience: true is a valid sibling field.
        let yaml = "software:\n  fleet_maintained_apps:\n    - slug: santa/darwin # NorthpoleSec Santa\n      setup_experience: true # OOBE\n";
        assert!(check(yaml).is_empty(), "got: {:?}", check(yaml));
        // A genuinely unknown slug is still flagged, with a clean message.
        let bad = check("software:\n  fleet_maintained_apps:\n    - slug: not-a-real-app/darwin # x\n");
        assert_eq!(bad.len(), 1);
        assert!(bad[0].message.contains("not-a-real-app/darwin"));
        assert!(!bad[0].message.contains('#'));
    }

    #[test]
    fn search_finds_by_substring() {
        let hits = search("slack");
        assert!(hits.iter().any(|a| a.name == "slack"));
    }
}
