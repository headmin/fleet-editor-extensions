---
icon: lucide/rocket
---

# Getting started

## 1. Initialize configuration

Run `flint init` in your Fleet GitOps repo root:

```bash
cd /path/to/your/fleet-gitops-repo
flint init
```

This auto-detects your directory structure and creates `fleetlint.toml`. It then
walks your top-level directories and asks whether each one should be linted —
`[d]rill in` narrows further, one subdirectory at a time. Before writing
anything it shows the delta ("this config puts 704 of 1156 file(s) in scope")
and warns if you are excluding a directory that live config still references.

Answers become **directory** globs (`platforms/**`), never extension globs.
That matters: a non-empty `include` is authoritative and also scopes the
cross-file rules, which report on scripts and profiles rather than YAML.

`--no-interactive` skips the questions and writes an unnarrowed config. The
hidden `.fleetlint.toml` spelling is still read, so existing repos keep
working untouched.

## 2. Lint your repo

```bash
flint check .
```

Flint scans all YAML files and reports errors, warnings, and info:

```
🔍 Linting directory .

File: fleets/engineering.yml
warning: 'update_new_hosts' expects a boolean value, got null
  --> fleets/engineering.yml:21:22
  help: Use 'true' or 'false'

Summary: Linted 121 file(s)
  0 error(s)
  24 warning(s)
  54 info
```

## 3. Auto-fix

```bash
# Fix safe issues (key renames, typo corrections)
flint check . --fix

# Also apply risky fixes
flint check . --fix --unsafe-fixes
```

## 4. JSON output (for CI)

```bash
flint check . --format json
```

Returns structured JSON with diagnostics per file — exit code 1 on errors, 0 on success.

## 5. Generate & wire (beyond linting)

flint also generates GitOps YAML from real artifacts and repairs path references:

```bash
flint gen software --from app.pkg --full        # .pkg → software stanza (url, hash, …)
flint gen profile --from wifi.mobileconfig      # .mobileconfig → configuration_profiles entry
flint paths . --fix               # repair broken path: refs after a reorg
flint paths . --unwired --interactive   # wire orphaned profiles/scripts into fleets
```

See [Commands](commands.md) for the full set.

## 6. Set up your editor

Install the flint extension for your editor to get real-time diagnostics, completions, and hover docs. See [Editors](editors.md).

## 7. Agent integration

```bash
flint setup-agent       # Install Claude Code skills
flint help-ai           # Command reference for agents
flint help-ai --sop lint      # Step-by-step linting SOP
flint help-ai --sop paths     # Broken-path + unwired-artifact SOP
flint help-ai --sop software  # Artifact-generation SOP
flint help-ai --sop migrate   # Migration SOP
```
