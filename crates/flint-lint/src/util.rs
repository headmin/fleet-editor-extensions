//! Small shared helpers used across rule modules — the crate-internal home
//! for code that previously leaked between sibling rules via `pub(crate)`
//! (levenshtein lived in `structural`, path normalization in
//! `self_reference`).

use std::path::{Component, Path, PathBuf};

/// Classic dynamic-programming Levenshtein edit distance. Used for
/// "did you mean …" suggestions (unknown keys, unknown labels).
pub(crate) fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let (m, n) = (a_chars.len(), b_chars.len());

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = usize::from(a_chars[i - 1] != b_chars[j - 1]);
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Collapse `.` and `..` components without touching the filesystem.
/// A leading `..` with nothing to pop is preserved (not silently dropped),
/// so relative inputs keep their meaning.
pub(crate) fn normalize_path(path: &Path) -> PathBuf {
    let mut parts: Vec<Component> = Vec::new();
    for component in path.components() {
        match component {
            Component::CurDir => {} // skip `.`
            Component::ParentDir => {
                // Pop the last normal component if possible.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levenshtein_basics() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("", "abc"), 3);
        assert_eq!(levenshtein_distance("abc", ""), 3);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("policis", "policies"), 1);
    }

    #[test]
    fn normalize_collapses_dots() {
        assert_eq!(
            normalize_path(Path::new("a/./b/../c")),
            PathBuf::from("a/c")
        );
        // Leading `..` preserved.
        assert_eq!(
            normalize_path(Path::new("../x/./y")),
            PathBuf::from("../x/y")
        );
    }
}
