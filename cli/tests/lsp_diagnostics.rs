//! `flint lsp` over stdio, as an editor drives it: pull diagnostics
//! (LSP 3.17) when the client supports them, push otherwise.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

const BIN: &str = env!("CARGO_BIN_EXE_flint");

struct Lsp {
    dir: std::path::PathBuf,
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
}

impl Lsp {
    fn start(dir: &std::path::Path) -> Self {
        let mut child = Command::new(BIN).arg("lsp").current_dir(dir).env("HOME", dir)
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let (tx, rx) = channel();
        std::thread::spawn(move || loop {
            let mut len = 0;
            loop {
                let mut line = String::new();
                if out.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                if let Some(n) = line.strip_prefix("Content-Length: ") {
                    len = n.trim().parse().unwrap();
                }
                if line == "\r\n" {
                    break;
                }
            }
            let mut body = vec![0; len];
            out.read_exact(&mut body).unwrap();
            if tx.send(serde_json::from_slice(&body).unwrap()).is_err() {
                return;
            }
        });
        Lsp { dir: dir.to_path_buf(), child, stdin, rx }
    }

    fn send(&mut self, msg: Value) {
        let body = msg.to_string();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{body}", body.len()).unwrap();
        self.stdin.flush().unwrap();
    }

    /// Next message matching `want`; answers any server request on the way.
    fn wait(&mut self, want: impl Fn(&Value) -> bool) -> Value {
        loop {
            let msg = self.rx.recv_timeout(Duration::from_secs(20)).expect("no message from flint lsp");
            if want(&msg) {
                return msg;
            }
            if msg.get("method").is_some() && msg.get("id").is_some() {
                let id = msg["id"].clone();
                self.send(json!({"jsonrpc": "2.0", "id": id, "result": null}));
            }
        }
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        self.wait(|m| m["id"] == id && m.get("method").is_none())
    }

    fn open(&mut self, capabilities: Value) -> (Value, String) {
        let init = self.request(1, "initialize", json!({"processId": null, "rootUri": null, "capabilities": capabilities}));
        self.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
        let uri = format!("file://{}/fleets/a.yml", self.dir.display());
        self.send(json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": {
            "uri": uri, "languageId": "yaml", "version": 1, "text": "name: A\nbogus_key: 1\n"}}}));
        (init, uri)
    }
}

impl Drop for Lsp {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("default.yml"), "org_settings:\n  features:\n    enable_host_users: true\n").unwrap();
    std::fs::create_dir(dir.path().join("fleets")).unwrap();
    dir
}

fn rules(items: &Value) -> Vec<&str> {
    items.as_array().unwrap().iter().map(|d| d["code"].as_str().unwrap_or("")).collect()
}

#[test]
fn pull_client_gets_pull_diagnostics_with_unchanged_reports() {
    let dir = repo();
    let mut lsp = Lsp::start(dir.path());
    let (init, uri) = lsp.open(json!({"textDocument": {"diagnostic": {"dynamicRegistration": false}}}));
    assert_eq!(init["result"]["capabilities"]["diagnosticProvider"]["identifier"], "flint");

    let first = lsp.request(2, "textDocument/diagnostic", json!({"textDocument": {"uri": uri}}));
    assert_eq!(first["result"]["kind"], "full");
    assert!(rules(&first["result"]["items"]).contains(&"structural-validation"), "{first}");
    let id = first["result"]["resultId"].as_str().unwrap().to_string();

    let again = lsp.request(3, "textDocument/diagnostic", json!({"textDocument": {"uri": uri}, "previousResultId": id}));
    assert_eq!(again["result"]["kind"], "unchanged", "{again}");

    lsp.send(json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params": {
        "textDocument": {"uri": uri, "version": 2}, "contentChanges": [{"text": "name: A\n"}]}}));
    let fixed = lsp.request(4, "textDocument/diagnostic", json!({"textDocument": {"uri": uri}, "previousResultId": id}));
    assert_eq!(fixed["result"]["kind"], "full");
    assert!(!rules(&fixed["result"]["items"]).contains(&"structural-validation"), "{fixed}");
}

#[test]
fn push_client_still_gets_published_diagnostics() {
    let dir = repo();
    let mut lsp = Lsp::start(dir.path());
    let (init, _) = lsp.open(json!({}));
    assert!(init["result"]["capabilities"].get("diagnosticProvider").is_none());
    let published = lsp.wait(|m| m["method"] == "textDocument/publishDiagnostics");
    assert!(rules(&published["params"]["diagnostics"]).contains(&"structural-validation"), "{published}");
}
