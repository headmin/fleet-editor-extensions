//! Interactive wiring of artifacts into fleet/team files: the `paths
//! --unwired --interactive` walk and the generator commands' `--wire` flow.

use super::ask;
use flint_lint as linter;
use std::path::PathBuf;

/// Append a `<key>:` list of quoted labels to a YAML item (relative indent 2/4),
/// from a comma-separated string. When blank, `stub` controls the placeholder:
/// `None` omits the key, `Some("blank")` emits the empty key, `Some("comment")`
/// emits a commented stub.
pub(crate) fn append_label_block(item: &mut String, key: &str, raw: &str, stub: Option<&str>) {
    let names: Vec<&str> = raw
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if names.is_empty() {
        match stub {
            // Blank key — valid Fleet GitOps ("no items"), not flagged by flint.
            Some("blank") => item.push_str(&format!("\n  {key}:")),
            // Commented stub — inert until uncommented.
            Some("comment") => item.push_str(&format!("\n  # {key}:\n  #   - \"Label name\"")),
            _ => {}
        }
        return;
    }
    item.push_str(&format!("\n  {key}:"));
    for n in names {
        item.push_str(&format!("\n    - \"{n}\""));
    }
}

/// The section path to write into, matched to the spelling `source` already
/// uses.
///
/// `classify` returns current Fleet key names. A repository mid-migration
/// still has `controls.macos_settings.custom_settings`, and adding
/// `controls.apple_settings.configuration_profiles` next to it produces a
/// file `fleetctl gitops` refuses to parse. Falls back to the modern name
/// when the file has neither, so new sections are never created deprecated.
fn section_for(source: &str, modern: &'static str) -> String {
    match serde_yaml::from_str::<serde_yaml::Value>(source) {
        Ok(doc) => linter::deprecated_conflict::section_path_for(&doc, modern),
        // Unparseable file: keep the modern name rather than guessing.
        Err(_) => modern.to_string(),
    }
}

/// Interactively insert a generated entry into chosen fleet/team files found
/// under `root`. `entry(fleet_dir)` produces the YAML item (paths relative to
/// that fleet).
pub(crate) fn wire_into_fleets(
    root: &std::path::Path,
    section: &[&str],
    entry: impl Fn(&std::path::Path) -> String,
) -> anyhow::Result<()> {
    use colored::Colorize;
    use linter::unwired::{config_files, insert_under};

    let root = root.to_path_buf();
    let mut fleets = config_files(&root);
    if fleets.is_empty() {
        anyhow::bail!(
            "no fleet/team config files found under {} — run from inside your GitOps repo",
            root.display()
        );
    }
    fleets.sort();
    fleets.sort_by_key(|f| f.file_name().map(|n| n == "default.yml").unwrap_or(false));

    let stdin = std::io::stdin();
    let mut wired = 0usize;

    let do_wire = |fleet: &std::path::Path| -> anyhow::Result<String> {
        let dir = fleet.parent().unwrap_or(&root);
        let item = entry(dir);
        let src = std::fs::read_to_string(fleet)?;
        let new = insert_under(&src, section, &item, "");
        std::fs::write(fleet, new)?;
        Ok(fleet
            .strip_prefix(&root)
            .unwrap_or(fleet)
            .to_string_lossy()
            .replace('\\', "/"))
    };

    let mut i = 0;
    while i < fleets.len() {
        let mut rel = fleets[i]
            .strip_prefix(&root)
            .unwrap_or(&fleets[i])
            .to_string_lossy()
            .replace('\\', "/");
        if fleets[i].file_name().map(|n| n == "default.yml").unwrap_or(false) {
            rel.push_str(" (global)");
        }
        let ans = match ask(
            &stdin,
            &format!(
                "  Wire into {} ?  [y]es / [a]ll-remaining / [n]o / [q]uit: ",
                rel.bold()
            ),
        )? {
            Some(a) => a.to_lowercase(),
            None => break,
        };
        match ans.as_str() {
            "y" | "yes" => {
                let r = do_wire(&fleets[i])?;
                println!("    {} wired into {}", "✓".green(), r);
                wired += 1;
            }
            "a" | "all" => {
                for f in &fleets[i..] {
                    let r = do_wire(f)?;
                    println!("    {} wired into {}", "✓".green(), r);
                    wired += 1;
                }
                break;
            }
            "q" | "quit" => break,
            _ => {}
        }
        i += 1;
    }
    println!("\n{} wired into {} file(s).", "✓".green(), wired);
    Ok(())
}

/// Interactive wiring: for each unwired group, prompt whether to wire it into
/// each fleet/team file. On "yes", insert the `paths:` glob (computed relative
/// to *that* file) under the right section, tagged with a comment. Files are
/// modified only on an explicit yes.
pub(crate) fn interactive_unwired(
    path: &std::path::Path,
    label_stubs: Option<&str>,
    only: Option<&str>,
) -> anyhow::Result<()> {
    use crate::commands::paths::describe_artifact;
    use colored::Colorize;
    use linter::unwired::{
        config_files, find_unwired, glob_match, insert_under, known_labels, rel_to, UnwiredArtifact,
    };
    use std::collections::BTreeMap;
    use std::io;

    let raw_root = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent().map(|p| p.to_path_buf()).unwrap_or_default()
    };
    let root = raw_root.canonicalize().unwrap_or(raw_root);

    let report = find_unwired(&root);
    if report.artifacts.is_empty() {
        println!("{} No unwired artifacts found.", "✓".green());
        return Ok(());
    }

    let mut fleets = config_files(&root);
    if fleets.is_empty() {
        anyhow::bail!("no fleet/team config files found under {}", root.display());
    }
    // --only: keep fleets whose path or name matches the glob.
    if let Some(pat) = only {
        fleets.retain(|f| {
            let rel = f
                .strip_prefix(&root)
                .unwrap_or(f)
                .to_string_lossy()
                .replace('\\', "/");
            let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
            glob_match(pat, &rel) || glob_match(pat, name)
        });
        if fleets.is_empty() {
            anyhow::bail!("no fleet/team files match --only {pat}");
        }
    }

    // Deterministic order (read_dir is unsorted), then default.yml (global) last.
    fleets.sort();
    fleets.sort_by_key(|f| f.file_name().map(|n| n == "default.yml").unwrap_or(false));

    // Workspace labels, to suggest as targets when wiring per-file with labels_*.
    let labels = known_labels(&root);

    // Group orphans by (section + artifact directory) so one decision wires the
    // whole directory (via a glob) into a chosen fleet.
    struct Group<'a> {
        section: &'static str,
        single_only: bool,
        ext: String,
        dir_abs: PathBuf,
        files: Vec<&'a UnwiredArtifact>,
    }
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();
    for o in &report.artifacts {
        let dir_abs = o.path.parent().map(|p| p.to_path_buf()).unwrap_or_default();
        let ext = o
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let key = format!("{}|{}", o.section, dir_abs.display());
        groups
            .entry(key)
            .or_insert_with(|| Group {
                section: o.section,
                single_only: o.single_only,
                ext: ext.clone(),
                dir_abs: dir_abs.clone(),
                files: Vec::new(),
            })
            .files
            .push(o);
    }

    println!("{} {}", "Interactive wiring —".bold(), root.display());
    println!(
        "{}\n",
        "For each unwired group, choose whether to wire it into each fleet/team file.".dimmed()
    );

    let stdin = io::stdin();
    let mut wired = 0usize;

    'groups: for g in groups.values() {
        let dir_rel = g
            .dir_abs
            .strip_prefix(&root)
            .unwrap_or(&g.dir_abs)
            .to_string_lossy()
            .replace('\\', "/");
        println!(
            "{} {} unwired {}(s) in {}/  → {}",
            "▸".blue(),
            g.files.len(),
            g.ext,
            dir_rel,
            g.section.bold()
        );
        // Show WHAT is being wired (name/scope/type), not just the count.
        for o in &g.files {
            println!("    {} {}", "•".dimmed(), describe_artifact(&o.path));
        }
        if g.single_only {
            println!(
                "  {}",
                "(single-value field, one per team — wire manually; skipped here)".dimmed()
            );
            println!();
            continue;
        }

        // Resolved per FILE below: a fleet file that still uses the
        // deprecated spelling must not gain the modern key beside it. Fleet
        // rejects a document carrying both ("cannot specify both … use only
        // …"), and writing the modern name into an unmigrated file is exactly
        // how this command used to produce one.
        // Per-path label targeting only makes sense where Fleet accepts it.
        let labels_ok = g.section.ends_with("configuration_profiles")
            || g.section.ends_with("custom_settings")
            || g.section.ends_with("packages");

        for (fi, fleet) in fleets.iter().enumerate() {
            let mut fleet_rel = fleet
                .strip_prefix(&root)
                .unwrap_or(fleet)
                .to_string_lossy()
                .replace('\\', "/");
            if fleet.file_name().map(|n| n == "default.yml").unwrap_or(false) {
                fleet_rel.push_str(" (global)");
            }
            let fleet_dir = fleet.parent().unwrap_or(&root);

            let answer = match ask(
                &stdin,
                &format!(
                    "  Wire into {} ?  [y]es / [a]ll-remaining / [n]o / [s]kip group / [q]uit: ",
                    fleet_rel.bold()
                ),
            )? {
                Some(a) => a.to_lowercase(),
                None => break 'groups, // EOF
            };
            match answer.as_str() {
                "y" | "yes" => {}
                "a" | "all" => {
                    // Wire the glob into this and every remaining fleet at once.
                    for f in &fleets[fi..] {
                        let dir = f.parent().unwrap_or(&root);
                        let glob = format!("- paths: {}/*.{}", rel_to(dir, &g.dir_abs), g.ext);
                        let mut s = std::fs::read_to_string(f)?;
                        let section = section_for(&s, g.section);
                        let keys: Vec<&str> = section.split('.').collect();
                        s = insert_under(&s, &keys, &glob, "wired by flint paths --unwired");
                        std::fs::write(f, s)?;
                        let r = f
                            .strip_prefix(&root)
                            .unwrap_or(f)
                            .to_string_lossy()
                            .replace('\\', "/");
                        println!("    {} wired into {}", "✓".green(), r);
                        wired += 1;
                    }
                    continue 'groups;
                }
                "s" | "skip" => continue 'groups,
                "q" | "quit" => break 'groups,
                _ => continue, // n / anything else → don't wire
            }

            let mut src = std::fs::read_to_string(fleet)?;
            let section = section_for(&src, g.section);
            let keys: Vec<&str> = section.split('.').collect();

            // Glob covers the whole dir with one rule; per-file lets each entry
            // carry its own labels_* (globs can't be label-scoped).
            let per_file = labels_ok
                && matches!(
                    ask(
                        &stdin,
                        "    style: [g]lob (all, no labels) / [p]er-file (add labels)? "
                    )?
                    .unwrap_or_default()
                    .to_lowercase()
                    .as_str(),
                    "p" | "per-file" | "path"
                );

            if per_file {
                if !labels.is_empty() {
                    println!(
                        "    {}",
                        format!("known labels: {}", labels.join(", ")).dimmed()
                    );
                }
                // Build every entry first, then insert as ONE block — inserting
                // one at a time would orphan each entry's label stubs at the
                // end (the "insert after last real line" placement treats a
                // prior entry's comment stubs as non-real).
                let mut entries = Vec::new();
                for o in &g.files {
                    let rel = rel_to(fleet_dir, &o.path);
                    let fname = o.path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    println!("      {}", fname.bold());
                    // include_any and exclude_any are independent and combinable
                    // (broad include + targeted exclude).
                    let inc = ask(&stdin, "        labels_include_any (comma-sep, blank = none): ")?
                        .unwrap_or_default();
                    let exc = ask(&stdin, "        labels_exclude_any (comma-sep, blank = none): ")?
                        .unwrap_or_default();
                    // Comment goes on the path line directly so each entry keeps
                    // its own marker when joined.
                    let mut item = format!("- path: {rel}  # wired by flint paths --unwired");
                    append_label_block(&mut item, "labels_include_any", &inc, label_stubs);
                    append_label_block(&mut item, "labels_exclude_any", &exc, label_stubs);
                    entries.push(item);
                    wired += 1;
                }
                src = insert_under(&src, &keys, &entries.join("\n"), "");
            } else {
                let glob = format!("- paths: {}/*.{}", rel_to(fleet_dir, &g.dir_abs), g.ext);
                src = insert_under(&src, &keys, &glob, "wired by flint paths --unwired");
                wired += 1;
            }

            std::fs::write(fleet, src)?;
            println!("    {} wired into {}", "✓".green(), fleet_rel);
        }
        println!();
    }

    println!("\n{} wired {} construct(s).", "✓".green(), wired);
    Ok(())
}
