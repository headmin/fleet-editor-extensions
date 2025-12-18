# Fleet GitOps Zed Extension

Zed editor extension for Fleet GitOps YAML validation, completions, and diagnostics.

## Features

- **Validation**: Real-time diagnostics for Fleet configuration errors
- **Completions**: Context-aware autocompletion for field names and values
- **File Path Completions**: Suggests files when typing `path:` values
- **Hover Documentation**: Shows documentation on hover for fields and osquery tables
- **Go-to-Definition**: Navigate to referenced files

## Installation

1. Download `fleet-gitops-zed-<version>-<platform>.zip` from [GitHub Releases](https://github.com/headmin/fleetctl-ext/releases)

2. Extract to a local folder:
   ```bash
   unzip fleet-gitops-zed-0.1.0-darwin-arm64.zip -d ~/fleet-gitops-zed
   ```

3. In Zed:
   - Open Command Palette (`Cmd+Shift+P`)
   - Run "zed: install dev extension"
   - Select the extracted folder (`~/fleet-gitops-zed`)

4. Open a Fleet GitOps YAML file to verify it's working

## Package Contents

```
fleet-gitops-zed/
├── extension.toml                    # Extension manifest
├── extension.wasm                    # Extension logic
├── bin/
│   └── fleet-schema-gen-<platform>   # LSP binary
└── README.md
```

## File Patterns

The extension activates for YAML files matching Fleet GitOps patterns:
- `default.yml` / `default.yaml`
- `teams/**/*.yml`
- `lib/**/*.yml`

## Troubleshooting

### Extension not activating

1. Check Zed logs: `Cmd+Shift+P` → "zed: open log"
2. Look for "fleet" messages
3. Try reinstalling via "zed: install dev extension"

### Binary not found

Ensure the `bin/` directory contains the LSP binary and it's executable:
```bash
chmod +x ~/fleet-gitops-zed/bin/fleet-schema-gen-*
```
