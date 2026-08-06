# Flint — Zed Extension

Zed editor extension for Fleet GitOps YAML validation, completions, and diagnostics.

## Features

- **Validation**: Real-time diagnostics for Fleet configuration errors
- **Completions**: Context-aware autocompletion for field names and values
- **File Path Completions**: Suggests files when typing `path:` values
- **Hover Documentation**: Shows documentation on hover for fields and osquery tables
- **Go-to-Definition**: Navigate to referenced files
- **Code Actions**: Quick-fixes for deprecated keys and typos

## Installation

### Option A: Dev extension (from zip)

1. Download `flint-zed-<version>.zip` from [GitHub Releases](https://github.com/headmin/fleet-editor-extensions/releases)

2. Extract:
   ```bash
   unzip flint-zed-0.1.4.zip -d ~/flint-zed
   ```

3. In Zed: `Cmd+Shift+P` → "zed: install dev extension" → select `~/flint-zed`

### Option B: From source

```bash
git clone https://github.com/headmin/fleet-editor-extensions
cd fleet-editor-extensions/editors/zed
# In Zed: "zed: install dev extension" → select this directory
```

## Required: Zed Settings

Add to your Zed settings (`Cmd+,`):

```json
{
  "languages": {
    "YAML": {
      "language_servers": ["flint-lsp"]
    }
  }
}
```

This tells Zed to use Flint as the YAML language server. The built-in `yaml-language-server` is not needed and may conflict.

## Prerequisites

Flint must be installed on your system:

```bash
# Install via script
curl -fsSL https://raw.githubusercontent.com/headmin/fleet-editor-extensions/main/scripts/install.sh | sh

# Or via PKG (macOS)
# Download flint-<version>.pkg from GitHub Releases
```

The extension auto-discovers `flint` from PATH, `~/.cargo/bin`, `/opt/homebrew/bin`, or `/usr/local/bin`.

## File Patterns

The extension activates for YAML files in Fleet GitOps repos:
- `default.yml` / `default.yaml`
- `fleets/**/*.yml`
- `platforms/**/*.yml`
- `labels/**/*.yml`

## Troubleshooting

### Extension not activating

1. Verify `flint` is installed: `which flint && flint --version`
2. Check Zed settings has `"language_servers": ["flint-lsp"]` under YAML
3. Check Zed logs: `Cmd+Shift+P` → "zed: open log" → search for "flint"

### Hover/completions not working

If you see errors from `yaml-language-server`, remove it from the language servers list so only `flint-lsp` is active:

```json
"languages": {
  "YAML": {
    "language_servers": ["flint-lsp"]
  }
}
```
