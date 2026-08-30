//! Declarative repo-convention patterns (ADR-010 Phase 2).
//!
//! `[[patterns]]` entries in `.fleetlint.toml` encode conventions the repo
//! owns — naming schemes, token consistency, fan-out completeness — so they
//! never have to live in flint's Rust. Each pattern carries a REQUIRED
//! `why`; it is printed with every finding so the convention stays
//! auditable ("no commit, no rule", moved into config).
//!
//! Patterns run over the [`Workspace`] in the directory-lint graph pass.
//! Nine assertion kinds cover every convention rule observed in the source
//! defect study; the vocabulary only grows against evidence (two distinct
//! real-world uses).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde_yaml::Value;

use super::codes::pattern_code;
use super::config::{compile_glob, PatternConfig};
use super::error::{LintError, Severity};
use super::util::normalize_path;
use super::workspace::Workspace;

/// Run every configured pattern against the workspace.
pub(crate) fn check_patterns(
    patterns: &[PatternConfig],
    root: &Path,
    ws: &Workspace,
) -> Vec<(PathBuf, LintError)> {
    let mut findings = Vec::new();
    // Contents read at most once per file across all patterns.
    let mut cache: HashMap<PathBuf, Option<String>> = HashMap::new();
    for p in patterns {
        check_one(p, root, ws, &mut cache, &mut findings);
    }
    findings
}

fn severity_of(p: &PatternConfig) -> Severity {
    match p.severity.as_str() {
        "error" => Severity::Error,
        "info" => Severity::Info,
        _ => Severity::Warning,
    }
}

/// A finding for pattern `p` on `file`, with the `why` attached as help.
fn finding(p: &PatternConfig, file: &Path, message: String) -> (PathBuf, LintError) {
    let mut err = match severity_of(p) {
        Severity::Error => LintError::error(message, file),
        Severity::Info => LintError::info(message, file),
        Severity::Warning => LintError::warning(message, file),
    };
    err.rule_code = Some(pattern_code(&p.assert));
    err = err.with_help(format!("why: {}", p.why.trim()));
    (file.to_path_buf(), err)
}

/// The workspace files selected by the pattern's `files` glob (repo-root
/// relative), as (absolute, root-relative-string) pairs.
fn matched_files<'w>(
    p: &PatternConfig,
    root: &Path,
    ws: &'w Workspace,
) -> Vec<(&'w PathBuf, String)> {
    let pat = normalize_path(&root.join(&p.files));
    let pat_s = pat.to_string_lossy().replace('\\', "/");
    let Some(matcher) = compile_glob(&pat_s) else {
        return Vec::new();
    };
    ws.files
        .iter()
        .filter_map(|f| {
            let s = f.to_string_lossy().replace('\\', "/");
            if matcher.is_match(&s) {
                let rel = f
                    .strip_prefix(root)
                    .unwrap_or(f)
                    .to_string_lossy()
                    .replace('\\', "/");
                Some((f, rel))
            } else {
                None
            }
        })
        .collect()
}

fn read<'c>(cache: &'c mut HashMap<PathBuf, Option<String>>, f: &PathBuf) -> Option<&'c str> {
    cache
        .entry(f.clone())
        .or_insert_with(|| std::fs::read_to_string(f).ok())
        .as_deref()
}

fn check_one(
    p: &PatternConfig,
    root: &Path,
    ws: &Workspace,
    cache: &mut HashMap<PathBuf, Option<String>>,
    findings: &mut Vec<(PathBuf, LintError)>,
) {
    match p.assert.as_str() {
        "forbid-file" => {
            for (f, rel) in matched_files(p, root, ws) {
                findings.push(finding(p, f, format!("'{rel}' is forbidden by pattern")));
            }
        }
        "filename" => {
            let re = regex::Regex::new(&p.regex).expect("validated at config load");
            for (f, rel) in matched_files(p, root, ws) {
                let name = f.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                if !re.is_match(&name) {
                    findings.push(finding(
                        p,
                        f,
                        format!("'{rel}': filename does not match /{}/", p.regex),
                    ));
                }
            }
        }
        "content-must-match" | "content-must-not-match" => {
            let re = regex::Regex::new(&p.regex).expect("validated at config load");
            let want = p.assert == "content-must-match";
            for (f, rel) in matched_files(p, root, ws) {
                let Some(content) = read(cache, f) else { continue };
                if re.is_match(content) != want {
                    let verb = if want { "does not match" } else { "matches forbidden" };
                    findings.push(finding(p, f, format!("'{rel}': content {verb} /{}/", p.regex)));
                }
            }
        }
        "name-matches-filename" => {
            let key = if p.key.is_empty() { "name" } else { &p.key };
            for (f, rel) in matched_files(p, root, ws) {
                let Some(content) = read(cache, f) else { continue };
                let Ok(yaml) = serde_yaml::from_str::<Value>(content) else {
                    continue;
                };
                let Some(val) = yaml.get(key).and_then(Value::as_str) else {
                    continue;
                };
                let stem = f
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if val != stem {
                    findings.push(finding(
                        p,
                        f,
                        format!("'{rel}': `{key}: {val}` does not match filename stem '{stem}'"),
                    ));
                }
            }
        }
        "token-consistency" => {
            let seg = p.segment.expect("validated at config load");
            for (f, rel) in matched_files(p, root, ws) {
                let parts: Vec<&str> = rel.split('/').collect();
                let Some(token) = parts.get(seg) else { continue };
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                if !name.contains(&token.to_lowercase()) {
                    findings.push(finding(
                        p,
                        f,
                        format!(
                            "'{rel}': filename does not carry its path segment token '{token}'"
                        ),
                    ));
                }
            }
        }
        "must-be-referenced" => {
            // Which configs count as referencers.
            let by_matcher = if p.by.is_empty() {
                None
            } else {
                let pat = normalize_path(&root.join(&p.by));
                compile_glob(&pat.to_string_lossy().replace('\\', "/"))
            };
            let referencers: Vec<_> = ws
                .parsed
                .iter()
                .filter(|pf| {
                    by_matcher.as_ref().is_none_or(|m| {
                        m.is_match(pf.path.to_string_lossy().replace('\\', "/"))
                    })
                })
                .collect();
            // Per-referencer referenced sets (globs expanded against ws).
            let ref_sets: Vec<std::collections::HashSet<PathBuf>> = referencers
                .iter()
                .map(|pf| super::workspace::referenced_by(pf, ws))
                .collect();
            let need_all = p.quantifier == "all";
            for (f, rel) in matched_files(p, root, ws) {
                let count = ref_sets.iter().filter(|s| s.contains(f)).count();
                let ok = if need_all {
                    !ref_sets.is_empty() && count == ref_sets.len()
                } else {
                    count > 0
                };
                if !ok {
                    let msg = if need_all {
                        format!(
                            "'{rel}' is referenced by {count} of {} '{}' file(s) — pattern requires all",
                            ref_sets.len(),
                            p.by
                        )
                    } else {
                        format!("'{rel}' is referenced by nothing matching the pattern")
                    };
                    findings.push(finding(p, f, msg));
                }
            }
        }
        "unique-content-within" => {
            let mut by_content: HashMap<String, Vec<(PathBuf, String)>> = HashMap::new();
            for (f, rel) in matched_files(p, root, ws) {
                if let Some(content) = read(cache, f) {
                    by_content
                        .entry(content.to_string())
                        .or_default()
                        .push((f.clone(), rel));
                }
            }
            for group in by_content.values() {
                if group.len() < 2 {
                    continue;
                }
                for (f, rel) in group {
                    let other = group.iter().find(|(g, _)| g != f).map(|(_, r)| r.clone());
                    let mut item = finding(
                        p,
                        f,
                        format!(
                            "'{rel}' is byte-identical to '{}' within the pattern scope",
                            other.unwrap_or_default()
                        ),
                    );
                    for (g, _) in group.iter().filter(|(g, _)| g != f) {
                        item.1 = item.1.with_related(g.clone());
                    }
                    findings.push(item);
                }
            }
        }
        "required-structure" => {
            // `files` selects DIRECTORIES here: every unique parent chain of
            // the workspace file set that matches the glob.
            let pat = normalize_path(&root.join(&p.files));
            let Some(matcher) = compile_glob(&pat.to_string_lossy().replace('\\', "/")) else {
                return;
            };
            let mut dirs: Vec<&Path> = ws
                .files
                .iter()
                .flat_map(|f| f.ancestors().skip(1))
                .filter(|d| d.starts_with(root) && matcher.is_match(d.to_string_lossy().replace('\\', "/")))
                .collect();
            dirs.sort();
            dirs.dedup();
            for dir in dirs {
                for entry in &p.entries {
                    let expected = dir.join(entry);
                    let present = ws
                        .files
                        .iter()
                        .any(|f| f == &expected || f.starts_with(&expected));
                    if !present {
                        let rel = dir.strip_prefix(root).unwrap_or(dir);
                        findings.push(finding(
                            p,
                            &expected,
                            format!("'{}' is missing required entry '{entry}'", rel.display()),
                        ));
                    }
                }
            }
        }
        _ => unreachable!("validated at config load"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_reference::ParsedFile;

    fn pat(assert: &str) -> PatternConfig {
        PatternConfig {
            files: "**/*".into(),
            assert: assert.into(),
            why: "test".into(),
            ..Default::default()
        }
    }

    fn ws_of(files: Vec<PathBuf>, parsed: &[ParsedFile]) -> Workspace<'_> {
        Workspace::from_files(files, parsed)
    }

    #[test]
    fn validation_rejects_missing_why_and_unknown_assert() {
        let mut p = pat("forbid-file");
        p.why = " ".into();
        assert!(p.validate().unwrap_err().contains("why"));
        let mut p = pat("no-such-assert");
        p.why = "x".into();
        assert!(p.validate().unwrap_err().contains("unknown assert"));
        // Config-level: a [[patterns]] entry without why fails parse.
        let toml = "[[patterns]]\nfiles = \"**/*\"\nassert = \"forbid-file\"\n";
        assert!(crate::config::FleetLintConfig::parse(toml).is_err());
    }

    #[test]
    fn forbid_file_and_filename() {
        let root = Path::new("/repo");
        let files = vec![
            PathBuf::from("/repo/a/.DS_Store"),
            PathBuf::from("/repo/a/Good-Name.yml"),
        ];
        let ws = ws_of(files, &[]);
        let mut cache = HashMap::new();
        let mut out = Vec::new();

        let mut p = pat("forbid-file");
        p.files = "**/.DS_Store".into();
        check_one(&p, root, &ws, &mut cache, &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].1.help.as_deref().unwrap().contains("why: test"));

        out.clear();
        let mut p = pat("filename");
        p.files = "**/*.yml".into();
        p.regex = "^[a-z0-9.-]+$".into();
        check_one(&p, root, &ws, &mut cache, &mut out);
        assert_eq!(out.len(), 1, "{:?}", out);
        assert!(out[0].0.ends_with("Good-Name.yml"));
    }

    #[test]
    fn token_consistency_by_segment() {
        let root = Path::new("/repo");
        let files = vec![
            PathBuf::from("/repo/L2/CCS/kiosk-CCS.mobileconfig"),
            PathBuf::from("/repo/L2/CFG/kiosk-CCS.mobileconfig"),
        ];
        let ws = ws_of(files, &[]);
        let mut p = pat("token-consistency");
        p.files = "L2/*/*.mobileconfig".into();
        p.segment = Some(1);
        let mut out = Vec::new();
        check_one(&p, root, &ws, &mut HashMap::new(), &mut out);
        assert_eq!(out.len(), 1);
        assert!(out[0].0.to_string_lossy().contains("CFG"));
    }

    #[test]
    fn must_be_referenced_all_and_any() {
        let root = Path::new("/repo");
        let f1 = ParsedFile {
            path: PathBuf::from("/repo/fleets/a.yml"),
            source: String::new(),
            yaml: serde_yaml::from_str("controls:\n  scripts:\n    - path: ../base/base.sh\n")
                .unwrap(),
        };
        let f2 = ParsedFile {
            path: PathBuf::from("/repo/fleets/b.yml"),
            source: String::new(),
            yaml: serde_yaml::from_str("name: b\n").unwrap(),
        };
        let parsed = [f1, f2];
        let ws = ws_of(vec![PathBuf::from("/repo/base/base.sh")], &parsed);

        let mut p = pat("must-be-referenced");
        p.files = "base/**".into();
        p.by = "fleets/*.yml".into();
        p.quantifier = "all".into();
        let mut out = Vec::new();
        check_one(&p, root, &ws, &mut HashMap::new(), &mut out);
        assert_eq!(out.len(), 1, "b.yml doesn't reference base.sh");
        assert!(out[0].1.message.contains("1 of 2"));

        p.quantifier = "any".into();
        out.clear();
        check_one(&p, root, &ws, &mut HashMap::new(), &mut out);
        assert!(out.is_empty(), "referenced by a.yml — any is satisfied");
    }

    #[test]
    fn unique_content_and_required_structure() {
        let dir = tempfile::tempdir().unwrap();
        let root = normalize_path(dir.path());
        std::fs::create_dir_all(root.join("site/zoom/scripts")).unwrap();
        std::fs::create_dir_all(root.join("site/webex")).unwrap();
        std::fs::write(root.join("site/zoom/scripts/a.sh"), "same").unwrap();
        std::fs::write(root.join("site/webex/b.sh"), "same").unwrap();
        let files = vec![
            root.join("site/zoom/scripts/a.sh"),
            root.join("site/webex/b.sh"),
        ];
        let ws = ws_of(files, &[]);

        let mut p = pat("unique-content-within");
        p.files = "site/**/*.sh".into();
        let mut out = Vec::new();
        check_one(&p, &root, &ws, &mut HashMap::new(), &mut out);
        assert_eq!(out.len(), 2);
        assert!(!out[0].1.related.is_empty());

        let mut p = pat("required-structure");
        p.files = "site/*".into();
        p.entries = vec!["scripts".into()];
        out.clear();
        check_one(&p, &root, &ws, &mut HashMap::new(), &mut out);
        assert_eq!(out.len(), 1, "{:?}", out);
        assert!(out[0].1.message.contains("webex"));
    }
}
