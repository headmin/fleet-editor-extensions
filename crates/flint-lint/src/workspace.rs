//! Repo-wide Workspace and the `WorkspaceRule` trait (ADR-010 Phase 1).
//!
//! The Workspace extends the cross-reference graph pass with what
//! `RepoIndex` alone can't see: the full exact-case file set (one walk) and
//! payload content hashes. Rules over it are index lookups — never per-file
//! process work — and attach findings to whichever file makes the defect
//! actionable, with `LintError::related` carrying the other path involved.
//!
//! Resolution semantics: references resolve relative to the referring
//! file's parent (workspace-root-relative resolution false-positived in the
//! field; see flint-lsp/src/workspace.rs).

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde_yaml::Value;

use super::codes;
use super::config::compile_glob;
use super::cross_reference::ParsedFile;
use super::error::LintError;
use super::util::normalize_path;

/// Extensions of payload files worth content-hashing for duplicate
/// detection. Deliberately excludes images and archives.
const PAYLOAD_EXTS: &[&str] = &["mobileconfig", "json", "sh", "ps1", "py", "xml"];

/// Directory names never worth walking.
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", ".venv", "venv"];

/// The repo-wide view workspace rules query.
pub struct Workspace<'a> {
    /// Every file under the root, exact-case as reported by the filesystem,
    /// lexically normalized. Reference resolution runs against THIS set,
    /// never `Path::exists()` — macOS answers case-insensitively and
    /// case-only drift must be caught (playground commit ccbd016).
    pub files: Vec<PathBuf>,
    /// Lowercased path string -> indices into `files`, for case-collision
    /// and case-insensitive resolution.
    pub by_lower: HashMap<String, Vec<usize>>,
    /// The parsed YAML sources the graph pass already holds.
    pub parsed: &'a [ParsedFile],
}

impl<'a> Workspace<'a> {
    /// One walk from `root`; everything else is derived from it.
    pub fn build(root: &Path, parsed: &'a [ParsedFile]) -> Self {
        let mut files = Vec::new();
        walk(root, &mut files);
        Self::from_files(files, parsed)
    }

    /// Build from a pre-collected file set — the testable core (a real
    /// case-collision can't be created on a case-insensitive dev machine),
    /// and the seam a future `--staged` mode can feed a git file list into.
    pub fn from_files(mut files: Vec<PathBuf>, parsed: &'a [ParsedFile]) -> Self {
        files.sort();
        let mut by_lower: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, f) in files.iter().enumerate() {
            by_lower
                .entry(f.to_string_lossy().to_lowercase())
                .or_default()
                .push(i);
        }
        Workspace {
            files,
            by_lower,
            parsed,
        }
    }
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            // Dot-dirs (.github, .vscode, …) hold tooling, never payloads.
            if !SKIP_DIRS.contains(&name.as_ref()) && !name.starts_with('.') {
                walk(&path, out);
            }
        } else {
            out.push(normalize_path(&path));
        }
    }
}

/// A rule over the whole Workspace. Findings name the file they attach to;
/// the engine routes them through the same suppression/severity controls as
/// per-file rules.
pub trait WorkspaceRule: Sync {
    /// The diagnostic code — always one of the [`codes`] consts, which is
    /// also how `.fleetlint.toml` `[rules] disabled` gates the rule.
    fn code(&self) -> &'static str;
    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)>;
}

/// The built-in workspace rules, in reporting order.
pub fn workspace_rules() -> Vec<Box<dyn WorkspaceRule>> {
    vec![
        Box::new(BrokenReferenceRule),
        Box::new(CaseCollisionRule),
        Box::new(UnregisteredScriptRule),
        Box::new(DuplicateContentRule),
        Box::new(OrphanedFileRule),
        Box::new(DuplicateIdentifierRule),
        Box::new(ProfileWellFormedRule),
        Box::new(DuplicateFleetNameRule),
    ]
}

// ---------------------------------------------------------------------------
// duplicate-identifier: same PayloadIdentifier, different content, one fleet
// ---------------------------------------------------------------------------

/// Two profiles in ONE fleet carrying the same `PayloadIdentifier` but
/// different content: the host keeps whichever installed last, silently.
/// Deliberately per-fleet — the same identifier across different fleets is
/// legal (ADR-010 amendment; complements `duplicate-payload-uuid`, which
/// keys on PayloadUUID without content comparison). Payloads are read once
/// across all fleets via a shared cache.
pub struct DuplicateIdentifierRule;

impl WorkspaceRule for DuplicateIdentifierRule {
    fn code(&self) -> &'static str {
        codes::DUPLICATE_IDENTIFIER
    }

    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)> {
        // Payloads read once across every fleet that references them.
        let mut content_cache: HashMap<PathBuf, Option<String>> = HashMap::new();
        let mut read = |p: &PathBuf| -> Option<String> {
            content_cache
                .entry(p.clone())
                .or_insert_with(|| std::fs::read_to_string(p).ok())
                .clone()
        };

        let mut findings = Vec::new();
        for pf in ws.parsed {
            let base = pf.path.parent().unwrap_or(Path::new(""));

            // This fleet's referenced .mobileconfig payloads.
            let mut refs: Vec<PathBuf> = Vec::new();
            let mut singles = Vec::new();
            collect_string_values(&pf.yaml, "path", &mut singles);
            for rel in &singles {
                if rel.ends_with(".mobileconfig") && !rel.contains('$') {
                    refs.push(normalize_path(&base.join(rel)));
                }
            }
            let mut globs = Vec::new();
            collect_string_values(&pf.yaml, "paths", &mut globs);
            for pattern in &globs {
                if pattern.contains('$') {
                    continue;
                }
                let pat = normalize_path(&base.join(pattern));
                let pat_s = pat.to_string_lossy().replace('\\', "/");
                if let Some(matcher) = compile_glob(&pat_s) {
                    for f in &ws.files {
                        if f.extension().is_some_and(|e| e == "mobileconfig")
                            && matcher.is_match(f.to_string_lossy().replace('\\', "/"))
                        {
                            refs.push(f.clone());
                        }
                    }
                }
            }
            refs.sort();
            refs.dedup();

            // Group by PayloadIdentifier; flag groups whose contents differ.
            let mut by_id: HashMap<String, Vec<(PathBuf, String)>> = HashMap::new();
            for r in refs {
                let Some(content) = read(&r) else { continue };
                if let Some(id) = super::profile::parse_mobileconfig(&content).identifier {
                    // Canonical form, not raw bytes: two profiles differing only
                    // in XML escaping (`&quot;` vs `"`) decode identically and
                    // Fleet delivers them the same, so comparing bytes reports
                    // a divergence that does not exist.
                    by_id
                        .entry(id)
                        .or_default()
                        .push((r, super::profile::canonical_profile(&content)));
                }
            }
            for (id, group) in by_id {
                if group.len() < 2 || group.windows(2).all(|w| w[0].1 == w[1].1) {
                    continue;
                }
                let mut err = LintError::warning(
                    format!(
                        "'{}' and '{}' both use PayloadIdentifier '{}' with different \
                         content — the host keeps whichever installed last",
                        group[0].0.display(),
                        group[1].0.display(),
                        id,
                    ),
                    pf.path.as_path(),
                )
                .with_help(
                    "Give each profile a unique PayloadIdentifier \
                     (`flint gen profile --regen-uuid`)"
                        .to_string(),
                );
                err.rule_code = Some(codes::DUPLICATE_IDENTIFIER);
                for (p, _) in &group {
                    err = err.with_related(p.clone());
                }
                findings.push((pf.path.clone(), err));
            }
        }
        findings
    }
}

// ---------------------------------------------------------------------------
// orphaned-file: an artifact no config file references
// ---------------------------------------------------------------------------

/// A payload/script committed to the repo but referenced by nothing — dead
/// weight that silently stops being applied (a zsh rule; the
/// WorkspaceRule successor to `flint paths --unwired`'s CLI-only report,
/// minus its hardcoded fleets/-only discovery). Warn: orphans are sometimes
/// parked deliberately.
pub struct OrphanedFileRule;

/// Is `f` the kind of file a GitOps repo wires into configs?
/// Mirrors `unwired::is_artifact`: profiles and scripts always; JSON only
/// under declaration/enrollment profile dirs (everything else JSON is
/// tooling config).
fn is_artifact(f: &Path) -> bool {
    let Some(ext) = f.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    match ext {
        "mobileconfig" => true,
        // Scripts are ambiguous — repos also hold CI tooling and
        // package-internal scripts (e.g. a pkg's payload tree). Only a
        // `scripts/` ancestor marks one as GitOps-wireable.
        "sh" | "ps1" | "py" => f
            .ancestors()
            .skip(1)
            .any(|a| a.file_name().is_some_and(|n| n == "scripts")),
        "json" => {
            let s = f.to_string_lossy();
            s.contains("declaration-profiles") || s.contains("enrollment-profiles")
        }
        _ => false,
    }
}

impl WorkspaceRule for OrphanedFileRule {
    fn code(&self) -> &'static str {
        codes::ORPHANED_FILE
    }

    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)> {
        // Everything any parsed config references, single paths resolved and
        // globs expanded against the workspace set.
        let mut referenced: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
        for pf in ws.parsed {
            referenced.extend(referenced_by(pf, ws));
        }

        ws.files
            .iter()
            .filter(|f| is_artifact(f) && !referenced.contains(*f))
            .map(|f| {
                let mut err = LintError::warning(
                    format!("'{}' is referenced by no config file", f.display()),
                    f.as_path(),
                )
                .with_help(
                    "Wire it into a fleet file (or delete it) — unreferenced payloads \
                     are never applied. `flint paths --unwired --interactive` can wire it."
                        .to_string(),
                );
                err.rule_code = Some(codes::ORPHANED_FILE);
                (f.clone(), err)
            })
            .collect()
    }
}

// ---------------------------------------------------------------------------
// profile-well-formed / payload-uuid-format: every profile in the repo
// ---------------------------------------------------------------------------

/// Validates every configuration profile and DDM declaration **in the repo**,
/// not only the ones a fleet happens to reference.
///
/// A workspace rule rather than a per-file one for three reasons the per-file
/// version got wrong: a profile pulled in by N fleets' globs produced N copies
/// of the same finding; an artifact nothing references yet was never checked at
/// all, so its defect surfaced only once someone wired it up; and the finding
/// had nowhere to point but the fleet YAML. Reporting on the artifact fixes all
/// three at once.
pub struct ProfileWellFormedRule;

impl WorkspaceRule for ProfileWellFormedRule {
    fn code(&self) -> &'static str {
        codes::PROFILE_WELL_FORMED
    }

    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)> {
        let mut findings = Vec::new();
        for f in &ws.files {
            let ext = f.extension().and_then(|e| e.to_str()).unwrap_or_default();
            // `.json` is only this rule's business when it really is a DDM
            // declaration — the repo is full of other JSON.
            if ext == "json" {
                let Ok(bytes) = std::fs::read(f) else { continue };
                if !super::profile::looks_like_declaration(&bytes) {
                    continue;
                }
            } else if ext != "mobileconfig" {
                continue;
            }
            for err in super::profile::scan_and_report(f) {
                findings.push((f.clone(), err));
            }
        }
        findings
    }
}

// ---------------------------------------------------------------------------
// broken-reference: a `paths:` glob that matches zero files
// ---------------------------------------------------------------------------

/// `paths:` globs that resolve to nothing — the classic aftermath of a
/// folder rename (playground commit 89d0cc2 emptied nine folders that fleet
/// globs still pointed at). Single `path:` references are `path-exists`'s
/// job; this rule owns the glob half it can't see.
pub struct BrokenReferenceRule;

impl WorkspaceRule for BrokenReferenceRule {
    fn code(&self) -> &'static str {
        codes::BROKEN_REFERENCE
    }

    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)> {
        let file_strs: Vec<String> = ws
            .files
            .iter()
            .map(|f| f.to_string_lossy().replace('\\', "/"))
            .collect();

        let mut findings = Vec::new();
        for pf in ws.parsed {
            let base = pf.path.parent().unwrap_or(Path::new(""));
            let mut patterns = Vec::new();
            collect_string_values(&pf.yaml, "paths", &mut patterns);
            for pattern in patterns {
                // Env-var indirection and URLs can't be resolved statically.
                if pattern.contains('$') || pattern.contains("://") {
                    continue;
                }
                let joined = normalize_path(&base.join(&pattern));
                let pat_s = joined.to_string_lossy().replace('\\', "/");
                let Some(matcher) = compile_glob(&pat_s) else {
                    continue;
                };
                if file_strs.iter().any(|f| matcher.is_match(f)) {
                    continue;
                }
                // WARNING, not error: Fleet does NOT reject a zero-match glob.
                // `expandBaseItems` (pkg/spec/gitops.go:1833) logs
                // "[!] glob pattern %q matched no %s files" and `continue`s
                // without appending to errs — every entity type routes through
                // that one function. Verified end-to-end by running the real
                // `fleetctl gitops --dry-run` against a mocked datastore: the
                // run printed the [!] line and reported "dry run succeeded".
                // An empty glob is still almost always a mistake, so it stays
                // reported — at Fleet's own severity.
                let mut err = LintError::warning(
                    format!("'paths: {}' matches no files in the repo", pattern),
                    pf.path.as_path(),
                )
                .with_help(
                    "Fleet skips a glob that matches nothing (it is not an apply \
                     failure), but the folder may have been renamed or emptied — \
                     update the glob or restore the files it targets"
                        .to_string(),
                );
                err.rule_code = Some(codes::BROKEN_REFERENCE);
                if let Some(span) = find_value_span(&pf.source, "paths", &pattern) {
                    err = err.with_location(span.0, span.1);
                }
                findings.push((pf.path.clone(), err));
            }
        }
        findings
    }
}

// ---------------------------------------------------------------------------
// case-collision: two paths that differ only by case
// ---------------------------------------------------------------------------

/// Two repo paths differing only by case merge on a case-insensitive
/// filesystem (macOS default) and explode on a case-sensitive one (Linux
/// CI, `git checkout`) — playground commit ccbd016's failure class, the
/// half of it the per-file `path-case` rule can't see.
pub struct CaseCollisionRule;

impl WorkspaceRule for CaseCollisionRule {
    fn code(&self) -> &'static str {
        codes::CASE_COLLISION
    }

    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)> {
        let mut findings = Vec::new();
        for indices in ws.by_lower.values() {
            if indices.len() < 2 {
                continue;
            }
            for &i in indices {
                let this = &ws.files[i];
                let others: Vec<&PathBuf> = indices
                    .iter()
                    .filter(|&&j| j != i)
                    .map(|&j| &ws.files[j])
                    .collect();
                let mut err = LintError::error(
                    format!(
                        "'{}' collides with '{}' — the paths differ only by case",
                        this.display(),
                        others[0].display()
                    ),
                    this.as_path(),
                )
                .with_help(
                    "Case-insensitive filesystems (macOS default) treat these as one \
                     file; case-sensitive checkouts (Linux CI) as two. Rename one."
                        .to_string(),
                );
                err.rule_code = Some(codes::CASE_COLLISION);
                for o in others {
                    err = err.with_related(o.clone());
                }
                findings.push((this.clone(), err));
            }
        }
        findings
    }
}

// ---------------------------------------------------------------------------
// unregistered-script: policy run_script not under any controls.scripts
// ---------------------------------------------------------------------------

/// A policy's `run_script.path` must be registered under **that fleet's**
/// `controls.scripts`, or Fleet refuses the automation at apply time
/// (zsh rule FL-009).
///
/// Scoped per fleet, deliberately. This check used to accept a script
/// registered by *any* fleet, so a script declared in one fleet satisfied a
/// policy applied by another — which is exactly how `set-wifi-autojoin.sh`
/// reached production: registered in one fleet, run by eight others.
/// `fleetctl` validates per team ("was not defined in controls for ABC - GHI"),
/// and this now matches it.
pub struct UnregisteredScriptRule;

/// A fleet config declares `controls:` or `policies:` at the top level; a
/// policy file is a bare sequence and has neither.
fn is_fleet_config(pf: &ParsedFile) -> bool {
    pf.yaml.get("controls").is_some() || pf.yaml.get("policies").is_some()
}

/// The scripts one fleet registers under `controls.scripts`, resolved.
fn registered_scripts(fleet: &ParsedFile, ws: &Workspace) -> std::collections::HashSet<PathBuf> {
    let base = fleet.path.parent().unwrap_or(Path::new(""));
    let mut out = std::collections::HashSet::new();
    let Some(scripts) = fleet
        .yaml
        .get("controls")
        .and_then(|c| c.get("scripts"))
        .and_then(Value::as_sequence)
    else {
        return out;
    };
    for item in scripts {
        if let Some(rel) = item.get("path").and_then(Value::as_str).or_else(|| item.as_str()) {
            out.insert(normalize_path(&base.join(rel)));
        }
        if let Some(glob) = item.get("paths").and_then(Value::as_str) {
            let pat = normalize_path(&base.join(glob));
            let pat_s = pat.to_string_lossy().replace('\\', "/");
            if let Some(matcher) = compile_glob(&pat_s) {
                for f in &ws.files {
                    if matcher.is_match(f.to_string_lossy().replace('\\', "/")) {
                        out.insert(f.clone());
                    }
                }
            }
        }
    }
    out
}

/// Every policy this fleet applies, paired with the file that declares it —
/// inline entries belong to the fleet, referenced ones to the policy file,
/// because `run_script.path` resolves relative to whichever file it is written
/// in.
fn policy_sources<'a>(
    fleet: &'a ParsedFile,
    ws: &'a Workspace,
    by_path: &std::collections::HashMap<PathBuf, &'a ParsedFile>,
) -> Vec<(PathBuf, &'a Value)> {
    let base = fleet.path.parent().unwrap_or(Path::new(""));
    let mut out = Vec::new();
    let Some(items) = fleet.yaml.get("policies").and_then(Value::as_sequence) else {
        return out;
    };
    for item in items {
        let single = item.get("path").and_then(Value::as_str);
        let glob = item.get("paths").and_then(Value::as_str);
        if single.is_none() && glob.is_none() {
            // An inline policy: it lives in the fleet file itself.
            out.push((fleet.path.clone(), item));
            continue;
        }
        let mut targets: Vec<PathBuf> = Vec::new();
        if let Some(rel) = single {
            if !rel.contains('$') {
                targets.push(normalize_path(&base.join(rel)));
            }
        }
        if let Some(pattern) = glob {
            if !pattern.contains('$') {
                let pat = normalize_path(&base.join(pattern));
                let pat_s = pat.to_string_lossy().replace('\\', "/");
                if let Some(matcher) = compile_glob(&pat_s) {
                    for f in &ws.files {
                        if matcher.is_match(f.to_string_lossy().replace('\\', "/")) {
                            targets.push(f.clone());
                        }
                    }
                }
            }
        }
        for t in targets {
            if let Some(pf) = by_path.get(&t) {
                out.push((pf.path.clone(), &pf.yaml));
            }
        }
    }
    out
}

impl WorkspaceRule for UnregisteredScriptRule {
    fn code(&self) -> &'static str {
        codes::UNREGISTERED_SCRIPT
    }

    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)> {
        let by_path: std::collections::HashMap<PathBuf, &ParsedFile> = ws
            .parsed
            .iter()
            .map(|pf| (normalize_path(&pf.path), pf))
            .collect();

        let mut findings = Vec::new();
        for fleet in ws.parsed.iter().filter(|pf| is_fleet_config(pf)) {
            let registered = registered_scripts(fleet, ws);
            let fleet_label = fleet
                .yaml
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| fleet.path.display().to_string());

            let mut seen: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
            for (src_path, src_yaml) in policy_sources(fleet, ws, &by_path) {
                let src_base = src_path.parent().unwrap_or(Path::new("")).to_path_buf();
                let mut run_scripts = Vec::new();
                collect_run_scripts(src_yaml, &mut run_scripts);
                for rel in run_scripts {
                    if rel.contains('$') {
                        continue;
                    }
                    let resolved = normalize_path(&src_base.join(&rel));
                    if registered.contains(&resolved) || !seen.insert(resolved.clone()) {
                        continue;
                    }
                    let mut err = LintError::error(
                        format!(
                            "Policy runs '{}', but fleet '{}' does not register it under \
                             'controls.scripts'",
                            rel, fleet_label
                        ),
                        fleet.path.as_path(),
                    )
                    .with_help(format!(
                        "Fleet only runs scripts the team itself registers — add it to \
                         {}'s controls.scripts. Registering it in another fleet does not \
                         count.",
                        fleet
                            .path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("this fleet")
                    ))
                    .with_related(resolved);
                    err.rule_code = Some(codes::UNREGISTERED_SCRIPT);
                    if src_path == fleet.path {
                        if let Some(span) = find_value_span(&fleet.source, "path", &rel) {
                            err = err.with_location(span.0, span.1);
                        }
                    } else {
                        err = err.with_related(src_path.clone());
                    }
                    findings.push((fleet.path.clone(), err));
                }
            }
        }
        findings
    }
}

// ---------------------------------------------------------------------------
// duplicate-fleet-name: two fleet files claiming the same team
// ---------------------------------------------------------------------------

/// Two fleet files declaring the same `name:` do not conflict — Fleet
/// collapses them server-side and the second silently wins, so one team
/// quietly ceases to exist. Nothing in either file is wrong, which is why no
/// per-file rule can see it.
///
/// Quoted and unquoted forms compare equal because both parse to the same
/// scalar, so `name: "ABC - ACME"` and `name: ABC - ACME` are one name.
pub struct DuplicateFleetNameRule;

impl WorkspaceRule for DuplicateFleetNameRule {
    fn code(&self) -> &'static str {
        codes::DUPLICATE_FLEET_NAME
    }

    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)> {
        let mut by_name: HashMap<&str, Vec<&PathBuf>> = HashMap::new();
        for pf in ws.parsed.iter().filter(|pf| is_fleet_config(pf)) {
            if let Some(name) = pf.yaml.get("name").and_then(Value::as_str) {
                let trimmed = name.trim();
                if !trimmed.is_empty() {
                    by_name.entry(trimmed).or_default().push(&pf.path);
                }
            }
        }

        let mut dups: Vec<_> = by_name.into_iter().filter(|(_, v)| v.len() > 1).collect();
        dups.sort_by_key(|(name, _)| *name);

        let mut findings = Vec::new();
        for (name, mut files) in dups {
            files.sort();
            for (i, path) in files.iter().enumerate() {
                let others: Vec<&PathBuf> =
                    files.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, p)| *p).collect();
                let mut err = LintError::error(
                    format!(
                        "fleet name '{}' is also declared by '{}' — Fleet keeps only one of them",
                        name,
                        others[0].display()
                    ),
                    path.as_path(),
                )
                .with_help(
                    "Two files declaring one team do not conflict at apply time; the second \
                     overwrites the first and a team silently disappears. Give one of them a \
                     distinct name, or delete the copy."
                        .to_string(),
                );
                err.rule_code = Some(codes::DUPLICATE_FLEET_NAME);
                for o in others {
                    err = err.with_related(o.clone());
                }
                findings.push(((*path).clone(), err));
            }
        }
        findings
    }
}

// ---------------------------------------------------------------------------
// duplicate-content: byte-identical payloads at two paths
// ---------------------------------------------------------------------------

/// Byte-identical payload files are copy-paste divergence waiting to happen
/// (playground: 17 byte-identical payloads across brand folders). Grouped
/// by (size, hash), then byte-confirmed so a u64 hash collision can never
/// produce a false positive.
pub struct DuplicateContentRule;

impl WorkspaceRule for DuplicateContentRule {
    fn code(&self) -> &'static str {
        codes::DUPLICATE_CONTENT
    }

    fn check(&self, ws: &Workspace) -> Vec<(PathBuf, LintError)> {
        // (size, hash) -> candidate duplicates with their contents.
        type DupGroups<'f> = HashMap<(u64, u64), Vec<(&'f PathBuf, Vec<u8>)>>;
        let mut by_key: DupGroups = HashMap::new();
        for f in &ws.files {
            let Some(ext) = f.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !PAYLOAD_EXTS.contains(&ext) {
                continue;
            }
            let Ok(raw) = std::fs::read(f) else {
                continue;
            };
            if raw.is_empty() {
                continue;
            }
            // Compare profiles by canonical form so a copy that differs only in
            // XML escaping or whitespace is still recognised as the duplicate
            // it is; other payload types stay byte-exact.
            let content = match (ext, std::str::from_utf8(&raw)) {
                ("mobileconfig", Ok(text)) => {
                    super::profile::canonical_profile(text).into_bytes()
                }
                _ => raw,
            };
            let mut hasher = DefaultHasher::new();
            content.hash(&mut hasher);
            by_key
                .entry((content.len() as u64, hasher.finish()))
                .or_default()
                .push((f, content));
        }

        let mut findings = Vec::new();
        for group in by_key.values() {
            if group.len() < 2 {
                continue;
            }
            // Byte-confirm within the group.
            for (i, (path, content)) in group.iter().enumerate() {
                let twins: Vec<&PathBuf> = group
                    .iter()
                    .enumerate()
                    .filter(|(j, (_, c))| *j != i && c == content)
                    .map(|(_, (p, _))| *p)
                    .collect();
                if twins.is_empty() {
                    continue;
                }
                let same_bytes = std::fs::read(path).ok().is_some_and(|a| {
                    std::fs::read(twins[0]).ok().is_some_and(|b| a == b)
                });
                let how = if same_bytes {
                    "byte-identical to"
                } else {
                    "identical to, apart from XML escaping or whitespace,"
                };
                let mut err = LintError::warning(
                    format!("'{}' is {how} '{}'", path.display(), twins[0].display()),
                    path.as_path(),
                )
                .with_help(
                    "Duplicated payloads drift apart silently — reference one copy from \
                     both places, or make the difference real"
                        .to_string(),
                );
                err.rule_code = Some(codes::DUPLICATE_CONTENT);
                for t in twins {
                    err = err.with_related((*t).clone());
                }
                findings.push(((*path).clone(), err));
            }
        }
        findings
    }
}

/// Every workspace file `pf` references: each path-bearing key's value
/// (see [`crate::yaml_utils::PATH_BEARING_KEYS`]) resolved relative to the
/// referencing file's parent, plus `paths:` globs expanded against the
/// workspace set. Shared by `orphaned-file` and the `must-be-referenced`
/// pattern assertion.
pub(crate) fn referenced_by(
    pf: &ParsedFile,
    ws: &Workspace,
) -> std::collections::HashSet<PathBuf> {
    let base = pf.path.parent().unwrap_or(Path::new(""));
    let mut out = std::collections::HashSet::new();
    // Every key Fleet resolves as a path, not just `path:` — shared with
    // `path-exists` so a wired file cannot be invisible here while a typo in
    // the same key goes unreported there.
    let singles = crate::yaml_utils::collect_path_values(&pf.yaml);
    for rel in &singles {
        if rel.contains('$') || rel.contains("://") {
            continue;
        }
        out.insert(normalize_path(&base.join(rel)));
    }
    let mut globs = Vec::new();
    collect_string_values(&pf.yaml, "paths", &mut globs);
    for pattern in &globs {
        if pattern.contains('$') || pattern.contains("://") {
            continue;
        }
        let pat = normalize_path(&base.join(pattern));
        let pat_s = pat.to_string_lossy().replace('\\', "/");
        if let Some(matcher) = compile_glob(&pat_s) {
            for f in &ws.files {
                if matcher.is_match(f.to_string_lossy().replace('\\', "/")) {
                    out.insert(f.clone());
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// YAML walking helpers
// ---------------------------------------------------------------------------

/// Collect every string value of `key` anywhere in the document.
fn collect_string_values(value: &Value, key: &str, out: &mut Vec<String>) {
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                if k.as_str() == Some(key) {
                    if let Some(s) = v.as_str() {
                        out.push(s.to_string());
                    }
                }
                collect_string_values(v, key, out);
            }
        }
        Value::Sequence(seq) => {
            for item in seq {
                collect_string_values(item, key, out);
            }
        }
        _ => {}
    }
}

/// Collect every `run_script.path` value (policy automations).
fn collect_run_scripts(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                if k.as_str() == Some("run_script") {
                    if let Some(p) = v.get("path").and_then(Value::as_str) {
                        out.push(p.to_string());
                    }
                }
                collect_run_scripts(v, out);
            }
        }
        Value::Sequence(seq) => {
            for item in seq {
                collect_run_scripts(item, out);
            }
        }
        _ => {}
    }
}

/// Locate `key: value` in the source; returns (line, column) 1-based.
fn find_value_span(source: &str, key: &str, value: &str) -> Option<(usize, usize)> {
    for (idx, line) in source.lines().enumerate() {
        let trimmed = line.trim_start();
        let Some(after) = trimmed
            .strip_prefix(&format!("{}:", key))
            .or_else(|| trimmed.strip_prefix(&format!("- {}:", key)))
        else {
            continue;
        };
        let after = after.trim().trim_matches('"').trim_matches('\'');
        if after == value {
            let col = line.find(key).unwrap_or(0) + 1;
            return Some((idx + 1, col));
        }
    }
    None
}

#[cfg(test)]
mod tests_support {
    use super::*;
    pub(super) fn parsed(path: &str, source: &str) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from(path),
            source: source.to_string(),
            yaml: serde_yaml::from_str(source).unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Severity;

    fn parsed(path: &str, source: &str) -> ParsedFile {
        ParsedFile {
            path: PathBuf::from(path),
            source: source.to_string(),
            yaml: serde_yaml::from_str(source).unwrap(),
        }
    }

    #[test]
    fn broken_reference_glob_zero() {
        // Replay class 89d0cc2: a folder rename left globs matching nothing.
        let fleet = parsed(
            "fleets/eng.yml",
            r#"
controls:
  apple_settings:
    configuration_profiles:
      - paths: ../platforms/macos/site/vpn/configuration-profiles/*.mobileconfig
"#,
        );
        let files_hit = vec![PathBuf::from(
            "platforms/macos/site/vpn/configuration-profiles/tunnel.mobileconfig",
        )];
        let files_miss = vec![PathBuf::from(
            "platforms/macos/site/openvpn/configuration-profiles/tunnel.mobileconfig",
        )];
        let binding = [fleet];

        let ws = Workspace::from_files(files_hit, &binding);
        assert!(BrokenReferenceRule.check(&ws).is_empty());

        let ws = Workspace::from_files(files_miss, &binding);
        let findings = BrokenReferenceRule.check(&ws);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].1.message.contains("matches no files"));
        assert!(findings[0].1.span.is_some(), "span should be located");
        // Severity must stay WARNING: Fleet logs a zero-match glob and
        // continues (pkg/spec/gitops.go:1833), so it never blocks an apply.
        // Confirmed by running the real `fleetctl gitops --dry-run` against a
        // mocked datastore — it printed "[!] glob pattern ... matched no
        // report files" and then "gitops dry run succeeded".
        assert_eq!(
            findings[0].1.severity,
            Severity::Warning,
            "glob-zero must not gate an apply — Fleet accepts it"
        );
    }

    #[test]
    fn broken_reference_skips_env_vars() {
        let fleet = parsed(
            "fleets/eng.yml",
            "controls:\n  scripts:\n    - paths: $FLEET_VAR_DIR/*.sh\n",
        );
        let binding = [fleet];
        let ws = Workspace::from_files(vec![], &binding);
        assert!(BrokenReferenceRule.check(&ws).is_empty());
    }

    #[test]
    fn case_collision_pairs() {
        let binding = [];
        let ws = Workspace::from_files(
            vec![
                PathBuf::from("scripts/Setup.sh"),
                PathBuf::from("scripts/setup.sh"),
                PathBuf::from("scripts/other.sh"),
            ],
            &binding,
        );
        let findings = CaseCollisionRule.check(&ws);
        assert_eq!(findings.len(), 2, "one finding per colliding path");
        assert!(findings.iter().all(|(_, e)| !e.related.is_empty()));
        assert!(!findings
            .iter()
            .any(|(p, _)| p.to_string_lossy().contains("other")));
    }

    #[test]
    fn unregistered_script_detected_and_registered_ok() {
        let fleet = parsed(
            "fleets/eng.yml",
            r#"
controls:
  scripts:
    - path: ../scripts/registered.sh
policies:
  - name: fix things
    query: SELECT 1;
    run_script:
      path: ../scripts/registered.sh
  - name: fix other things
    query: SELECT 1;
    run_script:
      path: ../scripts/rogue.sh
"#,
        );
        let binding = [fleet];
        let ws = Workspace::from_files(
            vec![
                PathBuf::from("scripts/registered.sh"),
                PathBuf::from("scripts/rogue.sh"),
            ],
            &binding,
        );
        let findings = UnregisteredScriptRule.check(&ws);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].1.message.contains("rogue.sh"));
        assert_eq!(findings[0].1.related, vec![PathBuf::from("scripts/rogue.sh")]);
    }

    #[test]
    fn unregistered_script_glob_registration_counts() {
        let fleet = parsed(
            "fleets/eng.yml",
            r#"
controls:
  scripts:
    - paths: ../scripts/*.sh
policies:
  - name: fix
    query: SELECT 1;
    run_script:
      path: ../scripts/anything.sh
"#,
        );
        let binding = [fleet];
        let ws = Workspace::from_files(vec![PathBuf::from("scripts/anything.sh")], &binding);
        assert!(UnregisteredScriptRule.check(&ws).is_empty());
    }

    #[test]
    fn duplicate_content_byte_identical() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.sh");
        let b = dir.path().join("b.sh");
        let c = dir.path().join("c.sh");
        std::fs::write(&a, "#!/bin/sh\necho same\n").unwrap();
        std::fs::write(&b, "#!/bin/sh\necho same\n").unwrap();
        std::fs::write(&c, "#!/bin/sh\necho different\n").unwrap();

        let binding = [];
        let ws = Workspace::from_files(
            vec![
                crate::util::normalize_path(&a),
                crate::util::normalize_path(&b),
                crate::util::normalize_path(&c),
            ],
            &binding,
        );
        let findings = DuplicateContentRule.check(&ws);
        assert_eq!(findings.len(), 2, "a and b each flag the other");
        assert!(findings.iter().all(|(_, e)| !e.related.is_empty()));
        assert!(!findings
            .iter()
            .any(|(p, _)| p.to_string_lossy().ends_with("c.sh")));
    }
}

#[cfg(test)]
mod orphan_tests {
    use super::*;
    use super::tests_support::parsed;

    #[test]
    fn orphaned_file_detected_wired_ok() {
        let fleet = parsed(
            "fleets/eng.yml",
            r#"
controls:
  scripts:
    - path: ../scripts/wired.sh
  apple_settings:
    configuration_profiles:
      - paths: ../profiles/*.mobileconfig
"#,
        );
        let binding = [fleet];
        let ws = Workspace::from_files(
            vec![
                PathBuf::from("scripts/wired.sh"),
                PathBuf::from("scripts/orphan.sh"),
                PathBuf::from("profiles/wifi.mobileconfig"),
                PathBuf::from("README.md"), // not an artifact — never flagged
            ],
            &binding,
        );
        let findings = OrphanedFileRule.check(&ws);
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert!(findings[0].0.to_string_lossy().ends_with("orphan.sh"));
    }
}

#[cfg(test)]
mod dup_id_tests {
    use super::tests_support::parsed;
    use super::*;

    #[test]
    fn duplicate_identifier_per_fleet_content_differs() {
        let dir = tempfile::tempdir().unwrap();
        let mk = |name: &str, id: &str, extra: &str| -> PathBuf {
            let p = dir.path().join(name);
            std::fs::write(
                &p,
                format!(
                    "<plist><dict><key>PayloadIdentifier</key><string>{id}</string>{extra}</dict></plist>"
                ),
            )
            .unwrap();
            normalize_path(&p)
        };
        let a = mk("a.mobileconfig", "com.example.wifi", "");
        let b = mk("b.mobileconfig", "com.example.wifi", "<key>X</key><string>y</string>");
        let c = mk("c.mobileconfig", "com.example.wifi", ""); // identical to a

        // Fleet 1 references a+b (same id, DIFFERENT content) -> one finding.
        let f1 = parsed(
            &format!("{}/fleets/one.yml", dir.path().display()),
            &format!(
                "controls:\n  apple_settings:\n    configuration_profiles:\n      - path: ../{}\n      - path: ../{}\n",
                a.file_name().unwrap().to_str().unwrap(),
                b.file_name().unwrap().to_str().unwrap()
            ),
        );
        // Fleet 2 references a+c (same id, identical content) -> clean.
        let f2 = parsed(
            &format!("{}/fleets/two.yml", dir.path().display()),
            &format!(
                "controls:\n  apple_settings:\n    configuration_profiles:\n      - path: ../{}\n      - path: ../{}\n",
                a.file_name().unwrap().to_str().unwrap(),
                c.file_name().unwrap().to_str().unwrap()
            ),
        );
        let binding = [f1, f2];
        let ws = Workspace::from_files(vec![a, b, c], &binding);
        let findings = DuplicateIdentifierRule.check(&ws);
        assert_eq!(findings.len(), 1, "{:?}", findings);
        assert!(findings[0].0.to_string_lossy().ends_with("one.yml"));
        assert_eq!(findings[0].1.related.len(), 2);
        assert!(findings[0].1.message.contains("com.example.wifi"));
    }
}

#[cfg(test)]
mod profile_wellformed_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const BAD: &str = "<plist><dict><key>N</key><string>Foo & Bar</string></dict></plist>";
    const FLEET: &str = "controls:\n  apple_settings:\n    configuration_profiles:\n      - paths: ../profiles/*.mobileconfig\n";

    fn repo(fleets: &[&str], profiles: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("profiles")).unwrap();
        fs::create_dir_all(tmp.path().join("fleets")).unwrap();
        for (name, body) in profiles {
            fs::write(tmp.path().join("profiles").join(name), body).unwrap();
        }
        for f in fleets {
            fs::write(tmp.path().join("fleets").join(f), FLEET).unwrap();
        }
        tmp
    }

    fn parsed_fleets(tmp: &TempDir, fleets: &[&str]) -> Vec<ParsedFile> {
        fleets
            .iter()
            .map(|f| ParsedFile {
                path: tmp.path().join("fleets").join(f),
                source: FLEET.to_string(),
                yaml: serde_yaml::from_str(FLEET).unwrap(),
            })
            .collect()
    }

    /// The per-file version emitted one copy per referencing fleet; with 33
    /// fleets globbing a shared directory that was 33 identical errors.
    #[test]
    fn one_bad_profile_is_reported_once_however_many_fleets_reference_it() {
        let fleets = ["a.yml", "b.yml", "c.yml"];
        let tmp = repo(&fleets, &[("bad.mobileconfig", BAD)]);
        let parsed = parsed_fleets(&tmp, &fleets);
        let ws = Workspace::build(tmp.path(), &parsed);

        let found = ProfileWellFormedRule.check(&ws);
        assert_eq!(found.len(), 1, "one profile ⇒ one finding, got: {found:?}");
        assert!(found[0].0.ends_with("bad.mobileconfig"));
    }

    /// The per-file version only ever saw profiles a fleet referenced, so a
    /// defect sat undetected until someone wired the file up.
    #[test]
    fn a_profile_no_fleet_references_is_still_checked() {
        let tmp = repo(&[], &[("never-wired.mobileconfig", BAD)]);
        let ws = Workspace::build(tmp.path(), &[]);

        let found = ProfileWellFormedRule.check(&ws);
        assert_eq!(found.len(), 1, "got: {found:?}");
        assert_eq!(found[0].1.rule_code, Some(codes::PROFILE_WELL_FORMED));
    }

    #[test]
    fn clean_repo_reports_nothing() {
        let good = "<plist><dict>\
                    <key>PayloadUUID</key><string>95702CD6-A76F-466C-9F07-711416585D76</string>\
                    </dict></plist>";
        let tmp = repo(&["a.yml"], &[("ok.mobileconfig", good)]);
        let parsed = parsed_fleets(&tmp, &["a.yml"]);
        let ws = Workspace::build(tmp.path(), &parsed);
        assert!(ProfileWellFormedRule.check(&ws).is_empty());
    }

    /// `.json` that is not a DDM declaration must not be dragged in.
    #[test]
    fn unrelated_json_in_the_repo_is_ignored() {
        let tmp = repo(&[], &[]);
        fs::write(
            tmp.path().join("profiles/automatic-enrollment-ABC.dep.json"),
            r#"{"profile_name":"ABC","await_device_configured":true}"#,
        )
        .unwrap();
        fs::write(tmp.path().join("package-lock.json"), r#"{"lockfileVersion":3}"#).unwrap();
        let ws = Workspace::build(tmp.path(), &[]);
        assert!(ProfileWellFormedRule.check(&ws).is_empty());
    }
}

#[cfg(test)]
mod duplicate_semantics_tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const FLEET: &str = "controls:\n  apple_settings:\n    configuration_profiles:\n      - paths: ../profiles/*.mobileconfig\n";

    fn profile(id: &str, uuid: &str, desc: &str) -> String {
        format!(
            "<plist version=\"1.0\"><dict>\
             <key>PayloadIdentifier</key><string>{id}</string>\
             <key>PayloadUUID</key><string>{uuid}</string>\
             <key>Desc</key><string>{desc}</string>\
             </dict></plist>"
        )
    }

    fn build(profiles: &[(&str, String)]) -> (TempDir, Vec<ParsedFile>) {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("profiles")).unwrap();
        fs::create_dir_all(tmp.path().join("fleets")).unwrap();
        for (name, body) in profiles {
            fs::write(tmp.path().join("profiles").join(name), body).unwrap();
        }
        let fleet = tmp.path().join("fleets/a.yml");
        fs::write(&fleet, FLEET).unwrap();
        let parsed = vec![ParsedFile {
            path: fleet,
            source: FLEET.to_string(),
            yaml: serde_yaml::from_str(FLEET).unwrap(),
        }];
        (tmp, parsed)
    }

    /// Regression: the rule compared raw bytes, so two profiles differing only
    /// in XML escaping were reported as having "different content" when they
    /// decode identically and Fleet delivers them the same.
    #[test]
    fn duplicate_identifier_ignores_escaping_only_differences() {
        let escaped = profile("com.x.support", "A1B2C3D4-1111-2222-3333-444455556666", "Call &quot;SD&quot;");
        let literal = escaped.replace("&quot;", "\"");
        assert_ne!(escaped, literal, "fixture must differ in bytes");
        let (tmp, parsed) = build(&[("a.mobileconfig", escaped), ("b.mobileconfig", literal)]);
        let ws = Workspace::build(tmp.path(), &parsed);

        let found = DuplicateIdentifierRule.check(&ws);
        assert!(found.is_empty(), "escaping is not a content difference: {found:?}");
    }

    #[test]
    fn duplicate_identifier_still_reports_real_differences() {
        let a = profile("com.x.support", "A1B2C3D4-1111-2222-3333-444455556666", "Call SD");
        let b = profile("com.x.support", "B1B2C3D4-1111-2222-3333-444455556666", "Call HELPDESK");
        let (tmp, parsed) = build(&[("a.mobileconfig", a), ("b.mobileconfig", b)]);
        let ws = Workspace::build(tmp.path(), &parsed);

        let found = DuplicateIdentifierRule.check(&ws);
        assert_eq!(found.len(), 1, "got: {found:?}");
        assert_eq!(found[0].1.rule_code, Some(codes::DUPLICATE_IDENTIFIER));
    }

    /// The duplicate scan now catches copies a byte comparison missed.
    #[test]
    fn duplicate_content_catches_semantic_duplicates() {
        let escaped = profile("com.x.support", "A1B2C3D4-1111-2222-3333-444455556666", "Call &quot;SD&quot;");
        let literal = escaped.replace("&quot;", "\"");
        let (tmp, parsed) = build(&[("a.mobileconfig", escaped), ("b.mobileconfig", literal)]);
        let ws = Workspace::build(tmp.path(), &parsed);

        let found = DuplicateContentRule.check(&ws);
        assert_eq!(found.len(), 2, "one finding per twin: {found:?}");
        let msg = &found[0].1.message;
        assert!(
            msg.contains("apart from XML escaping"),
            "must not claim byte-identical: {msg}"
        );
    }

    #[test]
    fn duplicate_content_still_says_byte_identical_when_it_is() {
        let body = profile("com.x.support", "A1B2C3D4-1111-2222-3333-444455556666", "Call SD");
        let (tmp, parsed) = build(&[("a.mobileconfig", body.clone()), ("b.mobileconfig", body)]);
        let ws = Workspace::build(tmp.path(), &parsed);

        let found = DuplicateContentRule.check(&ws);
        assert_eq!(found.len(), 2, "got: {found:?}");
        assert!(found[0].1.message.contains("byte-identical"), "got: {}", found[0].1.message);
    }
}

#[cfg(test)]
mod per_fleet_scope_tests {
    use super::*;
    use super::tests_support::parsed;

    /// The regression this rescope exists for: a script registered by one
    /// fleet used to satisfy a policy applied by another. That is precisely
    /// how `set-wifi-autojoin.sh` reached production three times.
    #[test]
    fn registration_in_another_fleet_no_longer_counts() {
        let registrar = parsed(
            "fleets/acme.yml",
            "controls:\n  scripts:\n    - path: ../scripts/wifi.sh\n",
        );
        let runner = parsed(
            "fleets/ghi.yml",
            r#"
name: ABC - GHI
policies:
  - name: wifi autojoin
    query: SELECT 1;
    run_script:
      path: ../scripts/wifi.sh
"#,
        );
        let binding = [registrar, runner];
        let ws = Workspace::from_files(vec![PathBuf::from("scripts/wifi.sh")], &binding);

        let findings = UnregisteredScriptRule.check(&ws);
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert!(findings[0].0.ends_with("ghi.yml"), "reported on the fleet that must fix it");
        assert!(findings[0].1.message.contains("ABC - GHI"), "got: {}", findings[0].1.message);
        assert!(findings[0].1.message.contains("wifi.sh"));
    }

    #[test]
    fn registration_in_the_same_fleet_is_accepted() {
        let fleet = parsed(
            "fleets/ghi.yml",
            r#"
name: ABC - GHI
controls:
  scripts:
    - path: ../scripts/wifi.sh
policies:
  - name: wifi autojoin
    query: SELECT 1;
    run_script:
      path: ../scripts/wifi.sh
"#,
        );
        let binding = [fleet];
        let ws = Workspace::from_files(vec![PathBuf::from("scripts/wifi.sh")], &binding);
        assert!(UnregisteredScriptRule.check(&ws).is_empty());
    }

    /// A policy pulled in from its own file resolves `run_script.path`
    /// relative to *that* file, not to the fleet.
    #[test]
    fn referenced_policy_file_resolves_against_its_own_directory() {
        let fleet = parsed(
            "fleets/ghi.yml",
            "name: ABC - GHI\npolicies:\n  - path: ../policies/maint.yml\n",
        );
        let policy = parsed(
            "policies/maint.yml",
            "- name: wifi\n  query: SELECT 1;\n  run_script:\n    path: ../scripts/wifi.sh\n",
        );
        let binding = [fleet, policy];
        let ws = Workspace::from_files(vec![PathBuf::from("scripts/wifi.sh")], &binding);

        let findings = UnregisteredScriptRule.check(&ws);
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert_eq!(findings[0].1.related[0], PathBuf::from("scripts/wifi.sh"));
    }

    /// One shared policy file, two fleets, registered in only one: the fleet
    /// that is actually broken is the one reported.
    #[test]
    fn shared_policy_file_reports_only_the_fleet_missing_it() {
        let ok = parsed(
            "fleets/ok.yml",
            "name: OK\ncontrols:\n  scripts:\n    - path: ../scripts/wifi.sh\npolicies:\n  - path: ../policies/maint.yml\n",
        );
        let broken = parsed(
            "fleets/broken.yml",
            "name: BROKEN\npolicies:\n  - path: ../policies/maint.yml\n",
        );
        let policy = parsed(
            "policies/maint.yml",
            "- name: wifi\n  query: SELECT 1;\n  run_script:\n    path: ../scripts/wifi.sh\n",
        );
        let binding = [ok, broken, policy];
        let ws = Workspace::from_files(vec![PathBuf::from("scripts/wifi.sh")], &binding);

        let findings = UnregisteredScriptRule.check(&ws);
        assert_eq!(findings.len(), 1, "got: {findings:?}");
        assert!(findings[0].1.message.contains("BROKEN"), "got: {}", findings[0].1.message);
    }
}

#[cfg(test)]
mod duplicate_fleet_name_tests {
    use super::*;
    use super::tests_support::parsed;

    /// Quoted and unquoted forms are the same name — `365c712` had to fix
    /// exactly this in the shell guard that was meant to catch it.
    #[test]
    fn same_name_quoted_and_unquoted_is_one_fleet() {
        let a = parsed("fleets/acme.yml", "name: \"ABC - ACME\"\npolicies: []\n");
        let b = parsed("fleets/acme-copy.yml", "name: ABC - ACME\npolicies: []\n");
        let binding = [a, b];
        let ws = Workspace::from_files(vec![], &binding);

        let findings = DuplicateFleetNameRule.check(&ws);
        assert_eq!(findings.len(), 2, "one finding per file: {findings:?}");
        assert!(findings[0].1.message.contains("ABC - ACME"));
        assert_eq!(findings[0].1.severity, crate::error::Severity::Error);
        // Each finding points at the *other* file, whichever order they sort in.
        for (path, err) in &findings {
            assert!(
                err.related.iter().all(|r| r != path),
                "a file must not be its own twin: {err:?}"
            );
            assert_eq!(err.related.len(), 1, "got: {err:?}");
        }
        let reported: std::collections::HashSet<_> =
            findings.iter().map(|(p, _)| p.clone()).collect();
        assert_eq!(reported.len(), 2, "both files reported");
    }

    #[test]
    fn distinct_fleet_names_are_clean() {
        let a = parsed("fleets/acme.yml", "name: ABC - ACME\npolicies: []\n");
        let b = parsed("fleets/ghi.yml", "name: ABC - GHI\npolicies: []\n");
        let binding = [a, b];
        let ws = Workspace::from_files(vec![], &binding);
        assert!(DuplicateFleetNameRule.check(&ws).is_empty());
    }

    /// A policy file is a bare sequence and declares no fleet — it must not be
    /// dragged into the comparison.
    #[test]
    fn policy_files_are_not_fleets() {
        let fleet = parsed("fleets/acme.yml", "name: ABC - ACME\npolicies: []\n");
        let policy = parsed("policies/a.yml", "- name: ABC - ACME\n  query: SELECT 1;\n");
        let binding = [fleet, policy];
        let ws = Workspace::from_files(vec![], &binding);
        assert!(DuplicateFleetNameRule.check(&ws).is_empty());
    }
}
