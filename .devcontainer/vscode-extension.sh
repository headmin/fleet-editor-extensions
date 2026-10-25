#!/bin/bash
# The flint VS Code extension is not on the Marketplace: install the latest
# release VSIX. Runs on attach, when the editor's `code` CLI exists.
set -euo pipefail
command -v code >/dev/null || exit 0
code --list-extensions | grep -qx fleetdm.flint && exit 0
url=$(curl -fsSL https://api.github.com/repos/headmin/fleet-editor-extensions/releases/latest \
  | grep -o '"browser_download_url": *"[^"]*\.vsix"' | head -1 | cut -d'"' -f4)
vsix="$(mktemp -d)/flint.vsix"
curl -fsSL "$url" -o "$vsix"
code --install-extension "$vsix"
