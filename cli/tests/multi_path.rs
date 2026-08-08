//! Integration tests for `flint check` with multiple path arguments — the
//! shape a pre-commit hook produces when `pass_filenames: true` appends every
//! staged file to one invocation. Also covers `[files]` exclude being honored
//! for explicitly-passed files.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_flint");

/// Unique scratch dir under the system temp, no external crates.
fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flint-it-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(path: &PathBuf, body: &str) {
    if let Some(p) = path.parent() {
        fs::create_dir_all(p).unwrap();
    }
    fs::write(path, body).unwrap();
}

const POLICY: &str = "policies:\n  - name: Example\n    query: \"SELECT 1;\"\n    platform: darwin\n";

#[test]
fn check_accepts_multiple_file_arguments() {
    let dir = scratch("multi");
    let a = dir.join("fleets/a.yml");
    let b = dir.join("fleets/b.yml");
    write(&a, POLICY);
    write(&b, POLICY);

    // The exact invocation a `flint-files` pre-commit hook makes.
    let out = Command::new(BIN)
        .args(["check", a.to_str().unwrap(), b.to_str().unwrap()])
        .arg("--format")
        .arg("json")
        .output()
        .expect("run flint");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("expected JSON, got:\n{stdout}\nerr: {e}"));

    assert_eq!(
        json["summary"]["files_linted"], 2,
        "both files should be linted, got: {stdout}"
    );
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);

    fs::remove_dir_all(&dir).ok();
}

#[test]
fn explicit_files_honor_files_exclude() {
    let dir = scratch("exclude");
    write(&dir.join(".fleetlint.toml"), "[files]\nexclude = [\"**/vendor/**\"]\n");
    let keep = dir.join("fleets/a.yml");
    let drop = dir.join("vendor/c.yml");
    write(&keep, POLICY);
    write(&drop, POLICY);

    // Even though vendor/c.yml is passed explicitly (as a hook would), the
    // `[files]` exclude must skip it — only fleets/a.yml is linted.
    let out = Command::new(BIN)
        .args(["check", keep.to_str().unwrap(), drop.to_str().unwrap()])
        .arg("--format")
        .arg("json")
        .output()
        .expect("run flint");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    assert_eq!(
        json["summary"]["files_linted"], 1,
        "vendor/ file should be excluded, got: {stdout}"
    );
    let path = json["files"][0]["path"].as_str().unwrap();
    assert!(path.ends_with("a.yml"), "wrong file linted: {path}");

    fs::remove_dir_all(&dir).ok();
}
