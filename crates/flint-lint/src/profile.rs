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

// ---------------------------------------------------------------------------
// profile-well-formed / payload-uuid-format
// ---------------------------------------------------------------------------

/// A well-formedness defect found in a `.mobileconfig`.
#[derive(Debug, PartialEq, Eq)]
pub struct XmlDefect {
    /// 1-based line the defect was detected on.
    pub line: usize,
    pub message: String,
}

/// The five entities XML predefines. A `.mobileconfig` carries no DTD of its
/// own, so any other named entity is undefined and the document is not
/// well-formed — Apple's parser rejects it.
const PREDEFINED_ENTITIES: [&str; 5] = ["amp", "lt", "gt", "apos", "quot"];

fn find_from(b: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || start >= b.len() || b.len() - start < needle.len() {
        return None;
    }
    b[start..].windows(needle.len()).position(|w| w == needle).map(|q| q + start)
}

fn count_nl(b: &[u8], from: usize, to: usize) -> usize {
    b[from..to.min(b.len())].iter().filter(|&&c| c == b'\n').count()
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':'
}

fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b':' | b'-' | b'.')
}

/// Check that `xml` is well-formed enough for Apple's plist parser to accept.
///
/// Deliberately dependency-free and narrow: it catches the classes that
/// actually reach Fleet — a raw `&` in text (the `Foo & Bar GmbH` defect
/// that failed `fleetctl gitops`), an undefined entity, a mismatched or
/// unclosed element, and an unterminated comment/CDATA/PI. It is *not* a
/// validating parser and says nothing about the plist schema.
///
/// Existence of this check is the point: [`parse_mobileconfig`] is a
/// hand-rolled scanner that cannot fail, so before this rule a malformed
/// profile silently yielded an empty [`ProfileInfo`] and every profile-level
/// rule went dark for that file.
pub fn check_xml_well_formed(xml: &str) -> Option<XmlDefect> {
    let b = xml.as_bytes();
    let mut i = 0usize;
    let mut line = 1usize;
    let mut stack: Vec<(String, usize)> = Vec::new();
    let mut root_seen = false;

    while i < b.len() {
        match b[i] {
            b'<' if b[i..].starts_with(b"<!--") => {
                let Some(end) = find_from(b, i + 4, b"-->") else {
                    return Some(XmlDefect {
                        line,
                        message: "unterminated comment: `<!--` with no closing `-->`".to_string(),
                    });
                };
                line += count_nl(b, i, end + 3);
                i = end + 3;
            }
            b'<' if b[i..].starts_with(b"<![CDATA[") => {
                let Some(end) = find_from(b, i + 9, b"]]>") else {
                    return Some(XmlDefect {
                        line,
                        message: "unterminated CDATA section: `<![CDATA[` with no closing `]]>`"
                            .to_string(),
                    });
                };
                line += count_nl(b, i, end + 3);
                i = end + 3;
            }
            b'<' if b[i..].starts_with(b"<?") => {
                let Some(end) = find_from(b, i + 2, b"?>") else {
                    return Some(XmlDefect {
                        line,
                        message: "unterminated processing instruction: `<?` with no closing `?>`"
                            .to_string(),
                    });
                };
                line += count_nl(b, i, end + 2);
                i = end + 2;
            }
            // `<!DOCTYPE …>`, possibly with a bracketed internal subset.
            b'<' if b[i..].starts_with(b"<!") => {
                let start_line = line;
                let mut j = i + 2;
                let mut depth = 0usize;
                loop {
                    let Some(&c) = b.get(j) else {
                        return Some(XmlDefect {
                            line: start_line,
                            message: "unterminated declaration: `<!` with no closing `>`"
                                .to_string(),
                        });
                    };
                    match c {
                        b'[' => depth += 1,
                        b']' if depth > 0 => depth -= 1,
                        b'>' if depth == 0 => break,
                        _ => {}
                    }
                    j += 1;
                }
                line += count_nl(b, i, j + 1);
                i = j + 1;
            }
            b'<' => {
                let tag_line = line;
                let closing = b.get(i + 1) == Some(&b'/');
                let mut j = i + 1 + usize::from(closing);
                if j >= b.len() || !is_name_start(b[j]) {
                    return Some(XmlDefect {
                        line: tag_line,
                        message: "raw `<` in text must be escaped as `&lt;` (it opens a tag with \
                                  no valid element name)"
                            .to_string(),
                    });
                }
                let name_start = j;
                while j < b.len() && is_name_char(b[j]) {
                    j += 1;
                }
                let name = String::from_utf8_lossy(&b[name_start..j]).into_owned();

                // Scan to the closing `>`, honouring quoted attribute values so
                // a `>` inside an attribute does not end the tag early.
                let mut quote: Option<u8> = None;
                let self_closing;
                loop {
                    let Some(&c) = b.get(j) else {
                        return Some(XmlDefect {
                            line: tag_line,
                            message: format!("unterminated tag `<{name}`: no closing `>`"),
                        });
                    };
                    match quote {
                        Some(q) => {
                            if c == q {
                                quote = None;
                            }
                        }
                        None => match c {
                            b'"' | b'\'' => quote = Some(c),
                            b'>' => {
                                self_closing = j > 0 && b[j - 1] == b'/';
                                break;
                            }
                            _ => {}
                        },
                    }
                    j += 1;
                }
                line += count_nl(b, i, j + 1);
                i = j + 1;

                if closing {
                    match stack.pop() {
                        Some((open, _)) if open == name => {}
                        Some((open, open_line)) => {
                            return Some(XmlDefect {
                                line: tag_line,
                                message: format!(
                                    "closing tag `</{name}>` does not match `<{open}>` opened on \
                                     line {open_line}"
                                ),
                            });
                        }
                        None => {
                            return Some(XmlDefect {
                                line: tag_line,
                                message: format!(
                                    "closing tag `</{name}>` has no matching opening tag"
                                ),
                            });
                        }
                    }
                } else {
                    root_seen = true;
                    if !self_closing {
                        stack.push((name, tag_line));
                    }
                }
            }
            b'&' => {
                let mut j = i + 1;
                let limit = (i + 12).min(b.len());
                while j < limit && b[j] != b';' {
                    j += 1;
                }
                let raw_amp = XmlDefect {
                    line,
                    message: "raw `&` in text must be escaped as `&amp;`".to_string(),
                };
                if j >= limit || b[j] != b';' {
                    return Some(raw_amp);
                }
                let Ok(ent) = std::str::from_utf8(&b[i + 1..j]) else {
                    return Some(raw_amp);
                };
                let ok = if let Some(num) = ent.strip_prefix('#') {
                    match num.strip_prefix(['x', 'X']) {
                        Some(hex) => !hex.is_empty() && hex.bytes().all(|c| c.is_ascii_hexdigit()),
                        None => !num.is_empty() && num.bytes().all(|c| c.is_ascii_digit()),
                    }
                } else {
                    PREDEFINED_ENTITIES.contains(&ent)
                };
                if !ok {
                    return Some(XmlDefect {
                        line,
                        message: format!(
                            "undefined entity `&{ent};` — a .mobileconfig has no DTD, so only \
                             &amp; &lt; &gt; &apos; &quot; and numeric references are valid"
                        ),
                    });
                }
                i = j + 1;
            }
            b'\n' => {
                line += 1;
                i += 1;
            }
            _ => i += 1,
        }
    }

    if let Some((name, open_line)) = stack.pop() {
        return Some(XmlDefect {
            line: open_line,
            message: format!("unclosed element `<{name}>`"),
        });
    }
    if !root_seen {
        return Some(XmlDefect {
            line: 1,
            message: "no XML elements found — not a plist document".to_string(),
        });
    }
    None
}

/// Decode the entity references XML predefines, plus numeric ones.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..semi];
        let decoded = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "apos" => Some('\''),
            "quot" => Some('"'),
            _ => ent.strip_prefix('#').and_then(|n| {
                let code = match n.strip_prefix(['x', 'X']) {
                    Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                    None => n.parse::<u32>().ok()?,
                };
                char::from_u32(code)
            }),
        };
        match decoded {
            Some(c) => {
                out.push(c);
                rest = &rest[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Index just past the markup construct starting at `i` (which must be `<`).
fn markup_end(b: &[u8], i: usize) -> usize {
    let close = |needle: &[u8], from: usize, skip: usize| {
        find_from(b, from, needle).map_or(b.len(), |e| e + skip)
    };
    if b[i..].starts_with(b"<!--") {
        return close(b"-->", i + 4, 3);
    }
    if b[i..].starts_with(b"<![CDATA[") {
        return close(b"]]>", i + 9, 3);
    }
    if b[i..].starts_with(b"<?") {
        return close(b"?>", i + 2, 2);
    }
    // An element tag or a declaration: to the next `>` outside quotes.
    let mut j = i + 1;
    let mut quote: Option<u8> = None;
    while j < b.len() {
        match quote {
            Some(q) if b[j] == q => quote = None,
            Some(_) => {}
            None => match b[j] {
                b'"' | b'\'' => quote = Some(b[j]),
                b'>' => return j + 1,
                _ => {}
            },
        }
        j += 1;
    }
    b.len()
}

/// A canonical form of an XML profile, for equality comparison only.
///
/// Two profiles differing only in XML escaping (`&quot;` where the other uses a
/// literal `"`) or in insignificant whitespace decode to the same document and
/// Fleet delivers them identically — so comparing bytes reports a difference
/// that does not exist. This decodes text content and collapses whitespace runs
/// while copying markup verbatim.
///
/// Text and markup are held in separate regions by a sentinel, so decoded text
/// can never be mistaken for markup: `&lt;b&gt;` and `<b>` stay distinct.
pub fn canonical_profile(xml: &str) -> String {
    const SENTINEL: char = '\u{1}';
    let b = xml.as_bytes();
    let mut out = String::with_capacity(xml.len());
    let mut text = String::new();
    let mut i = 0usize;

    fn flush(out: &mut String, text: &mut String) {
        let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if !collapsed.is_empty() {
            out.push(SENTINEL);
            out.push_str(&collapsed);
            out.push(SENTINEL);
        }
        text.clear();
    }

    while i < b.len() {
        if b[i] == b'<' {
            let end = markup_end(b, i);
            if b[i..].starts_with(b"<![CDATA[") {
                // CDATA is text, not markup.
                let inner = i + 9;
                let stop = end.saturating_sub(3).max(inner);
                if let Ok(chunk) = std::str::from_utf8(&b[inner..stop]) {
                    text.push_str(chunk);
                }
            } else if !b[i..].starts_with(b"<!--") {
                flush(&mut out, &mut text);
                if let Ok(markup) = std::str::from_utf8(&b[i..end]) {
                    // Collapse whitespace inside the tag too, so reflowed
                    // attributes do not read as a difference.
                    out.push_str(&markup.split_whitespace().collect::<Vec<_>>().join(" "));
                }
            }
            // Comments carry no delivered content — dropped entirely.
            i = end;
            continue;
        }
        let next = b[i..].iter().position(|&c| c == b'<').map_or(b.len(), |p| p + i);
        if let Ok(chunk) = std::str::from_utf8(&b[i..next]) {
            text.push_str(&decode_entities(chunk));
        }
        i = next;
    }
    flush(&mut out, &mut text);
    out
}

/// Whether `s` has the syntactic shape of a UUID: 8-4-4-4-12 hexadecimal.
///
/// Case-insensitive — Apple's tooling emits upper-case, hand-edits often lower.
/// Templated brand tokens such as `ZZ000001-…` or `WXYZ0001-…` fail here
/// because `Z`, `W`, `X` and `Y` are not hex digits.
pub fn is_valid_payload_uuid(s: &str) -> bool {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut parts = s.split('-');
    for want in GROUPS {
        let Some(part) = parts.next() else {
            return false;
        };
        if part.len() != want || !part.bytes().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
    }
    parts.next().is_none()
}

/// The outcome of inspecting one profile artifact on disk.
///
/// Ordering is the point: a parse failure suppresses every downstream check,
/// and a format flint cannot read is reported rather than skipped — a silent
/// skip makes a green run indistinguishable from an unchecked one.
#[derive(Debug, PartialEq, Eq)]
pub enum ProfileScan {
    /// Well-formed XML plist, with the top-level payload metadata.
    Xml(Box<ProfileInfo>),
    /// Valid JSON DDM declaration.
    Declaration(Box<DeclarationInfo>),
    /// A recognised format flint does not parse (binary plist, signed DER).
    /// Nothing to check — but the caller must still say so.
    Opaque { kind: &'static str },
    /// The artifact does not parse.
    Malformed { line: usize, message: String },
}

/// Inspect a `.mobileconfig`'s raw bytes.
///
/// Reads bytes rather than a `String` deliberately: a signed or binary-plist
/// profile is not valid UTF-8, and `read_to_string` would fail and tempt the
/// caller into the silent `continue` this rule exists to abolish.
pub fn scan_mobileconfig(bytes: &[u8]) -> ProfileScan {
    // Apple's binary plist magic.
    if bytes.starts_with(b"bplist00") {
        return ProfileScan::Opaque { kind: "binary plist" };
    }
    // PKCS#7/CMS signed profile — a DER SEQUENCE.
    if bytes.first() == Some(&0x30) {
        return ProfileScan::Opaque { kind: "signed (DER/PKCS#7) profile" };
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return ProfileScan::Malformed {
            line: 1,
            message: "not valid UTF-8 text, and not a recognised binary plist or signed profile"
                .to_string(),
        };
    };
    match check_xml_well_formed(text) {
        Some(defect) => ProfileScan::Malformed { line: defect.line, message: defect.message },
        None => ProfileScan::Xml(Box::new(parse_mobileconfig(text))),
    }
}

/// Inspect a DDM declaration's raw bytes.
///
/// `serde_json` is used for the error path only — it reports a line and column,
/// where [`parse_declaration`]'s `serde_yaml` fallback silently yields an empty
/// [`DeclarationInfo`].
pub fn scan_declaration(bytes: &[u8]) -> ProfileScan {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return ProfileScan::Malformed {
            line: 1,
            message: "not valid UTF-8 text".to_string(),
        };
    };
    match serde_json::from_str::<serde_json::Value>(text) {
        Ok(_) => ProfileScan::Declaration(Box::new(parse_declaration(text))),
        Err(e) => ProfileScan::Malformed {
            line: e.line().max(1),
            message: format!("invalid JSON: {}", e),
        },
    }
}

/// Whether `path` names a DDM declaration flint should validate as JSON.
///
/// Deliberately narrow: the repo is full of `.json` that is not a declaration
/// (`.dep.json` enrollment profiles, lockfiles, fixtures). Only a file that
/// actually carries a DDM `Type` is treated as one, so a malformed *anything
/// else* is not misreported as a broken declaration.
pub fn looks_like_declaration(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    text.contains("\"Type\"")
        && (text.contains("com.apple.configuration.") || text.contains("com.apple.activation."))
}

/// Scan one artifact and turn the outcome into findings reported **on that
/// artifact**, with a line number where one is known.
///
/// Reporting on the profile rather than on each referencing fleet is what makes
/// the finding appear once instead of once per fleet, lets orphaned artifacts be
/// checked at all, and gives editors somewhere to jump to.
#[cfg(test)]
pub(crate) fn scan_and_report(path: &Path) -> Vec<LintError> {
    match std::fs::read(path) {
        Ok(bytes) => scan_bytes_and_report(path, &bytes),
        Err(_) => vec![LintError::warning(
            format!("could not read '{}' — profile checks did not run", path.display()),
            path,
        )
        .with_rule_code(crate::codes::PROFILE_WELL_FORMED)],
    }
}

/// [`scan_and_report`] over bytes the caller already holds, so a workspace
/// pass that has read the file for another rule does not read it again.
pub(crate) fn scan_bytes_and_report(path: &Path, bytes: &[u8]) -> Vec<LintError> {
    let report_on = path;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();

    let scan = if is_mobileconfig(path) {
        scan_mobileconfig(bytes)
    } else if looks_like_declaration(bytes) {
        scan_declaration(bytes)
    } else {
        // A `.json` that is not a DDM declaration (`.dep.json`, a lockfile) is
        // not this rule's business.
        return Vec::new();
    };

    match scan {
        ProfileScan::Malformed { line, message } => {
            let what = if is_mobileconfig(path) { "well-formed XML" } else { "valid JSON" };
            vec![LintError::error(
                format!("{name} is not {what} (line {line}): {message}"),
                report_on,
            )
            .with_rule_code(crate::codes::PROFILE_WELL_FORMED)
            .with_location(line, 1)
            .with_help(
                "fleetctl gitops rejects this artifact at apply time. Until it parses, every \
                 other profile-level check is skipped for this file — duplicate-payload-uuid \
                 and payload-uuid-format did not run against it.",
            )]
        }
        ProfileScan::Opaque { kind } => {
            // Reported, not skipped: flint must never let "unreadable" pass as
            // "checked". Info, because the format is legitimate.
            vec![LintError::info(
                format!(
                    "{name} is a {kind}; flint reads XML plists only, so its \
                     well-formedness and PayloadUUID were not checked"
                ),
                report_on,
            )
            .with_rule_code(crate::codes::PROFILE_WELL_FORMED)
            .with_help(
                "Not a defect. Keep an unsigned XML plist in the repo if you want flint \
                 to validate it — Fleet signs and delivers it either way.",
            )]
        }
        ProfileScan::Declaration(_) => Vec::new(),
        ProfileScan::Xml(info) => match info.uuid.as_deref() {
            Some(uuid) if !is_valid_payload_uuid(uuid) => vec![LintError::warning(
                format!(
                    "{name} has PayloadUUID '{uuid}', which is not a valid UUID \
                     (expected 8-4-4-4-12 hexadecimal)"
                ),
                report_on,
            )
            .with_rule_code(crate::codes::PAYLOAD_UUID_FORMAT)
            .with_help(
                "Advisory, and never auto-fixed: Fleet accepts this today, and a PayloadUUID \
                 is part of the profile's identity — rewriting it makes Fleet re-deliver the \
                 profile to every enrolled host in the team.",
            )],
            _ => Vec::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Severity;
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

    // -----------------------------------------------------------------------
    // profile-well-formed: XML well-formedness
    // -----------------------------------------------------------------------

    /// A profile carrying `body` as the display-name text, with a valid UUID.
    fn profile_with_text(body: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
             <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \
             \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
             <plist version=\"1.0\">\n\
             <dict>\n\
             <key>PayloadDisplayName</key><string>{body}</string>\n\
             <key>PayloadUUID</key><string>95702CD6-A76F-466C-9F07-711416585D76</string>\n\
             </dict>\n\
             </plist>"
        )
    }

    #[test]
    fn well_formed_profile_is_accepted() {
        assert_eq!(check_xml_well_formed(SAMPLE), None);
        assert_eq!(check_xml_well_formed(&profile_with_text("Plain Brand Name")), None);
    }

    /// The exact defect that failed `fleetctl gitops` for ABC - WXYZ: one raw
    /// ampersand inside a brand name.
    #[test]
    fn raw_ampersand_in_text_is_rejected() {
        let xml = profile_with_text("Example Holding Foo & Bar GmbH");
        let defect = check_xml_well_formed(&xml).expect("raw & must be reported");
        assert!(
            defect.message.contains("&amp;"),
            "message should name the escape: {}",
            defect.message
        );
        // The display-name line, not line 1 — the caller prints this.
        assert_eq!(defect.line, 5, "got: {defect:?}");
    }

    #[test]
    fn escaped_ampersand_is_accepted() {
        let xml = profile_with_text("Example Holding Foo &amp; Bar GmbH");
        assert_eq!(check_xml_well_formed(&xml), None);
    }

    #[test]
    fn all_predefined_and_numeric_entities_are_accepted() {
        let xml = profile_with_text("&amp; &lt; &gt; &apos; &quot; &#233; &#x1F600; &#X2F;");
        assert_eq!(check_xml_well_formed(&xml), None);
    }

    #[test]
    fn undefined_entity_is_rejected() {
        // No DTD in a .mobileconfig, so `&nbsp;` is undefined.
        let xml = profile_with_text("Taste&nbsp;More");
        let defect = check_xml_well_formed(&xml).expect("undefined entity must be reported");
        assert!(defect.message.contains("nbsp"), "got: {}", defect.message);
    }

    #[test]
    fn trailing_ampersand_at_eof_is_rejected() {
        assert!(check_xml_well_formed("<a>x &").is_some());
    }

    #[test]
    fn mismatched_closing_tag_is_rejected() {
        let defect = check_xml_well_formed("<plist><dict></plist></dict>")
            .expect("mismatched tags must be reported");
        assert!(defect.message.contains("dict"), "got: {}", defect.message);
        assert!(defect.message.contains("plist"), "got: {}", defect.message);
    }

    #[test]
    fn unclosed_element_is_rejected() {
        let defect =
            check_xml_well_formed("<plist>\n<dict>\n</plist>").expect("unclosed must be reported");
        assert!(defect.message.contains("dict"), "got: {}", defect.message);
    }

    #[test]
    fn stray_closing_tag_is_rejected() {
        assert!(check_xml_well_formed("<a></a></b>").is_some());
    }

    #[test]
    fn unterminated_comment_is_rejected() {
        let defect = check_xml_well_formed("<a><!-- never closed </a>")
            .expect("unterminated comment must be reported");
        assert!(defect.message.contains("comment"), "got: {}", defect.message);
    }

    /// Comments and CDATA are not parsed for entities — a raw `&` inside one
    /// is legal and must not be flagged.
    #[test]
    fn ampersand_inside_comment_or_cdata_is_fine() {
        assert_eq!(check_xml_well_formed("<a><!-- Foo & Bar --></a>"), None);
        assert_eq!(check_xml_well_formed("<a><![CDATA[Foo & Bar]]></a>"), None);
    }

    #[test]
    fn angle_bracket_inside_attribute_does_not_end_the_tag() {
        assert_eq!(check_xml_well_formed(r#"<a title="1 > 0" alt='x>y'>t</a>"#), None);
    }

    #[test]
    fn self_closing_elements_do_not_need_a_close() {
        assert_eq!(check_xml_well_formed("<plist><array/><dict/></plist>"), None);
    }

    #[test]
    fn raw_left_angle_in_text_is_rejected() {
        assert!(check_xml_well_formed("<a>1 < 2</a>").is_some());
    }

    #[test]
    fn document_without_elements_is_rejected() {
        assert!(check_xml_well_formed("<?xml version=\"1.0\"?>\n").is_some());
        assert!(check_xml_well_formed("").is_some());
    }

    #[test]
    fn doctype_with_internal_subset_is_skipped() {
        let xml = "<!DOCTYPE plist [ <!ELEMENT plist (#PCDATA)> ]>\n<plist>x</plist>";
        assert_eq!(check_xml_well_formed(xml), None);
    }

    /// Regression for the scanner this rule exists to backstop: a malformed
    /// profile parses to an empty `ProfileInfo` rather than failing, so
    /// without `check_xml_well_formed` nothing would ever notice.
    #[test]
    fn parse_mobileconfig_cannot_fail_on_malformed_input() {
        let xml = profile_with_text("Foo & Bar");
        // The scanner still returns something...
        let info = parse_mobileconfig(&xml);
        assert!(info.uuid.is_some());
        // ...which is exactly why the explicit check is required.
        assert!(check_xml_well_formed(&xml).is_some());
    }

    // -----------------------------------------------------------------------
    // canonical_profile — equality that ignores escaping, not meaning
    // -----------------------------------------------------------------------

    #[test]
    fn escaping_only_difference_canonicalizes_equal() {
        let escaped = "<plist><dict><key>D</key><string>Call &quot;Servicedesk&quot;</string></dict></plist>";
        let literal = "<plist><dict><key>D</key><string>Call \"Servicedesk\"</string></dict></plist>";
        assert_ne!(escaped, literal, "the fixture must differ in bytes");
        assert_eq!(canonical_profile(escaped), canonical_profile(literal));
    }

    /// The sentinel earns its keep: escaped markup in text must never
    /// canonicalize to the same thing as real markup.
    #[test]
    fn escaped_markup_is_not_equal_to_real_markup() {
        assert_ne!(
            canonical_profile("<a>&lt;b&gt;</a>"),
            canonical_profile("<a><b></b></a>")
        );
    }

    #[test]
    fn insignificant_whitespace_is_collapsed() {
        assert_eq!(
            canonical_profile("<dict>\n  <key>A</key>\n</dict>"),
            canonical_profile("<dict><key>A</key></dict>")
        );
    }

    #[test]
    fn comments_do_not_affect_equality() {
        assert_eq!(
            canonical_profile("<a><!-- generated 2026 --><b>x</b></a>"),
            canonical_profile("<a><b>x</b></a>")
        );
    }

    #[test]
    fn cdata_and_escaped_text_agree() {
        assert_eq!(
            canonical_profile("<a><![CDATA[Foo & Bar]]></a>"),
            canonical_profile("<a>Foo &amp; Bar</a>")
        );
    }

    #[test]
    fn genuinely_different_text_stays_different() {
        assert_ne!(
            canonical_profile("<a>Servicedesk</a>"),
            canonical_profile("<a>Helpdesk</a>")
        );
    }

    #[test]
    fn numeric_entities_decode() {
        assert_eq!(decode_entities("caf&#233; &#x41;&amp;B"), "café A&B");
        // A bare `&` and an unknown entity are left alone rather than eaten.
        assert_eq!(decode_entities("A & B"), "A & B");
        assert_eq!(decode_entities("&nbsp;"), "&nbsp;");
    }

    // -----------------------------------------------------------------------
    // payload-uuid-format
    // -----------------------------------------------------------------------

    #[test]
    fn valid_uuids_are_accepted() {
        // Hand-maintained ACME profile — the one correct case in the repo.
        assert!(is_valid_payload_uuid("95702CD6-A76F-466C-9F07-711416585D76"));
        // Case-insensitive.
        assert!(is_valid_payload_uuid("95702cd6-a76f-466c-9f07-711416585d76"));
        assert!(is_valid_payload_uuid("00000000-0000-0000-0000-000000000000"));
    }

    /// The real templated brand tokens `contour profile validate` flagged.
    #[test]
    fn non_hex_brand_tokens_are_rejected() {
        for uuid in [
            "ZZ000001-0001-4A01-8B01-000000000002",   // Z
            "QMP00001-0001-4A01-8B01-000000000002",   // Q, M, P
            "NPQR0001-0001-4A01-8B01-000000000002",   // N, P, Q, R
            "WXYZ0001-0001-4A01-8B01-000000000002",   // W, X, Y, Z
        ] {
            assert!(!is_valid_payload_uuid(uuid), "{uuid} must be rejected");
        }
    }

    #[test]
    fn wrong_group_lengths_are_rejected() {
        // GHI's first group is 7 characters, not 8.
        assert!(!is_valid_payload_uuid("CFC0001-0001-4A01-8B01-000000000002"));
        // Too few groups, too many groups, and the legacy test placeholder.
        assert!(!is_valid_payload_uuid("95702CD6-A76F-466C-9F07"));
        assert!(!is_valid_payload_uuid("95702CD6-A76F-466C-9F07-711416585D76-EXTRA"));
        assert!(!is_valid_payload_uuid("SAME-UUID"));
        assert!(!is_valid_payload_uuid(""));
    }

    // -----------------------------------------------------------------------
    // scan_* — the file-reading path (gaps the string-only tests missed)
    // -----------------------------------------------------------------------

    #[test]
    fn binary_plist_is_reported_as_opaque_not_malformed() {
        let bytes = b"bplist00\xd1\x01\x02_\x10\x0bPayloadUUID";
        assert_eq!(
            scan_mobileconfig(bytes),
            ProfileScan::Opaque { kind: "binary plist" }
        );
    }

    #[test]
    fn signed_der_profile_is_reported_as_opaque() {
        // PKCS#7 ContentInfo: a DER SEQUENCE.
        let bytes = b"\x30\x82\x04\x12\x06\x09\x2a\x86\x48\x86\xf7\x0d\x01\x07\x02";
        assert_eq!(
            scan_mobileconfig(bytes),
            ProfileScan::Opaque { kind: "signed (DER/PKCS#7) profile" }
        );
    }

    /// Non-UTF-8 that is neither a binary plist nor DER must be an error, not
    /// a silent skip — the whole point of reading bytes instead of a String.
    #[test]
    fn undecodable_garbage_is_malformed_not_skipped() {
        let bytes = b"\xff\xfe\x00garbage";
        assert!(matches!(
            scan_mobileconfig(bytes),
            ProfileScan::Malformed { .. }
        ));
    }

    #[test]
    fn xml_profile_scans_to_its_info() {
        let xml = profile_with_text("Acme");
        match scan_mobileconfig(xml.as_bytes()) {
            ProfileScan::Xml(info) => {
                assert_eq!(info.uuid.as_deref(), Some("95702CD6-A76F-466C-9F07-711416585D76"));
            }
            other => panic!("expected Xml, got {other:?}"),
        }
    }

    #[test]
    fn malformed_declaration_reports_a_line() {
        let bad = br#"{ "Type": "com.apple.configuration.passcode.settings", "Identifier": "#;
        match scan_declaration(bad) {
            ProfileScan::Malformed { line, message } => {
                assert!(line >= 1);
                assert!(message.contains("invalid JSON"), "got: {message}");
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    #[test]
    fn valid_declaration_scans_clean() {
        let ok = br#"{ "Type": "com.apple.configuration.passcode.settings", "Identifier": "com.x.p", "Payload": {} }"#;
        assert!(matches!(scan_declaration(ok), ProfileScan::Declaration(_)));
    }

    /// Only real DDM declarations are validated — the repo is full of other
    /// `.json` that must not be misreported.
    #[test]
    fn only_ddm_declarations_are_treated_as_declarations() {
        assert!(looks_like_declaration(
            br#"{"Type":"com.apple.configuration.passcode.settings"}"#
        ));
        assert!(looks_like_declaration(
            br#"{"Type":"com.apple.activation.simple"}"#
        ));
        // An ADE enrollment profile and a lockfile are not declarations.
        assert!(!looks_like_declaration(
            br#"{"profile_name":"ABC","await_device_configured":true}"#
        ));
        assert!(!looks_like_declaration(br#"{"lockfileVersion":3}"#));
    }

    // -----------------------------------------------------------------------
    // scan_and_report — findings land on the artifact, with a line
    // -----------------------------------------------------------------------

    fn write(tmp: &TempDir, name: &str, body: &[u8]) -> PathBuf {
        let p = tmp.path().join(name);
        fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn valid_profile_yields_no_findings() {
        let tmp = TempDir::new().unwrap();
        let p = write(&tmp, "ok.mobileconfig", profile_with_text("Acme GmbH").as_bytes());
        assert!(scan_and_report(&p).is_empty());
    }

    #[test]
    fn malformed_profile_is_an_error_on_the_profile_itself() {
        let tmp = TempDir::new().unwrap();
        let p = write(
            &tmp,
            "general-information-WXYZ.mobileconfig",
            profile_with_text("Example Holding Foo & Bar GmbH").as_bytes(),
        );
        let errors = scan_and_report(&p);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        let e = &errors[0];
        assert_eq!(e.severity, Severity::Error);
        assert_eq!(e.rule_code, Some(crate::codes::PROFILE_WELL_FORMED));
        // Reported ON the profile, not on some referencing fleet...
        assert!(e.file.ends_with("general-information-WXYZ.mobileconfig"));
        // ...and pointing at the offending line, so an editor can jump.
        assert_eq!(e.span.as_ref().map(|s| s.line), Some(5));
        assert!(e.message.contains("&amp;"), "got: {}", e.message);
    }

    /// "Never skip silently": the error names the checks it displaced.
    #[test]
    fn malformed_profile_error_names_the_skipped_rules() {
        let tmp = TempDir::new().unwrap();
        let p = write(&tmp, "bad.mobileconfig", profile_with_text("Foo & Bar").as_bytes());
        let help = scan_and_report(&p)[0].help.clone().unwrap_or_default();
        assert!(help.contains("duplicate-payload-uuid"), "got: {help}");
        assert!(help.contains("payload-uuid-format"), "got: {help}");
    }

    /// A malformed profile yields exactly one finding — never a second derived
    /// from the parse that just failed.
    #[test]
    fn malformed_profile_suppresses_the_uuid_check() {
        let tmp = TempDir::new().unwrap();
        let body = "<?xml version=\"1.0\"?>\n<plist><dict>\n\
                    <key>PayloadUUID</key><string>WXYZ0001-0001-4A01-8B01-000000000002</string>\n\
                    <key>PayloadDisplayName</key><string>Foo & Bar</string>\n\
                    </dict></plist>";
        let p = write(&tmp, "bad.mobileconfig", body.as_bytes());
        let errors = scan_and_report(&p);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        assert_eq!(errors[0].rule_code, Some(crate::codes::PROFILE_WELL_FORMED));
    }

    #[test]
    fn bad_uuid_is_a_warning_that_is_never_fixable() {
        let tmp = TempDir::new().unwrap();
        let body = "<?xml version=\"1.0\"?>\n<plist><dict>\n\
                    <key>PayloadUUID</key><string>WXYZ0001-0001-4A01-8B01-000000000002</string>\n\
                    </dict></plist>";
        let p = write(&tmp, "corp-dock-WXYZ.mobileconfig", body.as_bytes());
        let errors = scan_and_report(&p);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        let e = &errors[0];
        assert_eq!(e.severity, Severity::Warning, "Fleet accepts these today");
        assert_eq!(e.rule_code, Some(crate::codes::PAYLOAD_UUID_FORMAT));
        assert!(e.message.contains("WXYZ0001-0001-4A01-8B01-000000000002"));
        assert!(
            e.fix.is_none(),
            "a PayloadUUID must never be auto-rewritten — it re-delivers the profile"
        );
        assert!(e.help.as_deref().unwrap_or_default().contains("re-deliver"));
    }

    /// Gap that shipped in the first cut: a binary/signed profile is not valid
    /// UTF-8, so `read_to_string` failed and the file was skipped in silence.
    #[test]
    fn binary_profile_is_reported_as_info_never_skipped() {
        let tmp = TempDir::new().unwrap();
        let p = write(&tmp, "signed.mobileconfig", b"bplist00\xd1\x01\x02_\x10\x0bPayloadUUID");
        let errors = scan_and_report(&p);
        assert_eq!(errors.len(), 1, "a skip must still be reported: {errors:?}");
        assert_eq!(errors[0].severity, Severity::Info, "the format is legitimate");
        assert!(errors[0].message.contains("binary plist"));
        assert!(errors[0].message.contains("not checked"), "got: {}", errors[0].message);
    }

    #[test]
    fn malformed_declaration_is_an_error() {
        let tmp = TempDir::new().unwrap();
        let body = br#"{ "Type": "com.apple.configuration.passcode.settings", "Identifier": "#;
        let p = write(&tmp, "passcode.json", body);
        let errors = scan_and_report(&p);
        assert_eq!(errors.len(), 1, "got: {errors:?}");
        assert_eq!(errors[0].severity, Severity::Error);
        assert!(errors[0].message.contains("valid JSON"), "got: {}", errors[0].message);
    }

    #[test]
    fn non_declaration_json_is_left_alone() {
        let tmp = TempDir::new().unwrap();
        let p = write(&tmp, "automatic-enrollment-ABC.dep.json", br#"{"profile_name":"ABC"}"#);
        assert!(scan_and_report(&p).is_empty());
    }
}
