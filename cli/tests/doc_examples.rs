//! The docs' examples are tests (after ty's mdtest).
//!
//! In any page under docs/docs, each heading starts a case: a fresh repo
//! built from the section's fences that carry a `title="<repo path>"`
//! (untitled fences are illustrations). A minimal `default.yml` is added
//! when the case has none. Expected findings are trailing comments:
//!
//!   bogus_key: 1  # error: [structural-validation]
//!   # warning: [unused-ignore-comment]   (alone on a line: applies to the next line)
//!   # file-warning: [orphaned-file]      (a finding with no line, on this file)
//!
//! The match is exact: an unasserted finding fails, and so does an
//! assertion nothing reports. A page cannot show output flint doesn't give.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_flint");
const DEFAULT: &str = "org_settings:\n  features:\n    enable_host_users: true\n";

/// (file, line — None for a whole-file finding, severity, rule)
type Finding = (String, Option<u64>, String, String);

struct Case {
    at: String,
    files: Vec<(String, String)>,
}

fn cases(page: &Path) -> Vec<Case> {
    let text = fs::read_to_string(page).unwrap();
    let name = page.file_name().unwrap().to_string_lossy().to_string();
    let mut out: Vec<Case> = Vec::new();
    let mut heading = String::new();
    let mut lines = text.lines();
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if let Some(h) = trimmed.strip_prefix('#').filter(|_| line.starts_with('#')) {
            heading = h.trim_start_matches('#').trim().to_string();
            continue;
        }
        let Some(info) = trimmed.strip_prefix("```") else { continue };
        let indent = line.len() - trimmed.len();
        let mut body = String::new();
        for l in lines.by_ref() {
            if l.trim_start().starts_with("```") {
                break;
            }
            body.push_str(l.get(indent..).unwrap_or(l.trim_start()));
            body.push('\n');
        }
        let Some(title) = info.split("title=\"").nth(1).and_then(|t| t.split('"').next()) else { continue };
        let at = format!("{name} § {heading}");
        match out.last_mut() {
            Some(case) if case.at == at => case.files.push((title.to_string(), body)),
            _ => out.push(Case { at, files: vec![(title.to_string(), body)] }),
        }
    }
    out
}

/// `(whole_file, severity, codes)` of the assertion comment ending `line`.
fn assertion(line: &str) -> Option<(bool, &str, Vec<&str>)> {
    let comment = &line[line.rfind("# ")? + 2..];
    let (kind, rest) = comment.split_once(": [")?;
    let (whole_file, severity) = kind.strip_prefix("file-").map_or((false, kind), |s| (true, s));
    matches!(severity, "error" | "warning" | "info")
        .then(|| (whole_file, severity, rest.trim_end_matches(']').split(',').map(str::trim).collect()))
}

fn expected(files: &[(String, String)]) -> Vec<Finding> {
    let mut out = Vec::new();
    for (path, body) in files {
        let mut pending = Vec::new(); // own-line assertions waiting for their line
        for (i, line) in body.lines().enumerate() {
            let parsed = assertion(line);
            let own_line = line.trim_start().starts_with('#') && parsed.is_some();
            if let Some((whole_file, severity, codes)) = parsed {
                for code in codes {
                    match (whole_file, own_line) {
                        (true, _) => out.push((path.clone(), None, severity.to_string(), code.to_string())),
                        (false, true) => pending.push((severity.to_string(), code.to_string())),
                        (false, false) => out.push((path.clone(), Some(i as u64 + 1), severity.to_string(), code.to_string())),
                    }
                }
            }
            if !own_line {
                for (severity, code) in pending.drain(..) {
                    out.push((path.clone(), Some(i as u64 + 1), severity, code));
                }
            }
        }
    }
    out.sort();
    out
}

fn actual(dir: &Path) -> Vec<Finding> {
    let out = Command::new(BIN).args(["check", ".", "--format", "json"]).current_dir(dir).env("HOME", dir).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("bad JSON ({e}): {}", String::from_utf8_lossy(&out.stderr)));
    let mut found: Vec<Finding> = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| {
            let path = f["path"].as_str().unwrap().trim_start_matches("./").to_string();
            f["diagnostics"].as_array().unwrap().iter().map(move |d| {
                (path.clone(), d["line"].as_u64(), d["severity"].as_str().unwrap().to_string(), d["rule"].as_str().unwrap().to_string())
            })
        })
        .collect();
    found.sort();
    found
}

#[test]
fn every_titled_docs_example_reports_exactly_what_it_says() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/docs");
    let mut pages: Vec<_> = fs::read_dir(&docs).unwrap().flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "md")).collect();
    pages.sort();
    let mut failures = Vec::new();
    let mut ran = 0;
    for case in pages.iter().flat_map(|p| cases(p)) {
        let dir = tempfile::tempdir().unwrap();
        let mut files: BTreeMap<&str, &str> = case.files.iter().map(|(p, b)| (p.as_str(), b.as_str())).collect();
        if files.len() < case.files.len() {
            failures.push(format!("{}\n  two code blocks with the same title (start a new heading)", case.at));
            continue;
        }
        files.entry("default.yml").or_insert(DEFAULT);
        for (path, body) in &files {
            let p = dir.path().join(path);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
        let (want, got) = (expected(&case.files), actual(dir.path()));
        ran += 1;
        if want != got {
            let missing: Vec<_> = want.iter().filter(|f| !got.contains(f)).collect();
            let extra: Vec<_> = got.iter().filter(|f| !want.contains(f)).collect();
            failures.push(format!("{}\n  asserted, not reported: {missing:?}\n  reported, not asserted: {extra:?}", case.at));
        }
    }
    assert!(ran > 0, "no titled examples found under {}", docs.display());
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
