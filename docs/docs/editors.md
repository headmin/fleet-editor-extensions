---
icon: lucide/monitor
---

# Editor setup

All editors use `flint lsp` as a language server over stdio. Install flint first ([Installation](install.md)), then set up your editor.

## VS Code

Download the `.vsix` from [releases](https://github.com/headmin/fleet-editor-extensions/releases) and install:

```bash
code --install-extension flint-0.1.1.vsix
```

The extension activates automatically for Fleet GitOps YAML files.

### Settings

| Setting | Default | Description |
|---|---|---|
| `flint.enable` | `true` | Toggle extension |
| `flint.serverPath` | auto-detect | Custom binary path |
| `flint.fleetVersion` | `latest` | Schema version |
| `flint.trace.server` | `off` | LSP protocol logging |

## Zed

Install the **Flint** extension: download `flint-zed-<version>.zip` from [releases](https://github.com/headmin/fleet-editor-extensions/releases), extract, then in Zed: `Cmd+Shift+P` → "zed: install dev extension" → select the folder.

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

Requires `flint` on PATH. The built-in `yaml-language-server` should be removed from the list to avoid conflicts.

## Neovim

Add to your config:

```lua
require('flint').setup()
```

Or with lazy.nvim:

```lua
{
  "headmin/fleet-editor-extensions",
  config = function()
    vim.opt.rtp:append("editors/neovim")
    require("flint").setup()
  end,
}
```

Requires `flint` on your PATH.

## Sublime Text

1. Install the [LSP](https://packagecontrol.io/packages/LSP) package via Package Control
2. Install the **Flint** LSP package
3. The binary is auto-downloaded from GitHub releases on first use

## JetBrains (IntelliJ, GoLand, etc.)

1. Install the **Flint** plugin
2. Requires `flint` on your PATH

## What the LSP provides

| Feature | Description |
|---|---|
| **Diagnostics** | Real-time linting on every keystroke |
| **Completion** | Keys, platforms, osquery tables, SQL keywords, file paths |
| **Hover** | Field descriptions, platform info, deprecation notices |
| **Code actions** | Quick-fixes for deprecated keys, typos |
| **Go-to-definition** | Navigate `path:` references |
| **Document symbols** | Outline view (policies, queries, labels) |
| **Semantic tokens** | Syntax highlighting for Fleet YAML |
| **Document links** | Clickable `path:` references |
