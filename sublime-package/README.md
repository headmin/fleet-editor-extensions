# LSP-fleet

Sublime Text LSP client for Fleet GitOps YAML validation.

## Features

- **Validation**: Real-time diagnostics for Fleet configuration errors
- **Completions**: Context-aware autocompletion for field names and values
- **File Path Completions**: Suggests files when typing `path:` values
- **Hover Documentation**: Shows documentation for osquery tables and Fleet fields
- **Go-to-Definition**: Navigate to referenced files

## Installation

### Via Package Control (Recommended)

1. Install [Package Control](https://packagecontrol.io/installation) if not already installed
2. Open Command Palette (`Cmd+Shift+P` / `Ctrl+Shift+P`)
3. Run "Package Control: Install Package"
4. Search for "LSP-fleet" and install

The LSP binary will be automatically downloaded on first use.

### Manual Installation

1. Clone this repository into your Sublime Text `Packages` directory:
   ```bash
   cd ~/Library/Application\ Support/Sublime\ Text/Packages  # macOS
   # or: ~/.config/sublime-text/Packages  # Linux
   git clone https://github.com/fleetdm/fleet.git LSP-fleet
   ```

2. Install the [LSP](https://packagecontrol.io/packages/LSP) package via Package Control

## Configuration

Access settings via: `Preferences > Package Settings > LSP > Servers > LSP-fleet > Settings`

```json
{
    // Custom path to fleet-schema-gen binary
    // Leave empty for auto-download
    "binary_path": "",

    "client": {
        "enabled": true,
        "initializationOptions": {
            "fleetVersion": "latest"
        }
    }
}
```

### Custom Binary Path

If you prefer to manage the binary yourself:

```json
{
    "binary_path": "/usr/local/bin/fleet-schema-gen"
}
```

Or install manually:

```bash
# macOS (Apple Silicon)
curl -sL https://github.com/fleetdm/fleet/releases/latest/download/fleet-schema-gen-darwin-arm64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/

# macOS (Intel)
curl -sL https://github.com/fleetdm/fleet/releases/latest/download/fleet-schema-gen-darwin-x64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/

# Linux
curl -sL https://github.com/fleetdm/fleet/releases/latest/download/fleet-schema-gen-linux-x64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/
```

## File Patterns

The server activates for YAML files and internally filters to Fleet GitOps patterns:
- `default.yml` / `default.yaml`
- `teams/**/*.yml` / `teams/**/*.yaml`
- `lib/**/*.yml` / `lib/**/*.yaml`

## Troubleshooting

### Server not starting

1. Check LSP logs: `View > Show Console`
2. Look for `[LSP-fleet]` messages
3. Verify binary is downloaded: check `~/Library/Caches/Sublime Text/Package Storage/LSP-fleet/`

### Binary not found

If auto-download fails:
1. Download manually from [GitHub releases](https://github.com/fleetdm/fleet/releases)
2. Set `binary_path` in settings

### Debug logging

Enable verbose logging in LSP settings:
```json
{
    "log_debug": true
}
```

## Links

- [Fleet GitOps Documentation](https://fleetdm.com/docs/configuration/yaml-files)
- [GitHub Repository](https://github.com/fleetdm/fleet)
- [Report Issues](https://github.com/fleetdm/fleet/issues)
