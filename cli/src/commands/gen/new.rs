//! `flint new` — starter templates for Fleet GitOps artifacts.

use crate::args::NewArgs;

pub(crate) fn run(args: NewArgs) -> anyhow::Result<()> {
    let NewArgs { kind, output } = args;

    use colored::Colorize;
    let content = build_new_template(&kind)?;
    if let Some(out) = output {
        if out.exists() {
            anyhow::bail!("{} already exists — refusing to overwrite", out.display());
        }
        std::fs::write(&out, format!("{content}\n"))?;
        println!("{} wrote {} template to {}", "✓".green(), kind, out.display());
    } else {
        println!("{content}");
    }
    Ok(())
}

/// Build a starter template for `flint new <kind>`.
fn build_new_template(kind: &str) -> anyhow::Result<String> {
    match kind {
        "profile" => {
            use std::process::Command;
            let uuid = Command::new("uuidgen")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "REPLACE-WITH-UUID".to_string());
            Ok(format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
                 <plist version=\"1.0\">\n\
                 <dict>\n\
                 \x20 <key>PayloadDisplayName</key><string>New Profile</string>\n\
                 \x20 <key>PayloadIdentifier</key><string>com.example.newprofile</string>\n\
                 \x20 <key>PayloadType</key><string>Configuration</string>\n\
                 \x20 <key>PayloadScope</key><string>System</string>\n\
                 \x20 <key>PayloadVersion</key><integer>1</integer>\n\
                 \x20 <key>PayloadUUID</key><string>{uuid}</string>\n\
                 \x20 <key>PayloadContent</key>\n\
                 \x20 <array>\n\
                 \x20\x20 <!-- add payload dict(s) here -->\n\
                 \x20 </array>\n\
                 </dict>\n\
                 </plist>"
            ))
        }
        "fleet" => Ok("\
name: New Fleet
settings:
  secrets: []
controls:
  enable_disk_encryption: false
  apple_settings:
    configuration_profiles: []
  scripts: []
software:
  packages: []
policies: []
reports: []"
            .to_string()),
        "policy" => Ok("\
- name: New Policy
  query: \"SELECT 1;\"
  platform: darwin
  resolution: \"Describe how to remediate\"
  critical: false"
            .to_string()),
        "query" => Ok("\
- name: New Query
  query: \"SELECT 1;\"
  interval: 3600
  platform: darwin
  observer_can_run: true"
            .to_string()),
        "label" => Ok("\
- name: New Label
  description: \"\"
  label_membership_type: dynamic
  query: \"SELECT 1 FROM os_version WHERE name = 'macOS';\"
  platform: darwin"
            .to_string()),
        other => anyhow::bail!("unknown kind: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GUARD (surface audit 2026-08-07): scaffolds must never advertise
    /// deprecated key names — a fresh `flint gen new fleet` once emitted
    /// `team_settings:`/`queries:` next to new-name `apple_settings:`.
    /// Old names come straight from the deprecation registry so any future
    /// Fleet rename fails here until the templates are updated.
    #[test]
    fn scaffolds_never_advertise_deprecated_keys() {
        use flint_lint::deprecations::{DeprecationKind, DEPRECATION_REGISTRY};

        for kind in ["fleet", "policy", "query", "label", "profile"] {
            let Ok(template) = build_new_template(kind) else {
                continue;
            };
            for dep in DEPRECATION_REGISTRY.entries() {
                if let DeprecationKind::KeyRename { old_key, .. } = &dep.kind {
                    let as_key = format!("{}:", old_key);
                    assert!(
                        !template.lines().any(|l| l.trim_start().starts_with(&as_key)),
                        "gen new {kind} scaffold advertises deprecated key '{old_key}'"
                    );
                }
            }
        }
    }
}
