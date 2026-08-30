//! Broken path-reference detection rule (`path-exists`).
//!
//! Detects `path:` (and policy `install_software.package_path:`) references
//! whose target file does not exist on disk. When
//! the referenced file appears to have simply moved — i.e. exactly one file
//! with the same name exists elsewhere in the workspace — the rule suggests the
//! corrected relative path.
//!
//! The fix is emitted in the same `(context, suggestion)` form that
//! `flint check --fix` and the LSP quick-fix already consume:
//!   - `context`    = the old (broken) path string, verbatim
//!   - `suggestion` = the new, corrected relative path
//!   - location     = start of the path value (so the editor range covers it)
//!   - fix_safety   = Safe (only when there is exactly one unambiguous match)
//!
//! This complements two existing rules without overlapping them:
//!   - `self-reference` — a `path:` that resolves back to its own file (loop)
//!   - `path-reference` — `path`/`paths` *semantics* (globs, mutual exclusivity)

use super::error::{Fix, FixSafety, LintError};
use super::fleet_config::FleetConfig;
use super::rules::Rule;
use super::yaml_utils::{collect_path_values, find_path_value_line};
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

/// Detects `path:` references whose target file is missing, suggesting the
/// moved location when the file can be found elsewhere by name.
#[derive(Default)]
pub(crate) struct PathExistsRule {
    /// Basename index shared across files, keyed by workspace root. Before
    /// this cache, every file containing a broken reference re-walked the
    /// whole workspace to build suggestions (ADR-010 Phase 1 perf fix).
    index_cache: std::sync::Mutex<Option<(PathBuf, std::sync::Arc<BasenameIndex>)>>,
    /// Every path some config file references — the engine fills it once per
    /// directory lint. Decides whether a broken reference can fail an apply
    /// at all: one inside a fragment nothing references is never read.
    pub(crate) referenced: super::rules::ReferencedPaths,
}

impl PathExistsRule {
    pub(crate) fn with_referenced(referenced: super::rules::ReferencedPaths) -> Self {
        Self {
            referenced,
            ..Default::default()
        }
    }

    /// A policy/software/profile fragment that no fleet references. Fleet
    /// reads a fragment only through a `path:` in a fleet or the global
    /// config, so a broken reference INSIDE such a file cannot fail an apply
    /// — flint's own false-positive audit surfaced exactly this: a retired
    /// policy with a dead path, blocking, where Fleet had no opinion. Unknown
    /// wiring (a single-file lint) is not demoted; the conservative default
    /// there is the existing error.
    fn is_unwired_fragment(&self, file: &Path) -> bool {
        !matches!(
            super::engine::detect_file_type(file),
            super::engine::FileType::FleetConfig
        ) && self
            .referenced
            .get()
            .is_some_and(|set| !set.contains(&super::util::normalize_path(file)))
    }
    /// The cached basename index for `file`'s workspace root, building it
    /// on first use and rebuilding only if the root changes.
    fn cached_index(&self, file: &Path) -> std::sync::Arc<BasenameIndex> {
        let root = find_workspace_root(file);
        let mut guard = self.index_cache.lock().unwrap();
        if let Some((cached_root, idx)) = guard.as_ref() {
            if *cached_root == root {
                return std::sync::Arc::clone(idx);
            }
        }
        let idx = std::sync::Arc::new(BasenameIndex::build_at(&root));
        *guard = Some((root, std::sync::Arc::clone(&idx)));
        idx
    }
}

impl Rule for PathExistsRule {
    fn name(&self) -> &'static str {
        "path-exists"
    }

    fn description(&self) -> &'static str {
        "Detects path references whose target is missing, a directory, or whose case doesn't match disk"
    }

    fn category(&self) -> &'static str {
        "structural"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let yaml: serde_yaml::Value = match serde_yaml::from_str(source) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let mut errors = Vec::new();
        // The workspace basename index is built lazily — only the first time a
        // broken path is actually found — so clean repos pay nothing. Once
        // built it is cached on the rule for every later file.
        let mut index: Option<std::sync::Arc<BasenameIndex>> = None;
        let build_index = |f: &Path| self.cached_index(f);
        let unwired_fragment = self.is_unwired_fragment(file);
        for path_val in collect_path_values(&yaml) {
            check_path(
                file,
                &path_val,
                source,
                &mut index,
                &build_index,
                unwired_fragment,
                &mut errors,
            );
        }
        errors
    }
}

/// Check a single `path:` value: skip forms we cannot verify, then flag it if
/// the target is missing and attach a suggestion when the file has moved.
fn check_path(
    file: &Path,
    path_val: &str,
    source: &str,
    index: &mut Option<std::sync::Arc<BasenameIndex>>,
    build_index: &dyn Fn(&Path) -> std::sync::Arc<BasenameIndex>,
    unwired_fragment: bool,
    errors: &mut Vec<LintError>,
) {
    if !is_checkable(path_val) {
        return;
    }

    // Every finding below is a claim that Fleet would reject this reference.
    // Fleet reads a fragment only through a fleet that includes it, so inside
    // a fragment nothing references the claim is untrue — Fleet never sees the
    // line — and the finding is a warning that says so. One place decides
    // this for all three shapes (missing, directory, directory-with-labels).
    let apply_failure = |msg: String| -> LintError {
        if unwired_fragment {
            LintError::warning(
                format!("{msg} — in a file no fleet references, so Fleet never reads it"),
                file,
            )
        } else {
            LintError::error(msg, file)
        }
    };

    let base = match file.parent() {
        Some(p) => p,
        None => return,
    };

    let resolved = base.join(path_val);
    if resolved.exists() {
        // A `path:` must reference a single file. Pointing it at a directory
        // (often a trailing-slash leftover, or a redundant duplicate of a
        // sibling `paths:` glob) resolves on disk but fails `fleetctl gitops`.
        if resolved.is_dir() {
            let span = find_path_value_line(source, path_val);

            // A directory `path:` that carries per-profile label scoping
            // (`labels_include_any` / `labels_exclude_any` siblings) can't be
            // expressed as a `paths:` glob — a glob applies one label rule to
            // every match. So when labels are present, offer to expand the
            // entry into one `- path: <file>` entry per file in the directory,
            // propagating the label rule written on the directory entry to each
            // (a path-based, generic label rule you write once). Otherwise the
            // plain "use a glob" guidance stands.
            if let Some(s) = span {
                if let Some((block, count)) =
                    build_dir_expansion(source, s.line, path_val, &resolved)
                {
                    let err = apply_failure(format!(
                        "path reference points to a directory, not a file: {path_val}"
                    ))
                    .with_context(path_val.to_string())
                    .with_rule_code(crate::codes::PATH_IS_FILE)
                    .with_help(format!(
                        "This entry scopes labels, which a `paths:` glob can't carry per file. Run `flint check --fix --unsafe-fixes` to expand it into {count} `- path:` entr{}, each with this entry's label rule.",
                        if count == 1 { "y" } else { "ies" }
                    ))
                    .with_fix(block)
                    .with_span(s);
                    errors.push(err);
                    return;
                }
            }

            let glob_hint = if path_val.ends_with('/') {
                format!("{path_val}*")
            } else {
                format!("{path_val}/*")
            };
            let mut err = apply_failure(format!(
                "path reference points to a directory, not a file: {path_val}"
            ))
            .with_context(path_val.to_string())
            .with_rule_code(crate::codes::PATH_IS_FILE)
            .with_help(format!(
                "A `path:` must reference a single file. To include every file in a directory use `paths:` with a glob (e.g. `{glob_hint}`), or remove it if a sibling `paths:` already covers it."
            ));
            if let Some(s) = span {
                err = err.with_span(s);
            }
            errors.push(err);
            return;
        }

        // The referenced file exists but may have no usable content. For any
        // type, a blank/whitespace-only (or 0-byte) file is empty; for YAML,
        // comment-only counts too (e.g. a software file with just a
        // `# … version …` header and no `hash_sha256:`). It resolves on disk but
        // `fleetctl gitops` rejects an empty software/policy/profile/script.
        let is_yaml = matches!(
            resolved.extension().and_then(|e| e.to_str()),
            Some("yml") | Some("yaml")
        );
        if let Ok(target) = std::fs::read_to_string(&resolved) {
            if super::yaml_utils::is_effectively_empty(&target, is_yaml) {
                let span = find_path_value_line(source, path_val);
                let mut err = LintError::warning(
                    format!("path reference points to an empty file (no usable content): {path_val}"),
                    file,
                )
                .with_context(path_val.to_string())
                .with_rule_code(crate::codes::PATH_EMPTY)
                .with_help(
                    "The referenced file is empty (blank/whitespace, or comment-only for YAML). Fleet's parser ACCEPTS this and applies nothing from it — an empty software file contributes zero packages, an empty script installs nothing — so the apply succeeds and the intent is silently lost. Provide real content: a software file needs `hash_sha256:` or `url:` (regenerate with `flint gen software`); a script needs its commands. (An unparseable profile is `profile-well-formed`'s finding.)",
                );
                if let Some(s) = span {
                    err = err.with_span(s);
                }
                errors.push(err);
                return;
            }
        }

        // Target found — but on a case-insensitive filesystem (macOS) the
        // reference's case may differ from what's actually on disk, which a
        // case-sensitive CI (Linux `fleetctl gitops`) rejects. Flag the
        // mismatch with the on-disk casing as a Safe fix.
        if let Some(correct) = case_correct_path(base, path_val) {
            let span = find_path_value_line(source, path_val);
            let mut err = LintError::error(
                format!("path reference case does not match the file on disk: {path_val}"),
                file,
            )
            .with_context(path_val.to_string())
            .with_rule_code(crate::codes::PATH_CASE)
            .with_fix(Fix::Replace {
                old: Some(path_val.to_string()),
                new: correct.clone(),
                safety: FixSafety::Safe,
            })
            .with_help(format!(
                "Case-sensitive CI (Linux) resolves paths exactly; macOS does not, so this passes locally but fails the pipeline. Use the on-disk casing: '{correct}'."
            ));
            if let Some(s) = span {
                err = err.with_span(s);
            }
            errors.push(err);
        }
        return; // target present — nothing more to do
    }

    let basename = match Path::new(path_val).file_name().and_then(|n| n.to_str()) {
        Some(n) => n.to_string(),
        None => return,
    };

    let span = find_path_value_line(source, path_val);

    // `related` carries the missing target so `flint check --staged` blocks the
    // commit that DELETES a referenced file: the finding sits on an unstaged
    // fleet YAML, and only this link puts it in staged scope (ADR-010).
    let mut err = apply_failure(format!("path reference not found: {path_val}"))
        .with_context(path_val.to_string())
        .with_related(resolved.clone());
    if let Some(s) = span {
        err = err.with_span(s);
    }

    // Build (or fetch the cached) workspace index on demand, then look for
    // files by name.
    let idx = index.get_or_insert_with(|| build_index(file));
    let candidates: Vec<String> = idx
        .lookup(&basename)
        .iter()
        .filter_map(|target| relative_path(base, target))
        .filter(|rel| rel != path_val)
        .collect();

    match candidates.len() {
        0 => {
            err = err.with_help(
                "Referenced file does not exist. Check the path or restore the file.",
            );
        }
        1 => {
            err = err
                .with_help(format!("File appears to have moved to '{}'", candidates[0]))
                .with_fix(Fix::Replace {
                    old: Some(path_val.to_string()),
                    new: candidates[0].clone(),
                    safety: FixSafety::Safe,
                });
        }
        _ => {
            err = err
                .with_help(format!(
                    "Multiple files named '{}' found: {}. Update the path manually.",
                    basename,
                    candidates.join(", ")
                ))
                .with_fix(Fix::Candidates {
                    old: path_val.to_string(),
                    options: candidates.clone(),
                });
        }
    }

    errors.push(err);
}

/// Build a block-replacement fix that expands a directory `path:` list item
/// into one `- path: <file>` entry per file in the directory, propagating the
/// entry's label scoping (`labels_include_any` / `labels_exclude_any`) to each.
///
/// Returns `None` (so the caller falls back to the plain "use a glob" message)
/// when the entry is not a list item, carries no label siblings, or the
/// directory holds no profile files. `path_line` is 1-indexed.
fn build_dir_expansion(
    source: &str,
    path_line: usize,
    path_val: &str,
    resolved: &Path,
) -> Option<(Fix, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let marker_idx = path_line.checked_sub(1)?;
    let marker_line = *lines.get(marker_idx)?;

    // Only a list item (`- path:`) can be fanned out into sibling `- path:`
    // entries; a bare mapping `path:` has no list to grow.
    let marker_trimmed = marker_line.trim_start();
    if !marker_trimmed.starts_with("- ") {
        return None;
    }
    let marker_indent = marker_line.len() - marker_trimmed.len();

    // Collect the lines belonging to this list item — everything indented
    // deeper than the `-` marker, up to a blank line or a dedent. The item
    // must scope labels for expansion to be the right move.
    let mut sibling_end = marker_idx;
    let mut has_labels = false;
    let mut i = marker_idx + 1;
    while let Some(&line) = lines.get(i) {
        if line.trim().is_empty() {
            break;
        }
        let indent = line.len() - line.trim_start().len();
        if indent <= marker_indent {
            break;
        }
        let key = line.trim_start();
        if key.starts_with("labels_include_any") || key.starts_with("labels_exclude_any") {
            has_labels = true;
        }
        sibling_end = i;
        i += 1;
    }
    if !has_labels {
        return None;
    }
    let sibling_lines = &lines[marker_idx + 1..=sibling_end];

    // Enumerate the profile files in the directory (deterministic order).
    let mut names: Vec<String> = std::fs::read_dir(resolved)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| {
            matches!(
                Path::new(n).extension().and_then(|x| x.to_str()),
                Some("mobileconfig") | Some("json") | Some("xml")
            )
        })
        .collect();
    if names.is_empty() {
        return None;
    }
    names.sort();

    let dir_prefix = if path_val.ends_with('/') {
        path_val.to_string()
    } else {
        format!("{path_val}/")
    };

    // One expanded item per file: the marker line with its path value swapped
    // for the concrete file, followed by the item's label lines verbatim.
    let pos = marker_line.find(path_val)?;
    let mut items: Vec<String> = Vec::with_capacity(names.len());
    for name in &names {
        let new_path = format!("{dir_prefix}{name}");
        let mut item = format!(
            "{}{}{}",
            &marker_line[..pos],
            new_path,
            &marker_line[pos + path_val.len()..]
        );
        for s in sibling_lines {
            item.push('\n');
            item.push_str(s);
        }
        items.push(item);
    }

    let block = Fix::ReplaceLines {
        start_line: path_line,
        end_line: sibling_end + 1,
        replacement: items.join("\n"),
        safety: FixSafety::Unsafe,
    };
    Some((block, names.len()))
}

/// Whether a `path:` value is a plain relative file reference we can resolve.
/// Skips globs (handled by `path-reference`), env-var/templated values, URLs,
/// and absolute paths.
fn is_checkable(path_val: &str) -> bool {
    if path_val.trim().is_empty() {
        return false;
    }
    if path_val.starts_with('/') || path_val.starts_with('~') {
        return false; // absolute / home — out of scope
    }
    if path_val.contains("://") || path_val.contains('$') {
        return false; // URL or templated value
    }
    // Glob metacharacters — `paths:` territory, not a single concrete file.
    if path_val.contains('*')
        || path_val.contains('?')
        || path_val.contains('[')
        || path_val.contains('{')
    {
        return false;
    }
    true
}

/// Walk `path_val` from `base`, component by component, comparing each segment
/// against the real on-disk entry name. Returns the case-corrected relative
/// path (the casing a case-sensitive CI requires) when any component's case
/// differs from disk, or `None` when every component already matches.
///
/// Only called once the target is known to resolve, so a missing entry mid-walk
/// (`find_entry_ci` → None) just aborts the check — the missing-file branch
/// handles genuine absences.
fn case_correct_path(base: &Path, path_val: &str) -> Option<String> {
    let mut cur = base.to_path_buf();
    let mut parts: Vec<String> = Vec::new();
    let mut mismatched = false;

    for comp in Path::new(path_val).components() {
        match comp {
            Component::CurDir => parts.push(".".to_string()),
            Component::ParentDir => {
                cur.pop();
                parts.push("..".to_string());
            }
            Component::Normal(os) => {
                let want = os.to_str()?;
                let real = find_entry_ci(&cur, want)?;
                if real != want {
                    mismatched = true;
                }
                cur.push(&real);
                parts.push(real);
            }
            // RootDir / Prefix → absolute; `is_checkable` already excluded these.
            _ => return None,
        }
    }

    mismatched.then(|| parts.join("/"))
}

/// Find the real on-disk name of `want` in `dir`: returns `want` unchanged if it
/// exists case-exactly, the actual stored name if it only matches
/// case-insensitively, or `None` if no entry matches at all.
fn find_entry_ci(dir: &Path, want: &str) -> Option<String> {
    let mut ci: Option<String> = None;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let fname = entry.file_name();
        let name = match fname.to_str() {
            Some(n) => n,
            None => continue,
        };
        if name == want {
            return Some(want.to_string()); // exact match — preferred
        }
        if ci.is_none() && name.eq_ignore_ascii_case(want) {
            ci = Some(name.to_string());
        }
    }
    ci
}

/// An index of every file in the workspace keyed by file name, so a missing
/// reference can be matched to candidate locations by basename.
struct BasenameIndex {
    by_name: HashMap<String, Vec<PathBuf>>,
}

impl BasenameIndex {
    /// Build the index by walking `root`.
    fn build_at(root: &Path) -> Self {
        let mut by_name: HashMap<String, Vec<PathBuf>> = HashMap::new();
        collect_files(root, &mut by_name);
        Self { by_name }
    }

    fn lookup(&self, basename: &str) -> &[PathBuf] {
        self.by_name.get(basename).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

/// Find the workspace root by walking up from `file`. Prefers a `.git` or
/// `.fleetlint.toml` marker; falls back to the highest ancestor containing a
/// `default.yml` (the GitOps root); otherwise the file's own directory.
fn find_workspace_root(file: &Path) -> PathBuf {
    let start = file.parent().unwrap_or(file);
    let mut cur = Some(start);
    let mut gitops_root: Option<PathBuf> = None;

    while let Some(dir) = cur {
        if dir.join(".git").exists() || dir.join(".fleetlint.toml").exists() {
            return dir.to_path_buf();
        }
        if dir.join("default.yml").exists() {
            gitops_root = Some(dir.to_path_buf());
        }
        cur = dir.parent();
    }

    gitops_root.unwrap_or_else(|| start.to_path_buf())
}

/// Recursively collect files under `dir`, indexing each by its file name.
/// Skips hidden directories and common build/output dirs.
fn collect_files(dir: &Path, by_name: &mut HashMap<String, Vec<PathBuf>>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.')
                    || name == "node_modules"
                    || name == "target"
                    || name == "dist"
                {
                    continue;
                }
            }
            collect_files(&path, by_name);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            by_name.entry(name.to_string()).or_default().push(path);
        }
    }
}

/// Compute a POSIX-style relative path from `from_dir` to `to_file`, using
/// `..` to climb out of the common ancestor. Returns `None` if either path
/// cannot be canonicalized.
fn relative_path(from_dir: &Path, to_file: &Path) -> Option<String> {
    let from = from_dir.canonicalize().ok()?;
    let to = to_file.canonicalize().ok()?;

    let from_comps: Vec<_> = from.components().collect();
    let to_comps: Vec<_> = to.components().collect();

    let mut common = 0;
    while common < from_comps.len()
        && common < to_comps.len()
        && from_comps[common] == to_comps[common]
    {
        common += 1;
    }

    let mut result = PathBuf::new();
    for _ in common..from_comps.len() {
        result.push("..");
    }
    for comp in &to_comps[common..] {
        result.push(comp.as_os_str());
    }

    let s = result.to_string_lossy().replace('\\', "/");
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Severity;
    use std::fs;
    use tempfile::TempDir;

    /// Set up a workspace, write `yaml` to `file_name`, create any `targets`,
    /// and run the rule. A `.git` marker anchors the workspace root.
    fn run(
        tmp: &TempDir,
        file_name: &str,
        yaml: &str,
        targets: &[&str],
    ) -> Vec<LintError> {
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        for t in targets {
            let p = tmp.path().join(t);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            // Non-empty content so targets aren't flagged by the path-empty
            // check — these fixtures only need to *exist* for the test's intent.
            fs::write(&p, "x: 1\n").unwrap();
        }
        let file_path = tmp.path().join(file_name);
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        fs::write(&file_path, yaml).unwrap();

        PathExistsRule::default().check(&FleetConfig::default(), &file_path, yaml)
    }

    #[test]
    fn existing_target_no_error() {
        let tmp = TempDir::new().unwrap();
        let yaml = "software:\n  packages:\n    - path: ../lib/app.yml\n";
        let errors = run(&tmp, "fleets/team.yml", yaml, &["lib/app.yml"]);
        assert!(errors.is_empty(), "got: {errors:?}");
    }

    /// Is the filesystem under `dir` case-insensitive (macOS default)?
    fn fs_case_insensitive(dir: &std::path::Path) -> bool {
        fs::write(dir.join("CaseProbe.tmp"), "").unwrap();
        let insensitive = dir.join("caseprobe.tmp").exists();
        fs::remove_file(dir.join("CaseProbe.tmp")).ok();
        insensitive
    }

    #[test]
    fn wrong_case_reference_is_caught() {
        // The "passes on macOS, fails on case-sensitive CI" trap. The real file
        // is lib/app.yml; the reference uses ../lib/App.yml. On a case-
        // insensitive FS this resolves, so we must flag a `path-case` mismatch
        // with the on-disk casing as the fix. On a case-sensitive FS the file
        // is genuinely absent → the normal path-exists not-found path. Either
        // way a wrong-case reference is never silently OK.
        let tmp = TempDir::new().unwrap();
        let yaml = "software:\n  packages:\n    - path: ../lib/App.yml\n";
        let errors = run(&tmp, "fleets/team.yml", yaml, &["lib/app.yml"]);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        let e = &errors[0];
        if fs_case_insensitive(tmp.path()) {
            assert_eq!(e.rule_code, Some("path-case"));
            assert_eq!(e.suggestion(), Some("../lib/app.yml"));
            assert_eq!(e.fix_safety(), Some(FixSafety::Safe));
            assert!(e.message.contains("case does not match"));
        } else {
            // Case-sensitive FS: genuinely missing → path-exists not-found.
            assert!(e.message.contains("path reference not found"));
        }
    }

    #[test]
    fn path_to_directory_is_flagged() {
        // A `path:` resolving to a directory (e.g. a trailing-slash leftover or
        // a redundant duplicate of a sibling `paths:` glob) fails gitops.
        let tmp = TempDir::new().unwrap();
        // Create the target as a DIRECTORY (with a file inside so the dir exists).
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("lib/profiles")).unwrap();
        fs::write(tmp.path().join("lib/profiles/a.mobileconfig"), "").unwrap();
        let file_path = tmp.path().join("fleets/team.yml");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let yaml = "controls:\n  macos_settings:\n    custom_settings:\n      - path: ../lib/profiles/\n";
        fs::write(&file_path, yaml).unwrap();

        let errors = PathExistsRule::default().check(&FleetConfig::default(), &file_path, yaml);
        let dir_err: Vec<_> = errors
            .iter()
            .filter(|e| e.rule_code == Some("path-is-file"))
            .collect();
        assert_eq!(dir_err.len(), 1, "got: {errors:?}");
        assert_eq!(dir_err[0].severity, Severity::Error);
        assert!(dir_err[0].message.contains("directory"));
        assert!(dir_err[0].help.as_ref().unwrap().contains("paths:"));
        // No labels → no expansion fix, just the glob guidance.
        assert!(dir_err[0].fix.is_none());
    }

    #[test]
    fn directory_path_with_labels_expands_per_file() {
        // A directory `path:` that scopes labels can't become a `paths:` glob;
        // it must fan out into one entry per profile, each carrying the same
        // label rule (path-based generic labels, written once).
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        let dir = tmp.path().join("lib/profiles");
        fs::create_dir_all(&dir).unwrap();
        // Deliberately out of order + a non-profile file that must be ignored.
        for f in ["b.mobileconfig", "a.mobileconfig", "README.md"] {
            fs::write(dir.join(f), "x").unwrap();
        }
        let file_path = tmp.path().join("fleets/team.yml");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let yaml = "controls:\n  macos_settings:\n    custom_settings:\n      - path: ../lib/profiles/\n        labels_include_any: [Opt-in - Pilot]\n        labels_exclude_any: []\n";
        fs::write(&file_path, yaml).unwrap();

        let errors = PathExistsRule::default().check(&FleetConfig::default(), &file_path, yaml);
        let e = errors
            .iter()
            .find(|e| e.rule_code == Some("path-is-file"))
            .expect("path-is-file finding");

        let (start_line, end_line, r) = match e.fix.as_ref().expect("expansion block fix") {
            Fix::ReplaceLines {
                start_line,
                end_line,
                replacement,
                safety,
            } => {
                assert_eq!(*safety, FixSafety::Unsafe);
                (*start_line, *end_line, replacement)
            }
            other => panic!("expected ReplaceLines, got {other:?}"),
        };
        // Item spans the `- path:` line (4) through the two label lines (6).
        assert_eq!(start_line, 4);
        assert_eq!(end_line, 6);
        // Sorted, one entry per profile, README.md ignored.
        assert!(r.contains("- path: ../lib/profiles/a.mobileconfig"));
        assert!(r.contains("- path: ../lib/profiles/b.mobileconfig"));
        assert!(!r.contains("README"));
        assert_eq!(r.matches("- path:").count(), 2);
        // Each expanded entry carries the label rule verbatim.
        assert_eq!(r.matches("labels_include_any: [Opt-in - Pilot]").count(), 2);
        assert_eq!(r.matches("labels_exclude_any: []").count(), 2);
        // Original indentation is preserved.
        assert!(r.contains("      - path: ../lib/profiles/a.mobileconfig"));
        assert!(r.contains("        labels_include_any: [Opt-in - Pilot]"));
    }

    #[test]
    fn directory_path_with_labels_but_empty_dir_falls_back() {
        // Labels present but no profile files → nothing to expand, so the plain
        // glob guidance stands (no block fix).
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("lib/profiles")).unwrap();
        let file_path = tmp.path().join("fleets/team.yml");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let yaml = "controls:\n  macos_settings:\n    custom_settings:\n      - path: ../lib/profiles/\n        labels_include_any: [X]\n";
        fs::write(&file_path, yaml).unwrap();

        let errors = PathExistsRule::default().check(&FleetConfig::default(), &file_path, yaml);
        let e = errors
            .iter()
            .find(|e| e.rule_code == Some("path-is-file"))
            .expect("path-is-file finding");
        assert!(e.fix.is_none());
        assert!(e.help.as_ref().unwrap().contains("paths:"));
    }

    #[test]
    fn comment_only_referenced_yaml_is_flagged() {
        // A referenced software .yml with only a `# … version …` header (no
        // hash_sha256/url) resolves on disk but fails gitops → path-empty.
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("lib/software")).unwrap();
        fs::write(
            tmp.path().join("lib/software/corp-fonts.yml"),
            "# com.x.y (App-1.0.pkg) version 1.0\n",
        )
        .unwrap();
        let file_path = tmp.path().join("fleets/team.yml");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let yaml = "software:\n  packages:\n    - path: ../lib/software/corp-fonts.yml\n";
        fs::write(&file_path, yaml).unwrap();

        let errors = PathExistsRule::default().check(&FleetConfig::default(), &file_path, yaml);
        let empty: Vec<_> = errors
            .iter()
            .filter(|e| e.rule_code == Some("path-empty"))
            .collect();
        assert_eq!(empty.len(), 1, "got: {errors:?}");
        // Warning, not error: Fleet's parser accepts an empty software file and
        // applies nothing from it — verified against its own parser at
        // playground cb61a82, where an error here was the one blocking claim
        // Fleet did not share. The apply succeeds; the intent is silently lost.
        assert_eq!(empty[0].severity, Severity::Warning);
        assert!(empty[0].message.contains("empty file"));
    }

    #[test]
    fn empty_referenced_script_is_flagged() {
        // A run_script.path / install_script.path pointing at a blank .sh is
        // now caught (path-empty extends to non-YAML, truly-empty targets).
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("lib/scripts")).unwrap();
        fs::write(tmp.path().join("lib/scripts/setup.sh"), "   \n\n").unwrap(); // blank
        let file_path = tmp.path().join("fleets/team.yml");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let yaml = "policies:\n  - name: p\n    run_script:\n      path: ../lib/scripts/setup.sh\n";
        fs::write(&file_path, yaml).unwrap();

        let errors = PathExistsRule::default().check(&FleetConfig::default(), &file_path, yaml);
        let empty: Vec<_> = errors
            .iter()
            .filter(|e| e.rule_code == Some("path-empty"))
            .collect();
        assert_eq!(empty.len(), 1, "got: {errors:?}");
    }

    #[test]
    fn nonempty_script_with_shebang_only_no_error() {
        // A script with a shebang (or comments) has content — not flagged.
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        fs::create_dir_all(tmp.path().join("lib/scripts")).unwrap();
        fs::write(tmp.path().join("lib/scripts/setup.sh"), "#!/bin/sh\necho hi\n").unwrap();
        let file_path = tmp.path().join("fleets/team.yml");
        fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        let yaml = "controls:\n  scripts:\n    - path: ../lib/scripts/setup.sh\n";
        fs::write(&file_path, yaml).unwrap();
        let errors = PathExistsRule::default().check(&FleetConfig::default(), &file_path, yaml);
        assert!(
            errors.iter().all(|e| e.rule_code != Some("path-empty")),
            "shebang script must not be flagged empty: {errors:?}"
        );
    }

    #[test]
    fn correct_case_reference_no_error() {
        let tmp = TempDir::new().unwrap();
        let yaml = "software:\n  packages:\n    - path: ../lib/app.yml\n";
        // Exact-case reference must never trip the case check.
        let errors = run(&tmp, "fleets/team.yml", yaml, &["lib/app.yml"]);
        assert!(errors.is_empty(), "exact case must pass, got: {errors:?}");
    }

    #[test]
    fn wrong_case_in_directory_component_is_caught() {
        // Mismatch in a directory segment, not just the filename.
        let tmp = TempDir::new().unwrap();
        if !fs_case_insensitive(tmp.path()) {
            return; // only meaningful on a case-insensitive FS
        }
        let yaml = "software:\n  packages:\n    - path: ../Lib/app.yml\n";
        let errors = run(&tmp, "fleets/team.yml", yaml, &["lib/app.yml"]);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        assert_eq!(errors[0].rule_code, Some("path-case"));
        assert_eq!(errors[0].suggestion(), Some("../lib/app.yml"));
    }

    #[test]
    fn moved_target_suggests_fix() {
        let tmp = TempDir::new().unwrap();
        // Reference points at the old location; the file actually lives elsewhere.
        let yaml = "software:\n  packages:\n    - path: ../platforms/macos/software/base/supportapp.yml\n";
        let errors = run(
            &tmp,
            "fleets/abc.yml",
            yaml,
            &["platforms/macos/base/software/supportapp.yml"],
        );
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        let e = &errors[0];
        assert!(e.message.contains("path reference not found"));
        assert_eq!(
            e.context.as_deref(),
            Some("../platforms/macos/software/base/supportapp.yml")
        );
        assert_eq!(
            e.suggestion(),
            Some("../platforms/macos/base/software/supportapp.yml")
        );
        assert_eq!(e.fix_safety(), Some(FixSafety::Safe));
        assert!(e.line().is_some() && e.column().is_some());
    }

    fn wired(paths: &[&Path]) -> super::super::rules::ReferencedPaths {
        let r: super::super::rules::ReferencedPaths = Default::default();
        r.set(paths.iter().map(|p| crate::util::normalize_path(p)).collect()).unwrap();
        r
    }

    /// A broken reference inside a fragment no fleet includes cannot fail an
    /// apply — Fleet never reads the file — so it must not block.
    #[test]
    fn broken_reference_in_an_unwired_fragment_is_a_warning() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pol = tmp.path().join("platforms/macos/policies/retired.yml");
        std::fs::create_dir_all(pol.parent().unwrap()).unwrap();
        let yaml = "- name: x\n  query: SELECT 1;\n  run_script:\n    path: ../scripts/gone.sh\n";
        std::fs::write(&pol, yaml).unwrap();
        let other = tmp.path().join("platforms/macos/policies/live.yml");
        let rule = PathExistsRule::with_referenced(wired(&[&other]));
        let errs = rule.check(&FleetConfig::default(), &pol, yaml);
        let nf: Vec<_> = errs.iter().filter(|e| e.message.contains("not found")).collect();
        assert_eq!(nf.len(), 1, "got: {errs:?}");
        assert_eq!(nf[0].severity, Severity::Warning);
        assert!(nf[0].message.contains("no fleet references"), "got: {}", nf[0].message);
    }

    /// A `path:` at a directory is the other shape Fleet would reject — and
    /// equally unseen inside a fragment no fleet includes.
    #[test]
    fn directory_reference_in_an_unwired_fragment_is_a_warning() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pol = tmp.path().join("platforms/macos/policies/retired.yml");
        std::fs::create_dir_all(pol.parent().unwrap()).unwrap();
        std::fs::create_dir_all(tmp.path().join("platforms/macos/scripts")).unwrap();
        let yaml = "- name: x\n  query: SELECT 1;\n  run_script:\n    path: ../scripts\n";
        std::fs::write(&pol, yaml).unwrap();
        let other = tmp.path().join("platforms/macos/policies/live.yml");

        let unwired = PathExistsRule::with_referenced(wired(&[&other]))
            .check(&FleetConfig::default(), &pol, yaml);
        let dir: Vec<_> = unwired.iter().filter(|e| e.message.contains("directory")).collect();
        assert_eq!(dir.len(), 1, "got: {unwired:?}");
        assert_eq!(dir[0].severity, Severity::Warning);
        assert_eq!(dir[0].rule_code, Some(crate::codes::PATH_IS_FILE));
        assert!(dir[0].message.contains("no fleet references"), "got: {}", dir[0].message);

        let wired_rule = PathExistsRule::with_referenced(wired(&[&pol]))
            .check(&FleetConfig::default(), &pol, yaml);
        assert!(
            wired_rule.iter().any(|e| e.message.contains("directory") && e.severity == Severity::Error),
            "wired fragment must still block: {wired_rule:?}"
        );
    }

    /// The same reference in a fragment a fleet DOES include still blocks.
    #[test]
    fn broken_reference_in_a_wired_fragment_stays_an_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pol = tmp.path().join("platforms/macos/policies/live.yml");
        std::fs::create_dir_all(pol.parent().unwrap()).unwrap();
        let yaml = "- name: x\n  query: SELECT 1;\n  run_script:\n    path: ../scripts/gone.sh\n";
        std::fs::write(&pol, yaml).unwrap();
        let rule = PathExistsRule::with_referenced(wired(&[&pol]));
        let errs = rule.check(&FleetConfig::default(), &pol, yaml);
        assert!(errs.iter().any(|e| e.message.contains("not found") && e.severity == Severity::Error), "got: {errs:?}");
    }

    /// A fleet file is an entry point Fleet reads directly: never demoted,
    /// whatever the referenced set says.
    #[test]
    fn broken_reference_in_a_fleet_file_is_never_demoted() {
        let tmp = tempfile::TempDir::new().unwrap();
        let fleet = tmp.path().join("fleets/ABC-X.yml");
        std::fs::create_dir_all(fleet.parent().unwrap()).unwrap();
        let yaml = "name: ABC - X\ncontrols:\n  scripts:\n    - path: ../scripts/gone.sh\n";
        std::fs::write(&fleet, yaml).unwrap();
        let rule = PathExistsRule::with_referenced(wired(&[]));
        let errs = rule.check(&FleetConfig::default(), &fleet, yaml);
        assert!(errs.iter().any(|e| e.message.contains("not found") && e.severity == Severity::Error), "got: {errs:?}");
    }

    /// Unknown wiring — a single-file lint never fills the set — keeps the
    /// existing error: the conservative default is not to weaken a check on
    /// information that is absent.
    #[test]
    fn unknown_wiring_keeps_the_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let pol = tmp.path().join("platforms/macos/policies/p.yml");
        std::fs::create_dir_all(pol.parent().unwrap()).unwrap();
        let yaml = "- name: x\n  query: SELECT 1;\n  run_script:\n    path: ../scripts/gone.sh\n";
        std::fs::write(&pol, yaml).unwrap();
        let errs = PathExistsRule::default().check(&FleetConfig::default(), &pol, yaml);
        assert!(errs.iter().any(|e| e.message.contains("not found") && e.severity == Severity::Error), "got: {errs:?}");
    }

    #[test]
    fn missing_with_no_candidate_is_manual() {
        let tmp = TempDir::new().unwrap();
        let yaml = "software:\n  packages:\n    - path: ../lib/gone.yml\n";
        let errors = run(&tmp, "fleets/team.yml", yaml, &[]);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].suggestion().is_none());
        assert_eq!(errors[0].fix_safety(), None);
    }

    #[test]
    fn ambiguous_match_is_manual() {
        let tmp = TempDir::new().unwrap();
        let yaml = "software:\n  packages:\n    - path: ../lib/app.yml\n";
        let errors = run(
            &tmp,
            "fleets/team.yml",
            yaml,
            &["platforms/macos/app.yml", "platforms/windows/app.yml"],
        );
        assert_eq!(errors.len(), 1);
        assert!(errors[0].suggestion().is_none(), "ambiguous must not auto-fix");
        assert!(errors[0].help.as_deref().unwrap().contains("Multiple files"));
    }

    #[test]
    fn extension_agnostic_targets() {
        // .pkg / .mobileconfig / .dep.json / .sh must all be detected & fixed.
        let cases = [
            (
                "controls:\n  macos_settings:\n    custom_settings:\n      - path: ./profiles/wifi.mobileconfig\n",
                "platforms/macos/configuration-profiles/wifi.mobileconfig",
                "../platforms/macos/configuration-profiles/wifi.mobileconfig",
            ),
            (
                "controls:\n  scripts:\n    - path: ./scripts/setup.sh\n",
                "platforms/macos/scripts/setup.sh",
                "../platforms/macos/scripts/setup.sh",
            ),
            (
                "software:\n  packages:\n    - path: ./pkgs/tool.pkg\n",
                "platforms/macos/pkgs/tool.pkg",
                "../platforms/macos/pkgs/tool.pkg",
            ),
            (
                "mdm:\n  apple_bm:\n    - path: ./enroll.dep.json\n",
                "platforms/macos/enrollment-profiles/enroll.dep.json",
                "../platforms/macos/enrollment-profiles/enroll.dep.json",
            ),
        ];
        for (yaml, target, expected) in cases {
            let tmp = TempDir::new().unwrap();
            let errors = run(&tmp, "fleets/team.yml", yaml, &[target]);
            assert_eq!(errors.len(), 1, "yaml={yaml:?} got: {errors:?}");
            assert_eq!(
                errors[0].suggestion(),
                Some(expected),
                "yaml={yaml:?}"
            );
        }
    }

    #[test]
    fn package_path_present_no_error() {
        // A policy's install_software.package_path that resolves is fine.
        let tmp = TempDir::new().unwrap();
        let yaml = "\
- name: GlobalProtect is installed
  platform: darwin
  query: \"SELECT 1 FROM apps WHERE bundle_identifier = 'com.paloaltonetworks.GlobalProtect';\"
  install_software:
    package_path: ../../base/software/corp-fonts.yml
";
        let errors = run(
            &tmp,
            "platforms/macos/policies/autoinstalls/gp.yml",
            yaml,
            &["platforms/macos/base/software/corp-fonts.yml"],
        );
        assert!(errors.is_empty(), "got: {errors:?}");
    }

    #[test]
    fn package_path_moved_suggests_fix() {
        // A broken install_software.package_path is flagged and, when the file
        // exists elsewhere by the same name, the moved location is suggested.
        let tmp = TempDir::new().unwrap();
        let yaml = "\
- name: GlobalProtect is installed
  platform: darwin
  query: \"SELECT 1;\"
  install_software:
    package_path: ../../site/vpn-globalprotect/software/corp-fonts.yml
";
        let errors = run(
            &tmp,
            "platforms/macos/policies/autoinstalls/gp.yml",
            yaml,
            &["platforms/macos/base/software/corp-fonts.yml"],
        );
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        let e = &errors[0];
        assert!(e.message.contains("path reference not found"));
        assert_eq!(
            e.context.as_deref(),
            Some("../../site/vpn-globalprotect/software/corp-fonts.yml")
        );
        assert!(e.suggestion().unwrap().ends_with("base/software/corp-fonts.yml"));
        assert_eq!(e.fix_safety(), Some(FixSafety::Safe));
    }

    #[test]
    fn skips_globs_urls_and_vars() {
        let tmp = TempDir::new().unwrap();
        let yaml = "\
software:
  packages:
    - paths: ../lib/*.yml
    - path: \"https://example.com/x.yml\"
    - path: $SOFTWARE_DIR/app.yml
    - path: ../lib/glob-*.yml
";
        let errors = run(&tmp, "fleets/team.yml", yaml, &[]);
        assert!(
            errors.is_empty(),
            "globs/urls/vars must be skipped, got: {errors:?}"
        );
    }
}
