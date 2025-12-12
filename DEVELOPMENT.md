# Fleet GitOps VS Code Extension - Development Guide

This document covers the architecture, build process, common issues, and best practices for developing the Fleet GitOps VS Code extension and its LSP server.

## Architecture Overview

```
fleetctl-vscode/
├── fleet-schema-gen/          # Rust LSP server + CLI tool
│   ├── src/
│   │   ├── lsp/               # LSP server implementation
│   │   │   ├── mod.rs         # Entry point, start_server()
│   │   │   ├── backend.rs     # LanguageServer trait implementation
│   │   │   ├── diagnostics.rs # LintError -> LSP Diagnostic conversion
│   │   │   ├── code_actions.rs# Quick-fix generation
│   │   │   └── position.rs    # UTF-16 position utilities
│   │   ├── linter/            # Fleet-specific validation rules
│   │   └── main.rs            # CLI entry point
│   └── Cargo.toml
│
└── vscode-extension/          # VS Code extension (TypeScript)
    ├── src/
    │   └── extension.ts       # Extension entry point
    ├── bin/                   # Platform-specific binaries
    │   └── fleet-schema-gen-darwin-arm64
    ├── package.json           # Extension manifest
    ├── .vscodeignore          # Files to exclude from VSIX
    └── tsconfig.json
```

### Data Flow

```
User edits YAML file
        ↓
VS Code detects change (onLanguage:yaml)
        ↓
Extension activates, spawns LSP server binary
        ↓
Language Client sends textDocument/didOpen
        ↓
LSP Server (Rust):
  1. Parses YAML with serde_yaml
  2. Runs lint rules (RequiredFields, PlatformCompatibility, etc.)
  3. Converts LintError to LSP Diagnostic
  4. Publishes diagnostics to client
        ↓
VS Code displays squiggly underlines
        ↓
User clicks lightbulb (Cmd+.)
        ↓
Language Client sends textDocument/codeAction
        ↓
LSP Server returns CodeAction with WorkspaceEdit
        ↓
User applies fix
```

## Build Process

### Prerequisites

- **Rust** (for LSP server): `brew install rust` or https://rustup.rs
- **Node.js/pnpm** (for extension): `brew install node && npm install -g pnpm`
- **vsce** (for packaging): installed via pnpm devDependencies

### Building the LSP Server

```bash
cd fleet-schema-gen
cargo build --release

# Binary location
ls -la target/release/fleet-schema-gen
```

### Building the VS Code Extension

```bash
cd vscode-extension

# Install dependencies
pnpm install

# Compile TypeScript
pnpm run compile

# Copy binary for your platform
cp ../fleet-schema-gen/target/release/fleet-schema-gen bin/fleet-schema-gen-darwin-arm64

# Package VSIX
pnpm exec vsce package
```

### Platform-Specific Binaries

The extension supports multiple platforms. Binary naming convention:

| Platform       | Binary Name                        |
|----------------|-----------------------------------|
| macOS ARM64    | `fleet-schema-gen-darwin-arm64`   |
| macOS x64      | `fleet-schema-gen-darwin-x64`     |
| Linux x64      | `fleet-schema-gen-linux-x64`      |
| Linux ARM64    | `fleet-schema-gen-linux-arm64`    |
| Windows x64    | `fleet-schema-gen-win32-x64.exe`  |

For cross-platform releases, build on each platform or use cross-compilation:

```bash
# Cross-compile for Linux from macOS (requires cross)
cargo install cross
cross build --release --target x86_64-unknown-linux-gnu
```

## Common Issues and Solutions

### Issue 1: "Cannot find module 'vscode-languageclient/node'"

**Cause**: `node_modules` not included in VSIX package.

**Solution**: Update `.vscodeignore` to include dependencies:

```gitignore
# .vscodeignore - DO NOT exclude node_modules entirely
.vscode/**
.vscode-test/**
src/**
.gitignore
tsconfig.json
**/*.ts
**/*.map
.eslintrc.json

# Only exclude unnecessary parts of node_modules
node_modules/.bin/**
node_modules/**/test/**
node_modules/**/tests/**
node_modules/**/*.md
node_modules/**/*.ts
node_modules/**/LICENSE*
node_modules/**/CHANGELOG*
```

**Verification**:
```bash
unzip -l fleet-gitops-*.vsix | grep node_modules | head -5
# Should show vscode-languageclient files
```

### Issue 2: "unexpected argument '--stdio' found"

**Cause**: `vscode-languageclient` adds `--stdio` flag when using `TransportKind.stdio`.

**Solution**: Use `Executable` type without explicit transport:

```typescript
// CORRECT - matches typos-lsp pattern
import { Executable, ServerOptions } from 'vscode-languageclient/node';

const run: Executable = {
    command: serverPath,
    args: ['lsp'],
};

const serverOptions: ServerOptions = {
    run,
    debug: run,
};
```

```typescript
// WRONG - may add --stdio flag
const serverOptions: ServerOptions = {
    run: {
        command: serverPath,
        args: ['lsp'],
        transport: TransportKind.stdio,  // Don't use this
    },
};
```

**Alternative**: Accept `--stdio` in Rust CLI (for compatibility):

```rust
#[derive(Subcommand)]
enum Commands {
    Lsp {
        #[arg(long)]
        debug: bool,

        /// Accepted for compatibility, stdio is always used
        #[arg(long)]
        stdio: bool,
    },
}
```

### Issue 3: Extension not activating

**Cause**: File doesn't match activation patterns.

**Solution**: Check `package.json` activation events and document selector patterns:

```json
{
  "activationEvents": [
    "onLanguage:yaml",
    "workspaceContains:**/default.yml",
    "workspaceContains:**/teams/*.yml"
  ]
}
```

The LSP only processes files matching these patterns in `extension.ts`:
- `**/default.yml` or `**/default.yaml`
- `**/teams/**/*.yml` or `**/teams/**/*.yaml`
- `**/lib/**/*.yml` or `**/lib/**/*.yaml`

**Verification**:
```bash
# Create a file that matches the pattern
mkdir -p ~/test-fleet/lib
echo "policies: []" > ~/test-fleet/lib/test.yml
code ~/test-fleet/lib/test.yml
```

### Issue 4: LSP server crashes on startup

**Cause**: Binary architecture mismatch or missing dependencies.

**Verification**:
```bash
# Check binary architecture
file /path/to/fleet-schema-gen-darwin-arm64
# Should show: Mach-O 64-bit executable arm64

# Test binary runs
/path/to/fleet-schema-gen-darwin-arm64 --help
/path/to/fleet-schema-gen-darwin-arm64 lsp --help
```

**Test LSP protocol**:
```bash
# Send initialize request
printf 'Content-Length: 131\r\n\r\n{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"processId":null,"capabilities":{},"rootUri":null,"workspaceFolders":null}}' > /tmp/init.txt

cat /tmp/init.txt | /path/to/fleet-schema-gen lsp &
pid=$!
sleep 2
kill $pid

# Should output JSON response with capabilities
```

### Issue 5: No diagnostics appearing

**Causes**:
1. File doesn't match document selector patterns
2. YAML parse error (check Output panel)
3. No lint rules triggered

**Debugging**:
1. Open Output panel: `View > Output > Fleet GitOps`
2. Enable verbose tracing in settings:
   ```json
   {
     "fleetGitops.trace.server": "verbose"
   }
   ```
3. Check the file is valid YAML first

### Issue 6: Code actions not appearing

**Cause**: Diagnostic doesn't have suggestion data.

**Solution**: Ensure lint rules provide suggestions:

```rust
// In linter rules
LintError::error("Invalid platform", file)
    .with_suggestion("darwin")  // This enables code actions
    .with_help("Valid platforms: darwin, linux, windows")
```

The diagnostic must have `data.suggestion` for code actions to appear:

```rust
// In diagnostics.rs
let data = error.suggestion.as_ref().map(|s| {
    serde_json::json!({
        "suggestion": s,
        "help": error.help
    })
});
```

## Testing

### Unit Tests (Rust)

```bash
cd fleet-schema-gen
cargo test

# Run specific LSP tests
cargo test -- lsp
```

### Manual Testing

1. **Install extension in development mode**:
   ```bash
   cd vscode-extension
   code --extensionDevelopmentPath="$(pwd)"
   ```

2. **Or install VSIX**:
   ```bash
   code --install-extension fleet-gitops-0.1.0.vsix
   ```

3. **Test with sample file**:
   ```yaml
   # ~/test-fleet/default.yml
   policies:
     - name: test policy
       platform: invalid_platform  # Should show error
       query: SELECT * FROM users
   ```

4. **Check Output panel** for server logs

### Debugging the LSP Server

```bash
# Run with debug flag
fleet-schema-gen lsp --debug 2>lsp-debug.log

# In another terminal, send test messages
cat /tmp/init.txt | fleet-schema-gen lsp --debug
```

## Release Checklist

### Before Release

- [ ] All tests pass: `cargo test`
- [ ] Build succeeds: `cargo build --release`
- [ ] Binary runs: `./target/release/fleet-schema-gen --help`
- [ ] LSP responds: test with JSON-RPC initialize
- [ ] Extension compiles: `pnpm run compile`
- [ ] VSIX packages: `pnpm exec vsce package`
- [ ] VSIX contains node_modules: `unzip -l *.vsix | grep languageclient`
- [ ] VSIX contains binary: `unzip -l *.vsix | grep fleet-schema-gen`

### Building Release VSIX

```bash
# 1. Build Rust binary
cd fleet-schema-gen
cargo build --release

# 2. Copy binary
cd ../vscode-extension
cp ../fleet-schema-gen/target/release/fleet-schema-gen bin/fleet-schema-gen-darwin-arm64

# 3. Install dependencies (production only)
pnpm install --prod

# 4. Compile TypeScript
pnpm run compile

# 5. Package
pnpm exec vsce package

# 6. Verify
unzip -l fleet-gitops-*.vsix
```

### Multi-Platform Release

For each platform:
1. Build binary on target platform (or cross-compile)
2. Name binary according to convention
3. Place all binaries in `bin/` directory
4. Package once with all binaries

## Debugging Tips

### VS Code Developer Tools

- `Cmd+Shift+P` → "Developer: Toggle Developer Tools"
- Check Console tab for errors
- Filter by "fleet" to find relevant logs

### Extension Host Logs

- `Cmd+Shift+P` → "Developer: Show Logs..."
- Select "Extension Host"
- Look for activation errors

### LSP Tracing

In VS Code settings:
```json
{
  "fleetGitops.trace.server": "verbose"
}
```

This logs all JSON-RPC messages between client and server.

## Sublime Text LSP Setup

The Fleet LSP server also works with Sublime Text via the LSP package.

### Prerequisites

1. **Sublime Text 4** (recommended) or Sublime Text 3
2. **LSP package** - Install via Package Control

### Installation

#### Step 1: Install the LSP Package

1. Open Command Palette (`Cmd+Shift+P`)
2. Type "Package Control: Install Package"
3. Search for "LSP" and install it

#### Step 2: Build or Download fleet-schema-gen

```bash
cd fleet-schema-gen
cargo build --release
# Binary at: target/release/fleet-schema-gen
```

#### Step 3: Configure LSP

Create or edit `~/Library/Application Support/Sublime Text/Packages/User/LSP.sublime-settings`:

```json
{
    "clients": {
        "fleet-lsp": {
            "enabled": true,
            "command": ["/path/to/fleet-schema-gen", "lsp"],
            "selector": "source.yaml"
        }
    }
}
```

Replace `/path/to/fleet-schema-gen` with the actual path to your binary.

#### Step 4: Restart Sublime Text

Restart Sublime Text to activate the LSP server.

### Verification

1. Open a Fleet GitOps YAML file (e.g., `default.yml`)
2. Check the status bar - should show "fleet-lsp"
3. Type inside `software: packages: -` - you should see completions

### Debugging

Enable debug logging in LSP settings:

```json
{
    "log_debug": true,
    "log_server": ["panel"],
    "log_stderr": true,
    "clients": {
        "fleet-lsp": {
            "enabled": true,
            "command": ["/path/to/fleet-schema-gen", "lsp"],
            "selector": "source.yaml"
        }
    }
}
```

View logs: `Cmd+Shift+P` → "LSP: Toggle Log Panel"

### Generate Configuration Files

You can also generate Sublime Text configuration files:

```bash
fleet-schema-gen generate --editor sublime-lsp --output /tmp/sublime-config
```

This creates:
- `LSP.sublime-settings` - LSP client configuration
- `Fleet-LSP.sublime-settings` - Fleet-specific settings with file patterns
- `README.md` - Installation guide
- `install.sh` - Helper script

## Reference Implementation

This extension follows patterns from [typos-lsp](https://github.com/tekumara/typos-lsp):

- Uses `Executable` type for server options (no explicit transport)
- Bundles platform-specific binaries in `bin/` directory
- Configuration via `initializationOptions`
- Stdio transport by default

## Appendix: LSP Protocol Reference

### Key LSP Methods Implemented

| Method | Description |
|--------|-------------|
| `initialize` | Server capabilities negotiation |
| `initialized` | Post-initialization notification |
| `textDocument/didOpen` | Document opened, trigger validation |
| `textDocument/didChange` | Document changed, re-validate |
| `textDocument/didClose` | Document closed, clear diagnostics |
| `textDocument/codeAction` | Return quick-fix actions |
| `shutdown` | Graceful shutdown |

### Server Capabilities

```json
{
  "capabilities": {
    "textDocumentSync": 1,
    "codeActionProvider": true
  }
}
```

- `textDocumentSync: 1` = Full sync (entire document on change)
- `codeActionProvider: true` = Quick-fix support enabled
