//! Sublime Text LSP configuration generator.
//!
//! Generates configuration files to use the Fleet LSP server with Sublime Text's
//! LSP package, providing full feature parity with VS Code including:
//! - Context-aware autocompletion
//! - Platform-filtered osquery table suggestions
//! - Hover documentation
//! - Real-time diagnostics
//! - Code actions

use anyhow::Result;
use std::fs;
use std::path::Path;

/// Generate Sublime Text LSP configuration.
///
/// This generates the configuration needed to use `fleet-schema-gen lsp`
/// as a language server in Sublime Text via the LSP package.
pub fn generate(output_dir: &Path) -> Result<()> {
    println!("\n=== Generating Sublime Text LSP Configuration ===");

    fs::create_dir_all(output_dir)?;

    // 1. Generate LSP client configuration
    generate_lsp_settings(output_dir)?;

    // 2. Generate file association settings
    generate_syntax_settings(output_dir)?;

    // 3. Generate installation README
    generate_readme(output_dir)?;

    // 4. Generate helper script for binary installation
    generate_install_script(output_dir)?;

    println!("✓ Sublime Text LSP configuration generated at: {}", output_dir.display());

    Ok(())
}

/// Generate LSP settings for Sublime Text.
fn generate_lsp_settings(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating LSP client settings...");

    // LSP.sublime-settings for user configuration
    let lsp_settings = r#"{
    // Fleet LSP Server Configuration
    // Copy this to: Preferences > Package Settings > LSP > Settings

    "clients": {
        "fleet-lsp": {
            // Enable the Fleet LSP server
            "enabled": true,

            // Command to start the server
            // Option 1: If fleet-schema-gen is in PATH
            "command": ["fleet-schema-gen", "lsp"],

            // Option 2: Specify full path (uncomment and adjust)
            // "command": ["/usr/local/bin/fleet-schema-gen", "lsp"],

            // Option 3: Use bundled binary (uncomment and adjust)
            // "command": ["${packages}/User/fleet-lsp/bin/fleet-schema-gen", "lsp"],

            // File patterns to activate on
            "selector": "source.yaml",

            // Only activate for Fleet GitOps files
            "auto_complete_selector": "source.yaml",

            // File patterns (more specific activation)
            "schemes": ["file"],

            // Initialization options (optional)
            "initializationOptions": {},

            // Settings passed to server
            "settings": {}
        }
    },

    // Show diagnostics in the gutter
    "show_diagnostics_severity_level": 1,

    // Show code actions in the gutter
    "show_code_actions_in_hover": true,

    // Enable hover popups
    "show_hover_with_mouse": true
}
"#;

    fs::write(output_dir.join("LSP.sublime-settings"), lsp_settings)?;
    println!("    ✓ LSP.sublime-settings");

    // Generate a more targeted configuration for Fleet files only
    let fleet_lsp_settings = r#"{
    // Fleet-specific LSP settings
    // This provides more targeted activation for Fleet GitOps files

    "clients": {
        "fleet-lsp": {
            "enabled": true,
            "command": ["fleet-schema-gen", "lsp"],
            "selector": "source.yaml",

            // Only activate for files matching these patterns
            // Adjust based on your project structure
            "file_patterns": [
                "**/default.yml",
                "**/default.yaml",
                "**/teams/**/*.yml",
                "**/teams/**/*.yaml",
                "**/lib/**/*.yml",
                "**/lib/**/*.yaml",
                "**/*.fleet.yml",
                "**/*.fleet.yaml"
            ]
        }
    }
}
"#;

    fs::write(output_dir.join("Fleet-LSP.sublime-settings"), fleet_lsp_settings)?;
    println!("    ✓ Fleet-LSP.sublime-settings");

    Ok(())
}

/// Generate syntax/file association settings.
fn generate_syntax_settings(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating file association settings...");

    // YAML.sublime-settings to ensure YAML files use correct syntax
    let yaml_settings = r#"{
    // Ensure Fleet GitOps YAML files are recognized as YAML
    // Copy to: Preferences > Settings - Syntax Specific (when viewing a YAML file)

    "extensions": [
        "yml",
        "yaml"
    ]
}
"#;

    fs::write(output_dir.join("YAML.sublime-settings"), yaml_settings)?;
    println!("    ✓ YAML.sublime-settings");

    Ok(())
}

/// Generate installation README.
fn generate_readme(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating installation guide...");

    let readme = r#"# Fleet GitOps - Sublime Text LSP Integration

This configuration enables full LSP support for Fleet GitOps YAML files in Sublime Text,
providing the same rich editing experience as VS Code.

## Features

When using the Fleet LSP server, you get:

- ✅ **Context-aware autocompletion** - Smart suggestions based on cursor position
- ✅ **Platform-filtered osquery tables** - Only shows tables valid for the detected platform
- ✅ **Hover documentation** - Rich markdown docs for fields and osquery tables
- ✅ **Real-time diagnostics** - Errors and warnings as you type
- ✅ **Code actions** - Quick fixes for common issues
- ✅ **Document symbols** - Outline view of policies/queries/labels
- ✅ **Go-to-definition** - Navigate to path references

## Prerequisites

1. **Sublime Text 4** (recommended) or Sublime Text 3
2. **LSP package** - Install via Package Control

## Installation

### Step 1: Install the LSP Package

1. Open Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`)
2. Type "Package Control: Install Package"
3. Search for "LSP" and install it

### Step 2: Install fleet-schema-gen

#### Option A: Download Binary (Recommended)

Download the latest release from GitHub and add to your PATH:

```bash
# macOS/Linux
curl -L https://github.com/fleetdm/fleet-schema-gen/releases/latest/download/fleet-schema-gen-$(uname -s)-$(uname -m) -o /usr/local/bin/fleet-schema-gen
chmod +x /usr/local/bin/fleet-schema-gen
```

#### Option B: Build from Source

```bash
git clone https://github.com/fleetdm/fleet-schema-gen
cd fleet-schema-gen
cargo build --release
cp target/release/fleet-schema-gen /usr/local/bin/
```

#### Option C: Use Bundled Binary

Copy the binary to your Sublime Text Packages folder:
```
~/Library/Application Support/Sublime Text/Packages/User/fleet-lsp/bin/
```

Then update the command path in LSP settings.

### Step 3: Configure LSP

1. Open Command Palette
2. Type "Preferences: LSP Settings"
3. Copy the contents of `LSP.sublime-settings` to the right pane (User settings)

Alternatively, copy `LSP.sublime-settings` to:
- macOS: `~/Library/Application Support/Sublime Text/Packages/User/`
- Linux: `~/.config/sublime-text/Packages/User/`
- Windows: `%APPDATA%\Sublime Text\Packages\User\`

### Step 4: Restart Sublime Text

Restart Sublime Text to activate the LSP server.

## Verification

1. Open a Fleet GitOps YAML file (e.g., `default.yml`)
2. Check the status bar - you should see "fleet-lsp" indicator
3. Type `platform:` and press space - you should see completion suggestions
4. Hover over a field name - you should see documentation

## Troubleshooting

### Server Not Starting

1. Verify fleet-schema-gen is in PATH:
   ```bash
   fleet-schema-gen --version
   ```

2. Check LSP logs: `View > Show Console` or Command Palette > "LSP: Toggle Log Panel"

3. Try specifying the full path in settings:
   ```json
   "command": ["/full/path/to/fleet-schema-gen", "lsp"]
   ```

### No Completions Appearing

1. Ensure the file is recognized as YAML (check syntax in status bar)
2. Check that the file matches the activation patterns
3. Try triggering completion manually: `Ctrl+Space`

### Diagnostics Not Showing

1. Check that "show_diagnostics_severity_level" is set to 1 or higher
2. Ensure the file is saved (some diagnostics require saved files)

## Comparison with Static Sublime Package

| Feature | LSP (this) | Static Package |
|---------|------------|----------------|
| Context-aware completion | ✅ | ❌ |
| Platform-filtered tables | ✅ | ❌ |
| Hover documentation | ✅ | Limited |
| Real-time diagnostics | ✅ | Via LSP-json only |
| Code actions | ✅ | ❌ |
| Requires running server | Yes | No |

## File Patterns

By default, the LSP server activates for these file patterns:
- `**/default.yml` / `**/default.yaml`
- `**/teams/**/*.yml` / `**/teams/**/*.yaml`
- `**/lib/**/*.yml` / `**/lib/**/*.yaml`
- `**/*.fleet.yml` / `**/*.fleet.yaml`

Edit `Fleet-LSP.sublime-settings` to customize patterns for your project.

## Support

- Issues: https://github.com/fleetdm/fleet-schema-gen/issues
- Documentation: https://fleetdm.com/docs/configuration/yaml-files

---

Generated by `fleet-schema-gen generate --editor sublime-lsp`
"#;

    fs::write(output_dir.join("README.md"), readme)?;
    println!("    ✓ README.md");

    Ok(())
}

/// Generate helper installation script.
fn generate_install_script(output_dir: &Path) -> Result<()> {
    println!("\n  → Generating installation helper...");

    // Bash install script
    let install_sh = r#"#!/bin/bash
# Fleet LSP Installation Helper for Sublime Text
# Run this script to set up Fleet LSP integration

set -e

echo "=== Fleet LSP Installer for Sublime Text ==="
echo

# Detect OS
OS=$(uname -s)
ARCH=$(uname -m)

# Determine Sublime Text packages directory
case "$OS" in
    Darwin)
        PACKAGES_DIR="$HOME/Library/Application Support/Sublime Text/Packages/User"
        ;;
    Linux)
        PACKAGES_DIR="$HOME/.config/sublime-text/Packages/User"
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

echo "Detected: $OS ($ARCH)"
echo "Packages directory: $PACKAGES_DIR"
echo

# Check if fleet-schema-gen is available
if command -v fleet-schema-gen &> /dev/null; then
    echo "✓ fleet-schema-gen found in PATH"
    fleet-schema-gen --version
else
    echo "✗ fleet-schema-gen not found in PATH"
    echo
    echo "Please install fleet-schema-gen first:"
    echo "  cargo install fleet-schema-gen"
    echo "  OR download from GitHub releases"
    exit 1
fi

# Create packages directory if needed
mkdir -p "$PACKAGES_DIR"

# Copy settings files
echo
echo "Copying LSP settings..."

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ -f "$SCRIPT_DIR/LSP.sublime-settings" ]; then
    cp "$SCRIPT_DIR/LSP.sublime-settings" "$PACKAGES_DIR/"
    echo "  ✓ Copied LSP.sublime-settings"
fi

if [ -f "$SCRIPT_DIR/Fleet-LSP.sublime-settings" ]; then
    cp "$SCRIPT_DIR/Fleet-LSP.sublime-settings" "$PACKAGES_DIR/"
    echo "  ✓ Copied Fleet-LSP.sublime-settings"
fi

echo
echo "=== Installation Complete ==="
echo
echo "Next steps:"
echo "  1. Install the LSP package via Package Control"
echo "  2. Restart Sublime Text"
echo "  3. Open a Fleet GitOps YAML file to verify"
echo
"#;

    fs::write(output_dir.join("install.sh"), install_sh)?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(output_dir.join("install.sh"))?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(output_dir.join("install.sh"), perms)?;
    }

    println!("    ✓ install.sh");

    Ok(())
}
