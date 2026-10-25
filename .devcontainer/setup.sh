#!/bin/bash
# Installs flint in the container. Two cases:
#   - flint's own repo: build the checkout (rust-toolchain.toml picks the compiler)
#   - a GitOps repo using this as a template: install the release binary
set -euo pipefail

if [ -f crates/flint-lint/Cargo.toml ]; then
  echo "Building flint from source..."
  cargo build --release --locked -p flint
  sudo install -m 755 target/release/flint /usr/local/bin/flint
  # scripts/ecosystem.sh needs jq
  command -v jq >/dev/null || { sudo apt-get update -qq && sudo apt-get install -y -qq jq; }
else
  # FLINT_VERSION (from the host, see devcontainer.json) pins a release; unset = latest
  curl -fsSL https://raw.githubusercontent.com/headmin/fleet-editor-extensions/main/scripts/install.sh | sh
  if [ -f default.yml ] && [ ! -f fleetlint.toml ] && [ ! -f .fleetlint.toml ]; then
    flint init --no-interactive
  fi
fi

flint --version
