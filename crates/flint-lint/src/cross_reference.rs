//! Repo-wide cross-file reference resolution — the "graph pass".
//!
//! Per-file rules (the [`Rule`](crate::rules::Rule) trait) only see one file at
//! a time, so they cannot tell whether a *reference* points at something that
//! actually exists elsewhere in the repo. This pass closes that gap: it builds
//! a repo-wide index of **defined** entities once, then checks every file's
//! **references** against it — catching the "X references Y that doesn't exist"
//! mistakes that otherwise only surface in `fleetctl gitops --dry-run`.
//!
//! Two references are resolved:
//!   - **Labels** (`labels_include_any` / `labels_include_all` /
//!     `labels_exclude_any`) → a label defined in the repo, or a Fleet built-in.
//!   - **Policy `install_software.hash_sha256`** → a software package with that
//!     hash defined in the repo.
//!
//! Both findings are **warnings**, not errors: GitOps legitimately permits
//! referencing entities that live only on the server (a manually-created label,
//! or a package already cached in Fleet by hash). So an unresolved reference is
//! a *likely* mistake (typo, forgotten definition) worth surfacing, but not a
//! provable failure. Labels are global in Fleet, so the repo-wide index is
//! accurate for them; software is team-scoped, so the hash index can only miss
//! a cross-team mismatch (a false negative), never invent one.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_yaml::Value;

use super::config::compile_glob;
use super::error::LintError;
use super::util::levenshtein_distance;
use super::util::normalize_path as normalize;

/// Fleet's built-in (reserved) labels. These are never defined in a GitOps
/// repo but are always valid reference targets, so they must not be flagged.
/// Matched case-insensitively. Source: Fleet builtin labels (REST API
/// `label_type: "builtin"`).
const BUILTIN_LABELS: &[&str] = &[
    "all hosts",
    "all linux",
    "centos linux",
    "macos",
    "ms windows",
    "red hat linux",
    "ubuntu linux",
    "fedora linux",
    "amazon linux",
    "chrome",
    "chromebooks",
    "ios",
    "ipados",
];

/// The label-reference keys whose string values name a label.
const LABEL_REF_KEYS: &[&str] = &["labels_include_any", "labels_include_all", "labels_exclude_any"];

/// A repo-wide index of defined entities (labels, software hashes), built
/// once per directory lint — and reusable by other flint faces: the LSP
/// builds one per workspace to validate label references in open documents.
#[derive(Debug, Default)]
pub struct RepoIndex {
    /// Lowercased names of every label defined in the repo.
    labels: HashSet<String>,
    /// Original-case label names, for "did you mean" suggestions.
    label_names: Vec<String>,
    /// Lowercased sha256 of every software package defined in the repo.
    software_hashes: HashSet<String>,
}

impl RepoIndex {
    /// Build the index from every parsed file in the repo.
    pub fn build(files: &[ParsedFile]) -> Self {
        let mut idx = RepoIndex::default();
        for f in files {
            collect_label_defs(&f.path, &f.yaml, &mut idx);
            collect_software_hashes(&f.path, &f.yaml, &mut idx);
        }
        idx
    }

    /// Build an index from a pre-collected list of label names (no files) —
    /// for callers that discover labels themselves, like the LSP's workspace
    /// scan. Software hashes stay empty; only label checks are meaningful.
    pub fn from_label_names<S: AsRef<str>>(names: &[S]) -> Self {
        let mut idx = RepoIndex::default();
        for n in names {
            idx.labels.insert(n.as_ref().to_lowercase());
            idx.label_names.push(n.as_ref().to_string());
        }
        idx
    }

    /// Whether `name` is a defined or Fleet built-in label (case-insensitive).
    pub fn has_label(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        self.labels.contains(&lower) || BUILTIN_LABELS.contains(&lower.as_str())
    }

    /// Original-case names of every label defined in the repo.
    pub fn label_names(&self) -> &[String] {
        &self.label_names
    }

    fn has_hash(&self, hash: &str) -> bool {
        self.software_hashes.contains(&hash.to_lowercase())
    }

    /// Closest defined label within a small edit distance, for suggestions.
    pub fn closest_label(&self, name: &str) -> Option<&str> {
        let max = (name.len() / 3).max(2);
        self.label_names
            .iter()
            .map(|l| (l, levenshtein_distance(&name.to_lowercase(), &l.to_lowercase())))
            .filter(|(_, d)| *d <= max)
            .min_by_key(|(_, d)| *d)
            .map(|(l, _)| l.as_str())
    }
}

/// A file read and parsed once for the graph pass.
pub struct ParsedFile {
    pub path: PathBuf,
    pub source: String,
    pub yaml: Value,
}

/// Check ONE document's label references against a repo index — the narrow
/// entry point the LSP uses for live diagnostics (labels only; the full
/// cross-file pass adds software-hash resolution and whole-repo checks).
pub fn check_label_references(
    index: &RepoIndex,
    file: &Path,
    source: &str,
    yaml: &Value,
) -> Vec<LintError> {
    check_label_references_with_snapshot(index, file, source, yaml, None)
}

/// As [`check_label_references`], with an optional server snapshot.
///
/// Passing `None` keeps the historical behavior: unknown labels warn, because
/// without server knowledge their absence is unprovable.
pub fn check_label_references_with_snapshot(
    index: &RepoIndex,
    file: &Path,
    source: &str,
    yaml: &Value,
    snapshot: Option<&crate::snapshot::LoadedSnapshot>,
) -> Vec<LintError> {
    let mut errors = Vec::new();
    check_label_refs(index, file, source, yaml, snapshot, &mut errors);
    errors
}

/// Collect defined label names from one file into the index.
///
/// Detection is path-based on a `labels/` ancestor at ANY depth — Fleet repos
/// nest label files (`labels/dynamic/*.yml`, `lib/macos/labels/*.yml`), which
/// the immediate-parent `detect_file_type` misses. Over-collecting here is the
/// safe direction: an extra name only suppresses a warning, never invents one.
fn collect_label_defs(path: &Path, yaml: &Value, idx: &mut RepoIndex) {
    // Label lib files (under a `labels/` dir) are a top-level sequence.
    if has_ancestor_dir(path, "labels") {
        if let Value::Sequence(seq) = yaml {
            for item in seq {
                insert_label(item, idx);
            }
        }
    }
    // Any file may carry a top-level `labels:` sequence (default.yml, fleets/*).
    if let Some(Value::Sequence(seq)) = yaml.get("labels") {
        for item in seq {
            insert_label(item, idx);
        }
    }
}

fn insert_label(item: &Value, idx: &mut RepoIndex) {
    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
        idx.labels.insert(name.to_lowercase());
        idx.label_names.push(name.to_string());
    }
}

/// Collect defined software package hashes from one file into the index.
/// Like labels, software files nest (`software/macos/*.yml`), so a standalone
/// software file is recognized by a `software/` ancestor at any depth.
fn collect_software_hashes(path: &Path, yaml: &Value, idx: &mut RepoIndex) {
    // Standalone software file: top-level mapping carrying hash_sha256.
    if has_ancestor_dir(path, "software") {
        if let Some(h) = yaml.get("hash_sha256").and_then(|v| v.as_str()) {
            idx.software_hashes.insert(h.to_lowercase());
        }
    }
    // Inline packages: software.packages[].hash_sha256.
    if let Some(Value::Sequence(pkgs)) = yaml.get("software").and_then(|s| s.get("packages")) {
        for pkg in pkgs {
            if let Some(h) = pkg.get("hash_sha256").and_then(|v| v.as_str()) {
                idx.software_hashes.insert(h.to_lowercase());
            }
        }
    }
}

/// True if any ancestor directory of `path` is named exactly `name`.
fn has_ancestor_dir(path: &Path, name: &str) -> bool {
    path.parent()
        .map(|p| {
            p.components()
                .any(|c| c.as_os_str().to_str() == Some(name))
        })
        .unwrap_or(false)
}

/// Check one file's references against the repo index, returning findings.
pub(crate) fn check_references(
    index: &RepoIndex,
    file: &Path,
    source: &str,
    yaml: &Value,
    snapshot: Option<&crate::snapshot::LoadedSnapshot>,
) -> Vec<LintError> {
    let mut errors = Vec::new();
    check_label_refs(index, file, source, yaml, snapshot, &mut errors);
    check_install_software_hashes(index, file, source, yaml, &mut errors);
    errors
}

/// Recursively find every `labels_include_*/exclude_*` value and verify it
/// resolves to a defined or built-in label.
fn check_label_refs(
    index: &RepoIndex,
    file: &Path,
    source: &str,
    value: &Value,
    snapshot: Option<&crate::snapshot::LoadedSnapshot>,
    errors: &mut Vec<LintError>,
) {
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                if let Some(key) = k.as_str() {
                    if LABEL_REF_KEYS.contains(&key) {
                        if let Value::Sequence(seq) = v {
                            for item in seq {
                                if let Some(label) = item.as_str() {
                                    verify_label(index, file, source, label, snapshot, errors);
                                }
                            }
                        }
                    }
                }
                check_label_refs(index, file, source, v, snapshot, errors);
            }
        }
        Value::Sequence(seq) => {
            for item in seq {
                check_label_refs(index, file, source, item, snapshot, errors);
            }
        }
        _ => {}
    }
}

fn verify_label(
    index: &RepoIndex,
    file: &Path,
    source: &str,
    label: &str,
    snapshot: Option<&crate::snapshot::LoadedSnapshot>,
    errors: &mut Vec<LintError>,
) {
    if index.has_label(label) {
        return;
    }

    // The label is not in the repo. Whether that is an ERROR depends entirely
    // on whether we can see the server.
    //
    // Without a snapshot this stays a warning forever: a label can be created
    // in the Fleet UI and never appear in the repo, so "absent here" does not
    // imply "absent there". flint cannot prove the negative and must not
    // pretend to.
    //
    // With a FRESH snapshot that actually carries labels, the negative IS
    // provable — the server's own list does not contain it — so the finding
    // gates. A stale or empty snapshot falls back to the warning, because a
    // label deleted since the fetch would otherwise produce a confident,
    // wrong block. Staleness costs strictness, never accuracy.
    let authoritative = snapshot.filter(|s| s.freshness.may_gate() && s.has_labels());

    // Symmetry matters: an authoritative snapshot settles the question BOTH
    // ways. If the server knows the label, there is nothing to report — the
    // reference is correct, it simply resolves outside the repo. Escalating
    // absence while still nagging about presence would make the snapshot feel
    // like a punishment for adopting it.
    if authoritative.is_some_and(|s| s.knows_label(label)) {
        return;
    }
    let proven_absent = authoritative.is_some();
    let (line, col) = find_label_line(source, label);
    let mut err = if proven_absent {
        LintError::error(
            format!("label '{label}' does not exist on the Fleet server"),
            file,
        )
    } else {
        LintError::warning(
            format!("label '{label}' is referenced but not defined in this repo"),
            file,
        )
    }
    .with_rule_code(crate::codes::LABEL_REFERENCE);

    // A snapshot present but not authoritative: say so, so the reader knows
    // why this is only a warning and how to make it stronger.
    if !proven_absent {
        if let Some(caveat) = snapshot.and_then(|s| s.freshness.caveat()) {
            err = err.with_help(caveat);
        }
    }
    if let Some(s) = index.closest_label(label) {
        err = err.with_help(format!(
            "No label named '{label}' is defined here and it is not a Fleet built-in. Did you mean '{s}'? (GitOps will reject it unless the label exists on the server.)"
        ));
    } else {
        err = err.with_help(
            "Define this label in the repo (or confirm it exists on the server) — otherwise `fleetctl gitops` rejects the reference.",
        );
    }
    if let (Some(l), Some(c)) = (line, col) {
        err = err.with_location(l, c);
    }
    errors.push(err);
}

/// Recursively find every policy `install_software.hash_sha256` and verify a
/// software package with that hash is defined in the repo.
fn check_install_software_hashes(
    index: &RepoIndex,
    file: &Path,
    source: &str,
    value: &Value,
    errors: &mut Vec<LintError>,
) {
    match value {
        Value::Mapping(map) => {
            if let Some(Value::Mapping(is)) = map.get(Value::String("install_software".to_string())) {
                if let Some(hash) = is.get(Value::String("hash_sha256".to_string())).and_then(|v| v.as_str()) {
                    if !index.has_hash(hash) {
                        let (line, col) = find_line_containing(source, hash);
                        let mut err = LintError::warning(
                            format!("install_software references hash_sha256 '{hash}' but no software package with that hash is defined in this repo"),
                            file,
                        )
                        .with_rule_code(crate::codes::INSTALL_SOFTWARE_HASH)
                        .with_help(
                            "Add the package (with this hash) to a software list in this repo, or confirm it is already cached in Fleet by hash.",
                        );
                        if let (Some(l), Some(c)) = (line, col) {
                            err = err.with_location(l, c);
                        }
                        errors.push(err);
                    }
                }
            }
            for (_, v) in map {
                check_install_software_hashes(index, file, source, v, errors);
            }
        }
        Value::Sequence(seq) => {
            for item in seq {
                check_install_software_hashes(index, file, source, item, errors);
            }
        }
        _ => {}
    }
}

/// Find the 1-indexed (line, col) of a label reference in source — a list item
/// whose value (with optional quotes) equals the label.
fn find_label_line(source: &str, label: &str) -> (Option<usize>, Option<usize>) {
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let rest = trimmed.strip_prefix("- ").unwrap_or(trimmed);
        let unquoted = rest.trim().trim_matches('"').trim_matches('\'');
        if unquoted == label {
            let col = line.len() - line.trim_start().len() + 1;
            return (Some(idx + 1), Some(col));
        }
    }
    (None, None)
}

/// Find the 1-indexed (line, col) of the first line containing `needle`.
fn find_line_containing(source: &str, needle: &str) -> (Option<usize>, Option<usize>) {
    for (idx, line) in source.lines().enumerate() {
        if let Some(pos) = line.find(needle) {
            return (Some(idx + 1), Some(pos + 1));
        }
    }
    (None, None)
}

// ===========================================================================
// Team-scoped install_software membership (install-software-team)
// ===========================================================================

/// Per-team check: a policy's `install_software` action references a package the
/// team itself does not include in its `software` list. The install-on-fail can
/// then never run for that team, and `fleetctl gitops` reports
/// "package not found with hash" for it — even though the package exists
/// elsewhere in the repo. Most common when policies are pulled into a fleet by a
/// `paths:` glob while the matching package isn't added to that fleet. Reported
/// on the team/fleet file. Covers both `fleets/` and `teams/`.
///
/// Resolution rules mirror Fleet exactly:
///   - a fleet's `software.packages[].path`/`paths` and a policy's
///     `policies[].path`/`paths` resolve relative to the **fleet file**;
///   - a policy's `install_software.package_path` resolves relative to the
///     **policy file** (not the fleet that pulled it in).
///
/// Returns `(file_to_attach_finding_to, error)` pairs.
pub(crate) fn check_team_membership(files: &[ParsedFile]) -> Vec<(PathBuf, LintError)> {
    let by_path: HashMap<PathBuf, &ParsedFile> =
        files.iter().map(|f| (normalize(&f.path), f)).collect();
    let mut out = Vec::new();

    for f in files {
        if !is_team_file(&f.path) {
            continue;
        }
        let base = match f.path.parent() {
            Some(d) => d,
            None => continue,
        };

        let mut hashes = HashSet::new();
        let mut slugs = HashSet::new();
        let mut app_ids = HashSet::new();
        collect_team_software(f, base, files, &by_path, &mut hashes, &mut slugs, &mut app_ids);

        let policies = collect_team_policies(f, base, files, &by_path);

        let team_name = f
            .yaml
            .get("name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                f.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("fleet")
                    .to_string()
            });

        for (policy, policy_path) in &policies {
            if let Some(missing) =
                unresolved_install(policy, policy_path, &by_path, &hashes, &slugs, &app_ids)
            {
                let pol_name = policy
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unnamed)");
                let (line, col) = anchor_line(&f.source);
                // Error, not advisory: fleetctl refuses the apply outright —
                // "failed to parse policy install_software …: package_path
                // SHA256 … not found on team". A repo that wants it softer can
                // downgrade it via `[rules] warn`.
                let mut err = LintError::error(
                    format!("fleet '{team_name}': policy '{pol_name}' auto-installs {missing}, but this fleet's software list doesn't include it"),
                    &f.path,
                )
                .with_rule_code(crate::codes::INSTALL_SOFTWARE_TEAM)
                .with_help(
                    "The install-on-fail action can't run for this fleet — add the package to this fleet's software:, or scope the policy off this fleet. Often happens when policies are pulled in by a paths: glob.",
                );
                if let (Some(l), Some(c)) = (line, col) {
                    err = err.with_location(l, c);
                }
                out.push((f.path.clone(), err));
            }
        }
    }
    out
}

fn is_team_file(path: &Path) -> bool {
    matches!(
        path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()),
        Some("fleets") | Some("teams")
    )
}

fn resolve<'a>(
    by_path: &HashMap<PathBuf, &'a ParsedFile>,
    base: &Path,
    rel: &str,
) -> Option<&'a ParsedFile> {
    by_path.get(&normalize(&base.join(rel))).copied()
}

fn glob_files<'a>(files: &'a [ParsedFile], base: &Path, pattern: &str) -> Vec<&'a ParsedFile> {
    let pat = normalize(&base.join(pattern));
    let pat_s = pat.to_string_lossy().replace('\\', "/");
    // Compile once per pattern, then test every file against the matcher.
    let Some(matcher) = compile_glob(&pat_s) else {
        return Vec::new();
    };
    files
        .iter()
        .filter(|f| {
            let p = normalize(&f.path).to_string_lossy().replace('\\', "/");
            matcher.is_match(&p)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn collect_team_software(
    f: &ParsedFile,
    base: &Path,
    files: &[ParsedFile],
    by_path: &HashMap<PathBuf, &ParsedFile>,
    hashes: &mut HashSet<String>,
    slugs: &mut HashSet<String>,
    app_ids: &mut HashSet<String>,
) {
    let sw = match f.yaml.get("software") {
        Some(s) => s,
        None => return,
    };
    if let Some(Value::Sequence(pkgs)) = sw.get("packages") {
        for e in pkgs {
            if let Some(h) = e.get("hash_sha256").and_then(|v| v.as_str()) {
                hashes.insert(h.to_lowercase());
            }
            if let Some(s) = e.get("fleet_maintained_app_slug").and_then(|v| v.as_str()) {
                slugs.insert(s.to_string());
            }
            if let Some(a) = e.get("app_store_id").and_then(|v| v.as_str()) {
                app_ids.insert(a.to_string());
            }
            if let Some(p) = e.get("path").and_then(|v| v.as_str()) {
                if let Some(pf) = resolve(by_path, base, p) {
                    extract_software_hashes(&pf.yaml, hashes);
                }
            }
            if let Some(g) = e.get("paths").and_then(|v| v.as_str()) {
                for pf in glob_files(files, base, g) {
                    extract_software_hashes(&pf.yaml, hashes);
                }
            }
        }
    }
    if let Some(Value::Sequence(seq)) = sw.get("app_store_apps") {
        for e in seq {
            if let Some(a) = e.get("app_store_id").and_then(|v| v.as_str()) {
                app_ids.insert(a.to_string());
            }
        }
    }
    if let Some(Value::Sequence(seq)) = sw.get("fleet_maintained_apps") {
        for e in seq {
            for k in ["slug", "fleet_maintained_app_slug"] {
                if let Some(s) = e.get(k).and_then(|v| v.as_str()) {
                    slugs.insert(s.to_string());
                }
            }
        }
    }
}

/// Hashes defined in a software file (mapping form or sequence-of-packages).
fn extract_software_hashes(yaml: &Value, hashes: &mut HashSet<String>) {
    match yaml {
        Value::Mapping(_) => {
            if let Some(h) = yaml.get("hash_sha256").and_then(|v| v.as_str()) {
                hashes.insert(h.to_lowercase());
            }
        }
        Value::Sequence(seq) => {
            for e in seq {
                if let Some(h) = e.get("hash_sha256").and_then(|v| v.as_str()) {
                    hashes.insert(h.to_lowercase());
                }
            }
        }
        _ => {}
    }
}

fn collect_team_policies(
    f: &ParsedFile,
    base: &Path,
    files: &[ParsedFile],
    by_path: &HashMap<PathBuf, &ParsedFile>,
) -> Vec<(Value, PathBuf)> {
    let mut out = Vec::new();
    if let Some(Value::Sequence(seq)) = f.yaml.get("policies") {
        for e in seq {
            if let Some(p) = e.get("path").and_then(|v| v.as_str()) {
                if let Some(pf) = resolve(by_path, base, p) {
                    push_policies(&pf.yaml, &pf.path, &mut out);
                }
            } else if let Some(g) = e.get("paths").and_then(|v| v.as_str()) {
                for pf in glob_files(files, base, g) {
                    push_policies(&pf.yaml, &pf.path, &mut out);
                }
            } else if e.is_mapping() {
                out.push((e.clone(), f.path.clone()));
            }
        }
    }
    out
}

fn push_policies(yaml: &Value, path: &Path, out: &mut Vec<(Value, PathBuf)>) {
    if let Value::Sequence(seq) = yaml {
        for p in seq {
            out.push((p.clone(), path.to_path_buf()));
        }
    }
}

/// If a policy's `install_software` references a package not in the team's sets,
/// return a short human description of what's missing; else `None`.
fn unresolved_install(
    policy: &Value,
    policy_path: &Path,
    by_path: &HashMap<PathBuf, &ParsedFile>,
    hashes: &HashSet<String>,
    slugs: &HashSet<String>,
    app_ids: &HashSet<String>,
) -> Option<String> {
    let is = policy.get("install_software")?;
    match is {
        // Patch policy: `install_software: true` installs the policy's FMA.
        Value::Bool(true) => {
            let s = policy.get("fleet_maintained_app_slug").and_then(|v| v.as_str())?;
            (!slugs.contains(s)).then(|| format!("Fleet-maintained app '{s}'"))
        }
        Value::Mapping(_) => {
            if let Some(z) = is.get("package_path").and_then(|v| v.as_str()) {
                // Resolved relative to the POLICY file, not the fleet.
                let pbase = policy_path.parent()?;
                // If the file is missing, path-exists already reports it — skip.
                let pf = resolve(by_path, pbase, z)?;
                let mut req = HashSet::new();
                extract_software_hashes(&pf.yaml, &mut req);
                if req.is_empty() || req.iter().any(|h| hashes.contains(h)) {
                    return None;
                }
                return Some(format!("the package at {z} (its hash is not in this fleet's software)"));
            }
            if let Some(h) = is.get("hash_sha256").and_then(|v| v.as_str()) {
                return (!hashes.contains(&h.to_lowercase()))
                    .then(|| format!("a package with hash {h}"));
            }
            if let Some(s) = is.get("fleet_maintained_app_slug").and_then(|v| v.as_str()) {
                return (!slugs.contains(s)).then(|| format!("Fleet-maintained app '{s}'"));
            }
            if let Some(a) = is.get("app_store_id").and_then(|v| v.as_str()) {
                return (!app_ids.contains(a)).then(|| format!("App Store app '{a}'"));
            }
            None
        }
        _ => None,
    }
}

// ===========================================================================
// Policy query identifier vs installed package id (install-software-id)
// ===========================================================================

/// Heuristic check for the "installs but the policy never passes" trap: when a
/// policy's `install_software.package_path` points at a software file whose
/// header comment records a package identifier (`# <id> (<file>) version <ver>`,
/// the line `flint pkg` writes), warn if the policy's own query checks a
/// DIFFERENT `package_id` / `bundle_identifier`. Such a policy can install the
/// package yet never go green, so Fleet reinstalls until the 3-attempt cap then
/// stops — looking like "the install never runs".
///
/// Heuristic, hence a warning: for hash-only software files the identifier lives
/// only in the comment, and a query may legitimately check a different receipt.
pub(crate) fn check_package_id_match(files: &[ParsedFile]) -> Vec<(PathBuf, LintError)> {
    let by_path: HashMap<PathBuf, &ParsedFile> =
        files.iter().map(|f| (normalize(&f.path), f)).collect();
    let mut out = Vec::new();
    for f in files {
        let base = match f.path.parent() {
            Some(d) => d,
            None => continue,
        };
        for policy in policies_in_file(f) {
            let pp = match policy
                .get("install_software")
                .and_then(|is| is.get("package_path"))
                .and_then(|v| v.as_str())
            {
                Some(p) => p,
                None => continue,
            };
            let query = match policy.get("query").and_then(|v| v.as_str()) {
                Some(q) => q,
                None => continue,
            };
            let qid = match query_identifier(query) {
                Some(i) => i,
                None => continue,
            };
            // package_path resolves relative to the policy file.
            let pf = match resolve(&by_path, base, pp) {
                Some(p) => p,
                None => continue, // missing file → path-exists reports it
            };
            let sid = match software_comment_identifier(&pf.source) {
                Some(i) => i,
                None => continue, // no recorded id → can't compare
            };
            if qid != sid {
                let pol_name = policy.get("name").and_then(|v| v.as_str()).unwrap_or("(unnamed)");
                let (line, col) = find_line_containing(&f.source, &qid);
                let mut err = LintError::warning(
                    format!("policy '{pol_name}': query checks id '{qid}' but the installed package's id is '{sid}' — the policy may install yet never pass"),
                    &f.path,
                )
                .with_rule_code(crate::codes::INSTALL_SOFTWARE_ID)
                .with_help(
                    "Align the query's package_id/bundle_identifier with the id the installed package actually registers; otherwise Fleet reinstalls until the 3-attempt cap, then stops. (The package id is read from the software file's `# <id> (...)` header comment.)",
                );
                if let (Some(l), Some(c)) = (line, col) {
                    err = err.with_location(l, c);
                }
                out.push((f.path.clone(), err));
            }
        }
    }
    out
}

/// Policy mappings defined in a file: a top-level sequence (policy lib file) or
/// inline items under `policies:` (fleet/team file). Path/glob refs are skipped
/// (the referenced policy file is processed on its own, with the correct base).
fn policies_in_file(f: &ParsedFile) -> Vec<&Value> {
    let mut out = Vec::new();
    match &f.yaml {
        Value::Sequence(seq) => out.extend(seq.iter().filter(|p| p.is_mapping())),
        Value::Mapping(_) => {
            if let Some(Value::Sequence(seq)) = f.yaml.get("policies") {
                out.extend(seq.iter().filter(|p| p.get("install_software").is_some()));
            }
        }
        _ => {}
    }
    out
}

/// Extract the value of `package_id = '…'` from an osquery WHERE clause
/// (handles optional spaces around `=`).
///
/// Only `package_id` (the `package_receipts` column) is read — never
/// `bundle_identifier`. The software file's comment records the `.pkg`'s package
/// id, which is comparable to a `package_receipts.package_id` check but NOT to an
/// `apps.bundle_identifier` check: a package id and an app bundle id are
/// different namespaces and legitimately differ. Comparing them would false-flag
/// every "installs a .app, queries the apps table" policy.
fn query_identifier(query: &str) -> Option<String> {
    let key = "package_id";
    let mut from = 0;
    while let Some(rel) = query[from..].find(key) {
        // Require a word boundary before so `some_package_id` doesn't match.
        let start = from + rel;
        let prev_ok = start == 0
            || !query.as_bytes()[start - 1].is_ascii_alphanumeric()
                && query.as_bytes()[start - 1] != b'_';
        let after = query[start + key.len()..].trim_start();
        if prev_ok {
            if let Some(after_eq) = after.strip_prefix('=') {
                if let Some(v) = after_eq.trim_start().strip_prefix('\'') {
                    if let Some(end) = v.find('\'') {
                        return Some(v[..end].to_string());
                    }
                }
            }
        }
        from = start + key.len();
    }
    None
}

/// Pull the identifier out of a `# <id> (<file>) version <ver>` header comment.
fn software_comment_identifier(source: &str) -> Option<String> {
    for line in source.lines() {
        let t = line.trim_start();
        if let Some(rest) = t.strip_prefix("# ") {
            if rest.contains(" version ") {
                if let Some(paren) = rest.find(" (") {
                    let id = rest[..paren].trim();
                    if id.contains('.') && !id.contains(' ') {
                        return Some(id.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Anchor a team-level finding at the most actionable line: the `software:`
/// section (where you'd add the package), else `policies:`, else `name:`.
fn anchor_line(source: &str) -> (Option<usize>, Option<usize>) {
    for key in ["software:", "policies:", "name:"] {
        for (i, line) in source.lines().enumerate() {
            if line.trim_start().starts_with(key) {
                let col = line.len() - line.trim_start().len() + 1;
                return (Some(i + 1), Some(col));
            }
        }
    }
    (None, None)
}

// ===========================================================================
// App Store apps require VPP (app-store-vpp)
// ===========================================================================

/// `software.app_store_apps` install via Apple's Volume Purchase Program, so a
/// fleet that declares them needs `volume_purchasing_program` configured under
/// org_settings (default.yml). Without it, `fleetctl gitops` can't apply the
/// App Store apps. Cross-file (apps live in fleet files, VPP in default.yml).
///
/// Only the missing-config **precondition** is checkable here — the VPP token,
/// `location` match against Apple Business, and app availability are server-
/// side. Hence a warning. (From the git-history cross-check: the #123–#130 red
/// streak was an app_store_apps / VPP failure.)
pub(crate) fn check_app_store_vpp(files: &[ParsedFile]) -> Vec<(PathBuf, LintError)> {
    let vpp_configured = files
        .iter()
        .any(|f| has_nonempty_seq_key(&f.yaml, "volume_purchasing_program"));
    if vpp_configured {
        return Vec::new();
    }

    let mut out = Vec::new();
    for f in files {
        let has_apps = f
            .yaml
            .get("software")
            .and_then(|s| s.get("app_store_apps"))
            .and_then(|v| v.as_sequence())
            .is_some_and(|s| !s.is_empty());
        if !has_apps {
            continue;
        }
        let (line, col) = find_line_containing(&f.source, "app_store_apps:");
        let mut err = LintError::warning(
            "app_store_apps is declared but no volume_purchasing_program (VPP) is configured in org_settings".to_string(),
            &f.path,
        )
        .with_rule_code(crate::codes::APP_STORE_VPP)
        .with_help(
            "Fleet installs App Store apps via Apple's Volume Purchase Program. Configure org_settings.volume_purchasing_program (location + fleets) in default.yml, or `fleetctl gitops` cannot apply app_store_apps.",
        );
        if let (Some(l), Some(c)) = (line, col) {
            err = err.with_location(l, c);
        }
        out.push((f.path.clone(), err));
    }
    out
}

/// Recursively: does `yaml` contain a mapping key `name` whose value is a
/// non-empty sequence (anywhere in the tree)?
fn has_nonempty_seq_key(yaml: &Value, name: &str) -> bool {
    match yaml {
        Value::Mapping(map) => {
            if let Some(Value::Sequence(s)) = map.get(Value::String(name.to_string())) {
                if !s.is_empty() {
                    return true;
                }
            }
            map.iter().any(|(_, v)| has_nonempty_seq_key(v, name))
        }
        Value::Sequence(seq) => seq.iter().any(|v| has_nonempty_seq_key(v, name)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(path: &str, source: &str) -> ParsedFile {
        let yaml: Value = serde_yaml::from_str(source).unwrap();
        ParsedFile { path: PathBuf::from(path), source: source.to_string(), yaml }
    }

    #[test]
    fn label_defined_in_repo_resolves() {
        let files = vec![parsed(
            "lib/labels/teams.yml",
            "- name: Engineering\n  query: SELECT 1;\n  label_membership_type: dynamic\n",
        )];
        let index = RepoIndex::build(&files);
        let policy = parsed(
            "teams/eng.yml",
            "policies:\n  - name: p\n    query: SELECT 1;\n    labels_include_any:\n      - Engineering\n",
        );
        let errs = check_references(&index, &policy.path, &policy.source, &policy.yaml, None);
        assert!(errs.is_empty(), "got: {errs:?}");
    }

    #[test]
    fn label_defined_in_nested_labels_dir_resolves() {
        // Regression: Fleet repos nest label files (labels/dynamic/*.yml).
        // detect_file_type only matches the immediate parent, so the index must
        // recognize a `labels/` ancestor at any depth — else this false-flags.
        let files = vec![parsed(
            "labels/dynamic/macos-27-hosts.yml",
            "- name: macOS 27 hosts\n  query: SELECT 1;\n  label_membership_type: dynamic\n",
        )];
        let index = RepoIndex::build(&files);
        let f = parsed(
            "fleets/abc.yml",
            "policies:\n  - name: p\n    query: SELECT 1;\n    labels_include_any:\n      - macOS 27 hosts\n",
        );
        let errs = check_references(&index, &f.path, &f.source, &f.yaml, None);
        assert!(errs.is_empty(), "nested label dir must resolve, got: {errs:?}");
    }

    #[test]
    fn builtin_label_is_not_flagged() {
        let index = RepoIndex::build(&[]);
        let f = parsed(
            "teams/eng.yml",
            "policies:\n  - name: p\n    query: SELECT 1;\n    labels_include_any:\n      - macOS\n      - All Hosts\n",
        );
        let errs = check_references(&index, &f.path, &f.source, &f.yaml, None);
        assert!(errs.is_empty(), "built-ins must not be flagged, got: {errs:?}");
    }

    #[test]
    fn undefined_label_is_flagged_with_suggestion() {
        let files = vec![parsed(
            "lib/labels/teams.yml",
            "- name: Engineering\n  query: SELECT 1;\n",
        )];
        let index = RepoIndex::build(&files);
        let f = parsed(
            "teams/eng.yml",
            "software:\n  packages:\n    - path: ./x.yml\n      labels_include_any:\n        - Enginering\n",
        );
        let errs = check_references(&index, &f.path, &f.source, &f.yaml, None);
        assert_eq!(errs.len(), 1, "got: {errs:?}");
        assert_eq!(errs[0].rule_code, Some("label-reference"));
        assert!(errs[0].help.as_ref().unwrap().contains("Engineering"));
        assert!(errs[0].line().is_some());
    }

    #[test]
    fn install_software_hash_resolves_to_repo_package() {
        let hash = "fd22528a87f3cfdb81aca981953aa5c8d7084581b9209bb69abf69c09a0afaaf";
        let files = vec![parsed(
            "lib/software/firefox.yml",
            &format!("hash_sha256: {hash}\nurl: https://e.com/f.pkg\n"),
        )];
        let index = RepoIndex::build(&files);
        let policy = parsed(
            "teams/eng.yml",
            &format!("policies:\n  - name: ff\n    query: SELECT 1;\n    install_software:\n      hash_sha256: {hash}\n"),
        );
        let errs = check_references(&index, &policy.path, &policy.source, &policy.yaml, None);
        assert!(errs.is_empty(), "got: {errs:?}");
    }

    #[test]
    fn install_software_hash_unresolved_is_flagged() {
        let index = RepoIndex::build(&[]);
        let policy = parsed(
            "teams/eng.yml",
            "policies:\n  - name: ff\n    query: SELECT 1;\n    install_software:\n      hash_sha256: deadbeef\n",
        );
        let errs = check_references(&index, &policy.path, &policy.source, &policy.yaml, None);
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].rule_code, Some("install-software-hash"));
    }

    #[test]
    fn inline_package_hash_is_indexed() {
        // A package defined inline (not a standalone file) still satisfies a
        // policy hash reference.
        let hash = "abc123";
        let files = vec![parsed(
            "teams/eng.yml",
            &format!("software:\n  packages:\n    - url: https://e.com/a.pkg\n      hash_sha256: {hash}\n"),
        )];
        let index = RepoIndex::build(&files);
        assert!(index.has_hash(hash));
    }

    // -- team-scoped install_software membership --

    const HASH: &str = "3a673c556d864348df3702a806be41bcdf44721976c7aacac41682aa159a3be2";

    /// The ABC-BETA scenario: a fleet pulls an autoinstall policy via a glob but
    /// does NOT include the package the policy installs.
    #[test]
    fn team_missing_package_for_globbed_policy_is_flagged() {
        let files = vec![
            // software file (hash-only, the upload-by-hash pattern)
            parsed(
                "platforms/macos/base/software/corp-fonts.yml",
                &format!("# com.x (Corp.pkg) version 1.0\n- hash_sha256: {HASH}\n"),
            ),
            // policy pulled by glob, installs the package by package_path
            parsed(
                "platforms/macos/policies/autoinstalls/corp-fonts.yml",
                "- name: Corp-Fonts is installed\n  platform: darwin\n  query: \"SELECT 1;\"\n  install_software:\n    package_path: ../../base/software/corp-fonts.yml\n",
            ),
            // fleet pulls the policy via glob but has NO software
            parsed(
                "fleets/ABC-BETA.yml",
                "name: ABC - XX\npolicies:\n  - paths: ../platforms/macos/policies/autoinstalls/*.yml\nsoftware:\n  packages:\n",
            ),
        ];
        let findings = check_team_membership(&files);
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        let (path, err) = &findings[0];
        assert!(path.ends_with("fleets/ABC-BETA.yml"));
        assert_eq!(err.rule_code, Some("install-software-team"));
        // fleetctl refuses the apply outright — "package_path SHA256 … not
        // found on team" — so this states a failure, not a suggestion. A repo
        // that wants it softer downgrades it via `[rules] warn`.
        assert_eq!(err.severity, crate::error::Severity::Error);
        assert!(err.message.contains("ABC - XX"));
        assert!(err.message.contains("Corp-Fonts is installed"));
    }

    /// The ABC-ALPHA scenario: same policy, but the fleet DOES include the
    /// package (by path → hash) — no finding.
    #[test]
    fn team_with_package_included_is_ok() {
        let files = vec![
            parsed(
                "platforms/macos/base/software/corp-fonts.yml",
                &format!("# com.x (Corp.pkg) version 1.0\n- hash_sha256: {HASH}\n"),
            ),
            parsed(
                "platforms/macos/policies/autoinstalls/corp-fonts.yml",
                "- name: Corp-Fonts is installed\n  platform: darwin\n  query: \"SELECT 1;\"\n  install_software:\n    package_path: ../../base/software/corp-fonts.yml\n",
            ),
            parsed(
                "fleets/ABC-ALPHA.yml",
                "name: ABC - ALPHA\npolicies:\n  - paths: ../platforms/macos/policies/autoinstalls/*.yml\nsoftware:\n  packages:\n    - path: ../platforms/macos/base/software/corp-fonts.yml\n",
            ),
        ];
        let findings = check_team_membership(&files);
        assert!(findings.is_empty(), "package is included → no finding, got: {findings:?}");
    }

    #[test]
    fn team_direct_hash_reference_not_in_team_is_flagged() {
        let files = vec![parsed(
            "fleets/t.yml",
            &format!("name: T\npolicies:\n  - name: p\n    query: \"SELECT 1;\"\n    install_software:\n      hash_sha256: {HASH}\nsoftware:\n  packages:\n"),
        )];
        let findings = check_team_membership(&files);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].1.rule_code, Some("install-software-team"));
    }

    #[test]
    fn non_team_files_are_not_checked() {
        // A policy with install_software outside fleets/teams (e.g. default.yml)
        // is not a team membership concern here.
        let files = vec![parsed(
            "default.yml",
            "policies:\n  - name: p\n    query: \"SELECT 1;\"\n    install_software:\n      hash_sha256: deadbeef\n",
        )];
        assert!(check_team_membership(&files).is_empty());
    }

    // -- install-software-id (query id vs installed package id) --

    #[test]
    fn query_id_mismatch_with_package_is_flagged() {
        // Query checks com.example… but the software file records
        // com.fleetdm… → the policy may install yet never pass.
        let files = vec![
            parsed(
                "platforms/macos/base/software/corp-fonts.yml",
                "# com.fleetdm.fonts.corp (Corp-Fonts-1.0.pkg) version 1.0\n- hash_sha256: abc123\n",
            ),
            parsed(
                "platforms/macos/policies/autoinstalls/corp-fonts.yml",
                "- name: Corp-Fonts is installed\n  platform: darwin\n  query: \"SELECT 1 FROM package_receipts WHERE package_id = 'com.example.fonts.corp';\"\n  install_software:\n    package_path: ../../base/software/corp-fonts.yml\n",
            ),
        ];
        let findings = check_package_id_match(&files);
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        let (_, err) = &findings[0];
        assert_eq!(err.rule_code, Some("install-software-id"));
        assert!(err.message.contains("com.example.fonts.corp"));
        assert!(err.message.contains("com.fleetdm.fonts.corp"));
    }

    #[test]
    fn query_id_matching_package_is_ok() {
        let files = vec![
            parsed(
                "platforms/macos/base/software/corp-fonts.yml",
                "# com.example.fonts.corp (Corp-Fonts-1.0.pkg) version 1.0\n- hash_sha256: abc123\n",
            ),
            parsed(
                "platforms/macos/policies/autoinstalls/corp-fonts.yml",
                "- name: Corp-Fonts is installed\n  platform: darwin\n  query: \"SELECT 1 FROM package_receipts WHERE package_id = 'com.example.fonts.corp';\"\n  install_software:\n    package_path: ../../base/software/corp-fonts.yml\n",
            ),
        ];
        assert!(check_package_id_match(&files).is_empty());
    }

    #[test]
    fn query_id_bundle_identifier_not_compared() {
        // A policy that queries the apps table by bundle_identifier must NOT be
        // compared to the .pkg's package_id (different namespaces) — no finding.
        let files = vec![
            parsed(
                "software/gp.yml",
                "# com.paloaltonetworks.globalprotect.pkg (GP.pkg) version 6.2\n- hash_sha256: abc\n",
            ),
            parsed(
                "policies/gp.yml",
                "- name: GP\n  query: \"SELECT 1 FROM apps WHERE bundle_identifier='com.paloaltonetworks.GlobalProtect.client';\"\n  install_software:\n    package_path: ../software/gp.yml\n",
            ),
        ];
        assert!(
            check_package_id_match(&files).is_empty(),
            "bundle_identifier vs package_id must not be compared"
        );
    }

    // -- app-store-vpp --

    #[test]
    fn app_store_apps_without_vpp_is_flagged() {
        let files = vec![parsed(
            "fleets/ABC-ALPHA.yml",
            "name: ABC - ALPHA\nsoftware:\n  app_store_apps:\n    - app_store_id: \"1037126344\"\n      platform: darwin\n",
        )];
        let findings = check_app_store_vpp(&files);
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert_eq!(findings[0].1.rule_code, Some("app-store-vpp"));
        assert!(findings[0].1.message.contains("volume_purchasing_program"));
    }

    #[test]
    fn app_store_apps_with_vpp_configured_is_ok() {
        // VPP configured in default.yml (another file) satisfies the precondition.
        let files = vec![
            parsed(
                "default.yml",
                "org_settings:\n  volume_purchasing_program:\n    - location: \"HQ\"\n      fleets:\n        - All teams\n",
            ),
            parsed(
                "fleets/ABC-ALPHA.yml",
                "name: ABC - ALPHA\nsoftware:\n  app_store_apps:\n    - app_store_id: \"1037126344\"\n      platform: darwin\n",
            ),
        ];
        assert!(check_app_store_vpp(&files).is_empty(), "VPP configured → no finding");
    }

    #[test]
    fn no_app_store_apps_no_finding() {
        let files = vec![parsed(
            "fleets/t.yml",
            "name: t\nsoftware:\n  packages:\n    - path: ../lib/a.yml\n",
        )];
        assert!(check_app_store_vpp(&files).is_empty());
    }

    #[test]
    fn query_id_helpers() {
        assert_eq!(query_identifier("WHERE package_id = 'a.b.c'").as_deref(), Some("a.b.c"));
        assert_eq!(query_identifier("package_id='x.y'").as_deref(), Some("x.y"));
        // bundle_identifier is deliberately not extracted.
        assert_eq!(query_identifier("bundle_identifier='x.y'").as_deref(), None);
        assert_eq!(query_identifier("SELECT 1 FROM uptime").as_deref(), None);
        assert_eq!(
            software_comment_identifier("# com.a.b (F.pkg) version 1.0\n- hash_sha256: x\n").as_deref(),
            Some("com.a.b")
        );
    }
}
