//! `flint pkg` — Fleet software metadata (and policies/scripts/standalone
//! files) from a macOS .pkg installer.

use crate::args::PkgArgs;
use crate::commands::gen::with_setup_experience;
use flint_lint::installers::{inspect_pkg, InstallerInfo};
use crate::interactive::ask_yes;
use crate::interactive::wire::wire_into_fleets;
use flint_lint::discover_gitops_root;
use flint_lint as linter;
use std::path::PathBuf;

pub(crate) fn run(args: PkgArgs) -> anyhow::Result<()> {
    let PkgArgs {
        path,
        output,
        full,
        wire,
        update,
        scripts,
        policy,
        apps,
        enforce,
        outdated_only,
        file,
        setup_experience,
        yml,
    } = args;

    use colored::Colorize;

    // --policy: emit an "installed & up to date" policy. --apps targets
    // the `apps` table by bundle_identifier; --file checks a path on
    // disk; otherwise package_receipts. --enforce links the package by
    // sha so failing hosts auto-install. --outdated-only fails only on
    // an outdated copy (not when missing).
    if policy {
        let InstallerInfo { info, filename, sha256: sha } = inspect_pkg(&path)?;
        let install_hash = enforce.then_some(sha.as_str());
        let stanza = if let Some(fp) = &file {
            linter::pkg::install_policy_file(&info, &filename, fp, install_hash)
        } else if apps {
            linter::pkg::install_policy_apps(&info, &filename, install_hash, outdated_only)
        } else {
            linter::pkg::install_policy(&info, &filename, install_hash, outdated_only)
        };
        if let Some(out) = output {
            use std::io::Write;
            let existed = out.exists();
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&out)?;
            writeln!(f, "{stanza}")?;
            println!(
                "{} {} policy to {}",
                "✓".green(),
                if existed { "appended" } else { "wrote" },
                out.display()
            );
        } else {
            println!("{stanza}");
        }
        if enforce {
            println!(
                "  {}",
                "enforcement: add this package to the team's software list (same hash) so Fleet can install it on failing hosts.".dimmed()
            );
        }
        return Ok(());
    }

    // --scripts: write Fleet's default install/uninstall scripts.
    if let Some(dir) = scripts {
        use linter::pkg::{default_install_script, default_uninstall_script};
        std::fs::create_dir_all(&dir)?;
        let install = dir.join("install.sh");
        let uninstall = dir.join("uninstall.sh");
        for (p, body) in [
            (&install, default_install_script()),
            (&uninstall, default_uninstall_script()),
        ] {
            if p.exists() {
                anyhow::bail!("{} already exists — refusing to overwrite", p.display());
            }
            std::fs::write(p, body)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o755))?;
            }
            println!("{} wrote {}", "✓".green(), p.display());
        }
        println!(
            "  {}",
            "Fleet's defaults; $PACKAGE_ID is substituted by Fleet at upload.".dimmed()
        );
        return Ok(());
    }

    // --yml/--standalone: write complete standalone software file(s) named
    // `<slug>.package.yml` (GitOps convention, e.g. `ysoft-safeq-client
    // .package.yml`); on collision a `-2`/`-3` suffix keeps runs from
    // clobbering.
    if yml {
        // An explicit `-o foo.yml` writes that exact file; otherwise -o
        // (or, absent it, the .pkg's directory) is the target dir and
        // the name derives from the installer.
        let exact: Option<PathBuf> = match &output {
            Some(o)
                if matches!(
                    o.extension().and_then(|e| e.to_str()),
                    Some("yml" | "yaml")
                ) =>
            {
                Some(o.clone())
            }
            _ => None,
        };
        let dir: PathBuf = match (&output, &exact) {
            (Some(o), None) => o.clone(),
            (_, Some(e)) => e.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
            (None, None) if path.is_dir() => path.clone(),
            (None, None) => path.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
        };

        let pkgs: Vec<PathBuf> = if path.is_dir() {
            let mut v: Vec<_> = std::fs::read_dir(&path)?
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("pkg"))
                .collect();
            v.sort();
            if v.is_empty() {
                anyhow::bail!("no .pkg files in {}", path.display());
            }
            v
        } else {
            vec![path.clone()]
        };
        if exact.is_some() && pkgs.len() > 1 {
            anyhow::bail!("-o with an explicit .yml filename takes a single .pkg, not a directory");
        }

        let mkdir = if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir.clone() };
        std::fs::create_dir_all(&mkdir)?;

        for pkg in &pkgs {
            let InstallerInfo { info, filename, sha256 } = inspect_pkg(pkg)?;
            let mut body = linter::pkg::metadata_file(&info, &filename, &sha256, full);
            if setup_experience {
                body = with_setup_experience(&body);
            }
            let dest = match &exact {
                Some(p) => {
                    if p.exists() {
                        anyhow::bail!("{} already exists — refusing to overwrite", p.display());
                    }
                    p.clone()
                }
                None => {
                    let slug = super::package_slug(pkg);
                    let mut cand = mkdir.join(format!("{slug}.package.yml"));
                    let mut n = 2;
                    while cand.exists() {
                        cand = mkdir.join(format!("{slug}-{n}.package.yml"));
                        n += 1;
                    }
                    cand
                }
            };
            std::fs::write(&dest, format!("{body}\n"))?;
            println!(
                "{} wrote {} ({})",
                "✓".green(),
                dest.display(),
                filename.dimmed()
            );
            // Copy-pasteable reference for a fleet/team file: fleet files sit
            // one level below the repo root (fleets/*.yml), so the reference
            // is `../<path from root>`.
            let abs = dest.canonicalize().unwrap_or_else(|_| dest.clone());
            let root = discover_gitops_root(&abs);
            if let Ok(rel) = abs.strip_prefix(&root) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                println!("  reference it from a fleet file (software:):");
                println!("    - path: ../{rel}");
            }
        }
        return Ok(());
    }

    // --update: refresh an existing stanza in place (hash/version/url).
    if let Some(target) = update {
        let InstallerInfo { info, filename, sha256 } = inspect_pkg(&path)?;
        let identifier = info
            .identifier
            .clone()
            .ok_or_else(|| anyhow::anyhow!("could not read package identifier from {}", path.display()))?;
        let version = info.version.clone().unwrap_or_else(|| "unknown".to_string());
        let src = std::fs::read_to_string(&target)?;
        match linter::pkg::update_stanza(&src, &identifier, &filename, &version, &sha256) {
            Some((new_src, old_v, old_h)) => {
                std::fs::write(&target, new_src)?;
                println!("{} updated {} in {}", "✓".green(), identifier, target.display());
                println!("  version: {} → {}", old_v.dimmed(), version.green());
                println!("  hash:    {} → {}", old_h.dimmed(), sha256.green());
            }
            None => anyhow::bail!(
                "no stanza for '{}' found in {} (expects a '# {} (...)' header)",
                identifier,
                target.display(),
                identifier
            ),
        }
        return Ok(());
    }

    let mut block = build_pkg_block(&path, full)?;
    if setup_experience {
        block = with_setup_experience(&block);
    }
    if wire {
        if path.is_dir() {
            anyhow::bail!("--wire works on a single .pkg, not a directory");
        }
        // Software packages use `url`, not a path, so the full stanza is
        // the same for every fleet.
        let mut stanza = build_pkg_block(&path, true)?;
        // Flag wins; otherwise prompt.
        if setup_experience
            || ask_yes(
                &std::io::stdin(),
                "  Install during Setup Assistant (setup_experience: true)? [y/N]: ",
            )?
        {
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
            path.file_name().and_then(|n| n.to_str()).unwrap_or("pkg"),
            out.display()
        );
    } else {
        println!("{block}");
    }
    Ok(())
}

/// Read a macOS `.pkg` and build its Fleet software metadata block. Shells out
/// to `shasum` for the hash and `xar` to read the package's `Distribution`/
/// `PackageInfo` XML (both ship with macOS).
fn build_pkg_block(path: &std::path::Path, full: bool) -> anyhow::Result<String> {
    use linter::pkg::{metadata_block, metadata_block_full};

    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }

    // Directory → a block per .pkg (#4). Other installer formats
    // (.msi/.exe/.deb/.rpm/.ipa/.tar.gz) need their own extractors.
    if path.is_dir() {
        let mut pkgs: Vec<_> = std::fs::read_dir(path)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("pkg"))
            .collect();
        pkgs.sort();
        if pkgs.is_empty() {
            anyhow::bail!("no .pkg files in {}", path.display());
        }
        let mut blocks = Vec::new();
        for p in pkgs {
            blocks.push(build_pkg_block(&p, full)?);
        }
        return Ok(blocks.join("\n"));
    }

    let InstallerInfo { info, filename, sha256 } = inspect_pkg(path)?;
    Ok(if full {
        metadata_block_full(&info, &filename, &sha256)
    } else {
        metadata_block(&info, &filename, &sha256)
    })
}
