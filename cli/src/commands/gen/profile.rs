//! `flint profile` — a Fleet `configuration_profiles` entry from a
//! .mobileconfig / DDM .json / Windows .xml profile.

use crate::args::ProfileArgs;
use crate::interactive::wire::wire_into_fleets;
use flint_lint::discover_gitops_root;
use flint_lint as linter;

pub(crate) fn run(args: ProfileArgs) -> anyhow::Result<()> {
    let ProfileArgs {
        path,
        output,
        full,
        regen_uuid,
        wire,
    } = args;

    use colored::Colorize;
    if regen_uuid {
        let (old, new) = regen_profile_uuid(&path)?;
        println!(
            "{} regenerated PayloadUUID in {}\n  {} → {}",
            "✓".green(),
            path.display(),
            old.dimmed(),
            new.green()
        );
        return Ok(());
    }
    let block = build_profile_block(&path, full)?;
    if wire {
        if path.is_dir() {
            anyhow::bail!("--wire works on a single profile file, not a directory");
        }
        println!("{block}\n");
        let section = profile_section(&path);
        let art = path.canonicalize().unwrap_or(path.clone());
        // A profile lives in the repo and is referenced by path, so find
        // the repo from the profile's own location.
        let root = discover_gitops_root(&art);
        wire_into_fleets(&root, &section, |fleet_dir| {
            let rel = linter::unwired::rel_to(fleet_dir, &art);
            single_profile_entry(&art, &rel, full).unwrap_or_default()
        })?;
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
            "{} {} entry for {} to {}",
            "✓".green(),
            if existed { "appended" } else { "wrote" },
            path.file_name().and_then(|n| n.to_str()).unwrap_or("profile"),
            out.display()
        );
    } else {
        println!("{block}");
    }
    Ok(())
}

/// Build a `configuration_profiles`/`setup_experience` entry for one profile
/// file (.mobileconfig, Windows .xml CSP, DDM .json, or ADE .dep.json), or —
/// when given a directory — one block per profile in it (#4 batch).
fn build_profile_block(path: &std::path::Path, full: bool) -> anyhow::Result<String> {
    if !path.exists() {
        anyhow::bail!("path not found: {}", path.display());
    }

    // Directory → emit a block per profile file (#4).
    if path.is_dir() {
        let mut blocks = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(path)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("mobileconfig") | Some("xml") | Some("json")
                )
            })
            .collect();
        files.sort();
        for f in files {
            blocks.push(build_profile_block(&f, full)?);
        }
        if blocks.is_empty() {
            anyhow::bail!("no profile files (.mobileconfig/.xml/.json) in {}", path.display());
        }
        return Ok(blocks.join("\n"));
    }

    single_profile_entry(path, &path.to_string_lossy(), full)
}

/// Build the entry for one profile file using an explicit `path_value` (so
/// `--wire` can pass a path relative to the chosen fleet).
pub(crate) fn single_profile_entry(
    path: &std::path::Path,
    path_value: &str,
    full: bool,
) -> anyhow::Result<String> {
    use linter::profile::{
        declaration_block, enrollment_block, parse_declaration, parse_enrollment,
        parse_mobileconfig, parse_windows_csp, profile_block, windows_profile_block,
    };

    let content = std::fs::read_to_string(path)?;
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("profile");

    match path.extension().and_then(|e| e.to_str()) {
        Some("mobileconfig") => Ok(profile_block(&parse_mobileconfig(&content), path_value, full)),
        Some("xml") => Ok(windows_profile_block(
            filename,
            &parse_windows_csp(&content),
            path_value,
            full,
        )),
        Some("json") => {
            let decl = parse_declaration(&content);
            if decl.decl_type.is_some() {
                Ok(declaration_block(&decl, path_value, full))
            } else {
                let enr = parse_enrollment(&content);
                if enr.profile_name.is_some() {
                    Ok(enrollment_block(&enr, path_value))
                } else {
                    Ok(declaration_block(&decl, path_value, full))
                }
            }
        }
        _ => anyhow::bail!("expected .mobileconfig / .xml / .json, got: {}", path.display()),
    }
}

/// The GitOps section keys a profile file is wired under.
fn profile_section(path: &std::path::Path) -> Vec<&'static str> {
    match path.extension().and_then(|e| e.to_str()) {
        Some("xml") => vec!["controls", "windows_settings", "configuration_profiles"],
        Some("json") => {
            let is_enrollment = std::fs::read_to_string(path)
                .ok()
                .map(|c| {
                    let d = linter::profile::parse_declaration(&c);
                    d.decl_type.is_none()
                        && linter::profile::parse_enrollment(&c).profile_name.is_some()
                })
                .unwrap_or(false);
            if is_enrollment {
                vec!["controls", "setup_experience"]
            } else {
                vec!["controls", "apple_settings", "configuration_profiles"]
            }
        }
        _ => vec!["controls", "apple_settings", "configuration_profiles"],
    }
}

/// Rewrite a `.mobileconfig`'s top-level PayloadUUID with a fresh one
/// (`uuidgen`). Returns the (old, new) UUIDs.
fn regen_profile_uuid(path: &std::path::Path) -> anyhow::Result<(String, String)> {
    use linter::profile::parse_mobileconfig;
    use std::process::Command;

    let content = std::fs::read_to_string(path)?;
    let old = parse_mobileconfig(&content)
        .uuid
        .ok_or_else(|| anyhow::anyhow!("no top-level PayloadUUID found in {}", path.display()))?;

    let out = Command::new("uuidgen")
        .output()
        .map_err(|e| anyhow::anyhow!("failed to run uuidgen: {e}"))?;
    let new = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if new.is_empty() {
        anyhow::bail!("uuidgen produced no output");
    }

    // The top-level UUID value is unique in the file (nested payloads carry
    // their own), so replacing the first `<string>OLD</string>` is safe.
    let needle = format!("<string>{old}</string>");
    let updated = content.replacen(&needle, &format!("<string>{new}</string>"), 1);
    if updated == content {
        anyhow::bail!("could not locate the PayloadUUID value to replace in {}", path.display());
    }
    std::fs::write(path, updated)?;
    Ok((old, new))
}
