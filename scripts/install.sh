#!/bin/sh
set -eu

# Flint installer — downloads and installs the latest flint binary.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/headmin/fleet-editor-extensions/main/scripts/install.sh | sh
#
# Options (via env vars):
#   FLINT_VERSION=0.2.0                Pin to a specific version (default: latest)
#   FLINT_INSTALL_DIR=/usr/local/bin   Install location (default: /usr/local/bin)

REPO="headmin/fleet-editor-extensions"
BINARY="flint"
INSTALL_DIR="${FLINT_INSTALL_DIR:-/usr/local/bin}"

# Colors (if terminal)
if [ -t 1 ]; then
    GREEN='\033[0;32m'
    RED='\033[0;31m'
    CYAN='\033[0;36m'
    NC='\033[0m'
else
    GREEN='' RED='' CYAN='' NC=''
fi

info()  { printf "${GREEN}[INFO]${NC} %s\n" "$1"; }
error() { printf "${RED}[ERROR]${NC} %s\n" "$1" >&2; exit 1; }
step()  { printf "${CYAN}[====]${NC} %s\n" "$1"; }

detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        darwin) OS="darwin" ;;
        linux)  OS="linux" ;;
        *)      error "Unsupported OS: $OS" ;;
    esac

    case "$ARCH" in
        x86_64|amd64)
            if [ "$OS" = "darwin" ]; then
                error "macOS Intel (x86_64) is not supported. Use Apple Silicon."
            fi
            ARCH="x64" ;;
        aarch64|arm64)      ARCH="arm64" ;;
        *)                  error "Unsupported architecture: $ARCH" ;;
    esac

    PLATFORM="${OS}-${ARCH}"
    info "Platform: $PLATFORM"
}

get_latest_version() {
    if [ -n "${FLINT_VERSION:-}" ]; then
        VERSION="$FLINT_VERSION"
        info "Using pinned version: $VERSION"
        return
    fi

    step "Fetching latest version"

    if command -v curl >/dev/null 2>&1; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\([^"]*\)".*/\1/') || true
    fi

    # Fallback: list all releases (no /latest if only prereleases)
    if [ -z "${VERSION:-}" ]; then
        VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases" 2>/dev/null \
            | grep '"tag_name"' | head -1 | sed 's/.*"v\([^"]*\)".*/\1/') || true
    fi

    if [ -z "${VERSION:-}" ]; then
        error "Could not determine latest version. Set FLINT_VERSION=x.y.z manually."
    fi

    info "Latest version: $VERSION"
}

download_and_install() {
    TMPDIR=$(mktemp -d)
    trap 'rm -rf "$TMPDIR"' EXIT

    # macOS: use signed & notarized PKG installer
    if [ "$OS" = "darwin" ]; then
        PKG="${BINARY}-${VERSION}.pkg"
        URL="https://github.com/$REPO/releases/download/v${VERSION}/${PKG}"

        step "Downloading $PKG (signed & notarized)"

        if command -v curl >/dev/null 2>&1; then
            curl -fsSL "$URL" -o "$TMPDIR/$PKG" || error "Download failed: $URL"
        elif command -v wget >/dev/null 2>&1; then
            wget -q "$URL" -O "$TMPDIR/$PKG" || error "Download failed: $URL"
        else
            error "Neither curl nor wget found"
        fi

        step "Installing PKG (requires sudo)"
        sudo installer -pkg "$TMPDIR/$PKG" -target / || error "PKG installation failed"
        return
    fi

    # Linux: download tar.gz
    ARCHIVE="${BINARY}-${VERSION}-${PLATFORM}.tar.gz"
    URL="https://github.com/$REPO/releases/download/v${VERSION}/${ARCHIVE}"

    step "Downloading $ARCHIVE"

    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$URL" -o "$TMPDIR/$ARCHIVE" || error "Download failed: $URL"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$URL" -O "$TMPDIR/$ARCHIVE" || error "Download failed: $URL"
    else
        error "Neither curl nor wget found"
    fi

    step "Extracting"
    tar -xzf "$TMPDIR/$ARCHIVE" -C "$TMPDIR"

    if [ ! -f "$TMPDIR/$BINARY" ]; then
        error "Binary not found in archive"
    fi

    step "Installing to $INSTALL_DIR"

    if [ -w "$INSTALL_DIR" ]; then
        mv "$TMPDIR/$BINARY" "$INSTALL_DIR/$BINARY"
        chmod +x "$INSTALL_DIR/$BINARY"
    else
        info "Requires sudo for $INSTALL_DIR"
        sudo mv "$TMPDIR/$BINARY" "$INSTALL_DIR/$BINARY"
        sudo chmod +x "$INSTALL_DIR/$BINARY"
    fi
}

verify() {
    step "Verifying"

    if ! command -v "$INSTALL_DIR/$BINARY" >/dev/null 2>&1; then
        error "$BINARY not found in PATH after install"
    fi

    "$INSTALL_DIR/$BINARY" --version

    info "Installed successfully!"
    echo ""
    info "Get started:"
    info "  flint init              # create .fleetlint.toml"
    info "  flint check .           # lint your GitOps repo"
    info "  flint setup-agent       # install Claude Code skills"
    info "  flint help-ai           # agent command reference"
}

main() {
    echo ""
    echo "  Flint Installer"
    echo "  Fleet GitOps YAML linter & language server"
    echo ""

    detect_platform
    get_latest_version
    download_and_install
    verify
}

main
