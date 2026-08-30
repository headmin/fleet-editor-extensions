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

/// Extract any tree-ish — a commit, or a bare tree oid from `git merge-tree`
/// — into `dest` without touching the working copy. `git archive` writes the
/// tree exactly as git holds it, so a replay or a merge preview never
/// disturbs the checkout the user is sitting in.
pub(crate) fn materialize_tree(
    repo: &std::path::Path,
    treeish: &str,
    dest: &std::path::Path,
) -> anyhow::Result<()> {
    use anyhow::Context;
    let tar = dest.join(".flint-tree.tar");
    let out = std::process::Command::new("git")
        .args(["archive", "--format=tar", "-o"])
        .arg(&tar)
        .arg(treeish)
        .current_dir(repo)
        .output()
        .context("failed to run git archive")?;
    if !out.status.success() {
        anyhow::bail!(
            "git archive {treeish} failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let status = std::process::Command::new("tar")
        .arg("-xf")
        .arg(&tar)
        .arg("-C")
        .arg(dest)
        .status()
        .context("failed to run tar")?;
    if !status.success() {
        anyhow::bail!("tar failed to extract {treeish}");
    }
    let _ = std::fs::remove_file(&tar);
    Ok(())
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
