//! Workspace-level validation for Fleet GitOps YAML files.
//!
//! Provides cross-file validation including:
//! - Path reference validation (checking that referenced files exist)
//! - Go-to-definition for path references

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, DocumentLink, GotoDefinitionResponse, Location, Position,
    Range, TextEdit, Url, WorkspaceEdit,
};

/// Check path references in a document and return diagnostics for malformed
/// path *syntax* (shell prefixes, absolute paths, deep traversal).
///
/// Target existence is intentionally NOT checked here — that is owned by the
/// `path-exists` lint rule (run via `lint_content`), which resolves paths
/// relative to the referring file (correct for `../` refs) and additionally
/// suggests the moved location. Checking existence here too would
/// double-report and, because this used to resolve against the workspace root,
/// false-positive on the common `fleets/x.yml -> ../platforms/...` layout.
pub fn validate_path_references(
    source: &str,
    _file_path: &Path,
    _workspace_root: Option<&Path>,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();

    let lines: Vec<&str> = source.lines().collect();

    for (line_idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim().trim_start_matches('-').trim();

        // Check for path: references
        if trimmed.starts_with("path:") {
            if let Some(path_value) = extract_path_value(trimmed) {
                // Calculate character positions for the path value
                let path_start = line.find(&path_value).unwrap_or(0) as u32;
                let path_end = path_start + path_value.len() as u32;
                let range = Range {
                    start: Position {
                        line: line_idx as u32,
                        character: path_start,
                    },
                    end: Position {
                        line: line_idx as u32,
                        character: path_end,
                    },
                };

                // Check for malformed path syntax. Existence is handled by the
                // path-exists lint rule (see the function doc comment).
                if let Some(msg) = check_malformed_path(&path_value) {
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::ERROR),
                        source: Some("fleet-lint".to_string()),
                        message: msg,
                        ..Default::default()
                    });
                }
            }
        }
    }

    diagnostics
}

/// Extract path value from a line like "path: lib/policies.yml"
fn extract_path_value(line: &str) -> Option<String> {
    let value = line.strip_prefix("path:")?.trim();
    // Remove quotes if present
    let value = value.trim_matches('"').trim_matches('\'');
    if value.is_empty() {
        None
    } else {
        Some(value.to_string())
    }
}

/// Check if a path value is malformed and return an error message if so.
fn check_malformed_path(path: &str) -> Option<String> {
    // Shell source command prefix: `. ./script.sh` or `source ./script.sh`
    if path.starts_with(". ") {
        let suggested = path
            .strip_prefix(". ")
            .expect("starts_with checked above")
            .trim();
        return Some(format!(
            "Path starts with `. ` (shell source command). Did you mean `{}`?",
            suggested
        ));
    }
    if path.starts_with("source ") {
        let suggested = path
            .strip_prefix("source ")
            .expect("starts_with checked above")
            .trim();
        return Some(format!(
            "Path starts with `source ` (shell command). Did you mean `{}`?",
            suggested
        ));
    }

    // Shell execution prefixes: `bash ./script.sh`, `sh ./script.sh`, etc.
    for prefix in &["bash ", "sh ", "zsh ", "/bin/bash ", "/bin/sh "] {
        if path.starts_with(prefix) {
            let suggested = path
                .strip_prefix(prefix)
                .expect("starts_with checked above")
                .trim();
            return Some(format!(
                "Path starts with `{}` (shell command). Use the path only: `{}`",
                prefix.trim(),
                suggested
            ));
        }
    }

    // Absolute paths (should be relative)
    if path.starts_with('/') {
        return Some(
            "Path is absolute. Use a relative path from the gitops repo root.".to_string(),
        );
    }

    // Path traversal outside repo — only flag ../../ (two levels up) or deeper,
    // as ../ is normal for fleet YAML referencing sibling directories
    // (e.g., fleets/workstations.yml -> ../platforms/macos/policies/*.yml)
    if path.starts_with("../../") || path.contains("/../../") {
        return Some(
            "Path traverses multiple levels up. Verify it stays within the repo root.".to_string(),
        );
    }

    None
}

/// Get go-to-definition location for path references.
pub fn get_path_definition(
    source: &str,
    position: Position,
    file_path: &Path,
    workspace_root: Option<&Path>,
) -> Option<GotoDefinitionResponse> {
    let lines: Vec<&str> = source.lines().collect();
    let line = lines.get(position.line as usize)?;
    let trimmed = line.trim().trim_start_matches('-').trim();

    // Check if cursor is on a path: reference
    if !trimmed.starts_with("path:") {
        return None;
    }

    let path_value = extract_path_value(trimmed)?;

    // Check if cursor is actually on the path value (not the key)
    let value_start = line.find(&path_value)? as u32;
    let value_end = value_start + path_value.len() as u32;

    if position.character < value_start || position.character > value_end {
        return None;
    }

    // Resolve the path
    let base_dir = if let Some(root) = workspace_root {
        root.to_path_buf()
    } else {
        file_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };

    let resolved_path = base_dir.join(&path_value);

    if !resolved_path.exists() {
        return None;
    }

    // Convert to URI
    let uri = Url::from_file_path(&resolved_path).ok()?;

    Some(GotoDefinitionResponse::Scalar(Location {
        uri,
        range: Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 0,
            },
        },
    }))
}

/// Find all files in a workspace that are Fleet GitOps YAML files.
pub fn find_fleet_files(workspace_root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();

    if let Ok(entries) = std::fs::read_dir(workspace_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && is_fleet_yaml(&path) {
                files.push(path);
            } else if path.is_dir() {
                // Recursively scan subdirectories
                files.extend(find_fleet_files(&path));
            }
        }
    }

    files
}

/// Check if a file is likely a Fleet GitOps YAML file.
fn is_fleet_yaml(path: &Path) -> bool {
    if let Some(ext) = path.extension() {
        if ext != "yml" && ext != "yaml" {
            return false;
        }

        // Check for common Fleet file patterns
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            // Common Fleet GitOps file names
            if name == "default.yml"
                || name == "team.yml"
                || name.contains("policies")
                || name.contains("queries")
                || name.contains("labels")
            {
                return true;
            }
        }

        // Check if it's in a known Fleet directory
        if let Some(parent) = path.parent() {
            let parent_name = parent.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if matches!(
                parent_name,
                "fleets" | "teams" | "lib" | "labels" | "platforms"
            ) {
                return true;
            }

            // Check grandparent for nested directories (fleets/*, platforms/*)
            if let Some(grandparent) = parent.parent() {
                let grandparent_name = grandparent
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                if matches!(grandparent_name, "fleets" | "teams" | "platforms") {
                    return true;
                }

                // Check great-grandparent for platforms/*/labels/, platforms/*/lib/
                if grandparent_name == "labels" || grandparent_name == "lib" {
                    if let Some(ggp) = grandparent.parent() {
                        if let Some(ggp_parent) = ggp.parent() {
                            let ggp_name = ggp_parent
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("");
                            if ggp_name == "platforms" {
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // Fall back to checking file content (first few lines)
        if let Ok(content) = std::fs::read_to_string(path) {
            let first_lines: String = content.lines().take(10).collect::<Vec<_>>().join("\n");
            return first_lines.contains("policies:")
                || first_lines.contains("queries:")
                || first_lines.contains("reports:")
                || first_lines.contains("labels:")
                || first_lines.contains("agent_options:")
                || first_lines.contains("controls:");
        }
    }

    false
}

/// Generate document links for all path: references in a document.
///
/// Makes `path:` values clickable in the editor, navigating to the referenced file.
pub fn document_links(
    source: &str,
    file_path: &Path,
    workspace_root: Option<&Path>,
) -> Vec<DocumentLink> {
    let refs = extract_path_references(source, file_path);
    let lines: Vec<&str> = source.lines().collect();

    refs.into_iter()
        .filter_map(|path_ref| {
            let line = lines.get(path_ref.line)?;
            let path_start = line.find(&path_ref.path_value)? as u32;
            let path_end = path_start + path_ref.path_value.len() as u32;

            // Resolve against workspace root if available, otherwise file parent
            let base_dir =
                workspace_root.unwrap_or_else(|| file_path.parent().unwrap_or(Path::new(".")));
            let resolved = base_dir.join(&path_ref.path_value);
            let target = Url::from_file_path(&resolved).ok()?;

            Some(DocumentLink {
                range: Range {
                    start: Position {
                        line: path_ref.line as u32,
                        character: path_start,
                    },
                    end: Position {
                        line: path_ref.line as u32,
                        character: path_end,
                    },
                },
                target: Some(target),
                tooltip: Some(format!("Open {}", path_ref.path_value)),
                data: None,
            })
        })
        .collect()
}

/// PathReference represents a reference from one file to another.
#[derive(Debug, Clone)]
pub struct PathReference {
    /// Source file containing the reference
    pub source_file: PathBuf,
    /// Line number in source file (0-indexed)
    pub line: usize,
    /// The path value as written in the file
    pub path_value: String,
    /// Resolved absolute path (if resolvable)
    pub resolved_path: Option<PathBuf>,
}

/// Extract all path references from a document.
pub fn extract_path_references(source: &str, file_path: &Path) -> Vec<PathReference> {
    let mut refs = Vec::new();
    let base_dir = file_path.parent().unwrap_or(Path::new("."));

    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim().trim_start_matches('-').trim();

        if trimmed.starts_with("path:") {
            if let Some(path_value) = extract_path_value(trimmed) {
                let resolved = base_dir.join(&path_value);
                refs.push(PathReference {
                    source_file: file_path.to_path_buf(),
                    line: line_idx,
                    path_value: path_value.clone(),
                    resolved_path: if resolved.exists() {
                        Some(resolved)
                    } else {
                        None
                    },
                });
            }
        }
    }

    refs
}

/// Build a workspace edit that fixes *every* reference to a moved file.
///
/// Given one broken reference (`from_file` + the old/new path values for it),
/// scans all Fleet YAML files in the workspace for references that resolve to
/// the same missing target, and computes each one's own corrected relative
/// path to the moved file. This is what turns "fix this one line" into the
/// one-shot repair a folder reorg actually needs.
///
/// `doc_for` supplies in-editor (possibly unsaved) content for a path; the
/// builder falls back to reading from disk. Returns `(edit, count, basename)`
/// only when 2+ references are affected — a lone reference is already covered
/// by the per-line quick-fix.
pub fn fix_all_moved_references(
    workspace_root: &Path,
    from_file: &Path,
    old_value: &str,
    new_value: &str,
    doc_for: &dyn Fn(&Path) -> Option<String>,
) -> Option<(WorkspaceEdit, usize, String)> {
    let from_dir = from_file.parent()?;
    // Absolute-normalized identity of the (missing) target everyone points at.
    let old_abs = normalize_abs(from_dir, old_value)?;
    // The moved file's real location (it exists, so canonicalize).
    let new_abs = from_dir.join(new_value).canonicalize().ok()?;
    let basename = Path::new(new_value).file_name()?.to_str()?.to_string();

    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    let mut count = 0usize;

    for file in find_fleet_files(workspace_root) {
        let content = match doc_for(&file).or_else(|| std::fs::read_to_string(&file).ok()) {
            Some(c) => c,
            None => continue,
        };
        let file_dir = match file.parent() {
            Some(d) => d,
            None => continue,
        };
        let lines: Vec<&str> = content.lines().collect();

        for r in extract_path_references(&content, &file) {
            // Does this reference point at the same missing target?
            match normalize_abs(file_dir, &r.path_value) {
                Some(p) if p == old_abs => {}
                _ => continue,
            }
            // This referrer's own corrected relative path to the moved file.
            let new_rel = match relative_path(file_dir, &new_abs) {
                Some(s) => s,
                None => continue,
            };
            if new_rel == r.path_value {
                continue;
            }
            let line = match lines.get(r.line) {
                Some(l) => *l,
                None => continue,
            };
            let start = match line.find(&r.path_value) {
                Some(s) => s as u32,
                None => continue,
            };
            let url = match Url::from_file_path(&file) {
                Ok(u) => u,
                Err(_) => continue,
            };
            changes.entry(url).or_default().push(TextEdit {
                range: Range {
                    start: Position {
                        line: r.line as u32,
                        character: start,
                    },
                    end: Position {
                        line: r.line as u32,
                        character: start + r.path_value.len() as u32,
                    },
                },
                new_text: new_rel,
            });
            count += 1;
        }
    }

    if count < 2 {
        return None;
    }

    Some((
        WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        },
        count,
        basename,
    ))
}

/// Canonicalize `dir` (which exists) then join a possibly-missing relative
/// `value` and collapse `.`/`..` — yielding a stable absolute identity for a
/// target even when the target file does not exist.
fn normalize_abs(dir: &Path, value: &str) -> Option<PathBuf> {
    let base = dir.canonicalize().ok()?;
    Some(normalize_path(&base.join(value)))
}

/// Collapse `.` and `..` components without touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if let Some(Component::Normal(_)) = parts.last() {
                    parts.pop();
                } else {
                    parts.push(component);
                }
            }
            _ => parts.push(component),
        }
    }
    parts.iter().collect()
}

/// POSIX-style relative path from `from_dir` to `to_file`, climbing with `..`.
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

/// Validate label references in `labels_include_any`, `labels_exclude_any`,
/// and `labels_include_all` against the workspace's known label names —
/// engine-backed: builds a [`RepoIndex`] and runs the same `label-reference`
/// check `flint check` uses, so the LSP inherits built-in-label awareness and
/// the engine's "did you mean" suggestions. (Replaces a hand-rolled text
/// scanner and a third fuzzy-matcher implementation.)
pub fn validate_label_references(
    source: &str,
    file_path: &std::path::Path,
    known_labels: &[String],
) -> Vec<Diagnostic> {
    use flint_lint::cross_reference::{check_label_references, RepoIndex};

    let Ok(yaml) = serde_yaml::from_str::<serde_yaml::Value>(source) else {
        // Mid-edit unparseable documents produce no label diagnostics; the
        // main lint pass reports the syntax error.
        return Vec::new();
    };
    let index = RepoIndex::from_label_names(known_labels);
    check_label_references(&index, file_path, source, &yaml)
        .iter()
        .map(|e| super::diagnostics::lint_error_to_diagnostic(e, source))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn label_validation_flags_unknown_with_suggestion() {
        let known = vec!["Apple Silicon macOS hosts".to_string()];
        let source = "controls:\n  macos_settings:\n    custom_settings:\n      - path: ./a.mobileconfig\n        labels_include_any:\n          - Apple Silcon macOS hosts\n";
        let diags =
            validate_label_references(source, std::path::Path::new("fleets/t.yml"), &known);
        assert_eq!(diags.len(), 1, "got: {diags:?}");
        // Engine-backed: the diagnostic carries the registry code.
        assert_eq!(
            diags[0].code,
            Some(tower_lsp::lsp_types::NumberOrString::String(
                "label-reference".to_string()
            ))
        );
        assert!(diags[0].message.contains("Apple Silcon macOS hosts"));
        assert!(
            diags[0].message.contains("Apple Silicon macOS hosts"),
            "should suggest the close match: {}",
            diags[0].message
        );
    }

    #[test]
    fn label_validation_accepts_known_and_builtin() {
        let known = vec!["Workstations".to_string()];
        let source = "controls:\n  macos_settings:\n    custom_settings:\n      - path: ./a.mobileconfig\n        labels_include_any:\n          - Workstations\n          - macOS\n";
        let diags =
            validate_label_references(source, std::path::Path::new("fleets/t.yml"), &known);
        // "macOS" is a Fleet built-in — the old text scanner flagged it
        // (false positive); the engine check knows better.
        assert!(diags.is_empty(), "got: {diags:?}");
    }

    #[test]
    fn label_validation_silent_on_unparseable_source() {
        let known = vec!["X".to_string()];
        let diags = validate_label_references(
            "labels_include_any: [unclosed",
            std::path::Path::new("t.yml"),
            &known,
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn test_extract_path_value() {
        assert_eq!(
            extract_path_value("path: lib/policies.yml"),
            Some("lib/policies.yml".to_string())
        );
        assert_eq!(
            extract_path_value("path: \"lib/policies.yml\""),
            Some("lib/policies.yml".to_string())
        );
        assert_eq!(extract_path_value("path:"), None);
        assert_eq!(extract_path_value("name: test"), None);
    }

    #[test]
    fn test_validate_path_references_ignores_existence() {
        // Existence is the `path-exists` lint rule's job now — this function
        // only flags malformed syntax. A missing file with valid syntax must
        // produce no diagnostic here (regression: it used to, and resolved
        // against the workspace root, which false-positived on `../` refs).
        let temp_dir = TempDir::new().unwrap();
        let source = r#"policies:
  - path: lib/policies.yml
  - path: lib/missing.yml
"#;
        let main_file = temp_dir.path().join("default.yml");
        fs::write(&main_file, source).unwrap();

        let diagnostics = validate_path_references(source, &main_file, Some(temp_dir.path()));
        assert!(
            diagnostics.is_empty(),
            "existence is no longer checked here: {diagnostics:?}"
        );
    }

    #[test]
    fn test_check_malformed_path() {
        // Shell source prefix
        let msg = check_malformed_path(". ./lib/scripts/uninstall.sh");
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("./lib/scripts/uninstall.sh"));

        // `source` prefix
        let msg = check_malformed_path("source ./lib/scripts/uninstall.sh");
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("./lib/scripts/uninstall.sh"));

        // Shell interpreter prefix
        let msg = check_malformed_path("bash ./lib/scripts/uninstall.sh");
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("./lib/scripts/uninstall.sh"));

        // Absolute path
        let msg = check_malformed_path("/usr/local/bin/script.sh");
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("absolute"));

        // Path traversal
        let msg = check_malformed_path("../../etc/passwd");
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("traverses"));

        // Valid relative path — no error
        assert!(check_malformed_path("lib/scripts/uninstall.sh").is_none());
        assert!(check_malformed_path("./lib/scripts/uninstall.sh").is_none());
    }

    #[test]
    fn test_malformed_path_diagnostic() {
        let source = r#"controls:
  scripts:
    - path: . ./lib/macos/scripts/_uninstall-santa.sh
"#;
        let diagnostics = validate_path_references(source, Path::new("/fake/team.yml"), None);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("shell source command"));
        assert!(diagnostics[0]
            .message
            .contains("./lib/macos/scripts/_uninstall-santa.sh"));
    }

    #[test]
    fn test_extract_path_references() {
        let source = r#"policies:
  - path: lib/policies.yml
  - name: Local Policy
    query: SELECT 1
  - path: lib/more-policies.yml
"#;

        let refs = extract_path_references(source, Path::new("/fake/default.yml"));

        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].path_value, "lib/policies.yml");
        assert_eq!(refs[0].line, 1);
        assert_eq!(refs[1].path_value, "lib/more-policies.yml");
        assert_eq!(refs[1].line, 4);
    }

    #[test]
    fn test_fix_all_moved_references() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();

        // The moved file at its new home.
        let new_home = tmp.path().join("platforms/macos/site/software");
        fs::create_dir_all(&new_home).unwrap();
        fs::write(new_home.join("swiftdialog.yml"), "name: swiftdialog\n").unwrap();

        // Two fleet files still reference the old location.
        let fleets = tmp.path().join("fleets");
        fs::create_dir_all(&fleets).unwrap();
        let body = "software:\n  packages:\n    - path: ../platforms/macos/software/swiftdialog.yml\n";
        fs::write(fleets.join("a.yml"), body).unwrap();
        fs::write(fleets.join("b.yml"), body).unwrap();

        let from_file = fleets.join("a.yml");
        let doc_for = |_: &Path| None; // force disk reads

        let (edit, count, basename) = fix_all_moved_references(
            tmp.path(),
            &from_file,
            "../platforms/macos/software/swiftdialog.yml",
            "../platforms/macos/site/software/swiftdialog.yml",
            &doc_for,
        )
        .expect("should find 2+ references to the moved file");

        assert_eq!(count, 2);
        assert_eq!(basename, "swiftdialog.yml");
        let changes = edit.changes.unwrap();
        assert_eq!(changes.len(), 2, "both fleet files should be edited");
        for (_url, edits) in changes {
            assert_eq!(edits.len(), 1);
            assert_eq!(
                edits[0].new_text,
                "../platforms/macos/site/software/swiftdialog.yml"
            );
        }
    }

    #[test]
    fn test_fix_all_single_reference_returns_none() {
        // A lone reference is handled by the per-line quick-fix, so the
        // workspace-wide action must not appear.
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join(".git")).unwrap();
        let new_home = tmp.path().join("platforms/macos/site/software");
        fs::create_dir_all(&new_home).unwrap();
        fs::write(new_home.join("only.yml"), "x\n").unwrap();
        let fleets = tmp.path().join("fleets");
        fs::create_dir_all(&fleets).unwrap();
        fs::write(
            fleets.join("a.yml"),
            "software:\n  packages:\n    - path: ../platforms/macos/software/only.yml\n",
        )
        .unwrap();

        let result = fix_all_moved_references(
            tmp.path(),
            &fleets.join("a.yml"),
            "../platforms/macos/software/only.yml",
            "../platforms/macos/site/software/only.yml",
            &(|_: &Path| None),
        );
        assert!(result.is_none(), "single reference must not yield fix-all");
    }
}
