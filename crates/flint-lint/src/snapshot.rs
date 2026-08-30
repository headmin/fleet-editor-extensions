//! `.fleet-snapshot.json` — a point-in-time record of server state that the
//! repo alone cannot supply.
//!
//! Some questions are unanswerable offline. "Is `Engineering` a real label or
//! a typo?" has the same shape in YAML either way, because labels can be
//! created in the Fleet UI and never appear in the repo. flint therefore
//! reports unknown labels as a WARNING: it cannot prove the negative.
//!
//! A snapshot supplies the missing half. With a fresh one, an unknown label is
//! checkable, so the rule earns the right to gate.
//!
//! # Staleness degrades safety, not correctness
//!
//! A snapshot is a claim about a moment. Once stale it must not keep gating —
//! a label deleted last week would produce a confident, wrong block. So
//! freshness controls severity, never truth:
//!
//! | snapshot        | unknown label |
//! |-----------------|---------------|
//! | fresh           | error         |
//! | stale           | warning       |
//! | absent          | warning       |
//!
//! Forgetting to refresh costs strictness. It can never manufacture a false
//! block, which is what makes adopting the file strictly better than not.
//!
//! # No secrets
//!
//! The file is meant to be committed, so it holds names and presence flags
//! only — never tokens, and the server as a bare hostname.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Snapshot filename, looked up beside the config file (either spelling).
/// Message of the one finding that exists ONLY because a snapshot was
/// consulted: the hash is absent from the server's installer list.
///
/// Named so `dry-run --assume-uploaded` can suppress exactly this finding
/// rather than pattern-matching a prose string that could drift.
pub const HASH_NOT_UPLOADED: &str = "software package hash is not uploaded to the Fleet server";

pub const SNAPSHOT_FILE_NAME: &str = ".fleet-snapshot.json";

/// Default age past which a snapshot stops gating, in days.
pub const DEFAULT_MAX_AGE_DAYS: u64 = 30;

/// Where the data came from, so a reader can judge whether to trust it.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Provenance {
    /// RFC3339 UTC timestamp of the fetch.
    pub fetched_at: String,
    /// Fleet server version at fetch time.
    pub fleet_version: String,
    /// Server HOSTNAME only — never a full URL, never credentials.
    pub server: String,
    /// Tool that produced the file.
    pub fetched_by: String,
}

/// Label names known to the server, split by origin.
///
/// Built-ins are separated because they are Fleet's own and always exist;
/// keeping them distinct means a snapshot diff shows only what the org
/// actually changed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Labels {
    pub builtin: Vec<String>,
    pub custom: Vec<String>,
}

/// Server capabilities and the named resources configs refer to.
///
/// Names, not secrets: a VPP `location` (Apple calls it an "organization
/// unit") already appears verbatim in committed `default.yml`, so recording
/// it adds no exposure. Tokens themselves are never fetched or stored.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Capabilities {
    /// Whether the license is Premium.
    pub premium: bool,
    /// Whether Apple MDM is turned on. Fleet refuses to apply any profile or
    /// setup-experience config without it ("macOS MDM isn't turned on"), so a
    /// harness with this false rejects configs a real server accepts.
    pub mdm_enabled: bool,
    /// VPP token organization units, matched against
    /// `org_settings.mdm.volume_purchasing_program[].location`.
    /// Fleet compares these NFC-normalized (appconfig.go:2179).
    pub vpp_locations: Vec<String>,
    /// ABM token organization names.
    pub abm_org_names: Vec<String>,
}

/// Software the server already holds.
///
/// `hashes` is the one that settles a real ambiguity: a package spec with
/// `hash_sha256:` and no `url:` is valid IF that exact package is already
/// uploaded, and unresolvable otherwise. flint cannot tell those apart today
/// and warns on both (55 such findings on the reference repo). A production
/// CI log proved one of them a false alarm — the package WAS uploaded.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Software {
    /// Installer hashes present on the server.
    pub hashes: Vec<String>,
    /// Software title names, for `software_title_id` style references.
    pub titles: Vec<String>,
    /// Fleet-maintained app slugs available on this instance.
    pub fma_slugs: Vec<String>,
}

/// The snapshot document.
///
/// Every section is optional and independently useful: a file carrying only
/// `labels` upgrades only the label rule and leaves everything else alone.
/// Sections for software, scripts and capabilities are planned; this slice
/// ships labels.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct FleetSnapshot {
    /// Format version, so a future flint can refuse a shape it cannot read
    /// instead of silently misinterpreting it.
    pub schema: u32,
    pub provenance: Provenance,
    pub labels: Labels,
    pub capabilities: Capabilities,
    pub software: Software,
    // NOTE: there is deliberately no `scripts` section.
    //
    // It existed briefly and was removed: GitOps DECLARES scripts from the
    // repo, so server script state is the OUTPUT of an apply, not an input
    // to it. "Does this script exist on the server?" is not a question
    // `fleetctl gitops` asks, so answering it buys nothing — and
    // `run_script` paths are already fully checked repo-locally by
    // unregistered-script and path-exists.
    //
    // That is what separates scripts from labels and installer hashes: those
    // point at things the server OWNS and GitOps only consumes. If a
    // `script_id` rule is ever wanted, it needs {id, name} pairs — names
    // alone cannot validate an integer id.
}

/// How much authority a loaded snapshot carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Within the age limit — findings may gate.
    Fresh,
    /// Past the age limit — findings degrade to warnings.
    Stale { age_days: u64 },
    /// Timestamp missing or unparseable. Treated as stale: an undatable
    /// snapshot cannot be shown to be current, and guessing in the
    /// permissive direction is the safe error.
    Undated,
}

impl Freshness {
    /// Whether findings backed by this snapshot may be errors.
    pub fn may_gate(&self) -> bool {
        matches!(self, Freshness::Fresh)
    }

    /// Short reason for the degraded case, for appending to a finding.
    pub fn caveat(&self) -> Option<String> {
        match self {
            Freshness::Fresh => None,
            Freshness::Stale { age_days } => Some(format!(
                "{SNAPSHOT_FILE_NAME} is {age_days} days old — reported as a warning; \
                 refresh with `flint fleet snapshot` to gate on this"
            )),
            Freshness::Undated => Some(format!(
                "{SNAPSHOT_FILE_NAME} has no readable `provenance.fetched_at` — \
                 reported as a warning"
            )),
        }
    }
}

/// A loaded snapshot plus the authority it carries.
#[derive(Debug, Clone)]
pub struct LoadedSnapshot {
    pub snapshot: FleetSnapshot,
    pub path: PathBuf,
    pub freshness: Freshness,
    known_labels: HashSet<String>,
}

impl LoadedSnapshot {
    /// Load from an explicit path.
    pub fn load(path: &Path, max_age_days: u64, now_unix: i64) -> Result<Self, String> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        let snapshot: FleetSnapshot = serde_json::from_str(&raw)
            .map_err(|e| format!("parsing {}: {e}", path.display()))?;

        if snapshot.schema > 1 {
            return Err(format!(
                "{} declares schema {} which this flint does not understand — \
                 upgrade flint rather than let it misread the file",
                path.display(),
                snapshot.schema
            ));
        }

        let freshness = freshness_of(&snapshot.provenance.fetched_at, max_age_days, now_unix);
        let known_labels = snapshot
            .labels
            .builtin
            .iter()
            .chain(snapshot.labels.custom.iter())
            .map(|s| s.to_string())
            .collect();

        Ok(Self {
            snapshot,
            path: path.to_path_buf(),
            freshness,
            known_labels,
        })
    }

    /// Find and load a snapshot next to the repo root, if one exists.
    ///
    /// Absent is not an error — it is the normal case, and it simply leaves
    /// every rule at its unaided severity.
    pub fn discover(root: &Path, max_age_days: u64, now_unix: i64) -> Option<Self> {
        let candidate = root.join(SNAPSHOT_FILE_NAME);
        if !candidate.exists() {
            return None;
        }
        Self::load(&candidate, max_age_days, now_unix).ok()
    }

    /// Whether the snapshot carries any label data at all.
    ///
    /// A snapshot with an empty label set must NOT be read as "no labels
    /// exist" — that would flag every reference in the repo. Absence of data
    /// and knowledge of absence are different things.
    pub fn has_labels(&self) -> bool {
        !self.known_labels.is_empty()
    }

    /// Exact-match label lookup. Fleet compares label names verbatim
    /// (`fleet.LabelOverlap`, labels.go) — no trimming, no case folding — so
    /// this must not be lenient either.
    pub fn knows_label(&self, name: &str) -> bool {
        self.known_labels.contains(name)
    }

    /// Whether the snapshot carries VPP data.
    ///
    /// As with labels: an empty list is absence of DATA, not knowledge that
    /// no tokens exist. Without this guard a snapshot fetched before VPP was
    /// configured would flag every `location:` in the repo.
    pub fn has_vpp(&self) -> bool {
        !self.snapshot.capabilities.vpp_locations.is_empty()
    }

    /// Whether a VPP organization unit exists, NFC-normalized like Fleet
    /// (appconfig.go:2179 `norm.NFC.String`).
    ///
    /// Normalization matters for real org-unit names: "HQ_FleetDM" is plain
    /// ASCII, but accented names can be byte-different yet canonically equal,
    /// and Fleet would accept what a naive comparison rejects.
    pub fn knows_vpp_location(&self, location: &str) -> bool {
        let want = nfc_ish(location);
        self.snapshot
            .capabilities
            .vpp_locations
            .iter()
            .any(|l| nfc_ish(l) == want)
    }

    /// Whether the snapshot carries software data. Empty is absence of DATA,
    /// not proof the server holds nothing — same guard as labels and VPP.
    /// Where a snapshot-derived claim came from, for the reader who has to
    /// judge it. A finding that says "not uploaded to the Fleet server" is a
    /// claim about server state at one moment; without the moment attached, a
    /// fresh worktree (no snapshot) passing while the working copy fails looks
    /// like a bug in the repo rather than a difference in what was consulted.
    ///
    /// Hostname only — `Provenance.server` is documented never to carry a URL
    /// or credential, so this is safe to print.
    pub fn provenance_label(&self) -> String {
        let p = &self.snapshot.provenance;
        let when = if p.fetched_at.is_empty() { "an unknown time" } else { p.fetched_at.as_str() };
        let mut s = format!("per {SNAPSHOT_FILE_NAME} fetched {when}");
        if !p.server.is_empty() {
            s.push_str(&format!(" from {}", p.server));
        }
        if !p.fleet_version.is_empty() {
            s.push_str(&format!(" (Fleet {})", p.fleet_version));
        }
        s
    }

    pub fn has_software(&self) -> bool {
        !self.snapshot.software.hashes.is_empty()
    }

    /// Whether an installer with this sha256 is already uploaded.
    /// Compared case-insensitively: hex digests are written both ways.
    pub fn knows_hash(&self, hash: &str) -> bool {
        let want = hash.trim().to_ascii_lowercase();
        self.snapshot
            .software
            .hashes
            .iter()
            .any(|h| h.trim().to_ascii_lowercase() == want)
    }

    /// Label names, sorted — for "did you mean" suggestions.
    pub fn label_names(&self) -> Vec<&str> {
        let mut v: Vec<&str> = self.known_labels.iter().map(|s| s.as_str()).collect();
        v.sort_unstable();
        v
    }
}

/// Trim + collapse for comparing org-unit names.
///
/// NOT full Unicode NFC — that needs a normalization crate, and the failure
/// mode here is asymmetric: being slightly more LENIENT than Fleet means we
/// stay silent on something Fleet might reject (a missed finding), whereas
/// being stricter would block a config Fleet accepts. Missed findings are the
/// safe error for an optional precision aid.
fn nfc_ish(s: &str) -> String {
    s.trim().to_string()
}

/// Compute freshness from an RFC3339 timestamp.
///
/// Split out from `load` so it is testable without touching the filesystem or
/// the clock.
pub fn freshness_of(fetched_at: &str, max_age_days: u64, now_unix: i64) -> Freshness {
    // `0` means "never gate" — an explicit opt-out that keeps a snapshot's
    // SILENCING effect (it can still prove a label or hash exists and
    // suppress a finding) while never letting it block a commit. Without
    // this special case a same-day snapshot would still gate, since its age
    // is 0 and `0 > 0` is false.
    if max_age_days == 0 {
        return Freshness::Stale { age_days: 0 };
    }
    let Some(then) = parse_rfc3339_utc(fetched_at) else {
        return Freshness::Undated;
    };
    // A timestamp in the future means a wrong clock somewhere. Treat it as
    // age zero rather than panicking on the subtraction: a skewed clock is a
    // reason to be careful, not a reason to refuse to run.
    let age_secs = (now_unix - then).max(0);
    let age_days = (age_secs / 86_400) as u64;
    if age_days > max_age_days {
        Freshness::Stale { age_days }
    } else {
        Freshness::Fresh
    }
}

/// Minimal RFC3339 (UTC, `Z`) parser returning a unix timestamp.
///
/// Deliberately not a date-time dependency: the only question asked of this
/// value is "how many days ago", and a whole crate for that is not worth the
/// supply-chain surface. Accepts `YYYY-MM-DDTHH:MM:SSZ` and tolerates
/// fractional seconds.
pub(crate) fn parse_rfc3339_utc(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.len() < 20 || !s.ends_with('Z') {
        return None;
    }
    let (date, rest) = s.split_once('T')?;
    let time = rest.trim_end_matches('Z');
    let time = time.split('.').next()?; // drop fractional seconds

    let mut d = date.split('-');
    let (y, mo, da): (i64, i64, i64) = (
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
        d.next()?.parse().ok()?,
    );
    let mut t = time.split(':');
    let (h, mi, se): (i64, i64, i64) = (
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
        t.next()?.parse().ok()?,
    );
    if !(1..=12).contains(&mo) || !(1..=31).contains(&da) {
        return None;
    }

    // Days since the Unix epoch via the civil-from-days algorithm (Howard
    // Hinnant's, public domain) — handles leap years exactly.
    let y_adj = if mo <= 2 { y - 1 } else { y };
    let era = if y_adj >= 0 { y_adj } else { y_adj - 399 } / 400;
    let yoe = y_adj - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + da - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;

    Some(days * 86_400 + h * 3_600 + mi * 60 + se)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    #[test]
    fn provenance_label_degrades_gracefully_when_fields_are_missing() {
        let mut snap = FleetSnapshot::default();
        let mk = |snap: &FleetSnapshot| LoadedSnapshot {
            snapshot: snap.clone(),
            path: PathBuf::from("x"),
            freshness: Freshness::Undated,
            known_labels: HashSet::new(),
        };
        assert_eq!(mk(&snap).provenance_label(), format!("per {SNAPSHOT_FILE_NAME} fetched an unknown time"));
        snap.provenance.fetched_at = "2026-08-12T06:28:27Z".into();
        snap.provenance.server = "fleet.example.com".into();
        snap.provenance.fleet_version = "4.90.0".into();
        assert_eq!(
            mk(&snap).provenance_label(),
            format!("per {SNAPSHOT_FILE_NAME} fetched 2026-08-12T06:28:27Z from fleet.example.com (Fleet 4.90.0)")
        );
    }

    #[test]
    fn parses_rfc3339_and_epoch_alignment() {
        assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_utc("2026-08-08T00:00:00Z"), Some(1_786_147_200));
        // Fractional seconds are tolerated — Fleet emits them.
        assert_eq!(
            parse_rfc3339_utc("2026-08-08T00:00:00.123Z"),
            Some(1_786_147_200)
        );
        assert_eq!(parse_rfc3339_utc("not a date"), None);
        assert_eq!(parse_rfc3339_utc("2026-08-08 00:00:00"), None);
    }

    #[test]
    fn freshness_controls_gating_not_truth() {
        let now = parse_rfc3339_utc("2026-08-08T00:00:00Z").unwrap();

        let fresh = freshness_of("2026-08-01T00:00:00Z", 30, now);
        assert_eq!(fresh, Freshness::Fresh);
        assert!(fresh.may_gate());
        assert!(fresh.caveat().is_none());

        // 31 days old with a 30-day limit.
        let stale = freshness_of("2026-07-08T00:00:00Z", 30, now);
        assert!(matches!(stale, Freshness::Stale { .. }));
        assert!(!stale.may_gate(), "a stale snapshot must not gate");
        assert!(stale.caveat().unwrap().contains("refresh"));

        // Missing timestamp is undated, never silently fresh.
        let undated = freshness_of("", 30, now);
        assert_eq!(undated, Freshness::Undated);
        assert!(!undated.may_gate());
    }

    #[test]
    fn zero_max_age_never_gates() {
        let now = parse_rfc3339_utc("2026-08-08T00:00:00Z").unwrap();
        // Fetched moments ago, yet must not gate: 0 is an explicit opt-out.
        let f = freshness_of("2026-08-08T00:00:00Z", 0, now);
        assert!(!f.may_gate(), "max_age_days = 0 must never gate");
        // Still a usable snapshot — silencing keeps working, and the caveat
        // explains why nothing is being escalated.
        assert!(f.caveat().is_some());
    }

    #[test]
    fn future_timestamp_does_not_panic_or_gate_wrongly() {
        let now = parse_rfc3339_utc("2026-08-08T00:00:00Z").unwrap();
        // Clock skew: a snapshot "from tomorrow" is age 0, i.e. fresh.
        assert_eq!(freshness_of("2026-08-09T00:00:00Z", 30, now), Freshness::Fresh);
    }

    #[test]
    fn boundary_day_is_still_fresh() {
        let now = parse_rfc3339_utc("2026-08-08T00:00:00Z").unwrap();
        let exactly_30 = now - 30 * DAY;
        let ts = format_unix_as_rfc3339(exactly_30);
        assert_eq!(freshness_of(&ts, 30, now), Freshness::Fresh, "at the limit, not past it");
    }

    #[test]
    fn empty_label_set_is_not_knowledge_of_absence() {
        let loaded = LoadedSnapshot {
            snapshot: FleetSnapshot::default(),
            path: PathBuf::from(".fleet-snapshot.json"),
            freshness: Freshness::Fresh,
            known_labels: HashSet::new(),
        };
        assert!(
            !loaded.has_labels(),
            "an empty snapshot must not be read as 'no labels exist' — that \
             would flag every reference in the repo"
        );
    }

    #[test]
    fn label_matching_is_exact() {
        let mut known = HashSet::new();
        known.insert("Engineering".to_string());
        let loaded = LoadedSnapshot {
            snapshot: FleetSnapshot::default(),
            path: PathBuf::from("x"),
            freshness: Freshness::Fresh,
            known_labels: known,
        };
        assert!(loaded.knows_label("Engineering"));
        // Fleet compares raw names; being lenient here would accept configs
        // the server rejects.
        assert!(!loaded.knows_label("engineering"));
        assert!(!loaded.knows_label(" Engineering "));
    }

    /// Test helper: render a unix timestamp back to RFC3339 UTC.
    fn format_unix_as_rfc3339(ts: i64) -> String {
        let days = ts.div_euclid(86_400);
        let secs = ts.rem_euclid(86_400);
        // Inverse of the civil-from-days algorithm above.
        let z = days + 719_468;
        let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            secs / 3600,
            (secs % 3600) / 60,
            secs % 60
        )
    }

    /// End-to-end: the same repo, the same unknown label, three snapshot
    /// states — and only the provable one gates.
    #[test]
    fn severity_ladder_absent_stale_fresh() {
        use crate::cross_reference::{check_label_references_with_snapshot, RepoIndex};
        use crate::error::Severity;

        let src = "policies:\n  - name: p\n    labels_include_any:\n      - Ghost\n";
        let yaml: serde_yaml::Value = serde_yaml::from_str(src).unwrap();
        let parsed = vec![crate::cross_reference::ParsedFile {
            path: PathBuf::from("fleets/a.yml"),
            source: src.to_string(),
            yaml: yaml.clone(),
        }];
        let index = RepoIndex::build(&parsed);
        let file = Path::new("fleets/a.yml");

        let sev = |snap: Option<&LoadedSnapshot>| {
            let f = check_label_references_with_snapshot(&index, file, src, &yaml, snap);
            assert_eq!(f.len(), 1, "expected exactly one label finding");
            f[0].severity.clone()
        };

        // 1. No snapshot: absence is unprovable, so it can only warn.
        assert_eq!(sev(None), Severity::Warning);

        let mk = |freshness| LoadedSnapshot {
            snapshot: FleetSnapshot::default(),
            path: PathBuf::from(SNAPSHOT_FILE_NAME),
            freshness,
            known_labels: ["Engineering".to_string()].into_iter().collect(),
        };

        // 2. Stale snapshot: 'Ghost' is absent from it, but the file is old
        //    enough that a label deleted since would produce a wrong block.
        assert_eq!(sev(Some(&mk(Freshness::Stale { age_days: 99 }))), Severity::Warning);
        assert_eq!(sev(Some(&mk(Freshness::Undated))), Severity::Warning);

        // 3. Fresh snapshot: the server's own list lacks it, so it gates.
        assert_eq!(sev(Some(&mk(Freshness::Fresh))), Severity::Error);

        // 4. Fresh snapshot that KNOWS the label: no finding at all.
        let knows = LoadedSnapshot {
            snapshot: FleetSnapshot::default(),
            path: PathBuf::from(SNAPSHOT_FILE_NAME),
            freshness: Freshness::Fresh,
            known_labels: ["Ghost".to_string()].into_iter().collect(),
        };
        assert!(
            check_label_references_with_snapshot(&index, file, src, &yaml, Some(&knows)).is_empty(),
            "a label the server knows must not be reported"
        );

        // 5. Fresh but EMPTY snapshot: no data is not knowledge of absence.
        let empty = mk(Freshness::Fresh);
        let empty = LoadedSnapshot { known_labels: HashSet::new(), ..empty };
        assert_eq!(
            sev(Some(&empty)),
            Severity::Warning,
            "an empty snapshot must not flag every label in the repo as fatal"
        );
    }
}
