//! The single fix applier shared by every flint face.
//!
//! Rules attach a [`Fix`] to their diagnostics; this module turns those fixes
//! into text edits. The CLI (`flint check --fix`, `flint paths --fix`) applies
//! them to files via [`apply_fixes_to_file`]; the LSP builds editor quick-fix
//! actions from the same [`Fix`] values (serialized through `Diagnostic.data`).
//!
//! Semantics are inherited from the original CLI applier and preserved
//! exactly: edits are applied bottom-up so earlier line numbers never shift,
//! `Replace` substitutes the first occurrence of `old` on the error's line,
//! `ReplaceLines` splices an inclusive 1-indexed line range, and the source's
//! trailing-newline state is preserved.

use crate::error::{Fix, FixSafety, LintError, LintReport};
use std::path::Path;

/// Which fixes may be applied automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyMode {
    /// Only [`FixSafety::Safe`] fixes.
    SafeOnly,
    /// [`FixSafety::Safe`] and [`FixSafety::Unsafe`] fixes
    /// (`--fix --unsafe-fixes`). [`FixSafety::Display`] is never applied.
    IncludeUnsafe,
}

impl ApplyMode {
    fn allows(self, safety: FixSafety) -> bool {
        match safety {
            FixSafety::Safe => true,
            FixSafety::Unsafe => self == ApplyMode::IncludeUnsafe,
            FixSafety::Display => false,
        }
    }
}

/// One concrete, auto-applicable edit derived from an error's fix.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Edit {
    /// Replace the first occurrence of `old` on `line` (1-indexed).
    Single {
        line: usize,
        old: String,
        new: String,
    },
    /// Replace the inclusive 1-indexed line range with `text`.
    Block {
        start: usize,
        end: usize,
        text: String,
    },
}

impl Edit {
    fn start_line(&self) -> usize {
        match self {
            Edit::Single { line, .. } => *line,
            Edit::Block { start, .. } => *start,
        }
    }
}

/// Derive the auto-applicable edit for one error, if any.
///
/// Mirrors the historical contract: a `Replace` needs the error's `line` and a
/// concrete `old` token (display-only templates have `old: None` and are
/// skipped); `Candidates` are never auto-applied; `ReplaceLines` carries its
/// own range.
fn edit_for(error: &LintError, mode: ApplyMode) -> Option<Edit> {
    match error.fix.as_ref()? {
        Fix::Replace { old, new, safety } if mode.allows(*safety) => Some(Edit::Single {
            line: error.line()?,
            old: old.clone()?,
            new: new.clone(),
        }),
        Fix::ReplaceLines {
            start_line,
            end_line,
            replacement,
            safety,
        } if mode.allows(*safety) => Some(Edit::Block {
            start: *start_line,
            end: *end_line,
            text: replacement.clone(),
        }),
        _ => None,
    }
}

/// Whether `error` carries a fix that [`apply_fixes`] would actually apply
/// under `mode`.
///
/// Derived from [`edit_for`], so a caller counting "how many findings will
/// `--fix` resolve?" cannot drift from what the applier does. Display-only
/// templates (`old: None`) and `Candidates` answer `false`.
pub fn is_applicable(error: &LintError, mode: ApplyMode) -> bool {
    edit_for(error, mode).is_some()
}

/// Apply every auto-applicable fix in `errors` to `source`.
/// Returns the new source and the number of fixes applied.
pub fn apply_fixes<'a, I>(source: &str, errors: I, mode: ApplyMode) -> (String, usize)
where
    I: IntoIterator<Item = &'a LintError>,
{
    let mut edits: Vec<Edit> = errors
        .into_iter()
        .filter_map(|e| edit_for(e, mode))
        .collect();
    if edits.is_empty() {
        return (source.to_string(), 0);
    }

    let mut lines: Vec<String> = source.lines().map(|l| l.to_string()).collect();

    // Bottom-up, so an edit (which may change the line count) never shifts
    // the line numbers of edits above it.
    edits.sort_by_key(|e| std::cmp::Reverse(e.start_line()));

    let mut applied = 0;
    for edit in &edits {
        match edit {
            Edit::Single { line, old, new } => {
                let idx = line - 1;
                if idx >= lines.len() {
                    continue;
                }
                if let Some(pos) = lines[idx].find(old.as_str()) {
                    let cur = &lines[idx];
                    let mut replaced =
                        String::with_capacity(cur.len() - old.len() + new.len());
                    replaced.push_str(&cur[..pos]);
                    replaced.push_str(new);
                    replaced.push_str(&cur[pos + old.len()..]);
                    lines[idx] = replaced;
                    applied += 1;
                }
            }
            Edit::Block { start, end, text } => {
                let (s, e) = (start - 1, end - 1); // 0-indexed inclusive
                if s > e || e >= lines.len() {
                    continue;
                }
                let replacement: Vec<String> = text.split('\n').map(|l| l.to_string()).collect();
                lines.splice(s..=e, replacement);
                applied += 1;
            }
        }
    }

    let mut output = lines.join("\n");
    if source.ends_with('\n') {
        output.push('\n');
    }
    (output, applied)
}

/// Read `path`, apply the report's fixes, and write the file back if anything
/// changed. Returns the number of fixes applied.
pub fn apply_fixes_to_file(
    path: &Path,
    report: &LintReport,
    mode: ApplyMode,
) -> std::io::Result<usize> {
    let source = std::fs::read_to_string(path)?;
    let all = report
        .errors
        .iter()
        .chain(report.warnings.iter())
        .chain(report.infos.iter());
    let (fixed, applied) = apply_fixes(&source, all, mode);
    if applied > 0 {
        std::fs::write(path, fixed)?;
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn err_with(line: usize, fix: Fix) -> LintError {
        LintError::error("test", PathBuf::from("test.yml"))
            .with_location(line, 1)
            .with_fix(fix)
    }

    fn replace(old: &str, new: &str, safety: FixSafety) -> Fix {
        Fix::Replace {
            old: Some(old.to_string()),
            new: new.to_string(),
            safety,
        }
    }

    #[test]
    fn safe_replace_applies() {
        let src = "a: one\nb: two\n";
        let e = err_with(2, replace("two", "2", FixSafety::Safe));
        let (out, n) = apply_fixes(src, [&e], ApplyMode::SafeOnly);
        assert_eq!(n, 1);
        assert_eq!(out, "a: one\nb: 2\n");
    }

    #[test]
    fn unsafe_gated_by_mode() {
        let src = "a: one\n";
        let e = err_with(1, replace("one", "1", FixSafety::Unsafe));
        let (out, n) = apply_fixes(src, [&e], ApplyMode::SafeOnly);
        assert_eq!((out.as_str(), n), (src, 0));
        let (out, n) = apply_fixes(src, [&e], ApplyMode::IncludeUnsafe);
        assert_eq!(n, 1);
        assert_eq!(out, "a: 1\n");
    }

    #[test]
    fn display_and_candidates_never_apply() {
        let src = "a: one\n";
        let display = err_with(
            1,
            Fix::Replace {
                old: None,
                new: "a: example".into(),
                safety: FixSafety::Display,
            },
        );
        let cand = err_with(
            1,
            Fix::Candidates {
                old: "one".into(),
                options: vec!["1".into(), "uno".into()],
            },
        );
        let (out, n) = apply_fixes(src, [&display, &cand], ApplyMode::IncludeUnsafe);
        assert_eq!((out.as_str(), n), (src, 0));
    }

    #[test]
    fn block_splice_changes_line_count() {
        let src = "head\nold\ntail\n";
        let e = err_with(
            2,
            Fix::ReplaceLines {
                start_line: 2,
                end_line: 2,
                replacement: "new1\nnew2".into(),
                safety: FixSafety::Unsafe,
            },
        );
        let (out, n) = apply_fixes(src, [&e], ApplyMode::IncludeUnsafe);
        assert_eq!(n, 1);
        assert_eq!(out, "head\nnew1\nnew2\ntail\n");
    }

    #[test]
    fn bottom_up_ordering_keeps_lines_stable() {
        // A block edit ABOVE a single edit must not shift the single edit's
        // target: bottom-up application guarantees it.
        let src = "one\ntwo\nthree\nfour\n";
        let block = err_with(
            1,
            Fix::ReplaceLines {
                start_line: 1,
                end_line: 2,
                replacement: "merged".into(),
                safety: FixSafety::Safe,
            },
        );
        let single = err_with(4, replace("four", "4", FixSafety::Safe));
        let (out, n) = apply_fixes(src, [&block, &single], ApplyMode::SafeOnly);
        assert_eq!(n, 2);
        assert_eq!(out, "merged\nthree\n4\n");
    }

    #[test]
    fn trailing_newline_preserved_both_ways() {
        let e = err_with(1, replace("x", "y", FixSafety::Safe));
        let (out, _) = apply_fixes("x: 1", [&e], ApplyMode::SafeOnly);
        assert!(!out.ends_with('\n'));
        let (out, _) = apply_fixes("x: 1\n", [&e], ApplyMode::SafeOnly);
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn missing_old_on_line_is_skipped() {
        let src = "a: one\n";
        let e = err_with(1, replace("absent", "x", FixSafety::Safe));
        let (out, n) = apply_fixes(src, [&e], ApplyMode::SafeOnly);
        assert_eq!((out.as_str(), n), (src, 0));
    }

    #[test]
    fn replace_without_line_is_skipped() {
        let e = LintError::error("no location", PathBuf::from("t.yml"))
            .with_fix(replace("a", "b", FixSafety::Safe));
        let (out, n) = apply_fixes("a\n", [&e], ApplyMode::SafeOnly);
        assert_eq!((out.as_str(), n), ("a\n", 0));
    }
}
