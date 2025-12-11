use annotate_snippets::{Level, Renderer, Snippet};
use colored::*;
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Severity::Error => write!(f, "{}", "error".red().bold()),
            Severity::Warning => write!(f, "{}", "warning".yellow().bold()),
            Severity::Info => write!(f, "{}", "info".blue().bold()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LintError {
    pub severity: Severity,
    pub message: String,
    pub file: PathBuf,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub context: Option<String>,
    pub help: Option<String>,
    pub suggestion: Option<String>,
}

impl LintError {
    pub fn error(message: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self {
            severity: Severity::Error,
            message: message.into(),
            file: file.into(),
            line: None,
            column: None,
            context: None,
            help: None,
            suggestion: None,
        }
    }

    pub fn warning(message: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self {
            severity: Severity::Warning,
            message: message.into(),
            file: file.into(),
            line: None,
            column: None,
            context: None,
            help: None,
            suggestion: None,
        }
    }

    pub fn info(message: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self {
            severity: Severity::Info,
            message: message.into(),
            file: file.into(),
            line: None,
            column: None,
            context: None,
            help: None,
            suggestion: None,
        }
    }

    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        self.context = Some(context.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    /// Format error with rich annotations
    pub fn format(&self, source: Option<&str>) -> String {
        let mut output = String::new();

        // Header: severity and message
        output.push_str(&format!(
            "{}: {}\n",
            self.severity,
            self.message.bold()
        ));

        // Location
        if let Some(line) = self.line {
            let location = if let Some(col) = self.column {
                format!("{}:{}:{}", self.file.display(), line, col)
            } else {
                format!("{}:{}", self.file.display(), line)
            };
            output.push_str(&format!("  {} {}\n", "-->".blue().bold(), location));
        } else {
            output.push_str(&format!("  {} {}\n", "-->".blue().bold(), self.file.display()));
        }

        // Source snippet with annotation
        if let (Some(src), Some(line_num), Some(col)) = (source, self.line, self.column) {
            let snippet = self.create_snippet(src, line_num, col);
            output.push_str(&format!("\n{}\n", snippet));
        }

        // Help text
        if let Some(help) = &self.help {
            output.push_str(&format!("  {} {}\n", "help:".green().bold(), help));
        }

        // Suggestion
        if let Some(suggestion) = &self.suggestion {
            output.push_str(&format!("  {} {}\n", "suggestion:".cyan().bold(), suggestion));
        }

        output
    }

    /// Create annotated source snippet
    fn create_snippet(&self, source: &str, line: usize, col: usize) -> String {
        let lines: Vec<&str> = source.lines().collect();

        // Get context lines (2 before, target, 2 after)
        let start = line.saturating_sub(3);
        let end = (line + 2).min(lines.len());

        let mut output = String::new();
        let line_num_width = end.to_string().len();

        for (idx, line_content) in lines[start..end].iter().enumerate() {
            let current_line = start + idx + 1;

            if current_line == line {
                // Highlight the error line
                output.push_str(&format!(
                    "{:>width$} {} {}\n",
                    current_line.to_string().blue().bold(),
                    "|".blue().bold(),
                    line_content,
                    width = line_num_width
                ));

                // Add pointer to error column
                let pointer_offset = col.saturating_sub(1);
                output.push_str(&format!(
                    "{:>width$} {} {}{}\n",
                    "",
                    "|".blue().bold(),
                    " ".repeat(pointer_offset),
                    "^".repeat(self.context.as_ref().map(|s| s.len()).unwrap_or(1)).red().bold(),
                    width = line_num_width
                ));
            } else {
                // Context lines
                output.push_str(&format!(
                    "{:>width$} {} {}\n",
                    current_line.to_string().dimmed(),
                    "|".blue().bold(),
                    line_content.dimmed(),
                    width = line_num_width
                ));
            }
        }

        output
    }
}

impl fmt::Display for LintError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.format(None))
    }
}

pub type LintResult<T> = Result<T, Vec<LintError>>;

/// Collection of lint errors/warnings
#[derive(Debug, Default)]
pub struct LintReport {
    pub errors: Vec<LintError>,
    pub warnings: Vec<LintError>,
    pub infos: Vec<LintError>,
}

impl LintReport {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, error: LintError) {
        match error.severity {
            Severity::Error => self.errors.push(error),
            Severity::Warning => self.warnings.push(error),
            Severity::Info => self.infos.push(error),
        }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len() + self.infos.len()
    }

    pub fn print(&self, source: Option<&str>) {
        // Print all issues
        for error in &self.errors {
            println!("{}", error.format(source));
        }
        for warning in &self.warnings {
            println!("{}", warning.format(source));
        }
        for info in &self.infos {
            println!("{}", info.format(source));
        }

        // Summary
        println!();
        if self.has_errors() {
            println!(
                "{} {} error(s), {} warning(s), {} info",
                "✗".red().bold(),
                self.errors.len(),
                self.warnings.len(),
                self.infos.len()
            );
        } else if !self.warnings.is_empty() {
            println!(
                "{} {} warning(s), {} info",
                "⚠".yellow().bold(),
                self.warnings.len(),
                self.infos.len()
            );
        } else if !self.infos.is_empty() {
            println!("{} {} info", "ℹ".blue().bold(), self.infos.len());
        } else {
            println!("{} No issues found!", "✓".green().bold());
        }
    }
}
