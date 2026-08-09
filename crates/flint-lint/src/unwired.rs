//! Unwired-artifact detection (`flint paths --unwired`).
//!
//! The inverse of the `path-exists` rule: instead of finding references that
//! point at missing files, this finds *files that exist but nothing references*
//! — orphaned profiles, scripts, software, etc. that were dropped into the repo
//! but never wired into a fleet via `path:`/`paths:`.
//!
//! Reachability is computed across the whole repo:
//!   - collect every `path:`/`paths:` reference in the fleet config files
//!     (`default.yml`, `fleets/*`, `teams/*`), skipping commented-out lines;
//!   - expand `paths:` globs (doublestar `**`, `*`, `?`);
//!   - an artifact is "wired" if some reference resolves to / matches it.
//!
//! Each orphan is classified to the GitOps section that would wire it, so the
//! caller can suggest a concrete construct.

use super::util::normalize_path;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// An artifact file that exists on disk but is referenced by nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnwiredArtifact {
    /// Absolute path to the artifact.
    pub path: PathBuf,
    /// Path relative to the workspace root, POSIX-style (canonical reference).
    pub rel: String,
    /// The value to write in `path:`/`paths:`, relative to where this repo's
    /// fleet/team config files actually live (NOT a hardcoded `../`). For a
    /// repo with fleets in `fleets/` this is `../platforms/…`; for a flat repo
    /// wiring from `default.yml` at the root it is `platforms/…`.
    pub wire: String,
    /// GitOps section that would wire this file (e.g.
    /// `controls.apple_settings.configuration_profiles`).
    pub section: &'static str,
    /// True when the section takes a single file only (no `paths:` glob form),
    /// e.g. `controls.setup_experience.apple_setup_assistant`.
    pub single_only: bool,
}

/// Result of an unwired scan.
pub struct UnwiredReport {
    /// The workspace-root-relative directory the `wire` paths are written from
    /// (where the repo's fleet/team files live), e.g. `fleets`. Empty if the
    /// repo wires from the root (`default.yml`).
    pub wire_base_rel: String,
    /// The orphaned artifacts, sorted by relative path.
    pub artifacts: Vec<UnwiredArtifact>,
}

/// Find every artifact under `root` that no fleet config references.
pub fn find_unwired(root: &Path) -> UnwiredReport {
    let root_abs = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let configs = config_files(&root_abs);

    // Where this repo's fleet/team files actually live — so suggested paths
    // match the repo's structure instead of assuming `fleets/`.
    let wire_base = representative_dir(&configs, &root_abs);
    let wire_base_rel = wire_base
        .strip_prefix(&root_abs)
        .unwrap_or(Path::new(""))
        .to_string_lossy()
        .replace('\\', "/");

    // 1. Reachable set: single refs (absolute, normalized) + glob patterns.
    let mut singles: HashSet<PathBuf> = HashSet::new();
    let mut globs: Vec<String> = Vec::new();
    for config in &configs {
        let dir = match config.parent() {
            Some(d) => d.canonicalize().unwrap_or_else(|_| d.to_path_buf()),
            None => continue,
        };
        let content = match std::fs::read_to_string(config) {
            Ok(c) => c,
            Err(_) => continue,
        };
        for value in path_refs(&content) {
            let resolved = normalize_path(&dir.join(&value));
            if value.contains('*') || value.contains('?') {
                globs.push(resolved.to_string_lossy().replace('\\', "/"));
            } else {
                singles.insert(resolved);
            }
        }
    }

    // 2. Walk artifacts; an artifact is orphaned if no single/glob covers it.
    let mut out = Vec::new();
    let mut artifacts = Vec::new();
    collect_artifacts(&root_abs, &mut artifacts);

    for path in artifacts {
        let canon = path.canonicalize().unwrap_or_else(|_| path.clone());
        if singles.contains(&canon) {
            continue;
        }
        let canon_str = canon.to_string_lossy().replace('\\', "/");
        if globs.iter().any(|g| glob_match(g, &canon_str)) {
            continue;
        }
        let (section, single_only) = classify(&canon);
        let rel = canon
            .strip_prefix(&root_abs)
            .unwrap_or(&canon)
            .to_string_lossy()
            .replace('\\', "/");
        let wire = rel_to(&wire_base, &canon);
        out.push(UnwiredArtifact {
            path: canon,
            rel,
            wire,
            section,
            single_only,
        });
    }

    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    UnwiredReport {
        wire_base_rel,
        artifacts: out,
    }
}

/// The directory most fleet/team config files share — the natural place a user
/// writes relative `path:` values from. Falls back to the repo root.
fn representative_dir(configs: &[PathBuf], root: &Path) -> PathBuf {
    use std::collections::HashMap;
    let mut counts: HashMap<PathBuf, usize> = HashMap::new();
    for c in configs {
        if let Some(p) = c.parent() {
            *counts.entry(p.to_path_buf()).or_default() += 1;
        }
    }
    // Most config files win; tie → prefer a `fleets`/`teams` dir, then deeper.
    counts
        .into_iter()
        .max_by_key(|(p, n)| {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let preferred = matches!(name, "fleets" | "teams") as usize;
            (*n, preferred, p.components().count())
        })
        .map(|(p, _)| p)
        .unwrap_or_else(|| root.to_path_buf())
}

/// POSIX-style relative path from `from_dir` to `target`, climbing with `..`.
/// Public so callers can compute a `path:` value relative to a *specific*
/// fleet file (correct regardless of that file's depth).
pub fn rel_to(from_dir: &Path, target: &Path) -> String {
    let from = from_dir.canonicalize().unwrap_or_else(|_| from_dir.to_path_buf());
    let to = target.canonicalize().unwrap_or_else(|_| target.to_path_buf());
    let fc: Vec<_> = from.components().collect();
    let tc: Vec<_> = to.components().collect();
    let mut i = 0;
    while i < fc.len() && i < tc.len() && fc[i] == tc[i] {
        i += 1;
    }
    let mut r = PathBuf::new();
    for _ in i..fc.len() {
        r.push("..");
    }
    for c in &tc[i..] {
        r.push(c.as_os_str());
    }
    r.to_string_lossy().replace('\\', "/")
}

/// The fleet/team config files that may contain (or receive) `path:`/`paths:`
/// references — the candidate targets for interactive wiring.
pub fn config_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let default = root.join("default.yml");
    if default.exists() {
        files.push(default);
    }
    for sub in ["fleets", "teams"] {
        collect_yaml(&root.join(sub), &mut files);
    }
    files
}

fn collect_yaml(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_yaml(&path, files);
        } else if matches!(path.extension().and_then(|e| e.to_str()), Some("yml") | Some("yaml")) {
            files.push(path);
        }
    }
}

/// Extract `path:`/`paths:` values from YAML source, skipping commented lines.
fn path_refs(source: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            continue; // commented-out wiring is not active
        }
        let body = trimmed.trim_start_matches('-').trim_start();
        let value = body
            .strip_prefix("paths:")
            .or_else(|| body.strip_prefix("path:"));
        if let Some(v) = value {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if v.is_empty() || v.starts_with('$') || v.contains("://") {
                continue;
            }
            refs.push(v.to_string());
        }
    }
    refs
}

/// Collect candidate artifact files under `root`.
fn collect_artifacts(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.')
                    || matches!(name, "node_modules" | "target" | "dist")
                {
                    continue;
                }
            }
            collect_artifacts(&path, out);
        } else if is_artifact(&path) {
            out.push(path);
        }
    }
}

/// Whether a file is a wirable artifact (profile / script / software / policy…).
fn is_artifact(path: &Path) -> bool {
    let name = match path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return false,
    };
    if name.starts_with('.') || name == ".gitkeep" {
        return false;
    }
    let lower = path.to_string_lossy().to_lowercase();
    match path.extension().and_then(|e| e.to_str()) {
        Some("mobileconfig") | Some("xml") | Some("sh") | Some("ps1") => true,
        // JSON is an artifact only inside profile dirs (avoids flagging stray json).
        Some("json") => {
            lower.contains("/declaration-profiles/") || lower.contains("/enrollment-profiles/")
        }
        // YAML is an artifact only as a leaf under these dirs (else it's config).
        Some("yml") | Some("yaml") => ["/software/", "/policies/", "/queries/", "/reports/"]
            .iter()
            .any(|d| lower.contains(d)),
        _ => false,
    }
}

/// Map an artifact to the GitOps section that would wire it.
///
/// Uses current Fleet key names: `apple_settings.configuration_profiles`
/// (the rename of `macos_settings.custom_settings`) and
/// `setup_experience.apple_setup_assistant` (rename of
/// `macos_setup.macos_setup_assistant`). See `structure.rs` KEY_REGISTRY.
fn classify(path: &Path) -> (&'static str, bool) {
    let lower = path.to_string_lossy().to_lowercase();
    match path.extension().and_then(|e| e.to_str()) {
        Some("mobileconfig") => ("controls.apple_settings.configuration_profiles", false),
        Some("xml") => ("controls.windows_settings.configuration_profiles", false),
        Some("sh") | Some("ps1") => ("controls.scripts", false),
        Some("json") if lower.contains("/enrollment-profiles/") => {
            ("controls.setup_experience.apple_setup_assistant", true)
        }
        Some("json") => ("controls.apple_settings.configuration_profiles", false),
        _ if lower.contains("/software/") => ("software.packages", false),
        _ if lower.contains("/policies/") => ("policies", false),
        _ if lower.contains("/queries/") || lower.contains("/reports/") => ("reports", false),
        _ => ("(wire manually)", true),
    }
}

/// Doublestar glob match. `**` spans directory separators, `*`/`?` do not.
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let p: Vec<&str> = pattern.split('/').collect();
    let t: Vec<&str> = text.split('/').collect();
    seg_match(&p, &t)
}

fn seg_match(p: &[&str], t: &[&str]) -> bool {
    if p.is_empty() {
        return t.is_empty();
    }
    if p[0] == "**" {
        // `**` matches zero or more whole segments.
        (0..=t.len()).any(|i| seg_match(&p[1..], &t[i..]))
    } else if t.is_empty() {
        false
    } else if fnmatch_segment(p[0], t[0]) {
        seg_match(&p[1..], &t[1..])
    } else {
        false
    }
}

/// Wildcard match within a single path segment (`*` and `?`, no `/` crossing).
fn fnmatch_segment(pat: &str, s: &str) -> bool {
    let pc: Vec<char> = pat.chars().collect();
    let sc: Vec<char> = s.chars().collect();
    let (np, ns) = (pc.len(), sc.len());
    let mut dp = vec![vec![false; ns + 1]; np + 1];
    dp[0][0] = true;
    for i in 1..=np {
        if pc[i - 1] == '*' {
            dp[i][0] = dp[i - 1][0];
        }
    }
    for i in 1..=np {
        for j in 1..=ns {
            dp[i][j] = match pc[i - 1] {
                '*' => dp[i - 1][j] || dp[i][j - 1],
                '?' => dp[i - 1][j - 1],
                c => dp[i - 1][j - 1] && c == sc[j - 1],
            };
        }
    }
    dp[np][ns]
}

// ---------------------------------------------------------------------------
// In-place wiring (used by `flint paths --unwired --interactive`)
// ---------------------------------------------------------------------------

/// Insert `item` as a list element under the nested `section` key path in YAML
/// `source`, creating any missing keys.
///
/// `item` may be multi-line — e.g. a `- path:` entry carrying a nested
/// `labels_include_any:` block. Its internal relative indentation is preserved
/// and shifted to the section's child indent. `comment` (without a leading
/// `#`) is appended inline to the item's first line. Line-based so existing
/// formatting/comments survive; uses 2-space indentation (Fleet convention).
pub fn insert_under(source: &str, section: &[&str], item: &str, comment: &str) -> String {
    let mut lines: Vec<String> = source.lines().map(|l| l.to_string()).collect();

    // Descend through existing keys, narrowing [lo, hi) to the current block.
    let mut lo = 0usize;
    let mut hi = lines.len();
    let mut indent = 0usize;
    let mut depth = 0usize;
    let mut last_key_idx: Option<usize> = None;
    while depth < section.len() {
        match find_key(&lines, lo, hi, indent, section[depth]) {
            Some(idx) => {
                hi = block_end(&lines, idx, indent, hi);
                lo = idx + 1;
                indent += 2;
                depth += 1;
                last_key_idx = Some(idx);
            }
            None => break,
        }
    }

    // If the deepest matched key holds an inline empty collection (`key: []` or
    // `key: {}`), strip it so block children can be added — otherwise we'd emit
    // invalid YAML (a `[]` mapping/sequence can't also have block items).
    if depth == section.len() {
        if let Some(idx) = last_key_idx {
            let line = &lines[idx];
            let trimmed_end = line.trim_end();
            if trimmed_end.ends_with(": []") || trimmed_end.ends_with(": {}") {
                lines[idx] = trimmed_end[..trimmed_end.len() - 3].trim_end().to_string();
            }
        }
    }

    // Reliable placement: insert right after the last *real* (non-comment,
    // non-blank) line of the block, so new entries group with the existing
    // ones and never land after a trailing commented-out sibling block.
    let mut insert_at = lo;
    for (j, line) in lines.iter().enumerate().take(hi).skip(lo) {
        let t = line.trim_start();
        if !t.is_empty() && !t.starts_with('#') {
            insert_at = j + 1;
        }
    }

    // Build the lines to insert: any missing tail keys, then the item block.
    let mut block = Vec::new();
    let mut pad = indent;
    for key in &section[depth..] {
        block.push(format!("{}{}:", " ".repeat(pad), key));
        pad += 2;
    }
    // `pad` is now the item's base indent. Shift each item line by it.
    for (k, raw) in item.lines().enumerate() {
        let mut line = format!("{}{}", " ".repeat(pad), raw);
        if k == 0 && !comment.is_empty() {
            line.push_str(&format!("  # {comment}"));
        }
        block.push(line);
    }
    for (k, line) in block.into_iter().enumerate() {
        lines.insert(insert_at + k, line);
    }

    let mut out = lines.join("\n");
    if source.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Find a `key:` mapping at exactly `indent` spaces within `lines[lo..hi]`.
fn find_key(lines: &[String], lo: usize, hi: usize, indent: usize, key: &str) -> Option<usize> {
    (lo..hi).find(|&i| {
        let line = &lines[i];
        let trimmed = line.trim_start();
        !trimmed.is_empty()
            && !trimmed.starts_with('#')
            && line.len() - trimmed.len() == indent
            && trimmed.len() > key.len()
            && trimmed.starts_with(key)
            && trimmed[key.len()..].starts_with(':')
    })
}

/// End (exclusive) of the block opened by the key at `start` (at `indent`):
/// the first later line whose indentation is `<= indent` (ignoring blanks and
/// comments), capped at `hi`.
fn block_end(lines: &[String], start: usize, indent: usize, hi: usize) -> usize {
    (start + 1..hi)
        .find(|&i| {
            let line = &lines[i];
            let trimmed = line.trim_start();
            !trimmed.is_empty()
                && !trimmed.starts_with('#')
                && line.len() - trimmed.len() <= indent
        })
        .unwrap_or(hi)
}

/// Collect label names defined in the workspace (files in any `labels/` dir or
/// `*.labels.yml`), for suggesting targets when wiring with `labels_*`.
pub fn known_labels(root: &Path) -> Vec<String> {
    let mut files = Vec::new();
    collect_label_files(root, &mut files);
    let mut names = Vec::new();
    for f in files {
        if let Ok(content) = std::fs::read_to_string(&f) {
            if let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&content) {
                collect_label_names(&value, &mut names);
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

fn collect_label_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with('.') || matches!(name, "node_modules" | "target" | "dist") {
                    continue;
                }
            }
            collect_label_files(&path, out);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            let in_labels_dir = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                == Some("labels");
            if in_labels_dir || name.contains(".labels.") {
                out.push(path);
            }
        }
    }
}

fn collect_label_names(value: &serde_yaml::Value, out: &mut Vec<String>) {
    match value {
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                if let Some(name) = item.get("name").and_then(|n| n.as_str()) {
                    out.push(name.to_string());
                }
            }
        }
        serde_yaml::Value::Mapping(_) => {
            if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                out.push(name.to_string());
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn parses(s: &str) -> serde_yaml::Value {
        serde_yaml::from_str(s).expect("inserted YAML must still parse")
    }

    #[test]
    fn test_insert_under_existing_section() {
        let src = "\
controls:
  apple_settings:
    configuration_profiles:
      - paths: ../platforms/macos/configuration-profiles/security/*.mobileconfig
software:
  packages: []
";
        let out = insert_under(
            src,
            &["controls", "apple_settings", "configuration_profiles"],
            "- paths: ../platforms/macos/configuration-profiles/*.mobileconfig",
            "added by flint",
        );
        // Inserted as a sibling list item at the right indent, with comment.
        assert!(out.contains(
            "      - paths: ../platforms/macos/configuration-profiles/*.mobileconfig  # added by flint"
        ));
        // The pre-existing item and unrelated sections are untouched.
        assert!(out.contains("security/*.mobileconfig"));
        assert!(out.contains("software:"));
        let v = parses(&out);
        let profs = &v["controls"]["apple_settings"]["configuration_profiles"];
        assert_eq!(profs.as_sequence().unwrap().len(), 2);
    }

    #[test]
    fn test_insert_multiline_with_labels() {
        let src = "controls:\n  apple_settings:\n    configuration_profiles:\n      - paths: ../x/*.mobileconfig\n";
        let item = "- path: ../platforms/macos/configuration-profiles/vip.mobileconfig\n  labels_include_any:\n    - \"VIP Users\"\n    - \"Execs\"";
        let out = insert_under(
            src,
            &["controls", "apple_settings", "configuration_profiles"],
            item,
            "wired by flint",
        );
        let v = parses(&out);
        let profs = v["controls"]["apple_settings"]["configuration_profiles"]
            .as_sequence()
            .unwrap();
        assert_eq!(profs.len(), 2);
        let added = &profs[1];
        assert_eq!(
            added["path"],
            serde_yaml::Value::from("../platforms/macos/configuration-profiles/vip.mobileconfig")
        );
        let labels = added["labels_include_any"].as_sequence().unwrap();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0], serde_yaml::Value::from("VIP Users"));
    }

    #[test]
    fn test_known_labels() {
        let tmp = TempDir::new().unwrap();
        write(
            &tmp,
            "labels/teams.yml",
            "- name: VIP Users\n  description: x\n- name: Execs\n  description: y\n",
        );
        let names = known_labels(tmp.path());
        assert_eq!(names, vec!["Execs".to_string(), "VIP Users".to_string()]);
    }

    #[test]
    fn test_insert_before_trailing_comments() {
        // A commented-out sibling block follows the real entries; the new entry
        // must land after the last real entry, NOT after the comments.
        let src = "controls:\n  apple_settings:\n    configuration_profiles:\n      - path: ../a.mobileconfig\n  # Windows — not used for this project:\n  # windows_settings:\n  #   configuration_profiles:\n";
        let out = insert_under(
            src,
            &["controls", "apple_settings", "configuration_profiles"],
            "- path: ../b.mobileconfig",
            "",
        );
        let a = out.find("../a.mobileconfig").unwrap();
        let b = out.find("../b.mobileconfig").unwrap();
        let win = out.find("# Windows").unwrap();
        assert!(a < b && b < win, "b must be after a and before the Windows comments:\n{out}");
        parses(&out);
    }

    #[test]
    fn test_insert_into_inline_empty_collection() {
        // `packages: []` must become `packages:` so block items are valid YAML.
        let src = "software:\n  packages: []\n";
        let out = insert_under(
            src,
            &["software", "packages"],
            "- url: https://x/app.pkg\n  hash_sha256: abc\n  setup_experience: true",
            "",
        );
        assert!(!out.contains("packages: []"), "inline [] must be stripped:\n{out}");
        let v = parses(&out);
        let pkgs = v["software"]["packages"].as_sequence().unwrap();
        assert_eq!(pkgs.len(), 1);
        assert_eq!(pkgs[0]["setup_experience"], serde_yaml::Value::from(true));
        assert_eq!(pkgs[0]["url"], serde_yaml::Value::from("https://x/app.pkg"));
    }

    #[test]
    fn test_insert_under_missing_leaf() {
        // controls + apple_settings exist; configuration_profiles does not.
        let src = "controls:\n  apple_settings:\n    foo: bar\n";
        let out = insert_under(
            src,
            &["controls", "apple_settings", "configuration_profiles"],
            "- paths: ../x/*.mobileconfig",
            "",
        );
        let v = parses(&out);
        assert_eq!(
            v["controls"]["apple_settings"]["configuration_profiles"][0]["paths"],
            serde_yaml::Value::from("../x/*.mobileconfig")
        );
        assert_eq!(v["controls"]["apple_settings"]["foo"], serde_yaml::Value::from("bar"));
    }

    #[test]
    fn test_insert_creates_full_chain() {
        // No controls at all — the whole nested block is created.
        let src = "policies:\n  - path: ./p.yml\n";
        let out = insert_under(
            src,
            &["controls", "scripts"],
            "- path: ../platforms/macos/scripts/setup.sh",
            "added by flint",
        );
        let v = parses(&out);
        assert_eq!(
            v["controls"]["scripts"][0]["path"],
            serde_yaml::Value::from("../platforms/macos/scripts/setup.sh")
        );
        assert!(v["policies"].is_sequence()); // untouched
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("/a/b/*.yml", "/a/b/c.yml"));
        assert!(!glob_match("/a/b/*.yml", "/a/b/sub/c.yml")); // * doesn't cross /
        assert!(glob_match("/a/**/*.yml", "/a/b/sub/c.yml")); // ** crosses
        assert!(glob_match("/a/**/*.yml", "/a/c.yml"));
        assert!(!glob_match("/a/b/*.mobileconfig", "/a/b/c.yml"));
        assert!(glob_match("/x/conf/security/*.mobileconfig", "/x/conf/security/p.mobileconfig"));
        assert!(!glob_match("/x/conf/security/*.mobileconfig", "/x/conf/top.mobileconfig"));
    }

    fn write(tmp: &TempDir, rel: &str, body: &str) {
        let p = tmp.path().join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }

    #[test]
    fn test_find_unwired() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        write(&tmp, "default.yml", "x\n");

        // A fleet wires the security/ subdir via a glob, and one software file.
        write(
            &tmp,
            "fleets/a.yml",
            "controls:\n  macos_settings:\n    custom_settings:\n      - paths: ../platforms/macos/configuration-profiles/security/*.mobileconfig\nsoftware:\n  packages:\n    - path: ../platforms/macos/software/slack.yml\n      # - path: ../platforms/macos/software/commented-out.yml\n",
        );

        // Wired:
        write(&tmp, "platforms/macos/configuration-profiles/security/fw.mobileconfig", "");
        write(&tmp, "platforms/macos/software/slack.yml", "");
        // Orphans (no glob/path covers these):
        write(&tmp, "platforms/macos/configuration-profiles/top-level.mobileconfig", "");
        write(&tmp, "platforms/macos/software/commented-out.yml", "");
        write(&tmp, "platforms/macos/scripts/setup.sh", "");
        write(&tmp, "platforms/macos/enrollment-profiles/dep.json", "");

        let report = find_unwired(tmp.path());
        let orphans = &report.artifacts;
        // Fleet files live in fleets/, so wire paths are written from there.
        assert_eq!(report.wire_base_rel, "fleets");
        let rels: Vec<&str> = orphans.iter().map(|o| o.rel.as_str()).collect();

        assert!(rels.contains(&"platforms/macos/configuration-profiles/top-level.mobileconfig"));
        assert!(rels.contains(&"platforms/macos/software/commented-out.yml")); // commented ref ≠ wired
        assert!(rels.contains(&"platforms/macos/scripts/setup.sh"));
        assert!(rels.contains(&"platforms/macos/enrollment-profiles/dep.json"));
        // The glob-covered profile and the software path are NOT orphans.
        assert!(!rels.contains(&"platforms/macos/configuration-profiles/security/fw.mobileconfig"));
        assert!(!rels.contains(&"platforms/macos/software/slack.yml"));

        // Classification spot-checks.
        let prof = orphans.iter().find(|o| o.rel.ends_with("top-level.mobileconfig")).unwrap();
        assert_eq!(prof.section, "controls.apple_settings.configuration_profiles");
        assert!(!prof.single_only);
        // wire path is relative to fleets/ (the structure), not a hardcoded ../
        assert_eq!(prof.wire, "../platforms/macos/configuration-profiles/top-level.mobileconfig");
        let dep = orphans.iter().find(|o| o.rel.ends_with("dep.json")).unwrap();
        assert_eq!(dep.section, "controls.setup_experience.apple_setup_assistant");
        assert!(dep.single_only);
    }
}
