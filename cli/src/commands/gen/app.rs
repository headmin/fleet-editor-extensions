//! `flint app` — a Fleet software block from any supported installer format.

use crate::args::AppArgs;
use crate::commands::gen::with_setup_experience;
use flint_lint::installers::{inspect, InstallerInfo};
use crate::interactive::ask_yes;
use crate::interactive::wire::wire_into_fleets;
use flint_lint::discover_gitops_root;
use flint_lint as linter;

pub(crate) fn run(args: AppArgs) -> anyhow::Result<()> {
    let AppArgs {
        path,
        output,
        full,
        wire,
    } = args;

    use colored::Colorize;
    let block = build_app_block(&path, full)?;
    if wire {
        let mut stanza = build_app_block(&path, true)?;
        if ask_yes(
            &std::io::stdin(),
            "  Install during Setup Assistant (setup_experience: true)? [y/N]: ",
        )? {
            stanza = with_setup_experience(&stanza);
        }
        println!("{stanza}\n");
        // Software is url-based (installer location is irrelevant), so
        // find the GitOps repo from the current directory.
        let root = discover_gitops_root(&std::env::current_dir()?);
        wire_into_fleets(&root, &["software", "packages"], |_dir| stanza.clone())?;
        return Ok(());
    }
    if let Some(out) = output {
        use std::io::Write;
        let existed = out.exists();
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&out)?;
        writeln!(f, "{block}")?;
        println!(
            "{} {} block for {} to {}",
            "✓".green(),
            if existed { "appended" } else { "wrote" },
            path.file_name().and_then(|n| n.to_str()).unwrap_or("app"),
            out.display()
        );
    } else {
        println!("{block}");
    }
    Ok(())
}

/// Build a software block for any supported installer format (#2).
fn build_app_block(path: &std::path::Path, full: bool) -> anyhow::Result<String> {
    use linter::pkg::{metadata_block, metadata_block_full};
    let InstallerInfo { info, filename, sha256 } = inspect(path)?;
    Ok(if full {
        metadata_block_full(&info, &filename, &sha256)
    } else {
        metadata_block(&info, &filename, &sha256)
    })
}
