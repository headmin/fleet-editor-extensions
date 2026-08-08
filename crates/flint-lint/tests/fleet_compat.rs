//! Fleet compatibility integration tests.
//!
//! These tests validate flint against real Fleet GitOps directory structures
//! to catch schema drift. They require either:
//! - FLEET_TEST_DIR env var pointing to a `fleetctl new` output, or
//! - A directory at ../fleet with the Fleet repo checked out
//!
//! Run with: cargo test -p flint-lint --test fleet_compat -- --ignored

use flint_lint::Linter;
use std::path::PathBuf;

/// Find a Fleet GitOps test directory.
/// Priority: FLEET_TEST_DIR env > /private/tmp/it-and-security > skip
fn find_test_dir() -> Option<PathBuf> {
    // 1. Explicit env var
    if let Ok(dir) = std::env::var("FLEET_TEST_DIR") {
        let p = PathBuf::from(dir);
        if p.is_dir() && p.join("default.yml").exists() {
            return Some(p);
        }
    }

    // 2. Common temp location from `fleetctl new`
    let tmp = PathBuf::from("/private/tmp/it-and-security");
    if tmp.is_dir() && tmp.join("default.yml").exists() {
        return Some(tmp);
    }

    // 3. /tmp fallback (Linux)
    let tmp2 = PathBuf::from("/tmp/it-and-security");
    if tmp2.is_dir() && tmp2.join("default.yml").exists() {
        return Some(tmp2);
    }

    None
}

#[test]
#[ignore] // Only run explicitly — requires Fleet test directory
fn test_fleetctl_new_output_has_zero_errors() {
    let test_dir = match find_test_dir() {
        Some(d) => d,
        None => {
            eprintln!("Skipping: no Fleet test directory found (set FLEET_TEST_DIR or run `fleetctl new`)");
            return;
        }
    };

    eprintln!("Testing against: {}", test_dir.display());

    let linter = Linter::new();
    let results = linter
        .lint_directory(&test_dir, None)
        .expect("lint_directory failed");

    let mut total_errors = 0;
    let mut total_warnings = 0;

    for (file, report) in &results {
        if report.has_errors() {
            eprintln!("\nErrors in {}:", file.display());
            for err in &report.errors {
                eprintln!("  error: {}", err.message);
                if let Some(help) = &err.help {
                    eprintln!("    help: {}", help);
                }
            }
        }
        total_errors += report.errors.len();
        total_warnings += report.warnings.len();
    }

    eprintln!(
        "\nSummary: {} file(s), {} error(s), {} warning(s)",
        results.len(),
        total_errors,
        total_warnings
    );

    assert_eq!(
        total_errors, 0,
        "flint should produce 0 errors against `fleetctl new` output. Got {} errors. \
         This likely means Fleet added new YAML keys or renames that flint doesn't know about yet. \
         Check FLEET_SYNC.toml and run `flint sync`.",
        total_errors
    );

    assert_eq!(
        total_warnings, 0,
        "flint should produce 0 warnings against `fleetctl new` output. Got {} warnings.",
        total_warnings
    );
}

#[test]
#[ignore]
fn test_default_yml_validates() {
    let test_dir = match find_test_dir() {
        Some(d) => d,
        None => return,
    };

    let default_yml = test_dir.join("default.yml");
    if !default_yml.exists() {
        return;
    }

    let linter = Linter::new();
    let report = linter.lint_file(&default_yml).expect("lint_file failed");

    assert!(
        !report.has_errors(),
        "default.yml should have 0 errors but got: {:?}",
        report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
}

#[test]
#[ignore]
fn test_fleet_files_validate() {
    let test_dir = match find_test_dir() {
        Some(d) => d,
        None => return,
    };

    let fleets_dir = test_dir.join("fleets");
    if !fleets_dir.is_dir() {
        return;
    }

    let linter = Linter::new();
    for entry in std::fs::read_dir(&fleets_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path
            .extension()
            .map(|e| e == "yml" || e == "yaml")
            .unwrap_or(false)
        {
            let report = linter.lint_file(&path).expect("lint_file failed");
            assert!(
                !report.has_errors(),
                "{} should have 0 errors but got: {:?}",
                path.display(),
                report.errors.iter().map(|e| &e.message).collect::<Vec<_>>()
            );
        }
    }
}
