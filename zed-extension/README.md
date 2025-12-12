# Fleet GitOps Zed Extension

Zed editor extension for Fleet GitOps YAML validation, completions, and diagnostics.

## Features

- **Validation**: Real-time diagnostics for Fleet configuration errors
- **Completions**: Context-aware autocompletion for field names and values
- **File Path Completions**: Suggests files when typing `path:` values
- **Hover Documentation**: Shows documentation on hover for fields and osquery tables
- **Go-to-Definition**: Navigate to referenced files

## Installation

### Option 1: Install from PATH (Recommended for Development)

1. Build and install `fleet-schema-gen`:
   ```bash
   cd ../fleet-schema-gen
   cargo install --path .
   ```

2. Install this extension as a dev extension in Zed:
   - Open Zed
   - Go to Extensions (`Cmd+Shift+X`)
   - Click "Install Dev Extension"
   - Select this `zed-extension` folder

### Option 2: Auto-download

If `fleet-schema-gen` is not in your PATH, the extension will automatically download the latest release from GitHub.

## Building

```bash
# Install the wasm32-wasip1 target
rustup target add wasm32-wasip1

# Build the extension
cargo build --target wasm32-wasip1 --release
```

## Configuration

You can configure the LSP server in your Zed settings (`~/.config/zed/settings.json`):

```json
{
  "lsp": {
    "fleet-lsp": {
      "settings": {
        "fleetVersion": "latest"
      }
    }
  }
}
```

## File Patterns

The extension activates for all YAML files. The LSP server internally filters to Fleet GitOps-specific patterns:
- `default.yml` / `default.yaml`
- `teams/**/*.yml` / `teams/**/*.yaml`
- `lib/**/*.yml` / `lib/**/*.yaml`

## Development

To test changes locally:

1. Make changes to `src/lib.rs`
2. Rebuild: `cargo build --target wasm32-wasip1`
3. Reload the extension in Zed: `Cmd+Shift+P` → "zed: reload extensions"
4. Open a Fleet YAML file to test

## Troubleshooting

### Extension not activating

1. Check that the extension is installed: Extensions → Installed
2. Verify the LSP is running: `Cmd+Shift+P` → "zed: open log"
3. Look for "Fleet LSP" messages

### Binary not found

If you see "Unable to find fleet-schema-gen", either:
- Install it globally: `cargo install --path ../fleet-schema-gen`
- Or wait for the auto-download to complete

### Debug logging

Run Zed from terminal with verbose logging:
```bash
zed --foreground
```
