---
icon: lucide/download
---

# Installation

## macOS

Download the PKG installer from [GitHub Releases](https://github.com/headmin/fleet-editor-extensions/releases/latest):

> [Flint v0.2.0](https://github.com/headmin/fleet-editor-extensions/releases/tag/v0.2.0) — signed & notarized by Apple

Double-click to install. Installs to `/usr/local/bin/flint`.

## Linux

```bash
curl -fsSL https://raw.githubusercontent.com/headmin/fleet-editor-extensions/main/scripts/install.sh | sh
```

The script detects your architecture (x64/arm64), downloads the latest
release tarball, and installs to `/usr/local/bin`.

```bash
# Install to your home directory instead (no sudo)
FLINT_INSTALL_DIR=$HOME/.local/bin curl -fsSL https://raw.githubusercontent.com/headmin/fleet-editor-extensions/main/scripts/install.sh | sh
```

The Linux binaries are **statically linked** (musl), so they carry no glibc
requirement and run on any distribution, including slim container images.
Available from v0.2.0 onward — earlier releases are macOS only.

## Manual download

| Platform | Asset |
|---|---|
| macOS (Apple Silicon) | `flint-x.y.z.pkg` (signed & notarized) |
| macOS (tar.gz) | `flint-x.y.z-darwin-arm64.tar.gz` |
| Linux x64 | `flint-x.y.z-linux-x64.tar.gz` (static, no glibc requirement) |
| Linux ARM64 | `flint-x.y.z-linux-arm64.tar.gz` (static) |

macOS Intel (x86_64) is not supported.

```bash
tar xzf flint-*.tar.gz
sudo mv flint /usr/local/bin/
```

## Build from source

Works on macOS and Linux.

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
# flint 0.2.0+20260808.1610 (Fleet sync: ...)
```
