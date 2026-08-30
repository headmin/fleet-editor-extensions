//! v0.2.0 `flint gen` surface: every legacy generator invocation and its
//! canonical `gen` replacement must produce byte-identical stdout; the legacy
//! form warns on stderr, the canonical form does not.

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

/// Mask v4 UUIDs — the blank profile template legitimately mints a fresh
/// PayloadUUID per invocation, so identity is asserted modulo UUIDs.
fn mask_uuids(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let rest = &s[i..];
        // .get() (not slicing) so a multibyte char inside the window simply
        // fails the check instead of panicking on a non-char boundary.
        let looks_like_uuid = rest.get(..36).is_some_and(|w| {
            w.chars().enumerate().all(|(j, c)| match j {
                8 | 13 | 18 | 23 => c == '-',
                _ => c.is_ascii_hexdigit(),
            })
        });
        if looks_like_uuid {
            out.push_str("<UUID>");
            i += 36;
        } else {
            out.push(s[i..].chars().next().unwrap());
            i += s[i..].chars().next().unwrap().len_utf8();
        }
    }
    out
}

/// Assert `old` and `new` invocations emit identical stdout (modulo minted
/// UUIDs), and that only the old one warns (deprecation on stderr).
fn assert_identical(old: &[&str], new: &[&str]) {
    let o = run(old);
    let n = run(new);
    assert_eq!(
        mask_uuids(&String::from_utf8_lossy(&o.stdout)),
        mask_uuids(&String::from_utf8_lossy(&n.stdout)),
        "stdout differs: {old:?} vs {new:?}"
    );
    assert_eq!(o.status.code(), n.status.code(), "exit codes differ");
    let old_err = String::from_utf8_lossy(&o.stderr);
    let new_err = String::from_utf8_lossy(&n.stderr);
    assert!(
        old_err.contains("deprecated"),
        "legacy {old:?} must warn on stderr, got: {old_err}"
    );
    assert!(
        !new_err.contains("deprecated"),
        "canonical {new:?} must not warn, got: {new_err}"
    );
}

#[test]
fn templates_match_legacy_new() {
    for kind in ["policy", "query", "profile", "fleet", "label"] {
        assert_identical(&["new", kind], &["gen", kind]);
    }
}

#[test]
fn query_and_policy_from_sql_match_legacy() {
    let dir = scratch("sql");
    let sql = dir.join("check.sql");
    std::fs::write(&sql, "SELECT 1 FROM apps WHERE bundle_identifier = 'x';\n").unwrap();
    let p = sql.to_str().unwrap();

    assert_identical(&["query", p], &["gen", "query", "--from", p]);
    assert_identical(&["query", p, "--policy"], &["gen", "policy", "--from", p]);

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
fn software_and_policy_from_pkg_match_legacy() {
    let dir = scratch("pkg");
    let Some(pkg) = fixture_pkg(&dir) else {
        eprintln!("skipping: pkgbuild unavailable");
        return;
    };
    let p = pkg.to_str().unwrap();

    assert_identical(&["pkg", p], &["gen", "software", "--from", p]);
    assert_identical(&["pkg", p, "--full"], &["gen", "software", "--from", p, "--full"]);
    assert_identical(&["pkg", p, "--policy"], &["gen", "policy", "--from", p]);
    assert_identical(
        &["pkg", p, "--policy", "--enforce"],
        &["gen", "policy", "--from", p, "--enforce"],
    );
    // Multi-format reader must accept .pkg too (dispatch parity with `app`).
    assert_identical(&["app", p], &["gen", "software", "--from", p]);

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn scripts_from_pkg_match_legacy() {
    let dir = scratch("scripts");
    let Some(pkg) = fixture_pkg(&dir) else {
        eprintln!("skipping: pkgbuild unavailable");
        return;
    };
    let p = pkg.to_str().unwrap();
    let out_old = dir.join("old");
    let out_new = dir.join("new");

    let o = run(&["pkg", p, "--scripts", out_old.to_str().unwrap()]);
    let n = run(&["gen", "scripts", "--from", p, "-o", out_new.to_str().unwrap()]);
    assert!(o.status.success() && n.status.success());
    for f in ["install.sh", "uninstall.sh"] {
        assert_eq!(
            std::fs::read_to_string(out_old.join(f)).unwrap(),
            std::fs::read_to_string(out_new.join(f)).unwrap(),
            "{f} differs between legacy and gen"
        );
    }

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

#[test]
fn legacy_commands_hidden_from_help() {
    let out = run(&["--help"]);
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(help.contains("\n  gen"), "gen must be visible");
    for legacy in ["\n  pkg", "\n  app", "\n  profile", "\n  query", "\n  new"] {
        assert!(
            !help.contains(legacy),
            "legacy command {legacy:?} must be hidden from --help"
        );
    }
}
