# Fleet GitOps Zed Extension

Zed editor extension for Fleet GitOps YAML validation, completions, and diagnostics.

## Features

- **Validation**: Real-time diagnostics for Fleet configuration errors
- **Completions**: Context-aware autocompletion for field names and values
- **File Path Completions**: Suggests files when typing `path:` values
- **Hover Documentation**: Shows documentation on hover for fields and osquery tables
- **Go-to-Definition**: Navigate to referenced files

## Installation

### Option 1: Auto-download (Recommended)

The extension automatically downloads the LSP binary from GitHub releases on first use.

1. Install the extension in Zed:
   - Open Zed
   - Go to Extensions (`Cmd+Shift+X`)
   - Search for "Fleet" and install

2. Open a Fleet GitOps YAML file - the binary will download automatically.

The binary is downloaded to Zed's extension work directory and kept up-to-date.

### Option 2: Manual Installation

If you prefer to manage the binary yourself:

```bash
# macOS (Apple Silicon)
curl -sL https://github.com/fleetdm/fleet/releases/latest/download/fleet-schema-gen-darwin-arm64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/

# macOS (Intel)
curl -sL https://github.com/fleetdm/fleet/releases/latest/download/fleet-schema-gen-darwin-x64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/

# Linux (x64)
curl -sL https://github.com/fleetdm/fleet/releases/latest/download/fleet-schema-gen-linux-x64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/

# Or build from source
cd fleet-schema-gen
cargo install --path .
```

### Option 3: Development Extension

For development, install as a dev extension:

1. Clone the repository
2. Build: `cargo build --target wasm32-wasip1 --release`
3. In Zed: Extensions → "Install Dev Extension" → select `zed-extension` folder

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
