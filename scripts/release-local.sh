#!/bin/bash
# ============================================================
# Fleet GitOps - Unified Local Release Script
# ============================================================
# Builds everything for the current platform in one go:
#   - LSP binary (signed + notarized)
#   - Standalone archive for Zed/Sublime
#   - VSIX for VS Code
#   - Uploads to GitHub release
#
# For private betas and local releases.
# ============================================================

set -e

# ============================================================
# CONFIGURATION
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
RUST_DIR="$PROJECT_ROOT/fleet-schema-gen"
EXTENSION_DIR="$PROJECT_ROOT/vscode-extension"
DIST_DIR="$PROJECT_ROOT/dist"

BINARY_NAME="fleet-schema-gen"

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
    darwin)
        case "$ARCH" in
            arm64) PLATFORM="darwin-arm64"; RUST_TARGET="aarch64-apple-darwin" ;;
            x86_64) PLATFORM="darwin-x64"; RUST_TARGET="x86_64-apple-darwin" ;;
        esac
        ;;
    linux)
        case "$ARCH" in
            aarch64) PLATFORM="linux-arm64"; RUST_TARGET="aarch64-unknown-linux-gnu" ;;
            x86_64) PLATFORM="linux-x64"; RUST_TARGET="x86_64-unknown-linux-gnu" ;;
        esac
        ;;
esac

if [ -z "$PLATFORM" ]; then
    echo "Unsupported platform: $OS $ARCH"
    exit 1
fi

# 1Password vault
VAULT_NAME="${VAULT_NAME:-dev-credentials}"
OP_ACCOUNT="${OP_ACCOUNT:-my.1password.eu}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

# GitHub CLI
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

log_info() { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step() { echo -e "${BLUE}▶${NC} $1"; }
log_section() { echo -e "\n${CYAN}════════════════════════════════════════════════════════════${NC}"; echo -e "${CYAN}  $1${NC}"; echo -e "${CYAN}════════════════════════════════════════════════════════════${NC}\n"; }

show_usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Unified local release for private betas. Builds everything for current platform:
  - LSP binary (signed + notarized on macOS)
  - Standalone archive (.tar.gz) for Zed/Sublime
  - VS Code extension (.vsix)

OPTIONS:
    --sign              Code sign binary (macOS only)
    --notarize          Sign and notarize binary (macOS only)
    --release           Upload to GitHub release
    -t, --tag TAG       Release tag (default: v{extension_version})
    --quick             Skip cargo clean (faster, may use stale build)
    --skip-vsix         Skip VSIX build (standalone LSP only)
    --dry-run           Show what would be done without executing
    -h, --help          Show this help

EXAMPLES:
    # Build everything, sign and notarize, upload to GitHub
    $(basename "$0") --notarize --release

    # Quick dev build (no signing)
    $(basename "$0") --quick

    # Just standalone LSP with notarization
    $(basename "$0") --notarize --skip-vsix

EOF
}

load_credentials() {
    log_step "Loading credentials from 1Password..."

    if ! op item list --vault "$VAULT_NAME" --account "$OP_ACCOUNT" >/dev/null 2>&1; then
        log_error "1Password CLI not connected"
        log_info "Enable: 1Password → Settings → Developer → Connect with CLI"
        exit 1
    fi

    export CODESIGN_IDENTITY=$(op read "op://$VAULT_NAME/CODESIGN_IDENTITY/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_APPLE_ID=$(op read "op://$VAULT_NAME/NOTARIZATION_APPLE_ID/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_TEAM_ID=$(op read "op://$VAULT_NAME/NOTARIZATION_TEAM_ID/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_PASSWORD=$(op read "op://$VAULT_NAME/NOTARIZATION_PASSWORD/credential" --account "$OP_ACCOUNT")

    log_info "Credentials loaded (Team: $NOTARIZATION_TEAM_ID)"
}

codesign_binary() {
    local binary="$1"
    log_step "Code signing: $(basename "$binary")"

    codesign --force --options runtime --timestamp --sign "$CODESIGN_IDENTITY" "$binary"
    codesign -vvv --deep --strict "$binary"

    log_info "Code signing complete"
}

notarize_binary() {
    local binary="$1"
    local name=$(basename "$binary")

    log_step "Notarizing: $name"

    local zip_path="$DIST_DIR/${name}-notarize.zip"
    ditto -c -k --keepParent "$binary" "$zip_path"

    local output=$(xcrun notarytool submit "$zip_path" \
        --apple-id "$NOTARIZATION_APPLE_ID" \
        --team-id "$NOTARIZATION_TEAM_ID" \
        --password "$NOTARIZATION_PASSWORD" \
        --wait 2>&1)

    echo "$output"
    rm -f "$zip_path"

    if echo "$output" | grep -q "status: Accepted"; then
        log_info "Notarization accepted"
        return 0
    else
        log_error "Notarization failed"
        return 1
    fi
}

# ============================================================
# PARSE ARGUMENTS
# ============================================================

CODESIGN=false
NOTARIZE=false
CREATE_RELEASE=false
TAG_NAME=""
QUICK_BUILD=false
SKIP_VSIX=false
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --sign) CODESIGN=true; shift ;;
        --notarize) CODESIGN=true; NOTARIZE=true; shift ;;
        --release) CREATE_RELEASE=true; shift ;;
        -t|--tag) TAG_NAME="$2"; shift 2 ;;
        --quick) QUICK_BUILD=true; shift ;;
        --skip-vsix) SKIP_VSIX=true; shift ;;
        --dry-run) DRY_RUN=true; shift ;;
        -h|--help) show_usage; exit 0 ;;
        *) log_error "Unknown option: $1"; show_usage; exit 1 ;;
    esac
done

# ============================================================
# DETECT VERSIONS
# ============================================================

cd "$RUST_DIR"
LSP_VERSION=$(grep '^version' Cargo.toml | head -n1 | cut -d'"' -f2)

cd "$EXTENSION_DIR"
EXT_VERSION=$(grep '"version"' package.json | head -1 | sed 's/.*"version": "\([^"]*\)".*/\1/')
EXT_NAME=$(grep '"name"' package.json | head -1 | sed 's/.*"name": "\([^"]*\)".*/\1/')

if [ -z "$TAG_NAME" ]; then
    TAG_NAME="v${EXT_VERSION}"
fi

# ============================================================
# SHOW PLAN
# ============================================================

echo ""
echo "┌────────────────────────────────────────────────────────────┐"
echo "│           Fleet GitOps - Local Release Build              │"
echo "├────────────────────────────────────────────────────────────┤"
printf "│  Platform:      %-40s │\n" "$PLATFORM"
printf "│  LSP Version:   %-40s │\n" "$LSP_VERSION"
printf "│  Ext Version:   %-40s │\n" "$EXT_VERSION"
printf "│  Tag:           %-40s │\n" "$TAG_NAME"
echo "├────────────────────────────────────────────────────────────┤"
printf "│  Code Sign:     %-40s │\n" "$CODESIGN"
printf "│  Notarize:      %-40s │\n" "$NOTARIZE"
printf "│  Build VSIX:    %-40s │\n" "$([[ $SKIP_VSIX == true ]] && echo 'false' || echo 'true')"
printf "│  GitHub Release:%-40s │\n" "$CREATE_RELEASE"
echo "└────────────────────────────────────────────────────────────┘"
echo ""

if [ "$DRY_RUN" = true ]; then
    log_warn "DRY RUN - no actions will be performed"
    exit 0
fi

# ============================================================
# STEP 1: SETUP
# ============================================================

log_section "Step 1: Setup"

mkdir -p "$DIST_DIR"
rm -f "$DIST_DIR"/*.tar.gz "$DIST_DIR"/*.vsix "$DIST_DIR"/*.sha256

if [ "$CODESIGN" = true ] && [ "$OS" = "darwin" ]; then
    load_credentials
fi

# ============================================================
# STEP 2: BUILD RUST BINARY (once)
# ============================================================

log_section "Step 2: Build LSP Binary"

cd "$RUST_DIR"

if [ "$QUICK_BUILD" = false ]; then
    log_step "Clean build..."
    cargo clean --target "$RUST_TARGET" 2>/dev/null || cargo clean
fi

log_step "Building for $PLATFORM ($RUST_TARGET)..."
cargo build --release --target "$RUST_TARGET"

# Copy to dist
BINARY_PATH="$DIST_DIR/$BINARY_NAME-$PLATFORM"
cp "$RUST_DIR/target/$RUST_TARGET/release/$BINARY_NAME" "$BINARY_PATH"
chmod +x "$BINARY_PATH"
strip "$BINARY_PATH" 2>/dev/null || true

log_info "Binary: $BINARY_PATH"
ls -lh "$BINARY_PATH"
"$BINARY_PATH" --version || true

# ============================================================
# STEP 3: SIGN & NOTARIZE (once, macOS only)
# ============================================================

if [ "$CODESIGN" = true ] && [ "$OS" = "darwin" ]; then
    log_section "Step 3: Code Sign & Notarize"

    codesign_binary "$BINARY_PATH"

    if [ "$NOTARIZE" = true ]; then
        if ! notarize_binary "$BINARY_PATH"; then
            log_error "Notarization failed - continuing anyway"
        fi
    fi

    # Verify
    log_step "Verifying signature..."
    codesign -dvv "$BINARY_PATH" 2>&1 | grep -E "(Authority|Timestamp)" || true
    spctl -a -vv -t install "$BINARY_PATH" 2>&1 || true
else
    log_section "Step 3: Skip Signing (not macOS or not requested)"
fi

# ============================================================
# STEP 4: CREATE STANDALONE ARCHIVE
# ============================================================

log_section "Step 4: Create Standalone Archive"

ARCHIVE_NAME="$BINARY_NAME-$LSP_VERSION-$PLATFORM.tar.gz"
ARCHIVE_PATH="$DIST_DIR/$ARCHIVE_NAME"

cd "$DIST_DIR"
tar -czf "$ARCHIVE_NAME" "$(basename "$BINARY_PATH")"
shasum -a 256 "$ARCHIVE_NAME" > "${ARCHIVE_NAME}.sha256"

log_info "Archive: $ARCHIVE_PATH"
log_info "Checksum: ${ARCHIVE_PATH}.sha256"

# ============================================================
# STEP 5: BUILD VSIX
# ============================================================

if [ "$SKIP_VSIX" = false ]; then
    log_section "Step 5: Build VS Code Extension"

    # Copy signed binary to extension bin/
    VSIX_BINARY="$EXTENSION_DIR/bin/$BINARY_NAME-$PLATFORM"
    mkdir -p "$EXTENSION_DIR/bin"
    cp "$BINARY_PATH" "$VSIX_BINARY"
    chmod +x "$VSIX_BINARY"

    log_info "Binary copied to extension: $VSIX_BINARY"

    # Install dependencies
    cd "$EXTENSION_DIR"
    log_step "Installing npm dependencies..."
    npm ci --silent

    # Compile TypeScript
    log_step "Compiling TypeScript..."
    npm run compile

    # Package VSIX
    log_step "Packaging VSIX..."
    rm -f "$EXTENSION_DIR"/*.vsix
    npx vsce package --allow-missing-repository

    VSIX_FILE="${EXT_NAME}-${EXT_VERSION}.vsix"
    if [ ! -f "$EXTENSION_DIR/$VSIX_FILE" ]; then
        log_error "VSIX not created"
        exit 1
    fi

    # Copy to dist
    cp "$EXTENSION_DIR/$VSIX_FILE" "$DIST_DIR/"
    cd "$DIST_DIR"
    shasum -a 256 "$VSIX_FILE" > "${VSIX_FILE}.sha256"

    log_info "VSIX: $DIST_DIR/$VSIX_FILE"
    log_info "Checksum: $DIST_DIR/${VSIX_FILE}.sha256"

    # Show VSIX contents summary
    log_step "VSIX contents:"
    unzip -l "$DIST_DIR/$VSIX_FILE" | grep -E "(bin/|node_modules/vscode)" | head -5
else
    log_section "Step 5: Skip VSIX (--skip-vsix)"
fi

# ============================================================
# STEP 6: GITHUB RELEASE
# ============================================================

if [ "$CREATE_RELEASE" = true ]; then
    log_section "Step 6: GitHub Release"

    cd "$PROJECT_ROOT"

    # Check/create release
    if $GH_CLI release view "$TAG_NAME" >/dev/null 2>&1; then
        log_info "Release $TAG_NAME exists, uploading artifacts..."
    else
        log_step "Creating release $TAG_NAME..."

        RELEASE_NOTES="## Private Beta Release

**Platform:** $PLATFORM

### Downloads

- **VS Code Extension:** \`${EXT_NAME}-${EXT_VERSION}.vsix\`
- **Standalone LSP:** \`${ARCHIVE_NAME}\`

### Install VS Code Extension
\`\`\`bash
code --install-extension ${EXT_NAME}-${EXT_VERSION}.vsix
\`\`\`

### Install Standalone LSP (for Zed/Sublime)
\`\`\`bash
curl -sL https://github.com/\$(gh repo view --json nameWithOwner -q .nameWithOwner)/releases/download/${TAG_NAME}/${ARCHIVE_NAME} | tar xz
sudo mv $BINARY_NAME /usr/local/bin/
\`\`\`
"
        $GH_CLI release create "$TAG_NAME" \
            --title "Release $TAG_NAME" \
            --prerelease \
            --notes "$RELEASE_NOTES"
    fi

    # Upload artifacts
    log_step "Uploading artifacts..."

    $GH_CLI release upload "$TAG_NAME" "$DIST_DIR/$ARCHIVE_NAME" --clobber
    $GH_CLI release upload "$TAG_NAME" "$DIST_DIR/${ARCHIVE_NAME}.sha256" --clobber

    if [ "$SKIP_VSIX" = false ]; then
        $GH_CLI release upload "$TAG_NAME" "$DIST_DIR/$VSIX_FILE" --clobber
        $GH_CLI release upload "$TAG_NAME" "$DIST_DIR/${VSIX_FILE}.sha256" --clobber
    fi

    log_info "Release URL:"
    $GH_CLI release view "$TAG_NAME" --json url -q .url

else
    log_section "Step 6: Skip GitHub Release (--release not specified)"
fi

# ============================================================
# SUMMARY
# ============================================================

log_section "Build Complete!"

echo "Output files in $DIST_DIR:"
echo ""
ls -lh "$DIST_DIR"/*.tar.gz "$DIST_DIR"/*.vsix 2>/dev/null || true
echo ""

if [ "$SKIP_VSIX" = false ]; then
    echo "Install VS Code extension:"
    echo "  code --install-extension $DIST_DIR/$VSIX_FILE"
    echo ""
fi

echo "Install standalone LSP:"
echo "  tar xzf $DIST_DIR/$ARCHIVE_NAME && sudo mv $BINARY_NAME /usr/local/bin/"
echo ""

if [ "$CREATE_RELEASE" = true ]; then
    echo "View release:"
    echo "  gh release view $TAG_NAME --web"
fi
