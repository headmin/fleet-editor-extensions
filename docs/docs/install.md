---
icon: lucide/download
---

# Installation

## macOS

Download the PKG installer from [GitHub Releases](https://github.com/headmin/fleet-editor-extensions/releases/latest):

> [Flint v0.1.4](https://github.com/headmin/fleet-editor-extensions/releases/tag/v0.1.4) — signed & notarized by Apple

Double-click to install. Installs to `/usr/local/bin/flint`.

## Linux

!!! warning "Build from source for now"

    No published release carries a Linux binary yet, so `install.sh` fails
    with a 404 on Linux. Use [Build from source](#build-from-source) below —
    it is the only working Linux path today.

    Static x64 and arm64 binaries land in the next release; until the
    release notes list a `flint-x.y.z-linux-*.tar.gz` asset, the installer
    script has nothing to download.

## Manual download

| Platform | Asset |
|---|---|
| macOS (Apple Silicon) | `flint-x.y.z.pkg` (signed & notarized) |
| macOS (tar.gz) | `flint-x.y.z-darwin-arm64.tar.gz` |
| Linux x64 / ARM64 | *not published yet — build from source* |

macOS Intel (x86_64) is not supported.

```bash
tar xzf flint-*.tar.gz
sudo mv flint /usr/local/bin/
```

## Build from source

Works on macOS and Linux, and is currently the only supported way to get
flint on Linux.

```bash
git clone https://github.com/headmin/fleet-editor-extensions
cd fleet-editor-extensions
cargo build --release -p flint
sudo cp target/release/flint /usr/local/bin/
```

Requires Rust 1.81+ (`rustup update stable`). No system libraries are
needed — there is no OpenSSL in the dependency tree.

## Dev container

A [`.devcontainer`](https://github.com/headmin/fleet-editor-extensions/tree/main/.devcontainer) config is included for GitHub Codespaces. It auto-installs flint and initializes `.fleetlint.toml`.

## Verify installation

```bash
flint --version
# flint 0.1.4+20260621.0730 (Fleet sync: ...)
```
