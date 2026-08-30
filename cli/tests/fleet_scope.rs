//! Integration tests for scoped `flint check` runs: `--fleet <glob>`, a single
//! fleet file, and a sub-directory. All three analyse from the repo root and
//! narrow the report — a single fleet file used to skip the cross-file pass
//! (false clean) and a sub-directory used to root the workspace at itself
//! (every `../platforms/…` glob "matched no files").

use std::fs;
use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_flint");

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flint-scope-{}-{}", tag, std::process::id()));
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

/// Two fleets pull the same patch policies by glob. A lists both apps; B is
/// missing zoom — the shape of the incident (a trimmed
/// `fleet_maintained_apps` list under policies that still name the apps).
fn repo(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    write(&dir.join(".fleetlint.toml"), "[rules]\n");
    write(
        &dir.join("default.yml"),
        "org_settings:\n  server_settings:\n    server_url: https://fleet.example.com\n",
    );
    write(
        &dir.join("fleets/CFG-A.yml"),
        "name: CFG - A\npolicies:\n  - paths: ../platforms/all/policies/patch/*.yml\nsoftware:\n  fleet_maintained_apps:\n    - slug: slack/darwin\n    - slug: zoom/darwin\n",
    );
    write(
        &dir.join("fleets/CFG-B.yml"),
        "name: CFG - B\npolicies:\n  - paths: ../platforms/all/policies/patch/*.yml\nsoftware:\n  fleet_maintained_apps:\n    - slug: slack/darwin\n",
    );
    write(
        &dir.join("platforms/all/policies/patch/slack.yml"),
        "- name: Slack patch\n  type: patch\n  fleet_maintained_app_slug: slack/darwin\n",
    );
    write(
        &dir.join("platforms/all/policies/patch/zoom.yml"),
        "- name: Zoom patch\n  type: patch\n  fleet_maintained_app_slug: zoom/darwin\n",
    );
    dir
}

fn run(dir: &PathBuf, args: &[&str]) -> (i32, String) {
    let out = Command::new(BIN)
        .current_dir(dir)
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run flint");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.code().unwrap_or(-1), text)
}

#[test]
fn a_single_fleet_file_is_judged_through_the_whole_repo() {
    let dir = repo("file");
    let (code, out) = run(&dir, &["check", "fleets/CFG-B.yml"]);
    assert_eq!(code, 1, "the missing app must block:\n{out}");
    assert!(out.contains("CFG - B"), "{out}");
    assert!(out.contains("zoom/darwin"), "{out}");
    assert!(!out.contains("CFG - A"), "A's findings must not leak into B's report:\n{out}");

    let (code, out) = run(&dir, &["check", "fleets/CFG-A.yml"]);
    assert_eq!(code, 0, "A lists both apps and must be clean:\n{out}");
    assert!(out.contains("No issues found"), "{out}");
}

#[test]
fn fleet_glob_scopes_the_report_and_counts_the_rest() {
    let dir = repo("glob");
    for glob in ["CFG-B", "fleets/CFG-B.yml", "CFG - B", "*-B"] {
        let (code, out) = run(&dir, &["check", ".", "--fleet", glob]);
        assert_eq!(code, 1, "--fleet {glob}:\n{out}");
        assert!(out.contains("zoom/darwin"), "--fleet {glob}:\n{out}");
        assert!(!out.contains("CFG - A"), "--fleet {glob} leaked A:\n{out}");
    }
    let (code, out) = run(&dir, &["check", ".", "--fleet", "CFG-A"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("outside --fleet 'CFG-A' not shown"), "the rest is counted, not hidden:\n{out}");
}

#[test]
fn a_fleet_glob_matching_nothing_is_an_error_not_a_clean_run() {
    let dir = repo("nomatch");
    let (code, out) = run(&dir, &["check", ".", "--fleet", "CFG-Z"]);
    assert_ne!(code, 0, "{out}");
    assert!(out.contains("no fleet file matches --fleet 'CFG-Z'"), "{out}");
}

#[test]
fn a_sub_directory_is_analysed_from_the_repo_root() {
    let dir = repo("subdir");
    let (code, out) = run(&dir, &["check", "fleets/"]);
    assert_eq!(code, 1, "{out}");
    assert!(!out.contains("matches no files"), "globs must resolve against the repo, not fleets/:\n{out}");
    assert!(out.contains("zoom/darwin"), "{out}");

    // From inside the directory, with a bare file name.
    let out = Command::new(BIN)
        .current_dir(dir.join("fleets"))
        .args(["check", "CFG-B.yml"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run flint");
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn a_policy_fragment_that_is_not_yaml_fails_the_directory_lint() {
    let dir = repo("syntax");
    write(
        &dir.join("platforms/all/policies/patch/zoom.yml"),
        "- name: Zoom patch (install_software: true)\n  type: patch\n",
    );
    let (code, out) = run(&dir, &["check", "."]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("yaml-syntax") || out.contains("YAML parse error"), "{out}");
    let (code, out) = run(&dir, &["dry-run", "."]);
    assert_eq!(code, 2, "dry-run must block on a fragment fleetctl cannot parse:\n{out}");
}
