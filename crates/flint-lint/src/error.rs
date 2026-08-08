//! Error types, severity levels, and fix safety for lint diagnostics.
//!
//! `LintError` is the primary diagnostic type produced by all rules.
//! `LintReport` collects errors, warnings, and infos for a single file.

use colored::*;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// Fix applicability level — determines whether `--fix` auto-applies the fix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FixSafety {
    /// Safe to auto-apply (typo corrections, case fixes, deprecated renames).
    Safe,
    /// May change semantics — only applied with `--unsafe-fixes`.
    Unsafe,
    /// Show to user but never auto-apply (informational suggestions).
    Display,
}

/// The machine-actionable remedy attached to a diagnostic — at most one per
/// error. Serialized whole into the LSP `Diagnostic.data` so editor quick-fix
/// actions and CLI `--fix` share one representation (applied by
/// [`crate::fix`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fix {
    /// Replace `old` with `new` on the error's line. `old: None` marks a
    /// display-only template (an example snippet, not a substitution) — the
    /// applier skips it, but editors may still offer it over the diagnostic
    /// range.
    Replace {
        old: Option<String>,
        new: String,
        safety: FixSafety,
    },
    /// Alternative replacements for `old` when the fix is ambiguous (e.g.
    /// multiple files match a moved path). Never auto-applied; surfaced as a
    /// choice of quick-fixes in the editor.
    Candidates { old: String, options: Vec<String> },
    /// Replace the inclusive 1-indexed line range with `replacement` — a
    /// multi-line structural rewrite (e.g. expanding a directory `path:`
    /// entry into one entry per file in the directory).
    ReplaceLines {
        start_line: usize,
        end_line: usize,
        replacement: String,
        safety: FixSafety,
    },
}

impl Fix {
    /// Whether this fix may be applied automatically.
    pub fn safety(&self) -> FixSafety {
        match self {
            Fix::Replace { safety, .. } | Fix::ReplaceLines { safety, .. } => *safety,
            Fix::Candidates { .. } => FixSafety::Display,
        }
    }
}

/// 1-based source location with an optional highlight width.
///
/// Replaces the loose `Option<usize>` line/column pairs that every rule used
/// to thread around (`if let (Some(l), Some(c)) = …`). `len` is the number of
/// characters to highlight; `0` means "rest of the line".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub len: usize,
}

impl Span {
    /// A whole-line span (column 1, highlight to end of line).
    pub fn line(line: usize) -> Self {
        Self {
            line,
            column: 1,
            len: 0,
        }
    }

    /// A point span at `line:column`.
    pub fn at(line: usize, column: usize) -> Self {
        Self {
            line,
            column,
            len: 1,
        }
    }

    /// A token span of `len` characters starting at `line:column`.
    pub fn token(line: usize, column: usize, len: usize) -> Self {
        Self { line, column, len }
    }
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
    /// Where in the source this diagnostic points, if known. See [`Span`].
    pub span: Option<Span>,
    pub context: Option<String>,
    pub help: Option<String>,
    /// Rule code for diagnostic identification — always one of the
    /// [`crate::codes`] consts (e.g. `codes::REQUIRED_FIELDS`).
    pub rule_code: Option<&'static str>,
    /// Full documentation URL for the rule, stamped by the engine from
    /// [`crate::codes::REGISTRY`].
    pub doc_url: Option<&'static str>,
    /// The machine-actionable remedy, if one exists. See [`Fix`].
    pub fix: Option<Fix>,
    /// Other file(s) involved in a cross-file finding — e.g. the renamed
    /// target a broken reference still points at, or the case-colliding
    /// sibling path. Empty for single-file findings. In `--staged`-style
    /// scoping, a finding blocks if its `file` OR any `related` path is
    /// staged (ADR-010).
    pub related: Vec<PathBuf>,
}

impl LintError {
    fn new(severity: Severity, message: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self {
            severity,
            message: message.into(),
            file: file.into(),
            span: None,
            context: None,
            help: None,
            rule_code: None,
            doc_url: None,
            fix: None,
            related: Vec::new(),
        }
    }

    /// Attach another file involved in this cross-file finding.
    pub fn with_related(mut self, path: impl Into<PathBuf>) -> Self {
        self.related.push(path.into());
        self
    }

    /// The 1-based line, if located.
    pub fn line(&self) -> Option<usize> {
        self.span.map(|s| s.line)
    }

    /// The 1-based column, if located.
    pub fn column(&self) -> Option<usize> {
        self.span.map(|s| s.column)
    }

    pub fn error(message: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self::new(Severity::Error, message, file)
    }

    pub fn warning(message: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self::new(Severity::Warning, message, file)
    }

    pub fn info(message: impl Into<String>, file: impl Into<PathBuf>) -> Self {
        Self::new(Severity::Info, message, file)
    }

    pub fn with_rule_code(mut self, code: &'static str) -> Self {
        self.rule_code = Some(code);
        self
    }

    pub fn with_fix(mut self, fix: Fix) -> Self {
        self.fix = Some(fix);
        self
    }

    pub fn with_location(mut self, line: usize, column: usize) -> Self {
        // Width comes from `context` at render time when unknown here.
        self.span = Some(Span { line, column, len: 0 });
        self
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
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

    /// Display-only convenience: attach an example/template suggestion that is
    /// shown to the user (and offered as a non-preferred editor action) but
    /// never auto-applied.
    pub fn with_suggestion(self, suggestion: impl Into<String>) -> Self {
        self.with_fix(Fix::Replace {
            old: None,
            new: suggestion.into(),
            safety: FixSafety::Display,
        })
    }

    /// The single replacement/suggestion text, if any (`Replace.new`). Kept
    /// for report rendering (`suggestion:` line, JSON output). Ambiguous
    /// `Candidates` deliberately yield `None` — presenting one option as "the"
    /// suggestion would misrepresent an ambiguous fix.
    pub fn suggestion(&self) -> Option<&str> {
        match &self.fix {
            Some(Fix::Replace { new, .. }) => Some(new),
            _ => None,
        }
    }

    /// The fix's applicability, if a fix is present.
    pub fn fix_safety(&self) -> Option<FixSafety> {
        self.fix.as_ref().map(|f| f.safety())
    }

    /// Format error with rich annotations
    pub fn format(&self, source: Option<&str>) -> String {
        let mut output = String::new();

        // Header: severity and message
        output.push_str(&format!("{}: {}\n", self.severity, self.message.bold()));

        // Location
        if let Some(line) = self.line() {
            let location = if let Some(col) = self.column() {
                format!("{}:{}:{}", self.file.display(), line, col)
            } else {
                format!("{}:{}", self.file.display(), line)
            };
            output.push_str(&format!("  {} {}\n", "-->".blue().bold(), location));
        } else {
            output.push_str(&format!(
                "  {} {}\n",
                "-->".blue().bold(),
                self.file.display()
            ));
        }

        // Source snippet with annotation
        if let (Some(src), Some(line_num), Some(col)) = (source, self.line(), self.column()) {
            let snippet = self.create_snippet(src, line_num, col);
            output.push_str(&format!("\n{}\n", snippet));
        }

        // Help text
        if let Some(help) = &self.help {
            output.push_str(&format!("  {} {}\n", "help:".green().bold(), help));
        }

        // Suggestion
        if let Some(suggestion) = self.suggestion() {
            output.push_str(&format!(
                "  {} {}\n",
                "suggestion:".cyan().bold(),
                suggestion
            ));
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
                    "^".repeat(self.context.as_ref().map(|s| s.len()).unwrap_or(1))
                        .red()
                        .bold(),
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

    /// Render every finding plus the summary line, as the CLI prints it.
    ///
    /// Returns the text rather than writing it: a library that prints can
    /// only be driven by a terminal, and the same rendering is wanted by the
    /// CLI, by tests that assert on output, and by anything embedding the
    /// engine. The trailing newline is included so callers can `print!` it
    /// directly.
    pub fn render(&self, source: Option<&str>) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();

        for finding in self.errors.iter().chain(&self.warnings).chain(&self.infos) {
            let _ = writeln!(out, "{}", finding.format(source));
        }

        out.push('\n');
        let summary = if self.has_errors() {
            format!(
                "{} {} error(s), {} warning(s), {} info",
                "✗".red().bold(),
                self.errors.len(),
                self.warnings.len(),
                self.infos.len()
            )
        } else if !self.warnings.is_empty() {
            format!(
                "{} {} warning(s), {} info",
                "⚠".yellow().bold(),
                self.warnings.len(),
                self.infos.len()
            )
        } else if !self.infos.is_empty() {
            format!("{} {} info", "ℹ".blue().bold(), self.infos.len())
        } else {
            format!("{} No issues found!", "✓".green().bold())
        };
        let _ = writeln!(out, "{summary}");
        out
    }
}
