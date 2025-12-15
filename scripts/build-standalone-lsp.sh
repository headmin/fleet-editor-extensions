#!/bin/bash
# ============================================================
# Fleet Schema Gen - Standalone LSP Binary Build Script
# ============================================================
# Builds, signs, notarizes, and packages the LSP binary for
# distribution via GitHub releases.
#
# Used by: Zed extension auto-download, Sublime package, manual install
# ============================================================

set -e

# ============================================================
# CONFIGURATION
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
RUST_DIR="$PROJECT_ROOT/fleet-schema-gen"
DIST_DIR="$PROJECT_ROOT/dist"

BINARY_NAME="fleet-schema-gen"

# 1Password vault for code signing credentials
VAULT_NAME="${VAULT_NAME:-dev-credentials}"
OP_ACCOUNT="${OP_ACCOUNT:-my.1password.eu}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# GitHub CLI path
GH_CLI="${GH_CLI:-}"
if [ -z "$GH_CLI" ]; then
    if command -v gh &> /dev/null; then
        GH_CLI="$(command -v gh)"
    elif [ -f "/opt/homebrew/bin/gh" ]; then
        GH_CLI="/opt/homebrew/bin/gh"
    elif [ -f "/usr/local/bin/gh" ]; then
        GH_CLI="/usr/local/bin/gh"
    else
        GH_CLI="gh"
    fi
fi

# ============================================================
# FUNCTIONS
# ============================================================

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_step() {
    echo -e "${BLUE}▶${NC} $1"
}

show_usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Build standalone LSP binary for distribution via GitHub releases.

OPTIONS:
    -p, --platform PLATFORM   Target platform (default: auto-detect)
                              Platforms: darwin-arm64, darwin-x64, linux-x64,
                                         linux-arm64, windows-x64
    --all                     Build for all platforms (requires cross-compilation)
    --sign                    Code sign (macOS only, requires Apple Developer cert)
    --notarize                Sign and notarize (macOS only)
    --release                 Upload to GitHub release
    -t, --tag TAG             Release tag (default: lsp-v{version})
    --quick                   Skip cargo clean (faster, may use stale timestamp)
    -h, --help                Show this help message

ENVIRONMENT:
    VAULT_NAME                1Password vault for credentials (default: dev-credentials)

EXAMPLES:
    # Build for current platform
    $(basename "$0")

    # Build for macOS with signing and notarization
    $(basename "$0") --sign --notarize

    # Build and upload to GitHub release
    $(basename "$0") --sign --notarize --release

    # Build for specific platform
    $(basename "$0") --platform linux-x64

EOF
}

# Detect current platform
detect_platform() {
    local os=$(uname -s | tr '[:upper:]' '[:lower:]')
    local arch=$(uname -m)

    case "$os" in
        darwin)
            case "$arch" in
                arm64) echo "darwin-arm64" ;;
                x86_64) echo "darwin-x64" ;;
                *) echo "darwin-$arch" ;;
            esac
            ;;
        linux)
            case "$arch" in
                aarch64) echo "linux-arm64" ;;
                x86_64) echo "linux-x64" ;;
                *) echo "linux-$arch" ;;
            esac
            ;;
        mingw*|msys*|cygwin*)
            echo "windows-x64"
            ;;
        *)
            echo "$os-$arch"
            ;;
    esac
}

# Get Rust target triple for platform
get_rust_target() {
    local platform="$1"
    case "$platform" in
        darwin-arm64) echo "aarch64-apple-darwin" ;;
        darwin-x64) echo "x86_64-apple-darwin" ;;
        linux-x64) echo "x86_64-unknown-linux-gnu" ;;
        linux-arm64) echo "aarch64-unknown-linux-gnu" ;;
        windows-x64) echo "x86_64-pc-windows-msvc" ;;
        *) echo "" ;;
    esac
}

# Get archive extension for platform
get_archive_ext() {
    local platform="$1"
    case "$platform" in
        windows-*) echo "zip" ;;
        *) echo "tar.gz" ;;
    esac
}

# Load credentials from 1Password
load_credentials() {
    log_info "Loading credentials from 1Password..."

    if ! op item list --vault "$VAULT_NAME" --account "$OP_ACCOUNT" >/dev/null 2>&1; then
        log_error "1Password CLI not connected"
        log_info "Enable 1Password CLI integration in Settings → Developer"
        exit 1
    fi

    export CODESIGN_IDENTITY=$(op read "op://$VAULT_NAME/CODESIGN_IDENTITY/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_APPLE_ID=$(op read "op://$VAULT_NAME/NOTARIZATION_APPLE_ID/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_TEAM_ID=$(op read "op://$VAULT_NAME/NOTARIZATION_TEAM_ID/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_PASSWORD=$(op read "op://$VAULT_NAME/NOTARIZATION_PASSWORD/credential" --account "$OP_ACCOUNT")

    log_info "✓ Credentials loaded"
}

# Code sign macOS binary
codesign_binary() {
    local binary_path="$1"

    log_step "Code signing binary..."

    codesign --force \
        --options runtime \
        --timestamp \
        --sign "$CODESIGN_IDENTITY" \
        "$binary_path"

    codesign -vvv --deep --strict "$binary_path"
    log_info "✓ Code signing completed"
}

# Notarize macOS binary
notarize_binary() {
    local binary_path="$1"
    local binary_name=$(basename "$binary_path")

    log_step "Submitting for notarization..."

    # Create ZIP for notarization
    local zip_path="$DIST_DIR/${binary_name}-notarize.zip"
    ditto -c -k --keepParent "$binary_path" "$zip_path"

    SUBMISSION_OUTPUT=$(xcrun notarytool submit "$zip_path" \
        --apple-id "$NOTARIZATION_APPLE_ID" \
        --team-id "$NOTARIZATION_TEAM_ID" \
        --password "$NOTARIZATION_PASSWORD" \
        --wait)

    echo "$SUBMISSION_OUTPUT"

    if echo "$SUBMISSION_OUTPUT" | grep -q "status: Accepted"; then
        log_info "✓ Notarization successful!"
        rm -f "$zip_path"
        return 0
    else
        log_error "Notarization failed"
        rm -f "$zip_path"
        return 1
    fi
}

# Build binary for a platform
build_binary() {
    local platform="$1"
    local rust_target=$(get_rust_target "$platform")

    if [ -z "$rust_target" ]; then
        log_error "Unknown platform: $platform"
        return 1
    fi

    log_step "Building for $platform ($rust_target)..."

    cd "$RUST_DIR"

    # Ensure target is installed
    if ! rustup target list --installed | grep -q "$rust_target"; then
        log_info "Installing target: $rust_target"
        rustup target add "$rust_target"
    fi

    # Clean build unless quick mode
    if [ "$QUICK_BUILD" = false ]; then
        cargo clean --target "$rust_target" 2>/dev/null || true
    fi

    # Build
    cargo build --release --target "$rust_target"

    # Copy binary to dist
    local src_binary="$RUST_DIR/target/$rust_target/release/$BINARY_NAME"
    local dest_binary="$DIST_DIR/$BINARY_NAME-$platform"

    # Add .exe for Windows
    if [[ "$platform" == windows-* ]]; then
        src_binary="${src_binary}.exe"
        dest_binary="${dest_binary}.exe"
    fi

    cp "$src_binary" "$dest_binary"
    chmod +x "$dest_binary"

    # Strip binary (except Windows)
    if [[ "$platform" != windows-* ]]; then
        strip "$dest_binary" 2>/dev/null || true
    fi

    log_info "✓ Built: $dest_binary"
    ls -lh "$dest_binary"

    echo "$dest_binary"
}

# Create distribution archive
create_archive() {
    local binary_path="$1"
    local platform="$2"
    local version="$3"

    local ext=$(get_archive_ext "$platform")
    local archive_name="$BINARY_NAME-$version-$platform.$ext"
    local archive_path="$DIST_DIR/$archive_name"
    local binary_name=$(basename "$binary_path")

    log_step "Creating archive: $archive_name"

    cd "$DIST_DIR"

    if [ "$ext" = "zip" ]; then
        zip -j "$archive_name" "$binary_name"
    else
        tar -czf "$archive_name" "$binary_name"
    fi

    # Create checksum
    shasum -a 256 "$archive_name" > "${archive_name}.sha256"

    log_info "✓ Archive: $archive_path"
    log_info "✓ Checksum: ${archive_path}.sha256"

    # Clean up standalone binary
    rm -f "$binary_path"

    echo "$archive_path"
}

# ============================================================
# PARSE ARGUMENTS
# ============================================================

PLATFORM=""
BUILD_ALL=false
CODESIGN=false
NOTARIZE=false
CREATE_RELEASE=false
TAG_NAME=""
QUICK_BUILD=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -p|--platform)
            PLATFORM="$2"
            shift 2
            ;;
        --all)
            BUILD_ALL=true
            shift
            ;;
        --sign)
            CODESIGN=true
            shift
            ;;
        --notarize)
            CODESIGN=true
            NOTARIZE=true
            shift
            ;;
        --release)
            CREATE_RELEASE=true
            shift
            ;;
        -t|--tag)
            TAG_NAME="$2"
            shift 2
            ;;
        --quick)
            QUICK_BUILD=true
            shift
            ;;
        -h|--help)
            show_usage
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            show_usage
            exit 1
            ;;
    esac
done

# ============================================================
# DETECT VERSION AND PLATFORMS
# ============================================================

cd "$RUST_DIR"
VERSION=$(grep '^version' Cargo.toml | head -n1 | cut -d'"' -f2)

if [ -z "$TAG_NAME" ]; then
    TAG_NAME="lsp-v${VERSION}"
fi

if [ -z "$PLATFORM" ] && [ "$BUILD_ALL" = false ]; then
    PLATFORM=$(detect_platform)
fi

# Determine which platforms to build
if [ "$BUILD_ALL" = true ]; then
    PLATFORMS="darwin-arm64 darwin-x64 linux-x64 linux-arm64"
    # Windows requires special cross-compilation setup, skip by default
    log_warn "Windows build requires cross-compilation setup, skipping"
else
    PLATFORMS="$PLATFORM"
fi

echo "============================================================"
echo "Fleet Schema Gen - Standalone LSP Build"
echo "============================================================"
echo "Version: $VERSION"
echo "Tag: $TAG_NAME"
echo "Platforms: $PLATFORMS"
if [ "$CODESIGN" = true ]; then
    echo "Code Signing: Enabled"
fi
if [ "$NOTARIZE" = true ]; then
    echo "Notarization: Enabled"
fi
if [ "$CREATE_RELEASE" = true ]; then
    echo "GitHub Release: Enabled"
fi
echo "============================================================"
echo ""

# ============================================================
# SETUP
# ============================================================

mkdir -p "$DIST_DIR"

# Load credentials if signing enabled
if [ "$CODESIGN" = true ]; then
    load_credentials
    echo ""
fi

# ============================================================
# BUILD EACH PLATFORM
# ============================================================

ARCHIVES=()

for plat in $PLATFORMS; do
    log_info "=== Building $plat ==="

    # Build
    BINARY_PATH=$(build_binary "$plat")

    # Sign macOS binaries
    if [[ "$plat" == darwin-* ]] && [ "$CODESIGN" = true ]; then
        codesign_binary "$BINARY_PATH"

        if [ "$NOTARIZE" = true ]; then
            if ! notarize_binary "$BINARY_PATH"; then
                log_warn "Notarization failed for $plat, continuing..."
            fi
        fi
    fi

    # Create archive
    ARCHIVE_PATH=$(create_archive "$BINARY_PATH" "$plat" "$VERSION")
    ARCHIVES+=("$ARCHIVE_PATH")

    echo ""
done

# ============================================================
# SUMMARY
# ============================================================

echo "============================================================"
log_info "Build complete!"
echo "============================================================"
echo ""
echo "Distribution files:"
for archive in "${ARCHIVES[@]}"; do
    echo "  $(basename "$archive")"
    echo "  $(basename "$archive").sha256"
done
echo ""

# ============================================================
# GITHUB RELEASE
# ============================================================

if [ "$CREATE_RELEASE" = true ]; then
    echo "============================================================"
    log_step "Creating GitHub release..."
    echo "============================================================"
    echo ""

    cd "$PROJECT_ROOT"

    # Check if release exists
    if $GH_CLI release view "$TAG_NAME" >/dev/null 2>&1; then
        log_info "Release $TAG_NAME exists, uploading artifacts..."
    else
        log_info "Creating release $TAG_NAME..."
        $GH_CLI release create "$TAG_NAME" \
            --title "LSP Binary $TAG_NAME" \
            --prerelease \
            --notes "Standalone fleet-schema-gen LSP binary for Zed, Sublime, and manual installation.

## Installation

### macOS (Apple Silicon)
\`\`\`bash
curl -sL https://github.com/fleetdm/fleet/releases/download/$TAG_NAME/fleet-schema-gen-$VERSION-darwin-arm64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/
\`\`\`

### macOS (Intel)
\`\`\`bash
curl -sL https://github.com/fleetdm/fleet/releases/download/$TAG_NAME/fleet-schema-gen-$VERSION-darwin-x64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/
\`\`\`

### Linux (x64)
\`\`\`bash
curl -sL https://github.com/fleetdm/fleet/releases/download/$TAG_NAME/fleet-schema-gen-$VERSION-linux-x64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/
\`\`\`

### Linux (ARM64)
\`\`\`bash
curl -sL https://github.com/fleetdm/fleet/releases/download/$TAG_NAME/fleet-schema-gen-$VERSION-linux-arm64.tar.gz | tar xz
sudo mv fleet-schema-gen /usr/local/bin/
\`\`\`
"
    fi

    # Upload artifacts
    log_info "Uploading artifacts..."
    for archive in "${ARCHIVES[@]}"; do
        $GH_CLI release upload "$TAG_NAME" "$archive" --clobber
        $GH_CLI release upload "$TAG_NAME" "${archive}.sha256" --clobber
    done

    echo ""
    log_info "✓ Release published!"
    echo ""
    echo "Release URL:"
    $GH_CLI release view "$TAG_NAME" --json url -q .url
    echo ""
fi

echo "Done!"
