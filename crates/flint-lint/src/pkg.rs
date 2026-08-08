//! macOS `.pkg` metadata extraction.
//!
//! A `.pkg` is a `xar` archive whose `Distribution` (product archive) or
//! `PackageInfo` (component) XML carries the package identifier and version.
//! This module parses that XML; the CLI shells out to `xar`/`shasum` to obtain
//! it and the file's SHA-256, then formats a Fleet software metadata block:
//!
//! ```text
//! # nl.root3.support (Support.3.0.3.pkg) version 3.0.3
//! - hash_sha256: 8c30a711…70088c
//! ```

/// Package identifier and version parsed from a `.pkg`'s XML metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PkgInfo {
    pub identifier: Option<String>,
    pub version: Option<String>,
}

/// Parse `identifier` and `version` from a `.pkg`'s `Distribution` or
/// `PackageInfo` XML. Tolerant string scan (no XML dependency): reads
/// `pkg-ref`, `product`, and `pkg-info` elements.
pub fn parse_pkg_metadata(xml: &str) -> PkgInfo {
    let mut info = PkgInfo::default();

    // Product archive: <pkg-ref id="…" version="…">, <product version="…"/>.
    for tag in tags(xml, "pkg-ref") {
        if info.identifier.is_none() {
            if let Some(id) = attr(&tag, "id") {
                if !id.is_empty() {
                    info.identifier = Some(id);
                }
            }
        }
        if info.version.is_none() {
            if let Some(v) = attr(&tag, "version") {
                if !v.is_empty() {
                    info.version = Some(v);
                }
            }
        }
    }
    if info.version.is_none() {
        for tag in tags(xml, "product") {
            if let Some(v) = attr(&tag, "version") {
                info.version = Some(v);
                break;
            }
        }
    }

    // Component package: <pkg-info identifier="…" version="…">.
    for tag in tags(xml, "pkg-info") {
        if info.identifier.is_none() {
            if let Some(id) = attr(&tag, "identifier") {
                info.identifier = Some(id);
            }
        }
        if info.version.is_none() {
            if let Some(v) = attr(&tag, "version") {
                info.version = Some(v);
            }
        }
    }

    info
}

/// A friendly default display name from a package identifier: the last
/// meaningful dot-segment, skipping a trailing `pkg` (e.g.
/// `io.declarative.flint.pkg` → `flint`, `nl.root3.support` → `support`).
fn display_name(id: &str) -> &str {
    id.rsplit('.')
        .find(|s| !s.is_empty() && !s.eq_ignore_ascii_case("pkg"))
        .unwrap_or(id)
}

/// Format the Fleet software metadata block for a package.
pub fn metadata_block(info: &PkgInfo, filename: &str, sha256: &str) -> String {
    let id = info.identifier.as_deref().unwrap_or("unknown.identifier");
    let version = info.version.as_deref().unwrap_or("unknown");
    format!("# {id} ({filename}) version {version}\n- hash_sha256: {sha256}")
}

/// Format a full Fleet software-package stanza with a `url:` placeholder and
/// commented-out optional fields. The active fields (`url`, `hash_sha256`,
/// `self_service`) form a valid custom-package entry; the commented lines are
/// schema-correct scaffolding to fill in.
pub fn metadata_block_full(info: &PkgInfo, filename: &str, sha256: &str) -> String {
    let id = info.identifier.as_deref().unwrap_or("unknown.identifier");
    let version = info.version.as_deref().unwrap_or("unknown");
    // A friendly default display name from the identifier's last segment.
    let display = display_name(id);
    format!(
        "# {id} ({filename}) version {version}\n\
         - url: https://REPLACE-ME.example.com/{filename}  # TODO: host the installer; Fleet downloads from this URL\n\
         \x20 hash_sha256: {sha256}\n\
         \x20 self_service: false\n\
         \x20 # Optional — uncomment and edit:\n\
         \x20 # display_name: \"{display}\"\n\
         \x20 # categories:\n\
         \x20 #   - Productivity\n\
         \x20 # labels_include_any:   # install on hosts matching ANY of these labels\n\
         \x20 #   - \"Label name\"\n\
         \x20 # labels_exclude_any:   # and/or exclude hosts matching ANY of these (combinable with include)\n\
         \x20 #   - \"Label name\"\n\
         \x20 # install_script:\n\
         \x20 #   path: ../platforms/macos/scripts/install.sh\n\
         \x20 # uninstall_script:\n\
         \x20 #   path: ../platforms/macos/scripts/uninstall.sh"
    )
}

/// Format a complete **standalone** Fleet software file — the top-level
/// mapping shape that a fleet/team file references via
/// `software.packages: [ - path: <this file> ]`. Unlike
/// [`metadata_block_full`], this is a whole file, not a list item: the keys
/// live at column 0 and the leading `- ` is gone. The `hash_sha256` (the value
/// `flint pkg` actually computes) is the payload; `url` is a fill-in
/// placeholder. With `full`, the commented optional fields are appended.
pub fn metadata_file(info: &PkgInfo, filename: &str, sha256: &str, full: bool) -> String {
    let id = info.identifier.as_deref().unwrap_or("unknown.identifier");
    let version = info.version.as_deref().unwrap_or("unknown");
    let display = display_name(id);
    let head = format!(
        "# {id} ({filename}) version {version}\n\
         hash_sha256: {sha256}\n\
         url: https://REPLACE-ME.example.com/{filename}  # TODO: host the installer; Fleet downloads from this URL\n\
         self_service: false"
    );
    if !full {
        return head;
    }
    format!(
        "{head}\n\
         # Optional — uncomment and edit:\n\
         # display_name: \"{display}\"\n\
         # categories:\n\
         #   - Productivity\n\
         # labels_include_any:   # install on hosts matching ANY of these labels\n\
         #   - \"Label name\"\n\
         # labels_exclude_any:   # and/or exclude hosts matching ANY of these (combinable with include)\n\
         #   - \"Label name\"\n\
         # install_script:\n\
         #   path: ../platforms/macos/scripts/install.sh\n\
         # uninstall_script:\n\
         #   path: ../platforms/macos/scripts/uninstall.sh"
    )
}

/// A Fleet policy that verifies the package is installed (and, when a version
/// is known, at that version or newer) via the macOS `package_receipts` table.
///
/// A Fleet policy is "compliant" when the query returns a row, so this passes
/// when a receipt for the package id exists meeting the version bar. Uses
/// osquery's built-in `version_compare` (semantic) for correct ordering — a
/// plain string `>=` would mis-rank e.g. 3.0.3 vs 3.0.10.
///
/// When `install_hash` is `Some(sha)`, an `install_software` automation
/// (referencing the package by that sha256) is appended, turning the policy
/// from a passive audit into an enforcement policy: Fleet auto-installs the
/// package on any host that *fails* the query. The package must also be listed
/// under `software` in the same team (by that hash).
pub fn install_policy(
    info: &PkgInfo,
    filename: &str,
    install_hash: Option<&str>,
    outdated_only: bool,
) -> String {
    build_policy(
        info,
        filename,
        "package_receipts",
        "package_id",
        "version",
        install_hash,
        outdated_only,
    )
}

/// Like [`install_policy`], but checks the macOS `apps` table by
/// `bundle_identifier` / `bundle_short_version` (for packages that install a
/// `.app`). Note: a `.pkg`'s package_id may differ from the app bundle id —
/// verify the identifier against the installed app.
pub fn install_policy_apps(
    info: &PkgInfo,
    filename: &str,
    install_hash: Option<&str>,
    outdated_only: bool,
) -> String {
    build_policy(
        info,
        filename,
        "apps",
        "bundle_identifier",
        "bundle_short_version",
        install_hash,
        outdated_only,
    )
}

/// Shared policy builder for the receipts and apps variants. When
/// `install_hash` is set, append the `install_software` automation block so a
/// failing host triggers an install (see [`install_policy`]).
///
/// The version query uses `EXISTS … AND NOT EXISTS (… < target)` rather than a
/// single `version_compare(…) >= 0`: a package can have multiple receipts, and
/// the latter passes if *any* copy is current even when an older one lingers.
/// The `EXISTS/NOT EXISTS` form passes only when the package is installed and no
/// copy is older than the target. With `outdated_only`, the `EXISTS` clause is
/// dropped so the policy passes when the package is *absent or current* and
/// fails only on an outdated copy — i.e. patch existing installs without forcing
/// a fresh install on hosts that don't have it.
fn build_policy(
    info: &PkgInfo,
    filename: &str,
    table: &str,
    id_col: &str,
    ver_col: &str,
    install_hash: Option<&str>,
    outdated_only: bool,
) -> String {
    let id = info.identifier.as_deref().unwrap_or("REPLACE.identifier");
    let base = base_name(filename, id);

    let (name, query, desc) = match info.version.as_deref() {
        // Pass if absent OR current; fail only when an outdated copy exists.
        Some(v) if outdated_only => (
            format!("{base} is up to date"),
            format!("SELECT 1 WHERE NOT EXISTS (SELECT 1 FROM {table} WHERE {id_col} = '{id}' AND version_compare({ver_col}, '{v}') < 0);"),
            format!("Passes when {id} is absent or at {v} or newer; fails only if an outdated copy is installed."),
        ),
        // Pass only if installed AND no copy is older than the target.
        Some(v) => (
            format!("{base} is installed and up to date"),
            format!("SELECT 1 WHERE EXISTS (SELECT 1 FROM {table} WHERE {id_col} = '{id}') AND NOT EXISTS (SELECT 1 FROM {table} WHERE {id_col} = '{id}' AND version_compare({ver_col}, '{v}') < 0);"),
            format!("Passes when {id} is installed and no copy is older than {v}; fails if missing or outdated."),
        ),
        None => (
            format!("{base} is installed"),
            format!("SELECT 1 FROM {table} WHERE {id_col} = '{id}';"),
            format!("Passes when {id} is installed."),
        ),
    };

    assemble_policy(&name, &query, &desc, install_hash)
}

/// A Fleet policy that verifies install by checking a file exists on disk via
/// the macOS `file` table — a proxy for packages that don't register a clean
/// `package_receipts` row or `apps` bundle (fonts, configs, dropped files).
/// Existence-only (the `file` table has no version), so this is an "installed"
/// check; pass the path you expect the package to drop, e.g.
/// `/Applications/YourApp.app/Contents/Info.plist`.
pub fn install_policy_file(
    info: &PkgInfo,
    filename: &str,
    file_path: &str,
    install_hash: Option<&str>,
) -> String {
    let id = info.identifier.as_deref().unwrap_or("REPLACE.identifier");
    let base = base_name(filename, id);
    let name = format!("{base} is installed");
    let query = format!("SELECT 1 FROM file WHERE path = '{file_path}';");
    let desc = format!("Passes when {file_path} exists on disk.");
    assemble_policy(&name, &query, &desc, install_hash)
}

/// Friendly base name from a filename: drop the version/extension tail, else
/// fall back to the package identifier.
fn base_name<'a>(filename: &'a str, id: &'a str) -> &'a str {
    filename
        .strip_suffix(".pkg")
        .unwrap_or(filename)
        .split(|c: char| c.is_ascii_digit())
        .next()
        .map(|s| s.trim_end_matches(['-', '_', '.', ' ']))
        // `Name v3.0` splits to `Name v` — drop the dangling version marker.
        .map(|s| match s.strip_suffix(['v', 'V']) {
            Some(rest) if rest.ends_with([' ', '-', '_']) => {
                rest.trim_end_matches([' ', '-', '_'])
            }
            _ => s,
        })
        .filter(|s| !s.is_empty())
        .unwrap_or(id)
}

/// Assemble a policy stanza from its parts, appending the `install_software`
/// enforcement block (and adjusting the resolution text) when `install_hash`
/// is set.
fn assemble_policy(name: &str, query: &str, desc: &str, install_hash: Option<&str>) -> String {
    // Enforcement changes what "failing" means: Fleet remediates automatically.
    let resolution = if install_hash.is_some() {
        "Fleet automatically installs or updates the package on hosts that fail this policy."
    } else {
        "Install or update the package (e.g. via Fleet software)."
    };

    let mut stanza = format!(
        "- name: {name}\n  platform: darwin\n  query: \"{query}\"\n  description: \"{desc}\"\n  resolution: \"{resolution}\"\n  critical: false"
    );

    // The install_software automation links the policy to the package by its
    // sha256 — the same value `flint pkg` computes for the software entry.
    if let Some(sha) = install_hash {
        stanza.push_str(&format!(
            "\n  install_software:\n    hash_sha256: {sha}  # this package must also be listed under software (by this hash)"
        ));
    }

    stanza
}

/// Fleet's default install script for a macOS `.pkg` (verbatim from
/// fleetdm/fleet `pkg/file/scripts/install_pkg.sh`). Fleet sets
/// `$INSTALLER_PATH` to the downloaded package at run time.
pub fn default_install_script() -> String {
    "#!/bin/sh\n\ninstaller -pkg \"$INSTALLER_PATH\" -target /\n".to_string()
}

/// Fleet's default uninstall script for a macOS `.pkg` (verbatim from
/// fleetdm/fleet `pkg/file/scripts/uninstall_pkg.sh`). `$PACKAGE_ID` is
/// substituted by the Fleet server at upload time — do not hardcode it.
pub fn default_uninstall_script() -> String {
    r#"#!/bin/sh

# Fleet extracts and saves package IDs.
pkg_ids=$PACKAGE_ID

# For each package id, get all .app folders associated with the package and remove them.
for pkg_id in "${pkg_ids[@]}"
do
  # Get volume and location of the package.
  volume=$(pkgutil --pkg-info "$pkg_id" | grep -i "volume" | awk '{if (NF>1) print $NF}')
  location=$(pkgutil --pkg-info "$pkg_id" | grep -i "location" | awk '{if (NF>1) print $NF}')
  # Check if this package id corresponds to a valid/installed package
  if [[ ! -z "$volume" ]]; then
    # Remove individual directories that end with ".app" belonging to the package.
    # Only process directories that end with ".app" to prevent Fleet from removing top level directories.
    pkgutil --only-dirs --files "$pkg_id" | grep "\.app$" | sed -e 's@^@'"$volume""$location"'/@' | tr '\n' '\0' | xargs -n 1 -0 rm -rf
    # Remove receipts
    pkgutil --forget "$pkg_id"
  else
    echo "WARNING: volume is empty for package ID $pkg_id"
  fi
done
"#
    .to_string()
}

/// Refresh an existing software stanza in `source` for `identifier`, updating
/// its header comment (`# <id> (<file>) version <ver>`), `hash_sha256:`, and
/// the trailing filename of any `url:` in that entry. Returns the updated
/// source and `(old_version, old_sha256)`, or `None` if the identifier's header
/// comment is not found.
pub fn update_stanza(
    source: &str,
    identifier: &str,
    new_filename: &str,
    new_version: &str,
    new_sha256: &str,
) -> Option<(String, String, String)> {
    let mut lines: Vec<String> = source.lines().map(|l| l.to_string()).collect();

    // Locate the header comment for this identifier.
    let needle = format!("# {identifier} (");
    let hdr = lines.iter().position(|l| l.trim_start().starts_with(&needle))?;

    let old_version = parse_header_version(&lines[hdr]);
    let indent: String = lines[hdr].chars().take_while(|c| *c == ' ').collect();
    lines[hdr] = format!("{indent}# {identifier} ({new_filename}) version {new_version}");

    // Scan the stanza (until the next package header) and refresh hash + url.
    let mut old_sha256 = String::new();
    let mut updated_hash = false;
    let mut updated_url = false;
    let mut i = hdr + 1;
    while i < lines.len() {
        let trimmed = lines[i].trim_start();
        if trimmed.starts_with("# ") && trimmed.contains(" version ") {
            break; // next package's header
        }
        if !updated_hash && trimmed.starts_with("hash_sha256:") {
            old_sha256 = trimmed
                .trim_start_matches("hash_sha256:")
                .trim()
                .to_string();
            let ind: String = lines[i].chars().take_while(|c| *c == ' ').collect();
            lines[i] = format!("{ind}hash_sha256: {new_sha256}");
            updated_hash = true;
        }
        if !updated_url {
            if let Some(pos) = lines[i].find("url:") {
                let (prefix, rest) = lines[i].split_at(pos + "url:".len());
                let rest = rest.trim_start();
                let mut parts = rest.splitn(2, char::is_whitespace);
                let url_val = parts.next().unwrap_or("");
                let trailing = parts
                    .next()
                    .map(|t| format!("  {}", t.trim_start()))
                    .unwrap_or_default();
                let new_url = match url_val.rfind('/') {
                    Some(s) => format!("{}/{}", &url_val[..s], new_filename),
                    None => url_val.to_string(),
                };
                lines[i] = format!("{prefix} {new_url}{trailing}");
                updated_url = true;
            }
        }
        i += 1;
    }

    let mut out = lines.join("\n");
    if source.ends_with('\n') {
        out.push('\n');
    }
    Some((out, old_version, old_sha256))
}

/// Pull the version out of a `# <id> (<file>) version <ver>` header line.
fn parse_header_version(line: &str) -> String {
    line.rsplit_once(" version ")
        .map(|(_, v)| v.trim().to_string())
        .unwrap_or_default()
}

/// Collect the opening-tag text of every `<name …>` element.
fn tags(xml: &str, name: &str) -> Vec<String> {
    let open = format!("<{name}");
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find(&open) {
        let after = &rest[i..];
        // Require a delimiter after the name so `<product>` doesn't match
        // a hypothetical `<products>`.
        let next = after[open.len()..].chars().next();
        if !matches!(next, Some(' ') | Some('>') | Some('/') | Some('\t') | Some('\n')) {
            rest = &after[open.len()..];
            continue;
        }
        match after.find('>') {
            Some(end) => {
                out.push(after[..end].to_string());
                rest = &after[end..];
            }
            None => break,
        }
    }
    out
}

/// Extract the value of `name="…"` from a tag's text. Matches `name` only at
/// an attribute boundary, so `version="…"` is not found inside
/// `format-version="…"`.
fn attr(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let bytes = tag.as_bytes();
    let mut from = 0;
    while let Some(rel) = tag[from..].find(&needle) {
        let i = from + rel;
        let boundary = i == 0 || matches!(bytes[i - 1], b' ' | b'\t' | b'\n' | b'\r');
        if boundary {
            let start = i + needle.len();
            let rest = &tag[start..];
            let end = rest.find('"')?;
            return Some(rest[..end].to_string());
        }
        from = i + needle.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_product_distribution() {
        // Mirrors the real Support.3.0.3.pkg Distribution.
        let xml = r#"<installer-gui-script>
  <pkg-ref id="nl.root3.support"/>
  <choice id="default"><pkg-ref id="nl.root3.support"/></choice>
  <pkg-ref id="nl.root3.support" version="3.0.3" onConclusion="none" installKBytes="5940">#nl.root3.support.pkg</pkg-ref>
</installer-gui-script>"#;
        let info = parse_pkg_metadata(xml);
        assert_eq!(info.identifier.as_deref(), Some("nl.root3.support"));
        assert_eq!(info.version.as_deref(), Some("3.0.3"));
        assert_eq!(
            metadata_block(&info, "Support.3.0.3.pkg", "abc123"),
            "# nl.root3.support (Support.3.0.3.pkg) version 3.0.3\n- hash_sha256: abc123"
        );
    }

    #[test]
    fn test_parse_product_element_version() {
        let xml = r#"<installer-gui-script>
  <product id="com.example.app" version="2.1.0"/>
  <pkg-ref id="com.example.app"/>
</installer-gui-script>"#;
        let info = parse_pkg_metadata(xml);
        assert_eq!(info.identifier.as_deref(), Some("com.example.app"));
        assert_eq!(info.version.as_deref(), Some("2.1.0"));
    }

    #[test]
    fn test_parse_component_packageinfo() {
        let xml = r#"<pkg-info format-version="2" identifier="com.acme.tool" version="1.4.2" install-location="/"></pkg-info>"#;
        let info = parse_pkg_metadata(xml);
        assert_eq!(info.identifier.as_deref(), Some("com.acme.tool"));
        assert_eq!(info.version.as_deref(), Some("1.4.2"));
    }

    #[test]
    fn test_full_stanza_is_valid_yaml() {
        let info = PkgInfo {
            identifier: Some("nl.root3.support".to_string()),
            version: Some("3.0.3".to_string()),
        };
        let block = metadata_block_full(&info, "Support.3.0.3.pkg", "8c30a711");
        let v: serde_yaml::Value = serde_yaml::from_str(&block).expect("full stanza must parse");
        let item = &v[0];
        assert_eq!(item["hash_sha256"], serde_yaml::Value::from("8c30a711"));
        assert_eq!(item["self_service"], serde_yaml::Value::from(false));
        let url = item["url"].as_str().unwrap();
        assert!(url.starts_with("https://"));
        assert!(url.ends_with("Support.3.0.3.pkg"));
        // Optional fields are commented out, so they must NOT parse as keys.
        assert!(item.get("categories").is_none());
        assert!(item.get("install_script").is_none());
        assert!(item.get("labels_include_any").is_none());
        assert!(item.get("display_name").is_none());
        // …but the stubs are present as scaffolding, with a derived display name.
        assert!(block.contains("# display_name: \"support\""));
        assert!(block.contains("# labels_include_any:"));
        assert!(block.contains("# labels_exclude_any:"));
        assert!(block.contains("# nl.root3.support (Support.3.0.3.pkg) version 3.0.3"));
    }

    #[test]
    fn test_display_name_skips_pkg_segment() {
        assert_eq!(display_name("io.declarative.flint.pkg"), "flint");
        assert_eq!(display_name("nl.root3.support"), "support");
        assert_eq!(display_name("com.acme.tool.PKG"), "tool");
        // No meaningful segment left → fall back to the whole id.
        assert_eq!(display_name("pkg"), "pkg");

        let info = PkgInfo {
            identifier: Some("io.declarative.flint.pkg".to_string()),
            version: Some("0.1.4".to_string()),
        };
        assert!(metadata_file(&info, "flint-0.1.4.pkg", "abc", true)
            .contains("# display_name: \"flint\""));
    }

    #[test]
    fn test_metadata_file_is_top_level_mapping() {
        let info = PkgInfo {
            identifier: Some("nl.root3.support".to_string()),
            version: Some("3.0.3".to_string()),
        };
        let file = metadata_file(&info, "Support.3.0.3.pkg", "8c30a711", true);
        // A standalone file is a top-level mapping, NOT a list item.
        assert!(!file.contains("\n- "));
        let v: serde_yaml::Value = serde_yaml::from_str(&file).expect("software file must parse");
        assert_eq!(v["hash_sha256"], serde_yaml::Value::from("8c30a711"));
        assert_eq!(v["self_service"], serde_yaml::Value::from(false));
        assert!(v["url"].as_str().unwrap().ends_with("Support.3.0.3.pkg"));
        // Optional fields stay commented out (scaffolding only).
        assert!(v.get("categories").is_none());
        assert!(v.get("install_script").is_none());
        assert!(file.contains("# display_name: \"support\""));
        assert!(file.contains("# nl.root3.support (Support.3.0.3.pkg) version 3.0.3"));

        // Minimal form: header + hash + url + self_service, no scaffolding.
        let minimal = metadata_file(&info, "Support.3.0.3.pkg", "8c30a711", false);
        assert!(minimal.contains("hash_sha256: 8c30a711"));
        assert!(!minimal.contains("# Optional"));
        serde_yaml::from_str::<serde_yaml::Value>(&minimal).expect("minimal file must parse");
    }

    #[test]
    fn test_install_policy() {
        let info = PkgInfo {
            identifier: Some("nl.root3.support".to_string()),
            version: Some("3.0.3".to_string()),
        };
        let p = install_policy(&info, "Support.3.0.3.pkg", None, false);
        let v: serde_yaml::Value = serde_yaml::from_str(&p).expect("policy must parse as YAML");
        let item = &v[0];
        let q = item["query"].as_str().unwrap();
        assert!(q.contains("package_receipts"));
        assert!(q.contains("package_id = 'nl.root3.support'"));
        // EXISTS … AND NOT EXISTS(… < target): installed AND no older copy.
        assert!(q.contains("EXISTS (SELECT 1 FROM package_receipts WHERE package_id = 'nl.root3.support')"));
        assert!(q.contains("version_compare(version, '3.0.3') < 0"));
        assert!(q.contains("NOT EXISTS"));
        assert_eq!(item["platform"], serde_yaml::Value::from("darwin"));
        assert!(item["name"].as_str().unwrap().contains("Support"));
        // Audit-only: no enforcement automation.
        assert!(item.get("install_software").is_none());
    }

    #[test]
    fn test_install_policy_outdated_only() {
        let info = PkgInfo {
            identifier: Some("nl.root3.support".to_string()),
            version: Some("3.0.3".to_string()),
        };
        let p = install_policy(&info, "Support.3.0.3.pkg", None, true);
        let v: serde_yaml::Value = serde_yaml::from_str(&p).expect("policy must parse as YAML");
        let item = &v[0];
        let q = item["query"].as_str().unwrap();
        // Fails only on an outdated copy: NOT EXISTS(… < target), no EXISTS gate.
        assert!(q.contains("NOT EXISTS"));
        assert!(q.contains("version_compare(version, '3.0.3') < 0"));
        assert!(!q.contains("AND NOT EXISTS")); // no leading EXISTS clause
        assert!(item["name"].as_str().unwrap().contains("up to date"));
    }

    #[test]
    fn test_install_policy_enforce() {
        let info = PkgInfo {
            identifier: Some("nl.root3.support".to_string()),
            version: Some("3.0.3".to_string()),
        };
        let p = install_policy(&info, "Support.3.0.3.pkg", Some("8c30a71170088c"), false);
        let v: serde_yaml::Value = serde_yaml::from_str(&p).expect("policy must parse as YAML");
        let item = &v[0];
        // The enforcement automation references the package by the sha flint
        // computed, so a failing host auto-installs it.
        assert_eq!(
            item["install_software"]["hash_sha256"],
            serde_yaml::Value::from("8c30a71170088c")
        );
        // Resolution reflects that Fleet remediates automatically.
        assert!(item["resolution"].as_str().unwrap().contains("automatically"));
        // The query is the EXISTS/NOT-EXISTS audit form.
        assert!(item["query"].as_str().unwrap().contains("version_compare(version, '3.0.3') < 0"));
    }

    #[test]
    fn test_install_policy_file() {
        let info = PkgInfo {
            identifier: Some("com.fleetdm.fonts.corp".to_string()),
            version: Some("1.0".to_string()),
        };
        let p = install_policy_file(
            &info,
            "Corp-Fonts.pkg",
            "/Library/Fonts/Corp.otf",
            None,
        );
        let v: serde_yaml::Value = serde_yaml::from_str(&p).expect("policy must parse as YAML");
        let item = &v[0];
        assert_eq!(
            item["query"].as_str().unwrap(),
            "SELECT 1 FROM file WHERE path = '/Library/Fonts/Corp.otf';"
        );
        assert!(item["name"].as_str().unwrap().contains("installed"));
        assert!(item["description"].as_str().unwrap().contains("/Library/Fonts/Corp.otf"));
        // file proxy is existence-only — no version_compare.
        assert!(!item["query"].as_str().unwrap().contains("version_compare"));
    }

    #[test]
    fn test_install_policy_file_enforce() {
        let info = PkgInfo::default();
        let p = install_policy_file(&info, "x.pkg", "/Applications/X.app/Contents/Info.plist", Some("deadbeef"));
        let v: serde_yaml::Value = serde_yaml::from_str(&p).unwrap();
        assert_eq!(v[0]["install_software"]["hash_sha256"], serde_yaml::Value::from("deadbeef"));
    }

    #[test]
    fn test_install_policy_apps() {
        let info = PkgInfo {
            identifier: Some("com.1password.1password".to_string()),
            version: Some("8.12.24".to_string()),
        };
        let p = install_policy_apps(&info, "1Password.pkg", None, false);
        let v: serde_yaml::Value = serde_yaml::from_str(&p).unwrap();
        let q = v[0]["query"].as_str().unwrap();
        assert!(q.contains("FROM apps WHERE bundle_identifier = 'com.1password.1password'"));
        assert!(q.contains("version_compare(bundle_short_version, '8.12.24') < 0"));
        assert!(q.contains("NOT EXISTS"));
    }

    #[test]
    fn test_install_policy_no_version() {
        let info = PkgInfo {
            identifier: Some("com.x.tool".to_string()),
            version: None,
        };
        let p = install_policy(&info, "tool.pkg", None, false);
        let v: serde_yaml::Value = serde_yaml::from_str(&p).unwrap();
        let q = v[0]["query"].as_str().unwrap();
        assert!(q.contains("WHERE package_id = 'com.x.tool';"));
        assert!(!q.contains("version_compare")); // no version → existence only
    }

    #[test]
    fn test_default_scripts() {
        assert!(default_install_script().contains("installer -pkg \"$INSTALLER_PATH\" -target /"));
        let u = default_uninstall_script();
        // Fleet substitutes $PACKAGE_ID at upload — it must stay literal.
        assert!(u.contains("pkg_ids=$PACKAGE_ID"));
        assert!(u.contains("pkgutil --forget"));
        assert!(u.starts_with("#!/bin/sh"));
    }

    #[test]
    fn test_update_stanza() {
        let src = "\
software:
  packages:
      # nl.root3.support (Support.3.0.3.pkg) version 3.0.3
      - url: https://host.example.com/Support.3.0.3.pkg  # TODO
        hash_sha256: OLDHASH
        self_service: false
      # com.other.app (Other.1.0.pkg) version 1.0
      - url: https://host.example.com/Other.1.0.pkg
        hash_sha256: KEEPME
";
        let (out, old_v, old_h) =
            update_stanza(src, "nl.root3.support", "Support.3.1.0.pkg", "3.1.0", "NEWHASH").unwrap();
        assert_eq!(old_v, "3.0.3");
        assert_eq!(old_h, "OLDHASH");
        // header, hash, and url filename refreshed for the matched package only
        assert!(out.contains("# nl.root3.support (Support.3.1.0.pkg) version 3.1.0"));
        assert!(out.contains("hash_sha256: NEWHASH"));
        assert!(out.contains("https://host.example.com/Support.3.1.0.pkg  # TODO"));
        // the other package is untouched
        assert!(out.contains("hash_sha256: KEEPME"));
        assert!(out.contains("Other.1.0.pkg"));
    }

    #[test]
    fn test_update_stanza_identifier_not_found() {
        let src = "packages:\n  # a.b (x.pkg) version 1\n  - hash_sha256: H\n";
        assert!(update_stanza(src, "nope", "y.pkg", "2", "H2").is_none());
    }

    #[test]
    fn test_missing_fields_use_placeholders() {
        let info = PkgInfo::default();
        assert_eq!(
            metadata_block(&info, "x.pkg", "deadbeef"),
            "# unknown.identifier (x.pkg) version unknown\n- hash_sha256: deadbeef"
        );
    }
}
