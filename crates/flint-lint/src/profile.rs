//! Configuration-profile (`.mobileconfig`) metadata extraction, stanza
//! generation, and duplicate-`PayloadUUID` detection.
//!
//! Twin of [`crate::pkg`], for the other most-authored artifact. A
//! `.mobileconfig` is an XML plist; this module reads the **top-level**
//! payload's identifier/display name/scope/UUID (skipping the nested payloads
//! inside `PayloadContent`, which carry their own) and formats a Fleet
//! `configuration_profiles` entry. The `DuplicatePayloadUuidRule` flags
//! profiles applied by the same fleet that share a `PayloadUUID` — a common
//! copy-paste mistake that makes Fleet overwrite/skip a profile.

use super::error::LintError;
use super::fleet_config::FleetConfig;
use super::rules::Rule;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level payload metadata read from a `.mobileconfig`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ProfileInfo {
    pub identifier: Option<String>,
    pub display_name: Option<String>,
    pub scope: Option<String>,
    pub uuid: Option<String>,
    /// The nested payload type(s) inside `PayloadContent` (e.g.
    /// `com.apple.loginitems.managed`) — the meaningful "what does this do",
    /// vs. the always-`Configuration` top-level type.
    pub payload_types: Vec<String>,
}

/// Parse the **top-level** payload keys from a `.mobileconfig` XML plist.
///
/// Depth-aware: only captures keys that are direct children of the outermost
/// `<dict>` (depth 1), so the nested payloads inside `PayloadContent` (with
/// their own `PayloadIdentifier`/`PayloadUUID`) are ignored.
pub fn parse_mobileconfig(xml: &str) -> ProfileInfo {
    #[derive(PartialEq)]
    enum Mode {
        None,
        Key,
        Value,
    }

    let mut info = ProfileInfo::default();
    let mut mode = Mode::None;
    let mut depth: i32 = 0;
    let mut pending_key: Option<String> = None;
    let mut idx = 0;

    while let Some(rel) = xml[idx..].find('<') {
        let lt = idx + rel;
        let chardata = xml[idx..lt].trim();

        match mode {
            Mode::Key => {
                // The text between <key> and </key> is the key name. Track it at
                // any depth: depth 1 gives the top-level fields, deeper gives the
                // nested PayloadType inside PayloadContent.
                pending_key = Some(chardata.to_string());
            }
            Mode::Value => {
                if let Some(k) = pending_key.take() {
                    if depth == 1 {
                        set_field(&mut info, &k, chardata);
                    } else if k == "PayloadType"
                        && !chardata.is_empty()
                        && !info.payload_types.iter().any(|t| t == chardata)
                    {
                        info.payload_types.push(chardata.to_string());
                    }
                }
            }
            Mode::None => {}
        }
        mode = Mode::None;

        let gt = match xml[lt + 1..].find('>') {
            Some(g) => lt + 1 + g,
            None => break,
        };
        let tag = &xml[lt + 1..gt];
        let close = tag.starts_with('/');
        let self_close = tag.ends_with('/');
        let name = tag
            .trim_start_matches('/')
            .trim_end_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("");

        match name {
            "dict" | "array" => {
                if close {
                    depth -= 1;
                } else if !self_close {
                    depth += 1;
                }
            }
            "key" if !close => mode = Mode::Key,
            "string" | "integer" | "real" | "date" | "data" if !close => mode = Mode::Value,
            _ => {}
        }

        idx = gt + 1;
    }

    info
}

fn set_field(info: &mut ProfileInfo, key: &str, value: &str) {
    match key {
        "PayloadIdentifier" if info.identifier.is_none() => {
            info.identifier = Some(value.to_string())
        }
        "PayloadDisplayName" if info.display_name.is_none() => {
            info.display_name = Some(value.to_string())
        }
        "PayloadScope" if info.scope.is_none() => info.scope = Some(value.to_string()),
        "PayloadUUID" if info.uuid.is_none() => info.uuid = Some(value.to_string()),
        _ => {}
    }
}

/// Format a Fleet `configuration_profiles` entry for a profile. `path_value`
/// is written verbatim into `- path:`. With `full`, adds commented label stubs.
pub fn profile_block(info: &ProfileInfo, path_value: &str, full: bool) -> String {
    let name = info.display_name.as_deref().unwrap_or("(no display name)");
    let id = info.identifier.as_deref().unwrap_or("unknown.identifier");
    let scope = info.scope.as_deref().unwrap_or("System");
    let types = if info.payload_types.is_empty() {
        String::new()
    } else {
        format!(" · {}", info.payload_types.join(", "))
    };
    let header = format!("# {name} — {id} [{scope}]{types}");
    if full {
        format!(
            "{header}\n\
             - path: {path_value}\n\
             \x20 # labels_include_any:   # include hosts matching ANY of these labels\n\
             \x20 #   - \"Label name\"\n\
             \x20 # labels_exclude_any:   # and/or exclude hosts matching ANY of these (combinable with include)\n\
             \x20 #   - \"Label name\""
        )
    } else {
        format!("{header}\n- path: {path_value}")
    }
}

/// DDM declaration (`.json`) metadata: its `Type` and `Identifier`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DeclarationInfo {
    pub identifier: Option<String>,
    pub decl_type: Option<String>,
}

/// Parse a DDM declaration `.json`. Uses `serde_yaml` (a JSON superset) so no
/// extra dependency is needed.
pub fn parse_declaration(json: &str) -> DeclarationInfo {
    let value: serde_yaml::Value = match serde_yaml::from_str(json) {
        Ok(v) => v,
        Err(_) => return DeclarationInfo::default(),
    };
    let get = |k: &str| {
        value
            .get(k)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    DeclarationInfo {
        identifier: get("Identifier"),
        decl_type: get("Type"),
    }
}

/// Format a Fleet `configuration_profiles` entry for a DDM declaration.
pub fn declaration_block(info: &DeclarationInfo, path_value: &str, full: bool) -> String {
    let id = info.identifier.as_deref().unwrap_or("unknown.identifier");
    let t = info.decl_type.as_deref().unwrap_or("com.apple.configuration.*");
    let header = format!("# {id} — {t} (DDM declaration)");
    if full {
        format!(
            "{header}\n\
             - path: {path_value}\n\
             \x20 # labels_include_any:\n\
             \x20 #   - \"Label name\"\n\
             \x20 # labels_exclude_any:\n\
             \x20 #   - \"Label name\""
        )
    } else {
        format!("{header}\n- path: {path_value}")
    }
}

/// Windows CSP profile (`.xml`) context: the `LocURI` target(s).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct WindowsCspInfo {
    pub loc_uris: Vec<String>,
}

/// Parse `<LocURI>` values from a Windows CSP (SyncML) profile.
pub fn parse_windows_csp(xml: &str) -> WindowsCspInfo {
    let mut uris = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find("<LocURI>") {
        let after = &rest[i + "<LocURI>".len()..];
        match after.find("</LocURI>") {
            Some(end) => {
                let uri = after[..end].trim().to_string();
                if !uri.is_empty() && !uris.contains(&uri) {
                    uris.push(uri);
                }
                rest = &after[end..];
            }
            None => break,
        }
    }
    WindowsCspInfo { loc_uris: uris }
}

/// Format a Fleet `windows_settings.configuration_profiles` entry. Windows
/// profiles have no embedded identifier — `name` (the filename) labels it.
pub fn windows_profile_block(
    name: &str,
    info: &WindowsCspInfo,
    path_value: &str,
    full: bool,
) -> String {
    let ctx = match info.loc_uris.len() {
        0 => String::new(),
        1 => format!(" · {}", info.loc_uris[0]),
        n => format!(" · {} (+{} more)", info.loc_uris[0], n - 1),
    };
    let header = format!("# {name} — Windows CSP{ctx}");
    if full {
        format!(
            "{header}\n\
             - path: {path_value}\n\
             \x20 # labels_include_any:\n\
             \x20 #   - \"Label name\"\n\
             \x20 # labels_exclude_any:\n\
             \x20 #   - \"Label name\""
        )
    } else {
        format!("{header}\n- path: {path_value}")
    }
}

/// ADE enrollment profile (`.dep.json`) metadata: its `profile_name`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnrollmentInfo {
    pub profile_name: Option<String>,
}

/// Parse an Apple Automated Device Enrollment `.dep.json`.
pub fn parse_enrollment(json: &str) -> EnrollmentInfo {
    let value: serde_yaml::Value = match serde_yaml::from_str(json) {
        Ok(v) => v,
        Err(_) => return EnrollmentInfo::default(),
    };
    EnrollmentInfo {
        profile_name: value
            .get("profile_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    }
}

/// Format the `controls.setup_experience.apple_setup_assistant` entry (a single
/// scalar path, one per team — not a list item).
pub fn enrollment_block(info: &EnrollmentInfo, path_value: &str) -> String {
    let name = info.profile_name.as_deref().unwrap_or("(ADE profile)");
    format!(
        "# {name} (ADE setup assistant) — under controls.setup_experience:\n\
         apple_setup_assistant: {path_value}"
    )
}

// ---------------------------------------------------------------------------
// Duplicate PayloadUUID / declaration-Identifier rule
// ---------------------------------------------------------------------------

/// Flags profiles applied by the same fleet that share a `PayloadUUID`.
pub struct DuplicatePayloadUuidRule;

impl Rule for DuplicatePayloadUuidRule {
    fn name(&self) -> &'static str {
        "duplicate-payload-uuid"
    }
    fn description(&self) -> &'static str {
        "Detects configuration profiles sharing a PayloadUUID (Fleet overwrites/skips duplicates)"
    }
    fn category(&self) -> &'static str {
        "semantic"
    }

    fn check(&self, _config: &FleetConfig, file: &Path, source: &str) -> Vec<LintError> {
        let refs = collect_profile_refs(file, source);

        // .mobileconfig grouped by PayloadUUID; DDM .json by Identifier.
        let mut by_uuid: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut by_decl_id: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for p in refs {
            let content = match std::fs::read_to_string(&p) {
                Ok(c) => c,
                Err(_) => continue,
            };
            if is_mobileconfig(&p) {
                if let Some(uuid) = parse_mobileconfig(&content).uuid {
                    by_uuid.entry(uuid).or_default().push(p);
                }
            } else if is_declaration(&p) {
                if let Some(id) = parse_declaration(&content).identifier {
                    by_decl_id.entry(id).or_default().push(p);
                }
            }
        }

        let mut errors = Vec::new();
        emit_dups(
            &mut errors,
            file,
            by_uuid,
            "duplicate PayloadUUID",
            "profiles",
            "Each profile needs a unique PayloadUUID — Fleet overwrites or skips duplicates. \
             Regenerate one with: flint profile <file> --regen-uuid",
        );
        emit_dups(
            &mut errors,
            file,
            by_decl_id,
            "duplicate declaration Identifier",
            "DDM declarations",
            "Each DDM declaration needs a unique Identifier — give one a distinct Identifier.",
        );
        errors
    }
}

/// Emit a warning for each key shared by 2+ files.
fn emit_dups(
    errors: &mut Vec<LintError>,
    file: &Path,
    groups: HashMap<String, Vec<PathBuf>>,
    label: &str,
    noun: &str,
    help: &str,
) {
    let mut dups: Vec<_> = groups.into_iter().filter(|(_, v)| v.len() > 1).collect();
    dups.sort_by(|a, b| a.0.cmp(&b.0));
    for (key, mut files) in dups {
        files.sort();
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string())
            .collect();
        errors.push(
            LintError::warning(
                format!("{label} {key} shared by {} {noun}: {}", files.len(), names.join(", ")),
                file,
            )
            .with_rule_code(crate::codes::DUPLICATE_PAYLOAD_UUID)
            .with_help(help),
        );
    }
}

/// Collect every `.mobileconfig` referenced by a fleet config (via `path:` and
/// `paths:` globs), resolved to absolute paths.
fn collect_profile_refs(file: &Path, source: &str) -> Vec<PathBuf> {
    let base = match file.parent() {
        Some(p) => p,
        None => return Vec::new(),
    };
    let yaml: serde_yaml::Value = match serde_yaml::from_str(source) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let mut refs = Vec::new();
    walk_refs(&yaml, &mut refs);

    let mut out = Vec::new();
    for value in refs {
        if value.contains('*') || value.contains('?') {
            // Glob — resolve relative to the fleet file, normalize, expand.
            let joined = super::util::normalize_path(&base.join(&value));
            let pattern = joined.to_string_lossy().replace('\\', "/");
            for hit in expand_glob(&pattern) {
                if is_mobileconfig(&hit) || is_declaration(&hit) {
                    out.push(hit);
                }
            }
        } else if value.ends_with(".mobileconfig") || value.ends_with(".json") {
            let resolved = base.join(&value);
            if resolved.exists() {
                out.push(resolved);
            }
        }
    }
    out
}

fn is_declaration(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some("json")
}

/// Collect all `path:`/`paths:` string values from a YAML tree.
fn walk_refs(value: &serde_yaml::Value, out: &mut Vec<String>) {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for key in ["path", "paths"] {
                if let Some(serde_yaml::Value::String(s)) =
                    map.get(serde_yaml::Value::String(key.to_string()))
                {
                    out.push(s.clone());
                }
            }
            for (_, v) in map {
                walk_refs(v, out);
            }
        }
        serde_yaml::Value::Sequence(seq) => {
            for item in seq {
                walk_refs(item, out);
            }
        }
        _ => {}
    }
}

fn is_mobileconfig(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()) == Some("mobileconfig")
}

/// Expand a glob pattern (absolute) to matching files on disk.
fn expand_glob(pattern: &str) -> Vec<PathBuf> {
    // Longest leading path with no wildcard = the directory to walk.
    let mut base = PathBuf::new();
    for comp in Path::new(pattern).components() {
        let s = comp.as_os_str().to_string_lossy();
        if s.contains('*') || s.contains('?') {
            break;
        }
        base.push(comp);
    }
    let mut out = Vec::new();
    walk_files(&base, pattern, &mut out);
    out
}

fn walk_files(dir: &Path, pattern: &str, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_files(&path, pattern, out);
        } else if super::unwired::glob_match(pattern, &path.to_string_lossy().replace('\\', "/")) {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>PayloadContent</key>
  <array>
    <dict>
      <key>PayloadIdentifier</key><string>com.example.loginitems.inner</string>
      <key>PayloadUUID</key><string>INNER-UUID-1111</string>
      <key>PayloadType</key><string>com.apple.loginitems.managed</string>
    </dict>
  </array>
  <key>PayloadDisplayName</key><string>System - Login Items (Items)</string>
  <key>PayloadIdentifier</key><string>com.example.loginitems</string>
  <key>PayloadScope</key><string>System</string>
  <key>PayloadType</key><string>Configuration</string>
  <key>PayloadUUID</key><string>TOP-UUID-9999</string>
</dict>
</plist>"#;

    #[test]
    fn test_parse_top_level_only() {
        let info = parse_mobileconfig(SAMPLE);
        // Must pick the TOP-LEVEL identifier/uuid, not the nested ones.
        assert_eq!(info.identifier.as_deref(), Some("com.example.loginitems"));
        assert_eq!(info.uuid.as_deref(), Some("TOP-UUID-9999"));
        assert_eq!(info.display_name.as_deref(), Some("System - Login Items (Items)"));
        assert_eq!(info.scope.as_deref(), Some("System"));
        // The nested payload type is captured (not the top-level "Configuration").
        assert_eq!(info.payload_types, vec!["com.apple.loginitems.managed".to_string()]);
    }

    #[test]
    fn test_profile_block() {
        let info = parse_mobileconfig(SAMPLE);
        let block = profile_block(&info, "./system-login-items.mobileconfig", false);
        assert_eq!(
            block,
            "# System - Login Items (Items) — com.example.loginitems [System] · com.apple.loginitems.managed\n- path: ./system-login-items.mobileconfig"
        );
        let full = profile_block(&info, "./x.mobileconfig", true);
        assert!(full.contains("# labels_include_any:"));
        assert!(full.contains("# labels_exclude_any:"));
    }

    fn profile(uuid: &str, id: &str) -> String {
        format!(
            "<plist version=\"1.0\"><dict>\
             <key>PayloadIdentifier</key><string>{id}</string>\
             <key>PayloadUUID</key><string>{uuid}</string>\
             </dict></plist>"
        )
    }

    #[test]
    fn test_duplicate_uuid_detected_via_glob() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("platforms/macos/configuration-profiles/security");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.mobileconfig"), profile("SAME-UUID", "com.x.a")).unwrap();
        fs::write(dir.join("b.mobileconfig"), profile("SAME-UUID", "com.x.b")).unwrap();
        fs::write(dir.join("c.mobileconfig"), profile("OTHER-UUID", "com.x.c")).unwrap();

        let fleet_dir = tmp.path().join("fleets");
        fs::create_dir_all(&fleet_dir).unwrap();
        let fleet = fleet_dir.join("team.yml");
        let yaml = "controls:\n  apple_settings:\n    configuration_profiles:\n      - paths: ../platforms/macos/configuration-profiles/security/*.mobileconfig\n";
        fs::write(&fleet, yaml).unwrap();

        let errors = DuplicatePayloadUuidRule.check(&FleetConfig::default(), &fleet, yaml);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        assert!(errors[0].message.contains("SAME-UUID"));
        assert!(errors[0].message.contains("a.mobileconfig"));
        assert!(errors[0].message.contains("b.mobileconfig"));
        assert!(!errors[0].message.contains("c.mobileconfig"));
    }

    #[test]
    fn test_declaration_parse_and_block() {
        let json = r#"{ "Type": "com.apple.configuration.passcode.settings", "Identifier": "com.example.passcode", "Payload": {} }"#;
        let info = parse_declaration(json);
        assert_eq!(info.identifier.as_deref(), Some("com.example.passcode"));
        assert_eq!(info.decl_type.as_deref(), Some("com.apple.configuration.passcode.settings"));
        let block = declaration_block(&info, "./passcode.json", false);
        assert_eq!(
            block,
            "# com.example.passcode — com.apple.configuration.passcode.settings (DDM declaration)\n- path: ./passcode.json"
        );
    }

    #[test]
    fn test_windows_csp_parse_and_block() {
        let xml = r#"<Replace><Item><Target><LocURI>./Device/Vendor/MSFT/Policy/Config/Bluetooth/AllowDiscoverableMode</LocURI></Target></Item></Replace>
<Add><Item><Target><LocURI>./Device/Vendor/MSFT/Firewall/Off</LocURI></Target></Item></Add>"#;
        let info = parse_windows_csp(xml);
        assert_eq!(info.loc_uris.len(), 2);
        let block = windows_profile_block("disable-bluetooth.xml", &info, "./disable-bluetooth.xml", false);
        assert!(block.starts_with("# disable-bluetooth.xml — Windows CSP · ./Device/Vendor/MSFT/Policy/Config/Bluetooth/AllowDiscoverableMode (+1 more)"));
        assert!(block.contains("- path: ./disable-bluetooth.xml"));
    }

    #[test]
    fn test_enrollment_parse_and_block() {
        let json = r#"{ "profile_name": "Corporate ADE", "allow_pairing": true, "skip_setup_items": ["Restore"] }"#;
        let info = parse_enrollment(json);
        assert_eq!(info.profile_name.as_deref(), Some("Corporate ADE"));
        let block = enrollment_block(&info, "../platforms/macos/enrollment-profiles/corp.dep.json");
        assert!(block.contains("# Corporate ADE (ADE setup assistant)"));
        assert!(block.contains("apple_setup_assistant: ../platforms/macos/enrollment-profiles/corp.dep.json"));
        assert!(!block.contains("- path:")); // scalar, not a list item
    }

    #[test]
    fn test_duplicate_declaration_identifier() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("platforms/macos/declaration-profiles");
        fs::create_dir_all(&dir).unwrap();
        let decl = |id: &str| format!(r#"{{"Type":"com.apple.configuration.x","Identifier":"{id}"}}"#);
        fs::write(dir.join("a.json"), decl("com.dup.decl")).unwrap();
        fs::write(dir.join("b.json"), decl("com.dup.decl")).unwrap();
        fs::write(dir.join("c.json"), decl("com.unique.decl")).unwrap();

        let fleet_dir = tmp.path().join("fleets");
        fs::create_dir_all(&fleet_dir).unwrap();
        let fleet = fleet_dir.join("team.yml");
        let yaml = "controls:\n  apple_settings:\n    configuration_profiles:\n      - paths: ../platforms/macos/declaration-profiles/*.json\n";
        fs::write(&fleet, yaml).unwrap();

        let errors = DuplicatePayloadUuidRule.check(&FleetConfig::default(), &fleet, yaml);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        assert!(errors[0].message.contains("duplicate declaration Identifier"));
        assert!(errors[0].message.contains("com.dup.decl"));
        assert!(errors[0].message.contains("a.json") && errors[0].message.contains("b.json"));
    }

    #[test]
    fn test_no_duplicate_no_error() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("p");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.mobileconfig"), profile("U1", "com.x.a")).unwrap();
        fs::write(dir.join("b.mobileconfig"), profile("U2", "com.x.b")).unwrap();
        let fleet = tmp.path().join("default.yml");
        let yaml = "controls:\n  apple_settings:\n    configuration_profiles:\n      - path: ./p/a.mobileconfig\n      - path: ./p/b.mobileconfig\n";
        fs::write(&fleet, yaml).unwrap();
        let errors = DuplicatePayloadUuidRule.check(&FleetConfig::default(), &fleet, yaml);
        assert!(errors.is_empty(), "got: {errors:?}");
    }
}
