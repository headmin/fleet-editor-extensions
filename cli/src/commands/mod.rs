//! One module per flint subcommand; each exposes a `run(...)` entry point
//! called from the thin dispatcher in `main.rs`.

pub(crate) mod agents;
pub(crate) mod check;
pub(crate) mod dry_run;
pub(crate) mod fleet;
pub(crate) mod fma;
pub(crate) mod gen;
pub(crate) mod history;
pub(crate) mod hooks;
pub(crate) mod init;
pub(crate) mod list_rules;
pub(crate) mod lsp;
pub(crate) mod migrate;
pub(crate) mod paths;
pub(crate) mod tree;
pub(crate) mod version;

use flint_lint as linter;

/// Print a deprecation notice for a legacy invocation. Goes to stderr so
/// scripts consuming stdout stay byte-compatible; the legacy form is removed
/// in v0.3.0.
pub(crate) fn deprecation_warning(old: &str, new: &str) {
    eprintln!("flint: warning: '{old}' is deprecated; use '{new}' — the legacy form is removed in v0.3.0");
}

/// Apply auto-fixable suggestions to a file.
///
/// Collects all fixable errors (Safe, or Unsafe if `include_unsafe` is true),
/// applies them bottom-up to preserve line numbers, and writes the file back.
/// Returns the number of fixes applied.
pub(crate) fn apply_fixes(
    file_path: &std::path::Path,
    report: &linter::error::LintReport,
    include_unsafe: bool,
) -> anyhow::Result<usize> {
    let mode = if include_unsafe {
        linter::ApplyMode::IncludeUnsafe
    } else {
        linter::ApplyMode::SafeOnly
    };
    Ok(linter::fix::apply_fixes_to_file(file_path, report, mode)?)
}
