#!/bin/bash
# Cougar Installation Script
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/yourusername/cougar/main/scripts/install.sh | bash
#
# Or with version:
#   curl -sSL https://raw.githubusercontent.com/yourusername/cougar/main/scripts/install.sh | bash -s v0.1.0

set -e

REPO="yourusername/cougar"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${1:-latest}"
GITHUB_API="https://api.github.com/repos/$REPO"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Detect platform
detect_platform() {
    local os=$(uname -s)
    local arch=$(uname -m)

    case "$os" in
        Linux)
            case "$arch" in
                x86_64)
                    echo "linux-x86_64"
                    ;;
                *)
                    log_error "Unsupported architecture: $arch"
                    log_info "Only x86_64 Linux is supported via this installer"
                    exit 1
                    ;;
            esac
            ;;
        Darwin)
            log_error "macOS binaries are not available via this installer"
            log_info "Please download from: https://github.com/$REPO/releases"
            log_info "Or build locally with: ./scripts/build-macos.sh"
            exit 1
            ;;
        *)
            log_error "Unsupported OS: $os"
            exit 1
            ;;
    esac
}

# Check dependencies
check_dependencies() {
    local missing=()

    if ! command -v curl &> /dev/null; then
        missing+=("curl")
    fi

    if ! command -v tar &> /dev/null; then
        missing+=("tar")
    fi

    if [ ${#missing[@]} -ne 0 ]; then
        log_error "Missing required dependencies: ${missing[*]}"
        exit 1
    fi
}

# Get download URL from GitHub releases
get_download_url() {
    local platform="$1"
    local api_url

    if [ "$VERSION" = "latest" ]; then
        api_url="$GITHUB_API/releases/latest"
    else
        api_url="$GITHUB_API/releases/tags/$VERSION"
    fi

    log_info "Fetching release info from GitHub..."

    local release_json=$(curl -fsSL "$api_url")
    if [ $? -ne 0 ]; then
        log_error "Failed to fetch release information"
        exit 1
    fi

    # Extract download URL for the platform
    local download_url=$(echo "$release_json" | grep -o "https://github.com/$REPO/releases/download/[^\"]*${platform}.tar.gz" | head -1)

    if [ -z "$download_url" ]; then
        log_error "Could not find ${platform} release"
        exit 1
    fi

    echo "$download_url"
}

# Download and install
install_cougar() {
    local platform=$(detect_platform)
    local download_url=$(get_download_url "$platform")

    log_info "Installing Cougar ($VERSION) for $platform"

    # Create temp directory
    local tmp_dir=$(mktemp -d)
    trap "rm -rf $tmp_dir" EXIT

    log_info "Downloading from GitHub releases..."

    if ! curl -fSL "$download_url" -o "$tmp_dir/cougar.tar.gz"; then
        log_error "Failed to download Cougar"
        exit 1
    fi

    # Try to download checksum
    if curl -fSL "${download_url}.sha256" -o "$tmp_dir/cougar.tar.gz.sha256" 2>/dev/null; then
        log_info "Verifying checksum..."
        cd "$tmp_dir"
        if ! sha256sum -c cougar.tar.gz.sha256; then
            log_error "Checksum verification failed"
            exit 1
        fi
        cd - > /dev/null
    else
        log_warn "Checksum not available, skipping verification"
    fi

    # Extract
    log_info "Extracting archive..."
    tar -xzf "$tmp_dir/cougar.tar.gz" -C "$tmp_dir"

    # Create install directory if it doesn't exist
    mkdir -p "$INSTALL_DIR"

    # Install binary
    log_info "Installing to $INSTALL_DIR/cougar..."
    mv "$tmp_dir/cougar" "$INSTALL_DIR/cougar"
    chmod +x "$INSTALL_DIR/cougar"

    log_info "Installation complete!"

    # Check if install directory is in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        log_warn "$INSTALL_DIR is not in your PATH"
        log_info "Add it to your PATH by adding this to your shell profile:"
        log_info '  export PATH="$HOME/.local/bin:$PATH"'
    fi

    # Show version
    if command -v cougar &> /dev/null; then
        log_info "Cougar version: $(cougar --version 2>&1 | head -1 || echo 'unknown')"
    else
        log_info "Run: export PATH=\"$INSTALL_DIR:\$PATH\" to use cougar"
    fi
}

main() {
    log_info "Cougar Installer"
    check_dependencies
    install_cougar
}

main "$@"
