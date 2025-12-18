# LSP-fleet

Sublime Text LSP client for Fleet GitOps YAML validation.

## Features

- **Validation**: Real-time diagnostics for Fleet configuration errors
- **Completions**: Context-aware autocompletion for field names and values
- **File Path Completions**: Suggests files when typing `path:` values
- **Hover Documentation**: Shows documentation for osquery tables and Fleet fields
- **Go-to-Definition**: Navigate to referenced files

## Prerequisites

Install the [LSP](https://packagecontrol.io/packages/LSP) package:
1. Open Command Palette (`Cmd+Shift+P`)
2. Run "Package Control: Install Package"
3. Search for "LSP" and install

## Installation

1. Download `LSP-fleet-<version>-<platform>.zip` from [GitHub Releases](https://github.com/headmin/fleetctl-ext/releases)

2. Extract to Sublime packages directory:
   ```bash
   # macOS
   unzip LSP-fleet-0.1.0-darwin-arm64.zip -d ~/Library/Application\ Support/Sublime\ Text/Packages/LSP-fleet

   # Linux
   unzip LSP-fleet-0.1.0-linux-x64.zip -d ~/.config/sublime-text/Packages/LSP-fleet
   ```

3. Restart Sublime Text

## Package Contents

```
LSP-fleet/
├── plugin.py                         # LSP client plugin
├── LSP-fleet.sublime-settings        # Default settings
├── Main.sublime-menu                 # Menu integration
├── dependencies.json                 # Package dependencies
├── messages.json                     # Install messages
├── messages/
│   └── install.txt
├── bin/
│   └── fleet-schema-gen-<platform>   # LSP binary
└── README.md
```

## File Patterns

The extension activates for YAML files matching Fleet GitOps patterns:
- `default.yml` / `default.yaml`
- `teams/**/*.yml`
- `lib/**/*.yml`

## Configuration

Access settings via: `Preferences > Package Settings > LSP > Servers > LSP-fleet`

```json
{
    "client": {
        "enabled": true
    }
}
```

## Troubleshooting

### Server not starting

1. Check LSP is installed: `Preferences > Package Control > List Packages`
2. View logs: `View > Show Console`
3. Look for `[LSP-fleet]` messages

### Binary not found

Ensure the `bin/` directory contains the LSP binary and it's executable:
```bash
chmod +x ~/Library/Application\ Support/Sublime\ Text/Packages/LSP-fleet/bin/fleet-schema-gen-*
```
