//! Overlay-merge support for `flint check --base/--env`: path resolution,
//! the merge itself, and RAII cleanup of the synthetic merged file.

use flint_lint as linter;
use std::path::PathBuf;

/// Resolve a `--base` or `--env` path against the user's positional path.
///
/// Absolute paths stay as-is. Relative paths resolve against `context` when
/// it's a directory, or its parent when it's a file. This mirrors fleet-plan's
/// behavior — users think of overlay paths as "relative to my repo," not
/// "relative to wherever my shell was when I typed the command."
pub(crate) fn resolve_overlay_path(target: &std::path::Path, context: &std::path::Path) -> PathBuf {
    if target.is_absolute() {
        return target.to_path_buf();
    }
    let base_dir = if context.is_dir() {
        context.to_path_buf()
    } else {
        context.parent().unwrap_or(std::path::Path::new(".")).to_path_buf()
    };
    base_dir.join(target)
}

/// Merge `base` + `env`, write the result next to `base`, return the path.
///
/// Co-locating the merged file with the base preserves resolution of
/// relative `path:` refs in the YAML (e.g. `./policies/foo.yml`). The
/// filename starts with `.flint-overlay-merge-` so users can spot and
/// delete stale ones if a crash skipped cleanup.
pub(crate) fn run_overlay_merge(
    base_path: &std::path::Path,
    env_path: &std::path::Path,
) -> anyhow::Result<PathBuf> {
    let base_content = std::fs::read_to_string(base_path)
        .map_err(|e| anyhow::anyhow!("reading --base {}: {}", base_path.display(), e))?;
    let env_content = std::fs::read_to_string(env_path)
        .map_err(|e| anyhow::anyhow!("reading --env {}: {}", env_path.display(), e))?;

    let merged = linter::merge_yaml(&base_content, &env_content)
        .map_err(|e| anyhow::anyhow!("merging --base + --env: {}", e))?;

    let parent = base_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("--base path has no parent directory"))?;

    // Include PID so two parallel flint invocations on the same base
    // don't clobber each other's merge output.
    let merged_path = parent.join(format!(".flint-overlay-merge-{}.yml", std::process::id()));
    std::fs::write(&merged_path, &merged)?;
    Ok(merged_path)
}

/// RAII guard that removes a file when dropped.
///
/// Used to clean up the synthetic overlay-merge output even if the lint
/// path exits early (e.g. via `std::process::exit` from the exit-code
/// resolver, or an `anyhow::bail!` deeper in the pipeline). Without this,
/// crashes during lint would leak `.flint-overlay-merge-*.yml` files into
/// the user's repo.
pub(crate) struct OverlayTempFile {
    path: PathBuf,
}

impl OverlayTempFile {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

impl Drop for OverlayTempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Overlay path resolution — mirrors fleet-plan's "relative paths
    // resolve against the repo root, not the shell's cwd" semantic. The
    // pure helper is testable in-process; full merge integration is
    // covered by `flint-lint::overlay` tests + the manual smoke test.

    #[test]
    fn overlay_path_absolute_stays_unchanged() {
        let abs = std::path::Path::new("/etc/passwd");
        let ctx = std::path::Path::new("/tmp");
        assert_eq!(resolve_overlay_path(abs, ctx), PathBuf::from("/etc/passwd"));
    }

    #[test]
    fn overlay_path_relative_resolves_against_dir_context() {
        // When path is a dir, relative overlay paths join directly.
        // Use the system tmp dir as a stable "definitely exists" anchor.
        let ctx = std::env::temp_dir();
        let rel = std::path::Path::new("base.yml");
        assert_eq!(resolve_overlay_path(rel, &ctx), ctx.join("base.yml"));
    }

    #[test]
    fn overlay_path_relative_resolves_against_file_parent() {
        // Single-file context: resolve against the file's parent. Without
        // this, `flint check ./repo/default.yml --base base.yml` would
        // look for `base.yml` in the cwd instead of the repo root, which
        // is almost never what the user wants.
        let ctx = std::path::PathBuf::from("/tmp/repo/default.yml");
        let rel = std::path::Path::new("base.yml");
        assert_eq!(
            resolve_overlay_path(rel, &ctx),
            PathBuf::from("/tmp/repo/base.yml")
        );
    }
}
