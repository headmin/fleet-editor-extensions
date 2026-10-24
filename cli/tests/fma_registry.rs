//! `flint fma refresh` writes a repo's `[fma] registry` file from Fleet's
//! apps.json, and `flint check` then judges slugs by it instead of the
//! bundled list. Offline: apps.json is served from a local socket.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_flint");

/// Serve `body` to every request; returns the URL.
fn serve(body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/apps.json", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for mut conn in listener.incoming().flatten() {
            let mut buf = [0; 4096];
            let _ = conn.read(&mut buf);
            let _ = write!(conn, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    url
}

fn fma_findings(dir: &std::path::Path, cache: &std::path::Path) -> Vec<String> {
    let out = Command::new(BIN).args(["check", ".", "--format", "json"]).current_dir(dir).env("XDG_CACHE_HOME", cache).output().unwrap();
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    json["files"].as_array().unwrap().iter().flat_map(|f| f["diagnostics"].as_array().unwrap().clone())
        .filter(|d| d["rule"] == "fma-slug")
        .map(|d| d["message"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn refresh_writes_the_repo_registry_and_check_uses_it() {
    let dir = tempfile::tempdir().unwrap();
    let cache = tempfile::tempdir().unwrap();
    let repo = dir.path();
    fs::write(repo.join("fleetlint.toml"), "[fma]\nregistry = \"fma-registry.toml\"\n").unwrap();
    fs::write(repo.join("default.yml"), "org_settings:\n  features:\n    enable_host_users: true\n").unwrap();
    fs::create_dir(repo.join("fleets")).unwrap();
    fs::write(
        repo.join("fleets/a.yml"),
        "name: A\nsoftware:\n  fleet_maintained_apps:\n    - slug: brand-new-app/darwin\n    - slug: slack/darwin\n",
    )
    .unwrap();

    // No registry file yet: the bundled list knows slack, not the new app.
    let before = fma_findings(repo, cache.path());
    assert!(before.iter().any(|m| m.contains("brand-new-app")) && !before.iter().any(|m| m.contains("slack")), "{before:?}");

    let url = serve(r#"{"version":2,"apps":[{"slug":"brand-new-app/darwin"},{"slug":"zoom/darwin"}]}"#);
    let out = Command::new(BIN).args(["fma", "refresh"]).current_dir(repo).env("XDG_CACHE_HOME", cache.path())
        .env("FLINT_FMA_APPS_URL", &url).env("FLINT_FMA_FEED_URL", "http://127.0.0.1:9/feed.xml").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let registry = fs::read_to_string(repo.join("fma-registry.toml")).unwrap();
    assert!(registry.contains("(2 apps,") && registry.contains("name = \"brand-new-app\""), "{registry}");
    assert!(!cache.path().join("flint/fma-registry.toml").exists(), "the repo file, not the cache");

    // Now the repo's list decides: the new app is known, slack is gone.
    let after = fma_findings(repo, cache.path());
    assert!(!after.iter().any(|m| m.contains("brand-new-app")) && after.iter().any(|m| m.contains("slack")), "{after:?}");
}
