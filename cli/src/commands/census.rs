//! `flint census` — line fleets up side by side and show where they differ.
//!
//! A report, not a rule: per-fleet values are usually deliberate, so nothing
//! here is a finding. With `--anchor` one fleet becomes the reference and
//! every row names the fleets that deviate from it — including a fleet that
//! no longer sets the key at all, which is how a setting quietly goes missing.

use crate::args::{CensusArgs, CensusFormat};
use anyhow::{bail, Context, Result};
use colored::Colorize;
use serde_yaml::Value;
use std::collections::{BTreeMap, BTreeSet};

/// key path -> fleet -> value. A fleet absent from the inner map is unset.
type Grid = BTreeMap<String, BTreeMap<String, String>>;

pub(crate) fn run(args: CensusArgs) -> Result<()> {
    let fleets = load_fleets(&args.path)?;
    if fleets.is_empty() {
        bail!(
            "no fleet files under {}/fleets or {}/teams",
            args.path.display(),
            args.path.display()
        );
    }
    // A fleet is named by its `name:` or its file stem, case-insensitively.
    let resolve = |a: &str| -> Result<String> {
        fleets
            .iter()
            .find(|(n, stem, _)| n.eq_ignore_ascii_case(a) || stem.eq_ignore_ascii_case(a))
            .map(|(n, _, _)| n.clone())
            .with_context(|| {
                let all: Vec<&str> = fleets.iter().map(|(n, _, _)| n.as_str()).collect();
                format!("no fleet named '{a}' — fleets: {}", all.join(", "))
            })
    };
    let anchor = args.anchor.as_deref().map(resolve).transpose()?;
    let picked = args
        .fleet
        .iter()
        .map(|f| resolve(f))
        .collect::<Result<BTreeSet<_>>>()?;
    if !picked.is_empty() && anchor.is_none() && picked.len() < 2 {
        bail!("--fleet compares against --anchor; give an anchor or at least two fleets");
    }
    // --fleet narrows the census to the anchor plus the picked fleets, so
    // every view (table, tally, CSV, JSON) only carries those columns.
    let mut fleets = fleets;
    if !picked.is_empty() {
        fleets.retain(|(n, _, _)| picked.contains(n) || anchor.as_ref() == Some(n));
    }
    let names: Vec<String> = fleets.iter().map(|(n, _, _)| n.clone()).collect();

    let mut grid = Grid::new();
    for (name, _, doc) in &fleets {
        let mut flat = Vec::new();
        flatten("", doc, &mut flat);
        for (k, v) in flat {
            grid.entry(k).or_default().insert(name.clone(), v);
        }
    }
    let wanted = |k: &str| args.key.is_empty() || args.key.iter().any(|p| k.contains(p.as_str()));
    let keep: BTreeSet<String> = grid
        .iter()
        .filter(|(k, row)| {
            wanted(k) && (args.all || differs(row, &in_scope(&grid, k, &names), anchor.as_deref()))
        })
        .map(|(k, _)| k.clone())
        .collect();
    // Scope is computed on the full grid above: a child row needs its
    // parent entry's row even when the filter drops the parent.
    let full = grid.clone();
    grid.retain(|k, _| keep.contains(k));
    let unset: Vec<&str> = if args.unset {
        PER_FLEET_SETTINGS
            .iter()
            .copied()
            .filter(|k| {
                // `settings.secrets` is set as `settings.secrets[#0]…`.
                let set = full.keys().any(|f| {
                    f.strip_prefix(*k)
                        .is_some_and(|rest| rest.is_empty() || rest.starts_with(['.', '[']))
                });
                wanted(k) && !set
            })
            .collect()
    } else {
        Vec::new()
    };

    // Anchor first, so a CSV opens with the reference column next to the key.
    let mut cols = names.clone();
    if let Some(a) = &anchor {
        cols.retain(|n| n != a);
        cols.insert(0, a.clone());
    }

    match args.format {
        CensusFormat::Table => {
            print_table(&grid, &full, &cols, anchor.as_deref());
            if !unset.is_empty() {
                println!("{}", "Per-fleet settings no fleet sets:".bold());
                unset.iter().for_each(|k| println!("  {k}"));
            }
        }
        CensusFormat::Csv => print!("{}", to_csv(&grid, &cols)),
        CensusFormat::Json => println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "anchor": anchor,
                "fleets": cols,
                "keys": grid,
                "unset": unset,
            }))?
        ),
    }
    Ok(())
}

/// `(display name, file stem, parsed doc)` for every fleet file, sorted by
/// name. The display name is the fleet's `name:`, falling back to the stem.
fn load_fleets(root: &std::path::Path) -> Result<Vec<(String, String, Value)>> {
    let mut out = Vec::new();
    for dir in ["fleets", "teams"] {
        let Ok(entries) = std::fs::read_dir(root.join(dir)) else {
            continue;
        };
        for e in entries.flatten() {
            let p = e.path();
            if !matches!(p.extension().and_then(|x| x.to_str()), Some("yml" | "yaml")) {
                continue;
            }
            let raw = std::fs::read_to_string(&p)?;
            let Ok(mut doc) = serde_yaml::from_str::<Value>(&raw) else {
                eprintln!("{} skipping {} (not valid YAML)", "!".yellow(), p.display());
                continue;
            };
            let stem = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            let name = doc
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| stem.clone());
            if let Value::Mapping(m) = &mut doc {
                m.remove("name");
            }
            out.push((name, stem, doc));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// Dotted path -> rendered value. A list of scalars is joined. A list of
/// entries (profiles, software, policies) is keyed by each entry's identity,
/// so `software.fleet_maintained_apps[slack/darwin]` is `present` and its
/// `.setup_experience` is a row of its own — a count would hide both.
fn flatten(prefix: &str, v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Mapping(m) => {
            for (k, child) in m {
                let k = match k {
                    Value::String(s) => s.clone(),
                    other => scalar(other),
                };
                let path = if prefix.is_empty() {
                    k
                } else {
                    format!("{prefix}.{k}")
                };
                flatten(&path, child, out);
            }
        }
        Value::Sequence(s)
            if s.iter()
                .all(|x| !matches!(x, Value::Mapping(_) | Value::Sequence(_))) =>
        {
            out.push((
                prefix.to_string(),
                s.iter().map(scalar).collect::<Vec<_>>().join(", "),
            ));
        }
        Value::Sequence(s) => {
            for (i, item) in s.iter().enumerate() {
                let Value::Mapping(m) = item else {
                    out.push((format!("{prefix}[#{i}]"), scalar(item)));
                    continue;
                };
                let id = IDENTITY.iter().find_map(|k| {
                    m.get(*k)
                        .filter(|v| !v.is_mapping() && !v.is_sequence())
                        .map(|v| (*k, scalar(v)))
                });
                let entry = match &id {
                    Some((_, v)) => format!("{prefix}[{v}]"),
                    None => format!("{prefix}[#{i}]"),
                };
                out.push((entry.clone(), "present".to_string()));
                for (k, child) in m {
                    if id.as_ref().is_some_and(|(ik, _)| k.as_str() == Some(*ik)) {
                        continue;
                    }
                    flatten(
                        &format!(
                            "{entry}.{}",
                            k.as_str().map_or_else(|| scalar(k), str::to_string)
                        ),
                        child,
                        out,
                    );
                }
            }
        }
        Value::Tagged(t) => flatten(prefix, &t.value, out),
        other => {
            let field = prefix.rsplit('.').next().unwrap_or(prefix);
            let v = if SENSITIVE.iter().any(|s| field.contains(s)) {
                hashed(&scalar(other))
            } else {
                scalar(other)
            };
            out.push((prefix.to_string(), v));
        }
    }
}

/// Field names whose values are credentials (enroll `secret`, `api_key_json`,
/// `client_secret`, …). Shown only as a hash, like a bootstrap token.
const SENSITIVE: &[&str] = &["secret", "token", "password", "api_key"];

fn scalar(v: &Value) -> String {
    let s = match v {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        _ => serde_yaml::to_string(v)
            .unwrap_or_default()
            .trim()
            .to_string(),
    };
    redact(&s)
}

/// A bootstrap package URL carries a download token — a credential. Replace
/// it with a short hash: the value stays hidden, but two fleets sharing one
/// package (the `bootstrap-package-shared` failure) still show the same hash.
fn redact(s: &str) -> String {
    let Some(at) = s.find("token=") else {
        return s.to_string();
    };
    let start = at + "token=".len();
    let end = s[start..].find('&').map_or(s.len(), |i| start + i);
    format!("{}{}{}", &s[..start], hashed(&s[start..end]), &s[end..])
}

/// `#xxxx`: 16 bits of a hash — enough to tell two values apart or see that
/// two fleets share one, far too little to recover anything.
fn hashed(s: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("#{:04x}", h.finish() & 0xffff)
}

/// With an anchor: any fleet whose value (or absence) differs from the
/// anchor's. Without: the fleets do not all agree. An anchor outside `names`
/// (it lacks the parent entry) makes the row incomparable, not divergent.
fn differs(row: &BTreeMap<String, String>, names: &[&String], anchor: Option<&str>) -> bool {
    match anchor {
        Some(a) if !names.iter().any(|n| *n == a) => false,
        Some(a) => names.iter().any(|n| row.get(*n) != row.get(a)),
        None => {
            names
                .iter()
                .map(|n| row.get(*n))
                .collect::<BTreeSet<_>>()
                .len()
                > 1
        }
    }
}

/// Fields used as a list entry's identity, most specific first.
const IDENTITY: &[&str] = &[
    "slug",
    "app_store_id",
    "path",
    "paths",
    "name",
    "hash_sha256",
    "url",
];

/// The fleets a row applies to. A field of a list entry only means something
/// in fleets that HAVE the entry — an app missing from a fleet is one
/// deviation (`[slug]` absent), not one more per field it would have had.
fn in_scope<'a>(grid: &Grid, key: &str, cols: &'a [String]) -> Vec<&'a String> {
    match key.rfind("].") {
        Some(at) => match grid.get(&key[..=at]) {
            Some(parent) => cols.iter().filter(|c| parent.contains_key(*c)).collect(),
            None => Vec::new(),
        },
        None => cols.iter().collect(),
    }
}

/// Per-fleet settings Fleet accepts in a fleet file. `--unset` lists those no
/// fleet sets: a gap rather than a deviation, so it is opt-in.
/// ponytail: hand-kept list, not derived from KEY_REGISTRY (which also holds
/// global-only keys) — refresh it when the oracle pin moves.
const PER_FLEET_SETTINGS: &[&str] = &[
    "controls.enable_disk_encryption",
    "controls.enable_recovery_lock_password",
    "controls.windows_require_bitlocker_pin",
    "controls.macos_updates.minimum_version",
    "controls.macos_updates.deadline",
    "controls.macos_updates.deadline_days",
    "controls.ios_updates.minimum_version",
    "controls.ipados_updates.minimum_version",
    "controls.windows_updates.deadline_days",
    "controls.windows_updates.grace_period_days",
    "controls.setup_experience.apple_setup_assistant",
    "controls.setup_experience.macos_bootstrap_package",
    "controls.setup_experience.macos_script",
    "controls.setup_experience.macos_manual_agent_install",
    "controls.setup_experience.enable_end_user_authentication",
    "controls.setup_experience.lock_end_user_info",
    "controls.setup_experience.enable_create_local_admin_account",
    "controls.setup_experience.end_user_local_account_type",
    "controls.setup_experience.require_all_software_macos",
    "controls.setup_experience.require_all_software_windows",
    "controls.setup_experience.apple_enable_release_device_manually",
    "controls.windows_settings.enable_disk_encryption",
    "controls.windows_settings.enable_managed_local_account",
    "settings.secrets",
    "settings.host_expiry_settings.host_expiry_enabled",
    "settings.features.enable_software_inventory",
    "settings.features.enable_host_users",
    "settings.webhook_settings.failing_policies_webhook.enable_failing_policies_webhook",
    "settings.webhook_settings.host_activities_webhook.enable_host_activities_webhook",
    "settings.integrations.conditional_access_enabled",
];

const UNSET: &str = "—";

fn print_table(grid: &Grid, full: &Grid, cols: &[String], anchor: Option<&str>) {
    if grid.is_empty() {
        println!("{} no differences", "✓".green());
        return;
    }
    for (key, row) in grid {
        // value -> fleets holding it; unset is a value too.
        let mut groups: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for c in in_scope(full, key, cols) {
            groups
                .entry(row.get(c).map_or(UNSET, String::as_str))
                .or_default()
                .push(c);
        }
        let reference = match anchor {
            Some(a) => row.get(a).map_or(UNSET, String::as_str),
            // No anchor: the most common value goes unnamed.
            None => groups
                .iter()
                .max_by_key(|(_, f)| f.len())
                .map(|(v, _)| *v)
                .unwrap_or(UNSET),
        };
        match anchor {
            Some(a) => println!(
                "{}  {} {}",
                key.bold(),
                format!("{a}:").dimmed(),
                reference.cyan()
            ),
            None => println!("{}", key.bold()),
        }
        let width = groups
            .keys()
            .map(|v| v.chars().count())
            .max()
            .unwrap_or(0)
            .min(60);
        for (v, fleets) in &groups {
            if *v == reference {
                if anchor.is_none() {
                    println!("  {v:<width$}  {:>3}", fleets.len());
                }
                continue;
            }
            println!(
                "  {:<width$}  {:>3}  {}",
                v.yellow(),
                fleets.len(),
                fleets.join(", ")
            );
        }
        println!();
    }
    if let Some(a) = anchor {
        // Per-fleet tally: who drifted furthest from the anchor.
        let mut tally: Vec<(usize, &String)> = cols[1..]
            .iter()
            .map(|c| {
                let n = grid
                    .iter()
                    .filter(|(k, r)| {
                        let scope = in_scope(full, k, cols);
                        scope.contains(&c) && scope.iter().any(|s| *s == a) && r.get(c) != r.get(a)
                    })
                    .count();
                (n, c)
            })
            .filter(|(n, _)| *n > 0)
            .collect();
        tally.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(y.1)));
        println!("{}", format!("Deviations from {a}:").bold());
        for (n, c) in tally {
            println!("  {n:>4}  {c}");
        }
    }
}

fn to_csv(grid: &Grid, cols: &[String]) -> String {
    fn field(s: &str) -> String {
        if s.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", s.replace('"', "\"\""))
        } else {
            s.to_string()
        }
    }
    let mut out = String::from("key");
    for c in cols {
        out.push(',');
        out.push_str(&field(c));
    }
    out.push('\n');
    for (key, row) in grid {
        out.push_str(&field(key));
        for c in cols {
            out.push(',');
            out.push_str(&field(row.get(c).map_or("", String::as_str)));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_redact_differs_csv() {
        let doc: Value = serde_yaml::from_str(
            "controls:\n  setup_experience:\n    enable_end_user_authentication: true\n    \
             macos_bootstrap_package: https://h/b?token=SECRET&x=1\n  scripts: [a, b]\n\
             software:\n  packages:\n    - path: x\n    - path: y\n      self_service: true\nsettings:\n  secrets:\n    - secret: S3CRET\n",
        )
        .unwrap();
        let mut flat = Vec::new();
        flatten("", &doc, &mut flat);
        let m: BTreeMap<_, _> = flat.into_iter().collect();
        assert_eq!(
            m["controls.setup_experience.enable_end_user_authentication"],
            "true"
        );
        assert_eq!(m["controls.scripts"], "a, b");
        assert_eq!(m["software.packages[x]"], "present");
        assert!(
            m["settings.secrets[#0].secret"].starts_with('#'),
            "enroll secret is hashed"
        );
        assert_eq!(m["software.packages[y].self_service"], "true");
        let url = &m["controls.setup_experience.macos_bootstrap_package"];
        assert!(
            !url.contains("SECRET")
                && url.starts_with("https://h/b?token=#")
                && url.ends_with("&x=1")
        );
        assert_eq!(
            redact("u?token=SECRET"),
            redact("v?token=SECRET").replacen('v', "u", 1)
        );

        let names = vec!["A".to_string(), "B".to_string()];
        let all: Vec<&String> = names.iter().collect();
        let mut row = BTreeMap::from([("A".to_string(), "true".to_string())]);
        assert!(
            differs(&row, &all, Some("A")),
            "unset in B deviates from anchor A"
        );
        assert!(differs(&row, &all, None));
        row.insert("B".into(), "true".into());
        assert!(!differs(&row, &all, Some("A")));

        // B lacks the entry: its field is out of scope, so only `[x]` deviates.
        let g = Grid::from([
            (
                "s[x]".into(),
                BTreeMap::from([("A".into(), "present".into())]),
            ),
            (
                "s[x].f".into(),
                BTreeMap::from([("A".into(), "true".into())]),
            ),
        ]);
        assert_eq!(in_scope(&g, "s[x].f", &names), vec![&names[0]]);
        assert!(!differs(
            &g["s[x].f"],
            &in_scope(&g, "s[x].f", &names),
            Some("A")
        ));
        assert!(
            !differs(
                &g["s[x].f"],
                &in_scope(&g, "s[x].f", &names[1..]),
                Some("A")
            ),
            "anchor out of scope"
        );

        let grid = Grid::from([("k".to_string(), row)]);
        assert_eq!(to_csv(&grid, &names), "key,A,B\nk,true,true\n");
        let quoted = Grid::from([(
            "k".into(),
            BTreeMap::from([("A".into(), "a, \"b\"".into())]),
        )]);
        assert_eq!(to_csv(&quoted, &names), "key,A,B\nk,\"a, \"\"b\"\"\",\n");
    }
}
