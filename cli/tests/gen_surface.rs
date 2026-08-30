//! `flint gen` surface. Through v0.2.x every legacy generator invocation
//! (`pkg`, `app`, `profile`, `query`, `new`) forwarded here and had to match
//! its `gen` replacement byte for byte; v0.3.0 removed the legacy spellings.
//! These tests now pin two things: the canonical `gen` forms work, and the
//! old spellings fail LOUDLY — an unrecognized subcommand, never a silent
//! no-op — so a script still using one breaks at its first run.

#![expect(clippy::print_stderr, reason = "a test explaining why it skipped is worth more than a silent pass")]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_flint");

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("flint-gen-{}-{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().expect("run flint")
}

/// The canonical form succeeds, prints something, and carries no deprecation
/// noise — there is nothing left to be deprecated.
fn assert_gen_works(args: &[&str]) {
    let out = run(args);
    assert!(out.status.success(), "{args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    assert!(!out.stdout.is_empty(), "{args:?} printed nothing");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("deprecated"),
        "{args:?} must not warn"
    );
}

/// A removed spelling is rejected by the parser, not forwarded.
fn assert_legacy_rejected(args: &[&str]) {
    let out = run(args);
    assert!(!out.status.success(), "removed spelling {args:?} must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("unrecognized subcommand") || err.contains("unexpected argument"),
        "removed spelling {args:?} must be rejected by clap, got: {err}"
    );
    assert!(out.stdout.is_empty(), "removed spelling {args:?} must produce no output");
}

#[test]
fn templates_via_gen_only() {
    for kind in ["policy", "query", "profile", "fleet", "label"] {
        assert_gen_works(&["gen", kind]);
        assert_legacy_rejected(&["new", kind]);
    }
}

#[test]
fn query_and_policy_from_sql_via_gen_only() {
    let dir = scratch("sql");
    let sql = dir.join("check.sql");
    std::fs::write(&sql, "SELECT 1 FROM apps WHERE bundle_identifier = 'x';\n").unwrap();
    let p = sql.to_str().unwrap();

    assert_gen_works(&["gen", "query", "--from", p]);
    assert_gen_works(&["gen", "policy", "--from", p]);
    assert_legacy_rejected(&["query", p]);
    assert_legacy_rejected(&["query", p, "--policy"]);

    std::fs::remove_dir_all(&dir).ok();
}

/// Build a minimal real .pkg via `pkgbuild`; None when unavailable (non-mac).
fn fixture_pkg(dir: &Path) -> Option<PathBuf> {
    let root = dir.join("root/usr/local/bin");
    std::fs::create_dir_all(&root).ok()?;
    std::fs::write(root.join("hello"), "#!/bin/sh\n").ok()?;
    let pkg = dir.join("hello.pkg");
    let ok = Command::new("pkgbuild")
        .args(["--root"])
        .arg(dir.join("root"))
        .args(["--identifier", "com.example.hello", "--version", "1.0"])
        .arg(&pkg)
        .output()
        .ok()?
        .status
        .success();
    ok.then_some(pkg)
}

#[test]
fn software_and_policy_from_pkg_via_gen_only() {
    let dir = scratch("pkg");
    let Some(pkg) = fixture_pkg(&dir) else {
        eprintln!("skipping: pkgbuild unavailable");
        return;
    };
    let p = pkg.to_str().unwrap();

    assert_gen_works(&["gen", "software", "--from", p]);
    assert_gen_works(&["gen", "software", "--from", p, "--full"]);
    assert_gen_works(&["gen", "policy", "--from", p]);
    assert_gen_works(&["gen", "policy", "--from", p, "--enforce"]);
    assert_legacy_rejected(&["pkg", p]);
    assert_legacy_rejected(&["app", p]);
    assert_legacy_rejected(&["profile", p]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scripts_from_pkg_via_gen_only() {
    let dir = scratch("scripts");
    let Some(pkg) = fixture_pkg(&dir) else {
        eprintln!("skipping: pkgbuild unavailable");
        return;
    };
    let p = pkg.to_str().unwrap();
    let out_new = dir.join("new");

    let n = run(&["gen", "scripts", "--from", p, "-o", out_new.to_str().unwrap()]);
    assert!(n.status.success());
    for f in ["install.sh", "uninstall.sh"] {
        assert!(out_new.join(f).is_file(), "{f} not generated");
    }
    assert_legacy_rejected(&["pkg", p, "--scripts", dir.join("old").to_str().unwrap()]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn standalone_names_file_by_slug_with_fleet_path_hint() {
    let dir = scratch("standalone");
    // GitOps-shaped repo so the path hint resolves relative to the root.
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    let swdir = dir.join("platforms/macos/site/hello/software");
    std::fs::create_dir_all(&swdir).unwrap();
    let Some(pkg) = fixture_pkg(&dir) else {
        eprintln!("skipping: pkgbuild unavailable");
        return;
    };

    let out = run(&[
        "gen", "software", "--from", pkg.to_str().unwrap(),
        "--standalone", "-o", swdir.to_str().unwrap(),
    ]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    // Named <slug>.package.yml — from fixture "hello.pkg" → hello.package.yml
    let dest = swdir.join("hello.package.yml");
    assert!(dest.exists(), "expected {}, stdout: {stdout}", dest.display());
    // Copy-pasteable fleet-file reference, relative to the repo root.
    assert!(
        stdout.contains("- path: ../platforms/macos/site/hello/software/hello.package.yml"),
        "missing path hint, stdout: {stdout}"
    );

    // Collision → -2 suffix, never overwrite.
    let out2 = run(&[
        "gen", "software", "--from", pkg.to_str().unwrap(),
        "--standalone", "-o", swdir.to_str().unwrap(),
    ]);
    assert!(out2.status.success());
    assert!(swdir.join("hello-2.package.yml").exists());

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn gen_policy_rejects_unknown_source_kind() {
    let out = run(&["gen", "policy", "--from", "thing.dmg"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains(".pkg or .sql"), "got: {err}");
}

/// `--help` never advertised the legacy spellings; now they are not merely
/// hidden but absent, and `setup-agent` is the only way to install the skill.
#[test]
fn legacy_spellings_are_gone_not_hidden() {
    let help = String::from_utf8_lossy(&run(&["--help"]).stdout).to_string();
    assert!(help.contains("\n  gen"), "gen must be visible");
    for legacy in ["\n  pkg", "\n  app", "\n  profile", "\n  query", "\n  new"] {
        assert!(!help.contains(legacy), "{legacy:?} must not appear in --help");
    }
    assert_legacy_rejected(&["help-agents", "--install-skill"]);
    assert!(help.contains("setup-agent"), "the replacement must be advertised");
}
