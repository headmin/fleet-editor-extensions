# LSP-flint

Sublime Text client for flint, the Fleet GitOps YAML linter and language
server. Requires the [LSP](https://packagecontrol.io/packages/LSP) package.

## Install

1. Install **LSP** from Package Control.
2. Copy this directory into your Packages folder as `LSP-flint`:

   ```bash
   # macOS
   cp -R editors/sublime ~/Library/Application\ Support/Sublime\ Text/Packages/LSP-flint
   # Linux
   cp -R editors/sublime ~/.config/sublime-text/Packages/LSP-flint
   ```

3. Restart Sublime Text and open a Fleet GitOps YAML file.

The plugin runs `flint lsp`. It uses, in order: `binary_path` from the
settings, `flint` on `PATH` (and `~/.cargo/bin`, `/opt/homebrew/bin`,
`/usr/local/bin`), or downloads the release binary from
[GitHub releases](https://github.com/headmin/fleet-editor-extensions/releases).
macOS (Apple Silicon) and Linux are supported.

## Settings

`Preferences > Package Settings > LSP > Servers > LSP-flint`:

```jsonc
{
  // Custom path to the flint binary; empty = PATH lookup / download.
  "binary_path": "",
  "client": {
    "initializationOptions": { "fleetVersion": "latest" }
  }
}
```

Lint rules and Fleet connection settings come from the repo's
`fleetlint.toml`, the same file `flint check` reads.

## Troubleshooting

Binary lookup and download messages (`[LSP-flint] …`) are in the console
(`View > Show Console`); server messages are in `Tools > LSP > Toggle Log
Panel`. Check the binary with `flint --version`.
