# Fleet GitOps VS Code Extension

VS Code extension for Fleet GitOps YAML validation, completions, and diagnostics.

## Features

- **Validation**: Real-time diagnostics for Fleet configuration errors
- **Completions**: Context-aware autocompletion for field names and values
- **File Path Completions**: Suggests files when typing `path:` values
- **Hover Documentation**: Shows documentation on hover for fields and osquery tables
- **Go-to-Definition**: Navigate to referenced files
- **Semantic Highlighting**: Syntax highlighting for osquery SQL in YAML

## Installation

1. Download `fleet-gitops-<version>.vsix` from [GitHub Releases](https://github.com/headmin/fleetctl-ext/releases)

2. Install via command line:
   ```bash
   code --install-extension fleet-gitops-0.1.0.vsix
   ```

   Or in VS Code:
   - Open Extensions (`Cmd+Shift+X`)
   - Click `...` menu → "Install from VSIX..."
   - Select the downloaded `.vsix` file

3. Reload VS Code

The VSIX includes the bundled LSP binary - no additional installation needed.

## File Patterns

The extension activates for YAML files matching Fleet GitOps patterns:
- `default.yml` / `default.yaml`
- `teams/**/*.yml`
- `lib/**/*.yml`

## Configuration

Open VS Code settings (`Cmd+,`) and search for "Fleet":

| Setting | Description | Default |
|---------|-------------|---------|
| `fleet-gitops.binaryPath` | Custom path to LSP binary | (bundled) |
| `fleet-gitops.trace.server` | LSP trace level | `off` |

## Troubleshooting

### Extension not activating

1. Check Output panel: `View > Output` → select "Fleet GitOps"
2. Verify file matches activation patterns
3. Reload: `Cmd+Shift+P` → "Developer: Reload Window"

### Debug logging

Enable verbose logging in settings:
```json
{
    "fleet-gitops.trace.server": "verbose"
}
```
