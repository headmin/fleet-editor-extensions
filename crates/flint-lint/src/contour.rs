//! Optional Apple-schema validation of configuration profiles via `contour`.
//!
//! flint validates the FLEET side of a profile: is it referenced, is it wired
//! into a fleet, is its PayloadUUID unique across the repo, is it in a section
//! Fleet accepts. It never looks inside the payload. [contour] validates the
//! APPLE side — payload fields against Apple's schema, required keys, and keys
//! Apple has deprecated. A profile can be perfectly wired and still be wrong
//! on the device.
//!
//! # Optional in every sense
//!
//! `contour` is not a dependency. When the binary is absent every function
//! here returns `None` and flint behaves exactly as before — same findings,
//! same timing. Callers must treat a `None` as "not checked", never as "no
//! problems". This follows the rule the snapshot work established: optional
//! precision on top, never a prerequisite.
//!
//! # Why one file at a time
//!
//! `contour profile validate <dir> --recursive --json` is fast (0.14s for 182
//! profiles versus ~10s one at a time) but currently unreliable: measured
//! against a real repository at contour 2026-08, batch mode marked 46 of 182
//! profiles `"valid": false` with an EMPTY `errors` array, and files it called
//! invalid in batch validate cleanly on their own. Per-file output is the
//! trustworthy one, so that is what this uses — which in turn is why callers
//! validate a profile they are already working on rather than sweeping a repo.
//!
//! [contour]: https://github.com/macadmins/contour

use serde::Deserialize;
use std::path::Path;
use std::process::Command;

/// One validation result for a single profile.
#[derive(Debug, Clone, Deserialize)]
pub struct Report {
    /// contour's verdict. Absent from its error-shaped response, hence the
    /// default — see [`validate`] on the two output shapes.
    #[serde(default)]
    pub valid: bool,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
}

impl Report {
    /// Whether there is anything worth showing a user.
    pub fn has_findings(&self) -> bool {
        !self.errors.is_empty() || !self.warnings.is_empty()
    }
}

/// Whether a usable `contour` is on `PATH`.
///
/// Probed once per process: the answer cannot change mid-run, and a failed
/// spawn per profile would cost more than the validation.
pub fn available() -> bool {
    static AVAILABLE: once_cell::sync::Lazy<bool> = once_cell::sync::Lazy::new(|| {
        Command::new("contour")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    });
    *AVAILABLE
}

/// Validate one `.mobileconfig` (or DDM `.json`) profile.
///
/// `None` means NOT CHECKED — contour missing, failed to run, or produced
/// output this cannot interpret. It never means "no problems found".
pub fn validate(path: &Path) -> Option<Report> {
    if !available() {
        return None;
    }
    let out = Command::new("contour")
        .args(["profile", "validate"])
        .arg(path)
        .arg("--json")
        .output()
        .ok()?;

    // NOT `out.status.success()`: contour exits 0 even for a profile missing
    // every required field. The verdict lives in the JSON, so the exit code is
    // deliberately ignored.
    parse(&String::from_utf8_lossy(&out.stdout))
}

/// Pull the per-file report out of contour's stdout.
///
/// Two shapes have to be handled, and they can BOTH appear in one run —
/// contour emits the per-file object followed by a summary object, which is
/// why this decodes a stream rather than calling `from_str` once:
///
/// ```text
/// {"file":…,"valid":true,"errors":[],"warnings":[…]}   <- the one we want
/// {"success":false,"error":"…","error_code":"…"}       <- summary, no `valid`
/// ```
///
/// The summary is skipped by requiring a `file` key; a document without one
/// carries no per-profile verdict.
fn parse(stdout: &str) -> Option<Report> {
    let mut stream = serde_json::Deserializer::from_str(stdout).into_iter::<serde_json::Value>();
    while let Some(Ok(doc)) = stream.next() {
        if doc.get("file").is_none() {
            continue; // summary/error document
        }
        return serde_json::from_value(doc).ok();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_per_file_document_and_skips_the_summary() {
        let out = r#"{"file":"a.mobileconfig","valid":false,"errors":["bad thing"],"warnings":["meh"]}
{"success":false,"error":"1 file(s) failed validation","error_code":"UNKNOWN"}"#;
        let r = parse(out).expect("per-file doc");
        assert!(!r.valid);
        assert_eq!(r.errors, vec!["bad thing"]);
        assert_eq!(r.warnings, vec!["meh"]);
    }

    /// The summary alone carries no verdict. Returning a default-constructed
    /// Report here would read as "valid, no findings" — the exact confusion
    /// between "checked and clean" and "not checked" this module exists to
    /// avoid.
    #[test]
    fn a_summary_only_response_is_not_a_verdict() {
        let out = r#"{"success":false,"error":"boom","error_code":"MISSING_PAYLOAD_TYPE"}"#;
        assert!(parse(out).is_none());
    }

    #[test]
    fn unparseable_output_is_not_checked() {
        assert!(parse("").is_none());
        assert!(parse("not json at all").is_none());
    }

    #[test]
    fn a_clean_profile_reports_no_findings() {
        let out = r#"{"file":"a.mobileconfig","valid":true,"errors":[],"warnings":[]}"#;
        let r = parse(out).unwrap();
        assert!(r.valid);
        assert!(!r.has_findings());
    }

    /// Missing arrays must default rather than fail the whole parse — a
    /// contour version that omits an empty list should not turn a usable
    /// verdict into "not checked".
    #[test]
    fn absent_arrays_default_to_empty() {
        let r = parse(r#"{"file":"a","valid":true}"#).unwrap();
        assert!(r.errors.is_empty() && r.warnings.is_empty());
    }
}
