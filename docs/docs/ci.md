---
icon: lucide/git-branch
---

# CI/CD integration

!!! warning "Linux runners: build from source for now"

    CI almost always runs on Linux, and no published release carries a Linux
    binary yet — `install.sh` fails there with a 404. Until the release notes
    list a `flint-x.y.z-linux-*.tar.gz` asset, build flint in the job. The
    examples below do that.

## GitHub Actions

```yaml
name: Lint Fleet GitOps
on: [push, pull_request]

jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      # Build flint from a pinned tag. Cargo's cache makes this cheap after
      # the first run; pinning keeps a new flint release from changing your
      # pipeline's verdict without you choosing it.
      - name: Install flint
        run: |
          cargo install \
            --git https://github.com/headmin/fleet-editor-extensions \
            --tag v0.1.4 flint

      # `--format github` emits workflow commands, which GitHub renders as
      # inline annotations on the PR diff. No token, no `gh`, no API call.
      - name: Lint
        run: flint check . --format github
```

Unlike `--git` (which posts one markdown comment), `--format github` needs no
credentials and works on every event — pushes and merge queues included, not
just `pull_request`.

Once Linux binaries ship, this collapses back to a download:

```yaml
      - name: Install flint
        run: |
          curl -fsSL https://raw.githubusercontent.com/headmin/fleet-editor-extensions/main/scripts/install.sh | sh
```

## Pre-commit hook

Add to `.pre-commit-config.yaml`:

```yaml
repos:
  - repo: https://github.com/headmin/fleet-editor-extensions
    rev: v0.1.1
    hooks:
      - id: flint-check
```

## GitLab CI

```yaml
lint:
  image: rust:latest
  script:
    - cargo install --git https://github.com/headmin/fleet-editor-extensions --tag v0.1.4 flint
    - flint check . --format json
```

## JSON output

Use `--format json` in CI for machine-readable output:

```bash
flint check . --format json
```

```json
{
  "version": "0.1.1",
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
