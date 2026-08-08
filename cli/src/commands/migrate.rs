//! `flint migrate` — migration report for upgrading Fleet GitOps YAML.

use crate::args::MigrateArgs;
use crate::commands::tree::walkdir_count;
use flint_lint as linter;

pub(crate) fn run(args: MigrateArgs) -> anyhow::Result<()> {
    let MigrateArgs {
        path,
        target_version,
    } = args;

    use linter::{
        DeprecationKind, FixSafety, Linter, RuleSet, VersionContext, DEPRECATION_REGISTRY,
    };

    if !path.is_dir() {
        anyhow::bail!("Not a directory: {}", path.display());
    }

    // Build version context with future_names enabled so all active deprecations fire
    let mut version_ctx = VersionContext::from_config(&target_version);
    version_ctx.future_names = true;

    let target_ver = version_ctx.version.clone();
    let linter = Linter::with_rules(RuleSet::standard(flint_lint::rules::RuleOptions { version: version_ctx, ..Default::default() }));
    let results = linter.lint_directory(&path, None)?;

    // Collect deprecation diagnostics from lint results
    let mut file_changes: Vec<serde_json::Value> = Vec::new();
    let mut total_key_renames = 0usize;
    let mut total_safe = 0usize;
    let mut total_unsafe = 0usize;

    for (file_path_buf, report) in &results {
        let all_errors: Vec<&linter::LintError> = report
            .errors
            .iter()
            .chain(report.warnings.iter())
            .chain(report.infos.iter())
            .collect();

        let key_renames: Vec<serde_json::Value> = all_errors
            .iter()
            .filter(|e| e.rule_code == Some("deprecated-keys"))
            .filter(|e| e.context.is_some() && e.suggestion().is_some())
            .map(|e| {
                let safety = match e.fix_safety() {
                    Some(FixSafety::Safe) => "safe",
                    _ => "unsafe",
                };
                if safety == "safe" {
                    total_safe += 1;
                } else {
                    total_unsafe += 1;
                }
                serde_json::json!({
                    "line": e.line().unwrap_or(0),
                    "old_key": e.context.as_deref().unwrap_or(""),
                    "new_key": e.suggestion().unwrap_or(""),
                    "safety": safety,
                })
            })
            .collect();

        if key_renames.is_empty() {
            continue;
        }

        total_key_renames += key_renames.len();

        // Compute relative path and potential move_to
        let file_path_str = file_path_buf.display().to_string();
        let rel_path = file_path_str
            .strip_prefix(&format!("{}/", path.display()))
            .or_else(|| file_path_str.strip_prefix(&path.display().to_string()))
            .unwrap_or(&file_path_str);

        let mut entry = serde_json::json!({
            "path": rel_path,
            "key_renames": key_renames,
        });

        // Check if this file is inside a directory that needs renaming
        for dep in DEPRECATION_REGISTRY.active_directory_renames(&target_ver) {
            if let DeprecationKind::DirectoryRename { old_dir, new_dir } = &dep.kind {
                let prefix = format!("{}/", old_dir);
                if rel_path.starts_with(&prefix) {
                    let new_path = format!("{}/{}", new_dir, &rel_path[prefix.len()..]);
                    entry
                        .as_object_mut()
                        .unwrap()
                        .insert("move_to".into(), serde_json::json!(new_path));
                }
            }
        }

        file_changes.push(entry);
    }

    // Scan for directory renames
    let mut dir_renames: Vec<serde_json::Value> = Vec::new();
    for dep in DEPRECATION_REGISTRY.active_directory_renames(&target_ver) {
        if let DeprecationKind::DirectoryRename { old_dir, new_dir } = &dep.kind {
            let old_path = path.join(old_dir);
            if old_path.is_dir() {
                let file_count = walkdir_count(&old_path);
                dir_renames.push(serde_json::json!({
                    "old": old_dir,
                    "new": new_dir,
                    "files_affected": file_count,
                }));
            }
        }
    }

    // Scan for file renames from registry
    let mut file_renames: Vec<serde_json::Value> = Vec::new();
    for dep in DEPRECATION_REGISTRY.active_file_renames(&target_ver) {
        if let DeprecationKind::FileRename { old_name, new_name } = &dep.kind {
            if path.join(old_name).exists() {
                file_renames.push(serde_json::json!({
                    "old": old_name,
                    "new": new_name,
                }));
            }
        }
    }

    let report = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "target_version": target_ver.to_string(),
        "summary": {
            "files_scanned": results.len(),
            "deprecations_found": total_key_renames + dir_renames.len() + file_renames.len(),
            "directory_renames": dir_renames.len(),
            "file_renames": file_renames.len(),
            "key_renames": total_key_renames,
            "safe_fixes": total_safe,
            "unsafe_fixes": total_unsafe,
        },
        "directory_renames": dir_renames,
        "file_renames": file_renames,
        "file_changes": file_changes,
    });

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
