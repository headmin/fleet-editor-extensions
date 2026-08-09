---
icon: lucide/git-branch
---

# CI/CD integration

## GitHub Actions

```yaml
name: Lint Fleet GitOps
on: [push, pull_request]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # Downloads a statically linked binary; no Rust toolchain needed and
      # no minimum glibc version, so the runner image does not matter.
      # Pin with FLINT_VERSION so a new release cannot change your
      # pipeline's verdict without you choosing it.
      - name: Install flint
        run: |
          FLINT_VERSION=0.2.0 curl -fsSL https://raw.githubusercontent.com/headmin/fleet-editor-extensions/main/scripts/install.sh | sh

      # `--format github` emits workflow commands, which GitHub renders as
      # inline annotations on the PR diff. No token, no `gh`, no API call.
      - name: Lint
        run: flint check . --format github
```

Unlike `--git` (which posts one markdown comment), `--format github` needs no
credentials and works on every event — pushes and merge queues included, not
just `pull_request`.

## Pre-commit hook

Add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/headmin/fleet-editor-extensions
    rev: v0.2.0
    hooks:
      - id: flint
```

## GitLab CI

```yaml
lint:
  image: ubuntu:latest
  script:
    # ubuntu:latest ships neither curl nor wget.
    - apt-get update && apt-get install -y curl ca-certificates
    - FLINT_VERSION=0.2.0 curl -fsSL https://raw.githubusercontent.com/headmin/fleet-editor-extensions/main/scripts/install.sh | sh
    - flint check . --format json
```

## JSON output

Use `--format json` in CI for machine-readable output:

```bash
flint check . --format json
```

```json
{
  "version": "0.2.0",
  "files": [...],
  "summary": {
    "files_linted": 121,
    "errors": 0,
    "warnings": 24,
    "infos": 54
  }
}
```

Exit code `1` when errors are found, `0` otherwise. Warnings and infos do not cause failure.

## Dev containers

A [`.devcontainer`](https://github.com/headmin/fleet-editor-extensions/tree/main/.devcontainer) config is included for GitHub Codespaces. It auto-installs flint, initializes `.fleetlint.toml`, and configures the VS Code extension.
