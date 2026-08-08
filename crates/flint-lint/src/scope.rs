//! Directory-level scope analysis for the `flint init` assistant.
//!
//! The assistant asks one in/out question per directory and emits a `[files]`
//! section from the answers. Everything here is the *analysis* half — the
//! prompting lives in the CLI (`cli/src/interactive/scope.rs`) so this stays
//! testable without a terminal.
//!
//! Two invariants shape the whole module:
//!
//! 1. **Scope units are DIRECTORIES (and exact root files), never
//!    extensions.** A non-empty `include` is authoritative
//!    ([`FleetLintConfig::is_out_of_scope_file`]) and it scopes the
//!    cross-file rules too — orphaned-file, duplicate-content,
//!    case-collision, unregistered-script all report on `.sh`,
//!    `.mobileconfig` and payloads. `include = ["**/*.yml"]` reads like a
//!    harmless tautology while silently switching every one of them off on
//!    non-YAML files. That shipped once. A [`ScopeUnit`] is a real path from
//!    the filesystem walk and its glob is `rel + "/**"`, so no code path here
//!    can produce an extension glob.
//!
//! 2. **The preview is computed by the enforcing code.** [`preview`] builds
//!    the exact [`FilesConfig`] it is about to write and asks
//!    `is_out_of_scope_file` about every file in the repo. A hand-rolled
//!    second matcher would be free to disagree with the linter.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::config::{FilesConfig, FleetLintConfig};
use super::cross_reference::ParsedFile;
use super::util::normalize_path;
use super::workspace::{referenced_by, Workspace};

/// One directory (or root-level YAML file) the assistant can put in or out
/// of scope.
#[derive(Debug, Clone)]
pub struct ScopeUnit {
    /// Slash-separated path relative to the repo root: `platforms`,
    /// `platforms/_retired`, `default.yml`.
    pub rel: String,
    /// False only for root-level YAML files, which are named exactly.
    pub is_dir: bool,
    /// 0 for a top-level unit, 1 for its children, and so on.
    pub depth: usize,
    /// Every file at or below this unit — not just YAML. The cross-file
    /// rules see all of them, which is exactly why the count matters.
    pub files: usize,
    /// How many of `files` are `.yml`/`.yaml`.
    pub yaml_files: usize,
    /// Files under this unit that a config OUTSIDE it reaches via
    /// `path:`/`paths:`.
    ///
    /// Deliberately excludes references that start inside the unit. A
    /// self-contained subtree — a retired software file pointing at its own
    /// icon — loses nothing referential when it goes out of scope, so
    /// warning about it is noise. What matters is whether live config
    /// elsewhere points in here.
    pub referenced: Vec<String>,
    /// The outside configs doing that referencing, deduplicated.
    pub referencing: BTreeSet<String>,
}

impl ScopeUnit {
    /// The glob that puts this unit in (or out of) scope. Directories get
    /// `/**`; a root file is named exactly. Never an extension pattern.
    pub fn glob(&self) -> String {
        if self.is_dir {
            format!("{}/**", self.rel)
        } else {
            self.rel.clone()
        }
    }

    /// The last path component, for display.
    pub fn label(&self) -> String {
        let leaf = self.rel.rsplit('/').next().unwrap_or(&self.rel);
        if self.is_dir {
            format!("{leaf}/")
        } else {
            leaf.to_string()
        }
    }
}

/// The result of walking a repo: every candidate scope unit plus the file
/// set the delta is measured against.
#[derive(Debug, Clone, Default)]
pub struct ScopeScan {
    /// The root the walk started from, lexically normalized.
    pub root: PathBuf,
    /// Units sorted by `rel`, so children follow their parent.
    pub units: Vec<ScopeUnit>,
    /// Every file in the repo, relative and slash-separated. This is the
    /// denominator in "N of M files in scope".
    pub files: Vec<String>,
}

impl ScopeScan {
    /// Units the first pass asks about: root YAML files first (they are the
    /// GitOps entry points), then top-level directories.
    pub fn top_level(&self) -> Vec<&ScopeUnit> {
        let mut out: Vec<&ScopeUnit> = self.units.iter().filter(|u| u.depth == 0).collect();
        out.sort_by(|a, b| a.is_dir.cmp(&b.is_dir).then_with(|| a.rel.cmp(&b.rel)));
        out
    }

    /// Direct subdirectories of `rel`, for the drill-down prompt.
    pub fn children(&self, rel: &str) -> Vec<&ScopeUnit> {
        let prefix = format!("{rel}/");
        let depth = rel.matches('/').count() + 1;
        let mut out: Vec<&ScopeUnit> = self
            .units
            .iter()
            .filter(|u| u.depth == depth && u.rel.starts_with(&prefix))
            .collect();
        out.sort_by(|a, b| a.rel.cmp(&b.rel));
        out
    }

    /// Total files in the repo, as flint sees it.
    pub fn total_files(&self) -> usize {
        self.files.len()
    }
}

/// Walk `root` and build the scope tree.
///
/// The walk is [`Workspace::build`]'s, deliberately: the preview has to
/// predict what the linter will do, and the linter's cross-file pass sees
/// exactly this file set. That means `.gitignore` is NOT consulted — a
/// gitignored build directory still shows up here, because it would still be
/// walked by the workspace pass if it were left in scope.
pub fn scan(root: &Path) -> ScopeScan {
    let root_norm = normalize_path(root);

    // One walk, reused twice: the first Workspace exists only to produce the
    // file list, which is also what the YAML parse needs.
    let files = Workspace::build(&root_norm, &[]).files;

    let parsed: Vec<ParsedFile> = files
        .iter()
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("yml" | "yaml")
            )
        })
        .filter_map(|path| {
            let source = std::fs::read_to_string(path).ok()?;
            Some(ParsedFile {
                path: path.clone(),
                yaml: serde_yaml::from_str(&source).ok()?,
                source,
            })
        })
        .collect();

    let ws = Workspace::from_files(files.clone(), &parsed);

    // target file -> the configs that reference it. Same resolution the
    // orphaned-file rule uses, so "referenced" here means what it means
    // there.
    let mut referenced: HashMap<PathBuf, BTreeSet<String>> = HashMap::new();
    for pf in ws.parsed {
        let by = rel_of(&root_norm, &pf.path);
        for target in referenced_by(pf, &ws) {
            referenced.entry(target).or_default().insert(by.clone());
        }
    }

    // Accumulate per-directory counts by walking each file's ancestors once.
    struct Acc {
        files: usize,
        yaml_files: usize,
        referenced: Vec<String>,
        referencing: BTreeSet<String>,
    }
    let mut dirs: BTreeMap<String, Acc> = BTreeMap::new();
    let mut root_yaml: Vec<ScopeUnit> = Vec::new();
    let mut rel_files: Vec<String> = Vec::with_capacity(files.len());

    for abs in &files {
        let rel = rel_of(&root_norm, abs);
        rel_files.push(rel.clone());

        let is_yaml = matches!(
            abs.extension().and_then(|e| e.to_str()),
            Some("yml" | "yaml")
        );
        let refs = referenced.get(abs);

        let parts: Vec<&str> = rel.split('/').collect();
        if parts.len() == 1 {
            // A root-level file. Only YAML gets its own unit — a README or a
            // stray .DS_Store is not a scoping decision, it just falls out of
            // scope with everything else unlisted.
            if is_yaml {
                root_yaml.push(ScopeUnit {
                    rel: rel.clone(),
                    is_dir: false,
                    depth: 0,
                    files: 1,
                    yaml_files: 1,
                    referenced: if refs.is_some() { vec![rel.clone()] } else { vec![] },
                    referencing: refs.cloned().unwrap_or_default(),
                });
            }
            continue;
        }

        for depth in 0..parts.len() - 1 {
            let dir = parts[..=depth].join("/");
            let inside = format!("{dir}/");
            // Only references that ORIGINATE outside this directory count —
            // see the `referenced` field docs.
            let external: Vec<&String> = refs
                .map(|by| by.iter().filter(|c| !c.starts_with(&inside)).collect())
                .unwrap_or_default();

            let acc = dirs.entry(dir).or_insert_with(|| Acc {
                files: 0,
                yaml_files: 0,
                referenced: Vec::new(),
                referencing: BTreeSet::new(),
            });
            acc.files += 1;
            if is_yaml {
                acc.yaml_files += 1;
            }
            if !external.is_empty() {
                acc.referenced.push(rel.clone());
                acc.referencing.extend(external.into_iter().cloned());
            }
        }
    }

    let mut units: Vec<ScopeUnit> = root_yaml;
    units.extend(dirs.into_iter().map(|(rel, acc)| ScopeUnit {
        depth: rel.matches('/').count(),
        is_dir: true,
        files: acc.files,
        yaml_files: acc.yaml_files,
        referenced: acc.referenced,
        referencing: acc.referencing,
        rel,
    }));
    units.sort_by(|a, b| a.rel.cmp(&b.rel));

    rel_files.sort();
    ScopeScan {
        root: root_norm,
        units,
        files: rel_files,
    }
}

/// `abs` relative to `root`, slash-separated. Falls back to the full path if
/// it somehow escapes the root.
fn rel_of(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

/// One in/out answer.
#[derive(Debug, Clone)]
pub struct ScopeDecision {
    /// The unit's `rel`.
    pub rel: String,
    pub is_dir: bool,
    pub depth: usize,
    /// True = stays in scope.
    pub keep: bool,
}

/// The answers, in the order they were given.
#[derive(Debug, Clone, Default)]
pub struct ScopeSelection {
    decisions: Vec<ScopeDecision>,
}

impl ScopeSelection {
    /// Record one answer. A repeated `rel` replaces the earlier answer, so a
    /// re-run of the prompt loop overwrites rather than accumulates.
    pub fn decide(&mut self, unit: &ScopeUnit, keep: bool) {
        let decision = ScopeDecision {
            rel: unit.rel.clone(),
            is_dir: unit.is_dir,
            depth: unit.depth,
            keep,
        };
        match self.decisions.iter_mut().find(|d| d.rel == unit.rel) {
            Some(slot) => *slot = decision,
            None => self.decisions.push(decision),
        }
    }

    pub fn decisions(&self) -> &[ScopeDecision] {
        &self.decisions
    }

    /// Units dropped at any depth, shallowest first.
    pub fn dropped(&self) -> Vec<&ScopeDecision> {
        let mut out: Vec<&ScopeDecision> = self.decisions.iter().filter(|d| !d.keep).collect();
        out.sort_by(|a, b| a.depth.cmp(&b.depth).then_with(|| a.rel.cmp(&b.rel)));
        out
    }

    /// Whether anything at all was put out of scope.
    pub fn narrows(&self) -> bool {
        self.decisions.iter().any(|d| !d.keep)
    }

    /// The `include` list.
    ///
    /// EMPTY unless a TOP-LEVEL unit was dropped. Two reasons:
    ///
    /// - Empty `include` means "everything not excluded", which is the right
    ///   default and the only value that leaves the cross-file rules fully
    ///   armed. Dropping just `platforms/_retired` is expressible as a pure
    ///   denylist, so don't reach for the authoritative form.
    /// - Once a top-level directory is out, an allowlist is what the user
    ///   actually asked for: a tooling directory added next month should be
    ///   ignored by default rather than needing a new deny rule.
    ///
    /// Every entry is a directory glob or an exact root filename — by
    /// construction, since each comes from a [`ScopeUnit`] found on disk.
    pub fn include_globs(&self) -> Vec<String> {
        if !self.decisions.iter().any(|d| d.depth == 0 && !d.keep) {
            return Vec::new();
        }
        let mut out: Vec<String> = self
            .decisions
            .iter()
            .filter(|d| d.depth == 0 && d.keep)
            .map(glob_of)
            .collect();
        out.sort_by(|a, b| {
            // Root files first, mirroring the prompt order.
            let a_dir = a.ends_with("/**");
            let b_dir = b.ends_with("/**");
            a_dir.cmp(&b_dir).then_with(|| a.cmp(b))
        });
        out
    }

    /// The `exclude` list: every dropped unit, then the always-on defaults.
    ///
    /// Dropped top-level units are listed even though the allowlist already
    /// omits them — `exclude` wins over `include`, so this is redundant but
    /// it makes the intent readable at a glance, and it keeps the file
    /// correct if someone later clears `include`.
    pub fn exclude_globs(&self) -> Vec<String> {
        let mut out: Vec<String> = self.dropped().into_iter().map(glob_of).collect();
        for default in DEFAULT_EXCLUDES {
            if !out.iter().any(|g| g == default) {
                out.push((*default).to_string());
            }
        }
        out
    }
}

fn glob_of(d: &ScopeDecision) -> String {
    if d.is_dir {
        format!("{}/**", d.rel)
    } else {
        d.rel.clone()
    }
}

/// Excludes the generated config always carries.
pub const DEFAULT_EXCLUDES: &[&str] = &[
    "**/node_modules/**",
    "**/target/**",
    "**/.git/**",
    "**/dist/**",
];

/// A dropped unit that live config still points at.
#[derive(Debug, Clone)]
pub struct ReferencedWarning {
    /// The unit being dropped.
    pub rel: String,
    /// Referenced files under it.
    pub files: usize,
    /// Distinct configs doing the referencing.
    pub configs: usize,
    /// Up to three `dir/ (n)` samples, so the warning names something real.
    pub examples: Vec<String>,
}

/// What a selection will do, measured before anything is written.
#[derive(Debug, Clone)]
pub struct ScopePreview {
    pub total: usize,
    pub in_scope: usize,
    pub skipped: usize,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    /// Dropped units that live config references. Not errors — see
    /// [`ReferencedWarning`] and the note in [`preview`].
    pub warnings: Vec<ReferencedWarning>,
}

impl Default for ScopePreview {
    /// The unnarrowed repo: no `include` at all, only the always-on
    /// excludes. This is what a non-interactive `flint init` writes, and it
    /// is the one shape guaranteed to leave the cross-file rules armed on
    /// every scripts and profile.
    fn default() -> Self {
        ScopePreview {
            total: 0,
            in_scope: 0,
            skipped: 0,
            include: Vec::new(),
            exclude: DEFAULT_EXCLUDES.iter().map(|s| (*s).to_string()).collect(),
            warnings: Vec::new(),
        }
    }
}

/// Measure `sel` against `scan`.
///
/// The in-scope count comes from the real [`FleetLintConfig`] predicate, so
/// the number shown is the number the linter will honor.
///
/// The warnings answer a different question. Excluding a referenced
/// directory does NOT break reference resolution: scoping filters the
/// finding *subject*, not the [`Workspace`] file set, so a `path:` into an
/// excluded directory still resolves and broken-reference still fires. What
/// it does do is silence every finding *about* those files. That is a
/// legitimate choice — the reference repo makes it for `tools-scripts/` —
/// but it should be made knowingly, so it is surfaced rather than inferred.
pub fn preview(scan: &ScopeScan, sel: &ScopeSelection) -> ScopePreview {
    let include = sel.include_globs();
    let exclude = sel.exclude_globs();

    let mut cfg = FleetLintConfig::default();
    cfg.files = FilesConfig {
        include: include.clone(),
        exclude: exclude.clone(),
        root: None,
    };

    let in_scope = scan
        .files
        .iter()
        .filter(|rel| !cfg.is_out_of_scope_file(Path::new(rel)))
        .count();

    // Warn once per dropped subtree: a user who drops `tools-scripts` and
    // then also drops `tools-scripts/ddm-examples` should see one warning,
    // not two saying the same thing.
    let dropped = sel.dropped();
    let mut warnings = Vec::new();
    for d in &dropped {
        let shadowed = dropped
            .iter()
            .any(|other| other.rel != d.rel && d.rel.starts_with(&format!("{}/", other.rel)));
        if shadowed {
            continue;
        }
        let Some(unit) = scan.units.iter().find(|u| u.rel == d.rel) else {
            continue;
        };
        if unit.referenced.is_empty() {
            continue;
        }
        // Group the referenced files by their directory so the example names
        // the real location (`tools-scripts/ddm-examples/`), not the unit the
        // user happened to answer about.
        let mut by_dir: BTreeMap<&str, usize> = BTreeMap::new();
        for f in &unit.referenced {
            let dir = f.rsplit_once('/').map(|(d, _)| d).unwrap_or(".");
            *by_dir.entry(dir).or_insert(0) += 1;
        }
        let mut ranked: Vec<(&str, usize)> = by_dir.into_iter().collect();
        ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        warnings.push(ReferencedWarning {
            rel: d.rel.clone(),
            files: unit.referenced.len(),
            configs: unit.referencing.len(),
            examples: ranked
                .iter()
                .take(3)
                .map(|(dir, n)| format!("{dir}/ ({n})"))
                .collect(),
        });
    }

    let total = scan.total_files();
    ScopePreview {
        total,
        in_scope,
        skipped: total.saturating_sub(in_scope),
        include,
        exclude,
        warnings,
    }
}

/// Render the `[files]` section for a selection.
///
/// Called with a selection that narrows nothing, this emits the same
/// comment-only `include` the non-interactive template has always written.
pub fn render_files_section(preview: &ScopePreview) -> String {
    let mut out = String::new();
    out.push_str("# File Patterns\n[files]\n");

    if preview.include.is_empty() {
        // `include` deliberately LEFT UNSET — see the module docs. Unset
        // means "everything not excluded", which keeps the cross-file rules
        // armed on scripts and profiles.
        out.push_str("# Scope. Leaving `include` unset lints everything not excluded.\n");
        out.push_str("# To narrow, list DIRECTORIES (not extensions) so scripts and\n");
        out.push_str("# profiles stay in scope for the cross-file rules:\n");
        out.push_str("#   include = [\"default.yml\", \"fleets/**\", \"platforms/**\"]\n");
    } else {
        out.push_str("# Scope, chosen with `flint init`. `include` is authoritative: a path\n");
        out.push_str("# matching none of these globs is out of scope — including for the\n");
        out.push_str("# cross-file rules (orphaned-file, duplicate-content, case-collision,\n");
        out.push_str("# unregistered-script), which report on scripts and profiles rather\n");
        out.push_str("# than YAML. That is why these are DIRECTORY globs: an extension glob\n");
        out.push_str("# like \"**/*.yml\" would read as a tautology while silently switching\n");
        out.push_str("# all of them off on every non-YAML file.\n");
        out.push_str("#\n");
        out.push_str("# It is also an allowlist: a directory added later is out of scope\n");
        out.push_str("# until it is listed here.\n");
        out.push_str("include = [\n");
        for g in &preview.include {
            out.push_str(&format!("    \"{g}\",\n"));
        }
        out.push_str("]\n");
    }

    out.push_str("\n# `exclude` wins over `include`.\n");
    out.push_str("exclude = [\n");
    for g in &preview.exclude {
        if let Some(w) = preview.warnings.iter().find(|w| glob_matches_unit(g, &w.rel)) {
            out.push_str(&format!(
                "    # {} config file(s) reference {} file(s) under {} — references still\n",
                w.configs, w.files, w.rel
            ));
            out.push_str("    # resolve (scoping filters the finding subject, not the workspace\n");
            out.push_str("    # file set); findings ABOUT those files are suppressed.\n");
        }
        out.push_str(&format!("    \"{g}\",\n"));
    }
    out.push_str("]\n");
    out
}

/// Whether `glob` is the entry generated for the unit at `rel`.
fn glob_matches_unit(glob: &str, rel: &str) -> bool {
    glob == rel || glob == format!("{rel}/**")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// A miniature GitOps repo: a referenced script under an otherwise
    /// tooling-looking directory, plus a directory nothing points at.
    fn fixture() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let r = tmp.path();
        fs::create_dir_all(r.join("fleets")).unwrap();
        fs::create_dir_all(r.join("platforms/macos/scripts")).unwrap();
        fs::create_dir_all(r.join("platforms/_retired")).unwrap();
        fs::create_dir_all(r.join("tools-scripts/ddm-examples")).unwrap();
        fs::create_dir_all(r.join("tools-scripts/build")).unwrap();

        fs::write(r.join("default.yml"), "org_settings:\n  scripts: []\n").unwrap();
        fs::write(
            r.join("fleets/workstations.yml"),
            "name: Workstations\ncontrols:\n  scripts:\n    - path: \
             ../platforms/macos/scripts/a.sh\n    - path: \
             ../tools-scripts/ddm-examples/ddm.sh\n",
        )
        .unwrap();
        fs::write(
            r.join("fleets/servers.yml"),
            "name: Servers\ncontrols:\n  scripts:\n    - path: \
             ../tools-scripts/ddm-examples/ddm.sh\n",
        )
        .unwrap();
        fs::write(r.join("platforms/macos/scripts/a.sh"), "#!/bin/sh\n").unwrap();
        // A self-contained subtree: `old.yml` points at an icon next to it
        // and nothing outside points in. Retiring it costs no reference.
        fs::write(r.join("platforms/_retired/icon.png"), "PNG").unwrap();
        fs::write(
            r.join("platforms/_retired/old.yml"),
            "name: Old\nsoftware:\n  - path: icon.png\n",
        )
        .unwrap();
        fs::write(r.join("tools-scripts/ddm-examples/ddm.sh"), "#!/bin/sh\n").unwrap();
        fs::write(r.join("tools-scripts/build/Makefile"), "all:\n").unwrap();
        fs::write(r.join("README.md"), "# repo\n").unwrap();
        tmp
    }

    fn select(scan: &ScopeScan, keep: &[&str], drop: &[&str]) -> ScopeSelection {
        let mut sel = ScopeSelection::default();
        for rel in keep {
            sel.decide(scan.units.iter().find(|u| &u.rel == rel).unwrap(), true);
        }
        for rel in drop {
            sel.decide(scan.units.iter().find(|u| &u.rel == rel).unwrap(), false);
        }
        sel
    }

    #[test]
    fn scan_finds_directories_and_root_yaml() {
        let tmp = fixture();
        let scan = scan(tmp.path());

        let names: Vec<&str> = scan.top_level().iter().map(|u| u.rel.as_str()).collect();
        assert_eq!(names, vec!["default.yml", "fleets", "platforms", "tools-scripts"]);

        // README.md counts toward the denominator but is not a scope unit —
        // it is not a decision anyone wants to be asked about.
        assert!(scan.files.contains(&"README.md".to_string()));
        assert!(!scan.units.iter().any(|u| u.rel == "README.md"));

        let platforms = scan.units.iter().find(|u| u.rel == "platforms").unwrap();
        assert_eq!(platforms.files, 3, "a.sh, _retired/old.yml, _retired/icon.png");
        assert_eq!(platforms.yaml_files, 1);

        let children: Vec<&str> = scan
            .children("platforms")
            .iter()
            .map(|u| u.rel.as_str())
            .collect();
        assert_eq!(children, vec!["platforms/_retired", "platforms/macos"]);
    }

    #[test]
    fn scan_records_who_references_what() {
        let tmp = fixture();
        let scan = scan(tmp.path());

        let tools = scan.units.iter().find(|u| u.rel == "tools-scripts").unwrap();
        assert_eq!(tools.referenced, vec!["tools-scripts/ddm-examples/ddm.sh"]);
        assert_eq!(
            tools.referencing.len(),
            2,
            "both fleet files point at the same script"
        );

        // The CONTROL: a directory nothing references must come back empty,
        // otherwise "no references" and "reference detection is broken" look
        // identical in every other assertion here.
        let build = scan
            .units
            .iter()
            .find(|u| u.rel == "tools-scripts/build")
            .unwrap();
        assert!(build.referenced.is_empty());
        assert!(build.referencing.is_empty());
    }

    #[test]
    fn keeping_everything_leaves_include_unset() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms", "tools-scripts"],
            &[],
        );

        assert!(!sel.narrows());
        let p = preview(&scan, &sel);
        assert!(
            p.include.is_empty(),
            "an unnarrowed selection must not write an authoritative include"
        );
        assert_eq!(p.in_scope, p.total, "nothing excluded, nothing skipped");
        assert_eq!(p.skipped, 0);
    }

    #[test]
    fn dropping_a_top_level_dir_emits_directory_globs() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms"],
            &["tools-scripts"],
        );
        let p = preview(&scan, &sel);

        assert_eq!(
            p.include,
            vec!["default.yml", "fleets/**", "platforms/**"],
            "include must be root files and directory globs"
        );
        assert!(p.exclude.contains(&"tools-scripts/**".to_string()));
    }

    #[test]
    fn nested_drop_alone_stays_a_denylist() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms", "tools-scripts"],
            &["platforms/_retired"],
        );
        let p = preview(&scan, &sel);

        assert!(
            p.include.is_empty(),
            "dropping only a subdirectory is expressible without an authoritative include"
        );
        assert!(p.exclude.contains(&"platforms/_retired/**".to_string()));
        assert_eq!(p.skipped, 2, "old.yml and icon.png under platforms/_retired");
    }

    /// Requirement 1, enforced end to end: no generated `include` entry may
    /// be an extension glob, because a non-empty `include` also scopes the
    /// cross-file rules and would switch them off on every `.sh`.
    #[test]
    fn include_entries_are_never_extension_globs() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms"],
            &["tools-scripts"],
        );
        let p = preview(&scan, &sel);

        assert!(!p.include.is_empty());
        for g in &p.include {
            assert!(
                !g.contains("*."),
                "generated include entry {g:?} is an extension glob"
            );
            assert!(
                g.ends_with("/**") || !g.contains('*'),
                "generated include entry {g:?} is neither a directory glob nor an exact file"
            );
        }

        // And the thing an extension include would have broken still works:
        // a script under an included directory is in scope for the
        // cross-file rules.
        let mut cfg = FleetLintConfig::default();
        cfg.files = FilesConfig {
            include: p.include.clone(),
            exclude: p.exclude.clone(),
            root: None,
        };
        assert!(
            !cfg.is_out_of_scope_file(Path::new("platforms/macos/scripts/a.sh")),
            "a directory include must keep scripts in scope"
        );

        // THE CONTROL. Without it, the assertion above passes just as
        // happily if `is_out_of_scope_file` were stubbed to `false` — "the
        // script is in scope" and "scoping does nothing" look identical.
        // Here is the exact mistake this whole module exists to prevent:
        let mut broken = FleetLintConfig::default();
        broken.files = FilesConfig {
            include: vec!["**/*.yml".to_string(), "**/*.yaml".to_string()],
            exclude: p.exclude.clone(),
            root: None,
        };
        assert!(
            broken.is_out_of_scope_file(Path::new("platforms/macos/scripts/a.sh")),
            "control failed: an extension include did NOT drop the script, so \
             the directory-include assertion above proves nothing"
        );
    }

    #[test]
    fn excluding_a_referenced_directory_warns() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms"],
            &["tools-scripts"],
        );
        let p = preview(&scan, &sel);

        assert_eq!(p.warnings.len(), 1);
        let w = &p.warnings[0];
        assert_eq!(w.rel, "tools-scripts");
        assert_eq!(w.files, 1);
        assert_eq!(w.configs, 2);
        assert_eq!(w.examples, vec!["tools-scripts/ddm-examples/ (1)"]);

        // The warning is carried into the file, not just the terminal.
        let section = render_files_section(&p);
        assert!(section.contains("2 config file(s) reference 1 file(s) under tools-scripts"));
    }

    /// A subtree whose only references start INSIDE it must not warn.
    ///
    /// `platforms/_retired/old.yml` points at `platforms/_retired/icon.png`.
    /// Retiring the whole directory breaks nothing — the reference goes out
    /// of scope together with its target. Counting that as "2 configs
    /// reference this" is the same overreach as flagging a glob that matches
    /// nothing: a warning about a consequence that cannot occur. Found by
    /// running the assistant against the reference repo, not by a unit test.
    #[test]
    fn internal_references_do_not_count_as_referenced() {
        let tmp = fixture();
        let scan = scan(tmp.path());

        let retired = scan
            .units
            .iter()
            .find(|u| u.rel == "platforms/_retired")
            .unwrap();
        assert_eq!(retired.files, 2, "control: the icon and the yml are both there");
        assert!(
            retired.referenced.is_empty(),
            "a self-contained subtree has no incoming references"
        );

        // The CONTROL on the control: the same file IS counted one level up,
        // where `platforms/macos/scripts/a.sh` is reached from `fleets/`.
        // Without this, a `referenced` that is always empty passes above.
        let platforms = scan.units.iter().find(|u| u.rel == "platforms").unwrap();
        assert_eq!(platforms.referenced, vec!["platforms/macos/scripts/a.sh"]);

        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms", "tools-scripts"],
            &["platforms/_retired"],
        );
        assert!(preview(&scan, &sel).warnings.is_empty());
    }

    /// The CONTROL for the warning: excluding a directory nothing points at
    /// must produce NO warning. Without this, a `warnings` list that is
    /// always populated (or a `referenced` set that is always non-empty)
    /// passes the test above.
    #[test]
    fn excluding_an_unreferenced_directory_does_not_warn() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms", "tools-scripts"],
            &["tools-scripts/build"],
        );
        let p = preview(&scan, &sel);
        assert!(p.warnings.is_empty());
    }

    #[test]
    fn nested_drop_does_not_double_warn() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms"],
            &["tools-scripts", "tools-scripts/ddm-examples"],
        );
        let p = preview(&scan, &sel);
        assert_eq!(p.warnings.len(), 1, "one warning per dropped subtree");
        assert_eq!(p.warnings[0].rel, "tools-scripts");
    }

    /// The delta shown must be the delta enforced: parse the rendered TOML
    /// back through the real config loader and re-count. This is what stops
    /// the preview and the written file from drifting apart.
    #[test]
    fn rendered_config_reproduces_the_previewed_delta() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let sel = select(
            &scan,
            &["default.yml", "fleets", "platforms"],
            &["tools-scripts", "platforms/_retired"],
        );
        let p = preview(&scan, &sel);

        assert!(p.skipped > 0, "control: this selection must skip something");

        let toml = render_files_section(&p);
        let parsed: FleetLintConfig = toml::from_str(&toml).expect("rendered [files] must parse");
        let recounted = scan
            .files
            .iter()
            .filter(|rel| !parsed.is_out_of_scope_file(Path::new(rel)))
            .count();

        assert_eq!(
            recounted, p.in_scope,
            "the written config must put exactly the previewed files in scope"
        );

        // Concretely, for this fixture: default.yml, 2 fleet files, a.sh.
        assert_eq!(p.in_scope, 4);
        assert_eq!(p.total, 9);
    }

    #[test]
    fn re_answering_a_unit_replaces_the_earlier_answer() {
        let tmp = fixture();
        let scan = scan(tmp.path());
        let unit = scan.units.iter().find(|u| u.rel == "tools-scripts").unwrap();

        let mut sel = ScopeSelection::default();
        sel.decide(unit, false);
        sel.decide(unit, true);

        assert_eq!(sel.decisions().len(), 1);
        assert!(!sel.narrows());
    }
}
