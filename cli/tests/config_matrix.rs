//! Every `fleetlint.toml` section, end to end: each case is a small repo, a
//! config, and the effect that config must have on one rule's findings.
//! A section that parses but stops doing anything fails here — the failure
//! mode behind `reports:` skipping the query rules and bare `exclude` names
//! matching nothing.

use std::fs;
use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_flint");
const DEFAULT: &str = "org_settings:\n  features:\n    enable_host_users: true\n";

struct Case {
    name: &'static str,
    files: &'static [(&'static str, &'static str)],
    config: Option<&'static str>,
    rule: &'static str,
    /// `None` = any severity.
    severity: Option<&'static str>,
    expected: usize,
}

fn findings(dir: &Path, rule: &str, severity: Option<&str>) -> usize {
    let out = Command::new(BIN)
        .args(["check", ".", "--format", "json"])
        .current_dir(dir)
        .env_remove("XDG_CONFIG_HOME")
        .env("HOME", dir) // no user-level config leaks in
        .output()
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&out.stderr)));
    json["files"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["diagnostics"].as_array().unwrap())
        .filter(|d| d["rule"] == rule && severity.is_none_or(|s| d["severity"] == s))
        .count()
}

fn materialize(case: &Case) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (path, body) in case.files {
        let p = dir.path().join(path);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, body).unwrap();
    }
    if let Some(cfg) = case.config {
        fs::write(dir.path().join("fleetlint.toml"), cfg).unwrap();
    }
    dir
}

const UNKNOWN_KEY: &[(&str, &str)] = &[("default.yml", DEFAULT), ("fleets/a.yml", "name: A\nbogus_key: 1\n")];
const REPORT: &[(&str, &str)] = &[
    ("default.yml", DEFAULT),
    ("fleets/a.yml", "name: A\nreports:\n  - name: q\n    query: SELECT * FROM osquery_info;\n    interval: 30\n"),
];
const QUERIES_KEY: &[(&str, &str)] = &[(
    "default.yml",
    "org_settings:\n  features:\n    enable_host_users: true\nqueries:\n  - name: q\n    query: SELECT 1;\n",
)];

const CASES: &[Case] = &[
    Case { name: "baseline: unknown key is an error", files: UNKNOWN_KEY, config: None, rule: "structural-validation", severity: Some("error"), expected: 1 },
    Case { name: "[rules] disabled", files: UNKNOWN_KEY, config: Some("[rules]\ndisabled = [\"structural-validation\"]\n"), rule: "structural-validation", severity: None, expected: 0 },
    Case { name: "[rules] warn downgrades", files: UNKNOWN_KEY, config: Some("[rules]\nwarn = [\"structural-validation\"]\n"), rule: "structural-validation", severity: Some("warning"), expected: 1 },
    Case { name: "[schema] allow_unknown_fields", files: UNKNOWN_KEY, config: Some("[schema]\nallow_unknown_fields = true\n"), rule: "structural-validation", severity: None, expected: 0 },
    Case { name: "[thresholds] min_interval on reports:", files: REPORT, config: Some("[thresholds]\nmin_interval = 60\n"), rule: "interval-validation", severity: None, expected: 1 },
    Case { name: "[thresholds] relaxed", files: REPORT, config: Some("[thresholds]\nmin_interval = 10\n"), rule: "interval-validation", severity: None, expected: 0 },
    Case { name: "[thresholds] max_query_length on reports:", files: REPORT, config: Some("[thresholds]\nmax_query_length = 10\n"), rule: "query-length", severity: None, expected: 1 },
    Case {
        name: "[files] exclude bare directory name",
        files: &[("default.yml", DEFAULT), ("fleets/a.yml", "name: A\n"), ("scratch/b.yml", "name: B\nbogus_key: 1\n")],
        config: Some("[files]\nexclude = [\"scratch\"]\n"),
        rule: "structural-validation",
        severity: None,
        expected: 0,
    },
    Case { name: "[deprecations] fleet_version before the rename", files: QUERIES_KEY, config: Some("[deprecations]\nfleet_version = \"4.80.0\"\n"), rule: "deprecated-keys", severity: None, expected: 0 },
    Case { name: "[deprecations] fleet_version after the rename", files: QUERIES_KEY, config: Some("[deprecations]\nfleet_version = \"4.82.0\"\n"), rule: "deprecated-keys", severity: None, expected: 1 },
    Case {
        name: "[orphans] allow",
        files: &[("default.yml", DEFAULT), ("fleets/a.yml", "name: A\n"), ("lib/scripts/unused.sh", "#!/bin/sh\necho\n")],
        config: Some("[orphans]\nallow = [\"lib/scripts/unused.sh\"]\n"),
        rule: "orphaned-file",
        severity: None,
        expected: 0,
    },
    Case {
        name: "[fleet.env] declares a variable",
        files: &[("default.yml", "org_settings:\n  server_settings:\n    server_url: ${MY_URL}\n"), ("fleets/a.yml", "name: A\n")],
        config: Some("[fleet.env]\nMY_URL = \"https://x\"\n"),
        rule: "env-var-resolvable",
        severity: None,
        expected: 0,
    },
    Case {
        name: "[[patterns]] forbid-file",
        files: &[("default.yml", DEFAULT), ("fleets/a.yml", "name: A\n"), ("lib/old.txt", "x\n")],
        config: Some("[[patterns]]\nfiles = \"lib/*.txt\"\nassert = \"forbid-file\"\nwhy = \"no txt\"\n"),
        rule: "pattern/forbid-file",
        severity: None,
        expected: 1,
    },
];

#[test]
fn every_config_section_has_its_effect() {
    let failures: Vec<String> = CASES
        .iter()
        .filter_map(|case| {
            let dir = materialize(case);
            let got = findings(dir.path(), case.rule, case.severity);
            (got != case.expected).then(|| format!("{}: want {}, got {got}", case.name, case.expected))
        })
        .collect();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn hidden_and_visible_config_names_and_precedence() {
    let case = &CASES[1]; // [rules] disabled
    let dir = materialize(case);
    fs::rename(dir.path().join("fleetlint.toml"), dir.path().join(".fleetlint.toml")).unwrap();
    assert_eq!(findings(dir.path(), case.rule, None), 0, "hidden .fleetlint.toml is read");
    fs::write(dir.path().join("fleetlint.toml"), "[rules]\n").unwrap();
    assert_eq!(findings(dir.path(), case.rule, None), 1, "visible fleetlint.toml wins");
}

#[test]
fn unparseable_config_exits_3_except_in_hook_mode() {
    let dir = materialize(&Case { config: Some("[rules\nx"), ..CASES[0] });
    let run = |extra: &[&str]| {
        Command::new(BIN).args(["check", "."]).args(extra).current_dir(dir.path()).output().unwrap()
    };
    assert_eq!(run(&[]).status.code(), Some(3));
    assert_eq!(run(&["--hook-mode"]).status.code(), Some(0));
}

#[test]
fn ignored_settings_warn_with_line_and_suggestion() {
    let dir = materialize(&Case { config: Some("[rules]\ndisabeld = [\"x\"]\n"), ..CASES[0] });
    let out = Command::new(BIN).args(["check", "."]).current_dir(dir.path()).output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("fleetlint.toml:2: `rules.disabeld` is not a setting"), "{stderr}");
    assert!(stderr.contains("did you mean `disabled`"), "{stderr}");
}
