use colored::*;
use similar::{ChangeTag, TextDiff};
use std::fmt;

/// Represents a diff between two versions of a file
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub old_content: String,
    pub new_content: String,
    pub additions: usize,
    pub deletions: usize,
}

impl FileDiff {
    pub fn new(path: String, old_content: String, new_content: String) -> Self {
        let diff = TextDiff::from_lines(&old_content, &new_content);
        let mut additions = 0;
        let mut deletions = 0;

        for change in diff.iter_all_changes() {
            match change.tag() {
                ChangeTag::Insert => additions += 1,
                ChangeTag::Delete => deletions += 1,
                ChangeTag::Equal => {}
            }
        }

        Self {
            path,
            old_content,
            new_content,
            additions,
            deletions,
        }
    }

    /// Generate a unified diff output
    pub fn unified_diff(&self) -> String {
        let diff = TextDiff::from_lines(&self.old_content, &self.new_content);
        let mut output = String::new();

        // Header
        output.push_str(&format!("--- {}\n", self.path.dimmed()));
        output.push_str(&format!("+++ {}\n", self.path.dimmed()));

        // Generate unified diff using iter_all_changes
        let mut in_hunk = false;
        let mut hunk_lines: Vec<String> = Vec::new();

        for (idx, change) in diff.iter_all_changes().enumerate() {
            let line = format!("{}", change);
            let formatted = match change.tag() {
                ChangeTag::Delete => {
                    if !in_hunk {
                        in_hunk = true;
                        hunk_lines.clear();
                    }
                    format!("-{}", line).red().to_string()
                }
                ChangeTag::Insert => {
                    if !in_hunk {
                        in_hunk = true;
                        hunk_lines.clear();
                    }
                    format!("+{}", line).green().to_string()
                }
                ChangeTag::Equal => {
                    if in_hunk && hunk_lines.len() > 0 {
                        // End of hunk, print accumulated lines
                        for hunk_line in &hunk_lines {
                            output.push_str(hunk_line);
                        }
                        hunk_lines.clear();
                        in_hunk = false;
                    }
                    format!(" {}", line)
                }
            };

            if in_hunk {
                hunk_lines.push(formatted);
            } else {
                output.push_str(&formatted);
            }
        }

        // Print any remaining hunk lines
        for hunk_line in &hunk_lines {
            output.push_str(hunk_line);
        }

        output
    }

    /// Generate a side-by-side diff
    pub fn side_by_side(&self, width: usize) -> String {
        let diff = TextDiff::from_lines(&self.old_content, &self.new_content);
        let mut output = String::new();
        let col_width = width / 2 - 2;

        output.push_str(&format!(
            "{:<width$} | {}\n",
            "OLD".bold(),
            "NEW".bold(),
            width = col_width
        ));
        output.push_str(&format!("{}\n", "=".repeat(width)));

        for (idx, group) in diff.grouped_ops(0).iter().enumerate() {
            if idx > 0 {
                output.push_str(&format!("{}\n", "-".repeat(width).dimmed()));
            }

            for op in group {
                for change in diff.iter_changes(op) {
                    let line = change.to_string().trim_end().to_string();
                    let truncated = if line.len() > col_width {
                        format!("{}...", &line[..col_width - 3])
                    } else {
                        format!("{:<width$}", line, width = col_width)
                    };

                    match change.tag() {
                        ChangeTag::Delete => {
                            output.push_str(&format!("{} | \n", truncated.red()));
                        }
                        ChangeTag::Insert => {
                            output.push_str(&format!("{:<width$} | {}\n", "", truncated.green(), width = col_width));
                        }
                        ChangeTag::Equal => {
                            output.push_str(&format!("{} | {}\n", truncated.dimmed(), truncated.dimmed()));
                        }
                    }
                }
            }
        }

        output
    }

    /// Generate a compact summary
    pub fn summary(&self) -> String {
        let total_changes = self.additions + self.deletions;
        if total_changes == 0 {
            format!("{} (no changes)", self.path.dimmed())
        } else {
            format!(
                "{} {} {}",
                self.path.bold(),
                format!("+{}", self.additions).green(),
                format!("-{}", self.deletions).red()
            )
        }
    }
}

/// Collection of file diffs
#[derive(Debug, Default)]
pub struct DiffSet {
    pub diffs: Vec<FileDiff>,
}

impl DiffSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, diff: FileDiff) {
        self.diffs.push(diff);
    }

    pub fn total_additions(&self) -> usize {
        self.diffs.iter().map(|d| d.additions).sum()
    }

    pub fn total_deletions(&self) -> usize {
        self.diffs.iter().map(|d| d.deletions).sum()
    }

    pub fn total_files(&self) -> usize {
        self.diffs.len()
    }

    /// Print a summary of all diffs
    pub fn print_summary(&self) {
        if self.diffs.is_empty() {
            println!("{}", "No changes".dimmed());
            return;
        }

        println!("\n{}", "Files changed:".bold());
        for diff in &self.diffs {
            println!("  {}", diff.summary());
        }

        println!(
            "\n{} file(s) changed, {} insertions(+), {} deletions(-)",
            self.total_files().to_string().bold(),
            self.total_additions().to_string().green(),
            self.total_deletions().to_string().red()
        );
    }

    /// Print full unified diffs
    pub fn print_unified(&self) {
        for (idx, diff) in self.diffs.iter().enumerate() {
            if idx > 0 {
                println!("\n{}\n", "=".repeat(80).dimmed());
            }
            println!("{}", diff.unified_diff());
        }
    }

    /// Print side-by-side diffs
    pub fn print_side_by_side(&self, width: usize) {
        for (idx, diff) in self.diffs.iter().enumerate() {
            if idx > 0 {
                println!("\n{}\n", "=".repeat(width).dimmed());
            }
            println!("\n{}\n", diff.path.bold().underline());
            println!("{}", diff.side_by_side(width));
        }
    }
}

impl fmt::Display for DiffSet {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} file(s), +{} -{}",
            self.total_files(),
            self.total_additions(),
            self.total_deletions()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_diff() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nline2 modified\nline3\nline4\n";

        let diff = FileDiff::new("test.yml".to_string(), old.to_string(), new.to_string());

        assert!(diff.additions > 0);
        assert!(diff.deletions > 0);
    }

    #[test]
    fn test_diff_set() {
        let mut set = DiffSet::new();

        set.add(FileDiff::new(
            "file1.yml".to_string(),
            "old1\n".to_string(),
            "new1\n".to_string(),
        ));

        set.add(FileDiff::new(
            "file2.yml".to_string(),
            "old2\n".to_string(),
            "new2\n".to_string(),
        ));

        assert_eq!(set.total_files(), 2);
        assert!(set.total_additions() > 0);
    }

    #[test]
    fn test_no_changes() {
        let content = "unchanged\n";
        let diff = FileDiff::new("test.yml".to_string(), content.to_string(), content.to_string());

        assert_eq!(diff.additions, 0);
        assert_eq!(diff.deletions, 0);
    }
}
