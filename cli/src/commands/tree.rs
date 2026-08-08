//! `flint tree` — directory tree of a Fleet GitOps repo.

use crate::args::TreeArgs;

pub(crate) fn run(args: TreeArgs) -> anyhow::Result<()> {
    let path = args.path;
    if !path.is_dir() {
        anyhow::bail!("Not a directory: {}", path.display());
    }
    println!("{}", path.display());
    print_tree(&path, "")?;
    Ok(())
}

/// Count YAML files in a directory recursively.
pub(crate) fn walkdir_count(dir: &std::path::Path) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                count += walkdir_count(&path);
            } else if let Some(ext) = path.extension() {
                if ext == "yml" || ext == "yaml" {
                    count += 1;
                }
            }
        }
    }
    count
}

/// Print a directory tree, excluding .git, .DS_Store, node_modules, target, dist.
fn print_tree(dir: &std::path::Path, prefix: &str) -> anyhow::Result<()> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            !matches!(
                s.as_ref(),
                ".git" | ".DS_Store" | "node_modules" | "target" | "dist" | ".gitkeep"
            )
        })
        .collect();

    entries.sort_by_key(|e| {
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        (!is_dir, e.file_name()) // dirs first, then alphabetical
    });

    let total = entries.len();
    for (i, entry) in entries.iter().enumerate() {
        let is_last = i == total - 1;
        let connector = if is_last { "└── " } else { "├── " };
        let child_prefix = if is_last { "    " } else { "│   " };
        let name = entry.file_name();

        println!("{}{}{}", prefix, connector, name.to_string_lossy());

        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            print_tree(&entry.path(), &format!("{}{}", prefix, child_prefix))?;
        }
    }

    Ok(())
}
