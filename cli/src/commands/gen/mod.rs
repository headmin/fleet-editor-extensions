//! The artifact-generator family — flint's third face: copy-pasteable Fleet
//! GitOps YAML from real artifacts or blank templates.
//!
//! `flint gen <kind> [--from <source>]` is the only surface. The per-kind
//! `run()` fns and their `*Args` structs here are the implementations `gen`
//! dispatches to; the legacy top-level spellings that used to forward to them
//! (`pkg`, `app`, `profile`, `query`, `new`) were removed in v0.3.0.

pub(crate) mod app;
pub(crate) mod new;
pub(crate) mod pkg;
pub(crate) mod profile;
pub(crate) mod query;

use crate::args::{
    AppArgs, GenKind, GenPolicyArgs, GenProfileArgs, GenQueryArgs, GenScriptsArgs,
    GenSoftwareArgs, GenTemplateArgs, NewArgs, PkgArgs, ProfileArgs, QueryArgs,
};

/// Dispatch a `flint gen` invocation. Each kind maps onto the existing
/// generator implementation by synthesizing its legacy argument struct —
/// output stays byte-identical to the old commands by construction.
pub(crate) fn run(what: GenKind) -> anyhow::Result<()> {
    match what {
        GenKind::Software(a) => run_software(a),
        GenKind::Policy(a) => run_policy(a),
        GenKind::Profile(a) => run_profile(a),
        GenKind::Query(a) => run_query(a),
        GenKind::Scripts(a) => run_scripts(a),
        GenKind::Fleet(a) => run_template("fleet", a),
        GenKind::Label(a) => run_template("label", a),
    }
}

/// Lowercased extension of a path ("" when none).
fn ext_of(p: &std::path::Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default()
}

fn run_software(a: GenSoftwareArgs) -> anyhow::Result<()> {
    if ext_of(&a.from) == "pkg" || a.from.is_dir() {
        // The .pkg reader owns the rich modes (--standalone batches over a
        // directory of .pkgs, hence the is_dir() route).
        pkg::run(PkgArgs {
            path: a.from,
            output: a.output,
            full: a.full,
            wire: a.wire,
            update: a.update,
            scripts: None,
            policy: false,
            apps: false,
            enforce: false,
            outdated_only: false,
            file: None,
            setup_experience: a.setup_experience,
            yml: a.standalone,
        })
    } else {
        // Multi-format reader. The .pkg-only modes have no meaning here.
        if a.standalone || a.update.is_some() || a.setup_experience {
            anyhow::bail!(
                "--standalone/--update/--setup-experience apply to .pkg sources only (got {})",
                a.from.display()
            );
        }
        app::run(AppArgs {
            path: a.from,
            output: a.output,
            full: a.full,
            wire: a.wire,
        })
    }
}

fn run_policy(a: GenPolicyArgs) -> anyhow::Result<()> {
    match a.from {
        None => new::run(NewArgs {
            kind: "policy".to_string(),
            output: a.output,
        }),
        Some(from) => match ext_of(&from).as_str() {
            "pkg" => pkg::run(PkgArgs {
                path: from,
                output: a.output,
                full: false,
                wire: false,
                update: None,
                scripts: None,
                policy: true,
                apps: a.apps,
                enforce: a.enforce,
                outdated_only: a.outdated_only,
                file: a.file,
                setup_experience: false,
                yml: false,
            }),
            "sql" => {
                if a.apps || a.enforce || a.outdated_only || a.file.is_some() {
                    anyhow::bail!(
                        "--apps/--enforce/--outdated-only/--file apply to .pkg sources only"
                    );
                }
                query::run(QueryArgs {
                    path: from,
                    policy: true,
                    output: a.output,
                })
            }
            other => anyhow::bail!(
                "gen policy takes a .pkg or .sql source (got .{other}); \
                 omit --from for a blank template"
            ),
        },
    }
}

fn run_profile(a: GenProfileArgs) -> anyhow::Result<()> {
    match a.from {
        None => new::run(NewArgs {
            kind: "profile".to_string(),
            output: a.output,
        }),
        Some(from) => {
            // Validate the payload itself before emitting an entry that wires
            // it into fleets. flint only checks the Fleet side (referenced,
            // wired, unique UUID); `contour` checks it against Apple's schema.
            //
            // Optional: with contour absent this is a no-op and the generated
            // YAML is unchanged. Findings go to STDERR precisely so stdout
            // stays byte-identical — `gen profile … > entry.yml` must keep
            // producing a clean file, and gen_surface asserts that identity
            // against the legacy command.
            report_profile_validation(&from);
            profile::run(ProfileArgs {
                path: from,
                output: a.output,
                full: a.full,
                regen_uuid: a.regen_uuid,
                wire: a.wire,
            })
        }
    }
}

/// Print contour's verdict on `path` to stderr, if contour is installed.
///
/// Silent when contour is absent, when it cannot run, or when the profile is
/// clean. Deliberately non-fatal: a schema complaint is worth seeing, but it
/// is not a reason to refuse to generate the YAML — the author may be mid-edit
/// and Fleet will accept the file regardless.
fn report_profile_validation(path: &std::path::Path) {
    use colored::Colorize;
    let Some(report) = flint_lint::contour::validate(path) else {
        return; // not installed, or not checked — say nothing either way
    };
    if !report.has_findings() {
        return;
    }
    eprintln!(
        "{} contour validated {}:",
        "note:".bold(),
        path.display()
    );
    for e in &report.errors {
        eprintln!("  {} {e}", "error".red().bold());
    }
    for w in &report.warnings {
        eprintln!("  {} {w}", "warning".yellow().bold());
    }
    eprintln!(
        "  {}",
        "(Apple-schema findings from `contour`; flint checks the Fleet side only)".dimmed()
    );
}

fn run_query(a: GenQueryArgs) -> anyhow::Result<()> {
    match a.from {
        None => new::run(NewArgs {
            kind: "query".to_string(),
            output: a.output,
        }),
        Some(from) => query::run(QueryArgs {
            path: from,
            policy: false,
            output: a.output,
        }),
    }
}

fn run_scripts(a: GenScriptsArgs) -> anyhow::Result<()> {
    pkg::run(PkgArgs {
        path: a.from,
        output: None,
        full: false,
        wire: false,
        update: None,
        scripts: Some(a.output),
        policy: false,
        apps: false,
        enforce: false,
        outdated_only: false,
        file: None,
        setup_experience: false,
        yml: false,
    })
}

fn run_template(kind: &str, a: GenTemplateArgs) -> anyhow::Result<()> {
    new::run(NewArgs {
        kind: kind.to_string(),
        output: a.output,
    })
}

/// Insert an active `setup_experience: true` into a software stanza, as a
/// sibling of `self_service:`/`hash_sha256:` (matching indentation). Used when
/// the user opts a package into install-during-Setup-Assistant.
pub(crate) fn with_setup_experience(stanza: &str) -> String {
    let mut out = Vec::new();
    let mut inserted = false;
    for line in stanza.lines() {
        out.push(line.to_string());
        if !inserted {
            let t = line.trim_start();
            if t.starts_with("self_service:") || t.starts_with("hash_sha256:") {
                let indent: String = line.chars().take_while(|c| *c == ' ').collect();
                out.push(format!("{indent}setup_experience: true"));
                inserted = true;
            }
        }
    }
    out.join("\n")
}

/// Derive a GitOps-style file slug from an installer's file name:
/// version-ish suffixes stripped, lowercased, non-alphanumerics collapsed to
/// single dashes. `YSoft SafeQ Client v3.0.pkg` → `ysoft-safeq-client`.
/// Used for standalone software files (`<slug>.package.yml`).
pub(crate) fn package_slug(pkg_path: &std::path::Path) -> String {
    let stem = pkg_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("software");

    // Strip trailing version tokens (` v3.0`, `-3.0.1`, `_2024.1`, `(17)` …),
    // repeatedly in case of `name 1.2 (3)`-style tails.
    let mut name = stem.trim();
    loop {
        // Parenthesized build number: `name (17)` → `name`.
        if let Some(stripped) = name.strip_suffix(')') {
            if let Some(open) = stripped.rfind('(') {
                let inner = &stripped[open + 1..];
                if !inner.is_empty()
                    && inner.chars().all(|c| c.is_ascii_digit() || c == '.')
                {
                    name = stripped[..open].trim_end();
                    continue;
                }
            }
        }
        let trimmed = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.');
        // Did we remove a version-looking tail? It must be preceded by a
        // separator (space/dash/underscore) or a lone 'v'.
        if trimmed.len() < name.len() {
            let tail_ok = trimmed.ends_with(['v', 'V'])
                && trimmed[..trimmed.len() - 1].ends_with([' ', '-', '_']);
            let sep_ok = trimmed.ends_with([' ', '-', '_']);
            if sep_ok {
                name = trimmed.trim_end_matches([' ', '-', '_']).trim_end();
                continue;
            }
            if tail_ok {
                name = trimmed[..trimmed.len() - 1]
                    .trim_end_matches([' ', '-', '_'])
                    .trim_end();
                continue;
            }
        }
        break;
    }

    // Slugify: lowercase, alphanumerics kept, everything else → dash.
    let mut slug = String::with_capacity(name.len());
    let mut prev_dash = true; // suppress leading dash
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    if slug.is_empty() {
        "software".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::package_slug;
    use std::path::Path;

    #[test]
    fn slug_strips_versions_and_slugifies() {
        for (input, want) in [
            ("YSoft SafeQ Client v3.0.pkg", "ysoft-safeq-client"),
            ("Google Chrome.pkg", "google-chrome"),
            ("firefox-128.0.1.pkg", "firefox"),
            ("Zoom_6.1.pkg", "zoom"),
            ("1Password 8.pkg", "1password"),
            ("Some App 2.4 (17).pkg", "some-app"),
            ("weird---name.pkg", "weird-name"),
            ("v2.pkg", "v2"), // bare version-y stem stays (never empty)
        ] {
            assert_eq!(package_slug(Path::new(input)), want, "for {input}");
        }
    }
}
