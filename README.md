# Fleet GitOps Editor Extensions

Language Server Protocol (LSP) extensions for Fleet GitOps YAML files. Provides validation, completions, hover docs, and go-to-definition across multiple editors.

## Features

- **Validation** - Real-time diagnostics for configuration errors
- **Completions** - Context-aware autocompletion for fields and values
- **File Path Completions** - Suggests files when typing `path:` values
- **Hover Documentation** - Shows docs for Fleet fields and osquery tables
- **Go-to-Definition** - Navigate to referenced files
- **Semantic Highlighting** - SQL syntax highlighting in query fields

## Supported Editors

| Editor | Package | Install |
|--------|---------|---------|
| VS Code | `fleet-gitops-<version>.vsix` | `code --install-extension *.vsix` |
| Zed | `fleet-gitops-zed-<version>-<platform>.zip` | "zed: install dev extension" |
| Sublime Text | `LSP-fleet-<version>-<platform>.zip` | Extract to Packages/LSP-fleet |

## Installation

Download packages from [GitHub Releases](https://github.com/headmin/fleetctl-ext/releases).

### VS Code

```bash
code --install-extension fleet-gitops-0.1.0.vsix
```

### Zed

1. Extract the zip to a local folder
2. In Zed: `Cmd+Shift+P` → "zed: install dev extension"
3. Select the extracted folder

### Sublime Text

**Prerequisite**: Install the [LSP](https://packagecontrol.io/packages/LSP) package first.

```bash
# macOS
unzip LSP-fleet-0.1.0-darwin-arm64.zip -d ~/Library/Application\ Support/Sublime\ Text/Packages/LSP-fleet

# Linux
unzip LSP-fleet-0.1.0-linux-x64.zip -d ~/.config/sublime-text/Packages/LSP-fleet
```

## File Patterns

Extensions activate for YAML files matching Fleet GitOps patterns:
- `default.yml` / `default.yaml`
- `teams/**/*.yml`
- `lib/**/*.yml`

## Project Structure

```
fleetctl-ext/
├── fleet-schema-gen/     # LSP server (Rust)
├── vscode-extension/     # VS Code extension
├── zed-extension/        # Zed extension (WASM)
├── sublime-package/      # Sublime Text package
├── scripts/
│   └── release-local.sh  # Unified build script
└── dist/                 # Build output
```

## Building

Build all packages for the current platform:

```bash
# Quick build (no signing)
./scripts/release-local.sh --quick

# Full release (sign, notarize, upload)
./scripts/release-local.sh --notarize --release
```

See [DEVELOPMENT.md](DEVELOPMENT.md) for detailed build instructions.

## License

Apache 2.0
