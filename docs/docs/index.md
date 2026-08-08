---
icon: lucide/shield-check
---

<p align="center">
  <img src="images/flint-icon.png" alt="Flint" width="128">
</p>

# Flint

Fleet GitOps YAML linter and language server — catches configuration errors, typos, and misplaced keys *before* `fleetctl gitops` runs.

![Flint autocomplete in VS Code](images/autocomplete.gif)

## What it does

- **Lint rules** — structural validation, semantic checks, security hygiene, deprecation warnings
- **LSP server** — real-time diagnostics, completions, hover docs, go-to-definition, code actions
- **Path tooling** — fix broken `path:` references after a reorg, and wire unwired profiles/scripts/software into fleets ([Commands](commands.md))
- **Generators** — turn a `.pkg`, `.mobileconfig`, or `.sql` into ready GitOps YAML; generate install policies and scripts
- **Migration reports** — JSON-based migration planning for Fleet version upgrades
- **Agent integration** — `help-ai` progressive discovery for AI-assisted workflows

``` mermaid
graph LR
  A[YAML files] --> B[flint check];
  A --> C[flint lsp];
  B --> D[Text / JSON output];
  C --> E[Editor diagnostics];
  C --> F[Completions & hover];
  C --> G[Code actions];
```

## Quick start

**macOS** — download the signed & notarized PKG:

> [flint-0.1.4.pkg](https://github.com/headmin/fleet-editor-extensions/releases/latest/download/flint-0.1.4.pkg)

**Linux** — build from source (no Linux binary is published yet):

```bash
cargo install --git https://github.com/headmin/fleet-editor-extensions --tag v0.1.4 flint
```

Then:

```bash
# Lint a Fleet GitOps repo
flint check .

# Auto-fix safe issues
flint check . --fix

# Initialize configuration
flint init
```

## Editor support

| Editor | Install |
|--------|---------|
| **VS Code** | Download [flint-0.1.4.vsix](https://github.com/headmin/fleet-editor-extensions/releases/latest/download/flint-0.1.4.vsix) → `Extensions: Install from VSIX` |
| **Zed** | Download [flint-zed-0.1.4.zip](https://github.com/headmin/fleet-editor-extensions/releases/latest/download/flint-zed-0.1.4.zip) → install `flint` binary to PATH |
| **Sublime Text** | Install `flint` binary, add [LSP-flint](editors.md) config |
| **Neovim** | Install `flint` binary, configure as LSP with `cmd = {"flint", "lsp"}` |

## How it works

Flint validates Fleet GitOps YAML at two levels:

1. **Structural** — unknown keys, misplaced keys, typo suggestions (Levenshtein distance), missing required fields
2. **Semantic** — platform compatibility, label consistency, date formats, secret hygiene, path/glob validation

All validation runs offline with no Fleet server required. The schema is regularly cross-checked against Fleet's Go source for accuracy.
