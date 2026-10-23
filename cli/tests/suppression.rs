//! `# flint: ignore` end to end: unused comments are reported and `--fix`
//! drops them; `--add-ignore` baselines today's findings without changing
//! the YAML's data and is a no-op the second time.

use std::fs;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_flint");

fn repo(fleet: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("default.yml"), "org_settings:\n  features:\n    enable_host_users: true\n").unwrap();
    fs::create_dir(dir.path().join("fleets")).unwrap();
    fs::write(dir.path().join("fleets/a.yml"), fleet).unwrap();
    dir
}

fn run(dir: &std::path::Path, args: &[&str]) -> Vec<(String, Option<u64>)> {
    let out = Command::new(BIN).args(["check", "."]).args(args).args(["--format", "json"]).current_dir(dir).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    json["files"].as_array().unwrap().iter().flat_map(|f| f["diagnostics"].as_array().unwrap().clone())
        .map(|d| (d["rule"].as_str().unwrap().to_string(), d["line"].as_u64()))
        .collect()
}

#[test]
fn unused_ignore_comments_are_reported_and_fixed() {
    let dir = repo("name: A\nbogus: 1  # flint: ignore [structural-validation, query-syntax]\n# flint: ignore [query-syntax]\nnote: x  # flint: ignore\n");
    let unused: Vec<_> = run(dir.path(), &[]).into_iter().filter(|(r, _)| r == "unused-ignore-comment").map(|(_, l)| l).collect();
    assert_eq!(unused, vec![Some(2), Some(3)], "the used blanket comment on line 4 is not reported");
    run(dir.path(), &["--fix"]);
    assert_eq!(
        fs::read_to_string(dir.path().join("fleets/a.yml")).unwrap(),
        "name: A\nbogus: 1  # flint: ignore [structural-validation]\nnote: x  # flint: ignore\n"
    );
}

#[test]
fn add_ignore_baselines_without_changing_data() {
    let fleet = "name: A\nbogus: 1\nreports:\n  - name: q\n    query:\n      SELECT\n        *\n      FROM\n        osquery_info\n    interval: 5\n";
    let dir = repo(fleet);
    assert!(run(dir.path(), &[]).iter().any(|(r, _)| r == "structural-validation"));
    run(dir.path(), &["--add-ignore"]);
    let after = fs::read_to_string(dir.path().join("fleets/a.yml")).unwrap();
    let parse = |s: &str| serde_yaml::from_str::<serde_yaml::Value>(s).unwrap();
    assert_eq!(parse(&after), parse(fleet), "only comments may change:\n{after}");
    let left = run(dir.path(), &[]);
    assert!(left.iter().all(|(_, line)| line.is_none() || after.lines().nth(line.unwrap() as usize - 1).unwrap().starts_with("      ")),
        "only lines inside the multi-line query may stay unannotated: {left:?}");
    run(dir.path(), &["--add-ignore"]);
    assert_eq!(fs::read_to_string(dir.path().join("fleets/a.yml")).unwrap(), after, "second run is a no-op");
}
