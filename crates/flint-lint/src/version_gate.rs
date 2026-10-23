//! Version resolution for deprecation gating.
//!
//! Resolves the target Fleet version from multiple sources in priority order:
//! 1. `.fleetlint.toml` `[deprecations] fleet_version`
//! 2. Auto-detected from YAML structure (via `VersionDetector`)
//! 3. Default: `"latest"` (resolves to highest known version)

use super::version::Version;

/// Resolved version context used by the deprecation rule.
#[derive(Debug, Clone)]
pub struct VersionContext {
    /// The resolved target version.
    pub version: Version,
    /// Where the version came from.
    pub source: VersionSource,
    /// Whether to treat dormant deprecations as active (future naming opt-in).
    pub future_names: bool,
}

/// How the version was determined.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionSource {
    /// Explicitly set in `.fleetlint.toml`.
    Config,
    /// Auto-detected from YAML structure.
    Detected,
    /// Default (latest known version).
    Default,
}

impl VersionContext {
    /// Create a dormant context that will never trigger deprecation warnings.
    /// Uses version 0.0.0 so all deprecations remain in Dormant phase.
    pub fn dormant() -> Self {
        Self {
            version: Version::new(0, 0, 0),
            source: VersionSource::Default,
            future_names: false,
        }
    }

    /// Resolve version context from config string.
    ///
    /// Accepts a version string like `"4.80.0"` or `"latest"`.
    pub fn from_config(version_str: &str) -> Self {
        if version_str == "latest" {
            return Self::latest();
        }

        match Version::parse(version_str) {
            Some(v) => Self {
                version: v,
                source: VersionSource::Config,
                future_names: false,
            },
            None => Self::latest(),
        }
    }

    /// Create a context with the latest known Fleet version: the one the
    /// schema and the oracle are pinned to (tools/gitops-oracle/go.mod).
    pub fn latest() -> Self {
        Self {
            version: Version::new(4, 93, 0),
            source: VersionSource::Default,
            future_names: false,
        }
    }

    /// Resolve version context from available sources.
    ///
    /// Priority:
    /// 1. Config value (if provided and not empty)
    /// 2. Default (latest)
    pub fn resolve(config_version: Option<&str>, future_names: bool) -> Self {
        let mut ctx = if let Some(v) = config_version {
            if !v.is_empty() {
                Self::from_config(v)
            } else {
                Self::latest()
            }
        } else {
            Self::latest()
        };
        ctx.future_names = future_names;
        ctx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dormant_context() {
        let ctx = VersionContext::dormant();
        assert_eq!(ctx.version, Version::new(0, 0, 0));
        assert_eq!(ctx.source, VersionSource::Default);
    }

    #[test]
    fn test_from_config_version() {
        let ctx = VersionContext::from_config("4.80.1");
        assert_eq!(ctx.version, Version::new(4, 80, 1));
        assert_eq!(ctx.source, VersionSource::Config);
    }

    #[test]
    fn test_from_config_latest() {
        let ctx = VersionContext::from_config("latest");
        assert_eq!(ctx.version, VersionContext::latest().version);
        assert_eq!(ctx.source, VersionSource::Default);
    }

    #[test]
    fn test_from_config_invalid() {
        let ctx = VersionContext::from_config("not-a-version");
        // Falls back to latest
        assert_eq!(ctx.version, VersionContext::latest().version);
    }

    #[test]
    fn test_resolve_with_config() {
        let ctx = VersionContext::resolve(Some("4.88.0"), false);
        assert_eq!(ctx.version, Version::new(4, 88, 0));
        assert_eq!(ctx.source, VersionSource::Config);
        assert!(!ctx.future_names);
    }

    #[test]
    fn test_resolve_without_config() {
        let ctx = VersionContext::resolve(None, false);
        assert_eq!(ctx.version, VersionContext::latest().version);
        assert_eq!(ctx.source, VersionSource::Default);
    }

    #[test]
    fn test_resolve_empty_config() {
        let ctx = VersionContext::resolve(Some(""), false);
        assert_eq!(ctx.version, VersionContext::latest().version);
    }

    #[test]
    fn test_resolve_with_future_names() {
        let ctx = VersionContext::resolve(Some("4.80.0"), true);
        assert_eq!(ctx.version, Version::new(4, 80, 0));
        assert!(ctx.future_names);
    }

    /// `latest` must be the Fleet the key snapshot was extracted from, so a
    /// pin bump that forgets one of them fails here.
    #[test]
    fn latest_matches_the_key_snapshot() {
        let snapshot: serde_json::Value =
            serde_json::from_str(include_str!("../data/fleet-gitops-keys.json")).unwrap();
        let pinned = snapshot["fleet_version"].as_str().unwrap().trim_start_matches('v');
        assert_eq!(VersionContext::latest().version, Version::parse(pinned).unwrap());
    }
}
