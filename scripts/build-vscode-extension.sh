#!/bin/bash
# ============================================================
# Fleet GitOps VS Code Extension Build & Release Script
# ============================================================
# Builds the VS Code extension and optionally uploads to
# GitHub as a pre-release
# ============================================================

set -e

# ============================================================
# CONFIGURATION
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
EXTENSION_DIR="$PROJECT_ROOT/vscode-extension"
RUST_DIR="$PROJECT_ROOT/fleet-schema-gen"
DIST_DIR="$PROJECT_ROOT/dist"

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
    x86_64)
        BINARY_SUFFIX="darwin-x64"
        RUST_TARGET="x86_64-apple-darwin"
        ;;
    arm64)
        BINARY_SUFFIX="darwin-arm64"
        RUST_TARGET="aarch64-apple-darwin"
        ;;
    *)
        BINARY_SUFFIX="$ARCH"
        RUST_TARGET=""
        ;;
esac

# 1Password vault for code signing credentials
VAULT_NAME="${VAULT_NAME:-dev-credentials}"
OP_ACCOUNT="${OP_ACCOUNT:-my.1password.eu}"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# GitHub CLI path - dynamic lookup
GH_CLI="${GH_CLI:-}"
if [ -z "$GH_CLI" ]; then
    if command -v gh &> /dev/null; then
        GH_CLI="$(command -v gh)"
    elif [ -d "$HOME/.local/share/mise/installs/gh" ]; then
        GH_CLI="$(find "$HOME/.local/share/mise/installs/gh" -name "gh" -type f -perm +111 2>/dev/null | head -1)"
    elif [ -f "/opt/homebrew/bin/gh" ]; then
        GH_CLI="/opt/homebrew/bin/gh"
    elif [ -f "/usr/local/bin/gh" ]; then
        GH_CLI="/usr/local/bin/gh"
    else
        GH_CLI="gh"
    fi
fi

# pnpm path via mise
PNPM="${PNPM:-}"
if [ -z "$PNPM" ]; then
    if command -v pnpm &> /dev/null; then
        PNPM="pnpm"
    elif [ -f "$HOME/.local/bin/mise" ]; then
        PNPM="$HOME/.local/bin/mise exec pnpm -- pnpm"
    else
        PNPM="pnpm"
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

# Load credentials from 1Password
load_credentials() {
    log_info "Loading credentials from 1Password..."
    log_info "Account: $OP_ACCOUNT"
    log_info "Vault: $VAULT_NAME"

    # Test 1Password CLI connection
    if ! op item list --vault "$VAULT_NAME" --account "$OP_ACCOUNT" >/dev/null 2>&1; then
        log_error "1Password CLI not connected to desktop app"
        log_info ""
        log_info "Please enable 1Password desktop app integration:"
        log_info "  1. Open 1Password app"
        log_info "  2. Go to Settings → Developer"
        log_info "  3. Enable 'Connect with 1Password CLI'"
        log_info ""
        exit 1
    fi
    log_info "✓ 1Password CLI connected to vault: $VAULT_NAME"

    # Load credentials
    export CODESIGN_IDENTITY=$(op read "op://$VAULT_NAME/CODESIGN_IDENTITY/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_APPLE_ID=$(op read "op://$VAULT_NAME/NOTARIZATION_APPLE_ID/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_TEAM_ID=$(op read "op://$VAULT_NAME/NOTARIZATION_TEAM_ID/credential" --account "$OP_ACCOUNT")
    export NOTARIZATION_PASSWORD=$(op read "op://$VAULT_NAME/NOTARIZATION_PASSWORD/credential" --account "$OP_ACCOUNT")

    log_info "✓ Credentials loaded successfully"
    log_info "  Code Signing: ${CODESIGN_IDENTITY:0:50}..."
    log_info "  Apple ID: $NOTARIZATION_APPLE_ID"
    log_info "  Team ID: $NOTARIZATION_TEAM_ID"

    # Check if certificate exists
    if ! security find-identity -v | grep -q "$CODESIGN_IDENTITY"; then
        log_error "Certificate '$CODESIGN_IDENTITY' not found in keychain"
        exit 1
    fi
    log_info "✓ Certificate found in keychain"
}

# Code sign the binary
codesign_binary() {
    local BINARY_PATH="$1"

    log_step "Code signing binary..."

    codesign --force \
        --options runtime \
        --timestamp \
        --sign "$CODESIGN_IDENTITY" \
        "$BINARY_PATH"

    # Verify signature
    codesign -vvv --deep --strict "$BINARY_PATH"

    log_info "✓ Code signing completed"
}

# Notarize the binary
notarize_binary() {
    local BINARY_PATH="$1"
    local BINARY_NAME=$(basename "$BINARY_PATH")

    log_step "Creating ZIP for notarization..."

    # Create ZIP with ditto (preserves code signature)
    local ZIP_PATH="$DIST_DIR/${BINARY_NAME}.zip"
    ditto -c -k --keepParent "$BINARY_PATH" "$ZIP_PATH"

    log_step "Submitting for notarization..."

    SUBMISSION_OUTPUT=$(xcrun notarytool submit "$ZIP_PATH" \
        --apple-id "$NOTARIZATION_APPLE_ID" \
        --team-id "$NOTARIZATION_TEAM_ID" \
        --password "$NOTARIZATION_PASSWORD" \
        --wait)

    echo "$SUBMISSION_OUTPUT"

    # Extract submission ID
    SUBMISSION_ID=$(echo "$SUBMISSION_OUTPUT" | grep "id:" | head -1 | awk '{print $2}')

    if echo "$SUBMISSION_OUTPUT" | grep -q "status: Accepted"; then
        log_info "✓ Notarization successful!"

        # Get notarization log for record
        log_info "Fetching notarization log..."
        xcrun notarytool log "$SUBMISSION_ID" \
            --apple-id "$NOTARIZATION_APPLE_ID" \
            --team-id "$NOTARIZATION_TEAM_ID" \
            --password "$NOTARIZATION_PASSWORD" \
            "$DIST_DIR/notarization-log.json" 2>/dev/null || true

        # Clean up ZIP file
        rm -f "$ZIP_PATH"

        return 0
    else
        log_error "Notarization failed"

        # Fetch detailed log
        xcrun notarytool log "$SUBMISSION_ID" \
            --apple-id "$NOTARIZATION_APPLE_ID" \
            --team-id "$NOTARIZATION_TEAM_ID" \
            --password "$NOTARIZATION_PASSWORD" 2>/dev/null || true

        return 1
    fi
}

# Verify the binary
verify_binary() {
    local BINARY_PATH="$1"

    log_step "Verifying binary..."

    # Check code signature
    log_info "Code signature info:"
    codesign -dvv "$BINARY_PATH" 2>&1 | grep -E "(Identifier|Authority|Timestamp)" || true

    # Check notarization (if notarized)
    if [ "$NOTARIZE" = true ]; then
        log_info "Notarization check:"
        spctl -a -vv -t install "$BINARY_PATH" 2>&1 || true
    fi

    log_info "✓ Verification complete"
}

# Validate the binary before packaging
validate_binary() {
    local BINARY_PATH="$1"

    log_step "Validating binary..."

    # Check binary exists and is executable
    if [ ! -x "$BINARY_PATH" ]; then
        log_error "Binary not found or not executable: $BINARY_PATH"
        return 1
    fi

    # Get binary version
    log_info "Binary version:"
    "$BINARY_PATH" --version 2>&1 || log_warn "Could not get version (binary may not support --version)"

    # Show binary info
    log_info "Binary info:"
    file "$BINARY_PATH"
    ls -lh "$BINARY_PATH"

    # Check build timestamp
    local BUILD_TIME=$(stat -f "%Sm" -t "%Y-%m-%d %H:%M:%S" "$BINARY_PATH")
    log_info "Binary build time: $BUILD_TIME"

    # Warn if binary is older than 1 hour
    local BINARY_AGE=$(( $(date +%s) - $(stat -f "%m" "$BINARY_PATH") ))
    if [ "$BINARY_AGE" -gt 3600 ]; then
        log_warn "Binary is older than 1 hour ($BINARY_AGE seconds old)"
        log_warn "Consider rebuilding with: cargo build --release"
        read -p "Continue anyway? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            log_error "Aborted by user"
            return 1
        fi
    fi

    log_info "✓ Binary validation passed"
    return 0
}

show_usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Build the Fleet GitOps VS Code extension and optionally create a GitHub pre-release.

OPTIONS:
    -r, --release          Create GitHub pre-release after building
    -t, --tag TAG          Release tag (default: v{version} from package.json)
    -f, --force            Force recreate release if exists
    --skip-rust            Skip Rust binary build entirely (use existing binary in bin/)
    --quick                Skip cargo clean (incremental build, faster but may use stale timestamp)
    --skip-install         Skip pnpm install (use if dependencies are already installed)
    --sign                 Code sign the binary (requires Apple Developer certificate)
    --notarize             Code sign and notarize the binary (requires Apple Developer account)
    -h, --help             Show this help message

NOTE: By default, Rust builds are clean (cargo clean) to ensure fresh BUILD_TIMESTAMP.
      Use --quick for faster incremental builds during development.

ENVIRONMENT:
    VAULT_NAME             1Password vault name for credentials (default: dev-credentials)

EXAMPLES:
    # Build only
    $(basename "$0")

    # Build and create pre-release
    $(basename "$0") --release

    # Build and create pre-release with specific tag
    $(basename "$0") --release --tag v0.2.0-beta.1

EOF
}

# ============================================================
# PARSE ARGUMENTS
# ============================================================

CREATE_RELEASE=false
TAG_NAME=""
FORCE=false
SKIP_RUST=false
QUICK_BUILD=false
SKIP_INSTALL=false
CODESIGN=false
NOTARIZE=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -r|--release)
            CREATE_RELEASE=true
            shift
            ;;
        -t|--tag)
            TAG_NAME="$2"
            shift 2
            ;;
        -f|--force)
            FORCE=true
            shift
            ;;
        --skip-rust)
            SKIP_RUST=true
            shift
            ;;
        --quick)
            QUICK_BUILD=true
            shift
            ;;
        --skip-install)
            SKIP_INSTALL=true
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
# DETECT VERSION
# ============================================================

log_info "Detecting extension version..."

PACKAGE_JSON="$EXTENSION_DIR/package.json"
if [ ! -f "$PACKAGE_JSON" ]; then
    log_error "package.json not found at $PACKAGE_JSON"
    exit 1
fi

VERSION=$(grep '"version"' "$PACKAGE_JSON" | head -1 | sed 's/.*"version": "\([^"]*\)".*/\1/')
EXTENSION_NAME=$(grep '"name"' "$PACKAGE_JSON" | head -1 | sed 's/.*"name": "\([^"]*\)".*/\1/')

if [ -z "$VERSION" ] || [ -z "$EXTENSION_NAME" ]; then
    log_error "Could not extract name or version from package.json"
    exit 1
fi

if [ -z "$TAG_NAME" ]; then
    TAG_NAME="v${VERSION}"
fi

VSIX_FILE="${EXTENSION_NAME}-${VERSION}.vsix"

echo "============================================================"
echo "Fleet GitOps VS Code Extension Builder"
echo "============================================================"
echo "Extension: $EXTENSION_NAME"
echo "Version: $VERSION"
echo "Tag: $TAG_NAME"
echo "Architecture: $ARCH ($BINARY_SUFFIX)"
if [ "$CODESIGN" = true ]; then
    echo "Code Signing: Enabled"
fi
if [ "$NOTARIZE" = true ]; then
    echo "Notarization: Enabled"
fi
echo "============================================================"
echo ""

# ============================================================
# STEP 0: Load credentials (if code signing is enabled)
# ============================================================

if [ "$CODESIGN" = true ]; then
    load_credentials
    echo ""
fi

# ============================================================
# STEP 1: Build Rust binary (fleet-schema-gen)
# ============================================================

if [ "$SKIP_RUST" = false ]; then
    log_step "Building fleet-schema-gen (Rust)..."

    if [ ! -d "$RUST_DIR" ]; then
        log_error "Rust project not found at $RUST_DIR"
        exit 1
    fi

    cd "$RUST_DIR"

    # Clean build by default to ensure fresh BUILD_TIMESTAMP
    if [ "$QUICK_BUILD" = false ]; then
        log_info "Clean build (cargo clean)..."
        cargo clean
    else
        log_info "Quick build (incremental, skipping cargo clean)..."
    fi

    cargo build --release

    # Copy binary to extension bin directory
    BINARY_NAME="fleet-schema-gen-$BINARY_SUFFIX"
    mkdir -p "$EXTENSION_DIR/bin"
    cp "$RUST_DIR/target/release/fleet-schema-gen" "$EXTENSION_DIR/bin/$BINARY_NAME"
    chmod +x "$EXTENSION_DIR/bin/$BINARY_NAME"

    log_info "Rust binary built and copied: $BINARY_NAME"
    ls -lh "$EXTENSION_DIR/bin/$BINARY_NAME"

    # Code sign and optionally notarize
    if [ "$CODESIGN" = true ]; then
        BINARY_PATH="$EXTENSION_DIR/bin/$BINARY_NAME"
        codesign_binary "$BINARY_PATH"

        if [ "$NOTARIZE" = true ]; then
            if notarize_binary "$BINARY_PATH"; then
                verify_binary "$BINARY_PATH"
            else
                log_error "Notarization failed. Build will continue but binary is not notarized."
            fi
        else
            verify_binary "$BINARY_PATH"
        fi
    fi
else
    log_info "Skipping Rust build (--skip-rust)"
    BINARY_NAME="fleet-schema-gen-$BINARY_SUFFIX"

    # Always copy fresh binary from target/release if it exists
    if [ -f "$RUST_DIR/target/release/fleet-schema-gen" ]; then
        log_info "Copying fresh binary from target/release..."
        mkdir -p "$EXTENSION_DIR/bin"
        cp "$RUST_DIR/target/release/fleet-schema-gen" "$EXTENSION_DIR/bin/$BINARY_NAME"
        chmod +x "$EXTENSION_DIR/bin/$BINARY_NAME"
        ls -lh "$EXTENSION_DIR/bin/$BINARY_NAME"
    fi

    if [ ! -f "$EXTENSION_DIR/bin/$BINARY_NAME" ]; then
        log_warn "Binary not found at $EXTENSION_DIR/bin/$BINARY_NAME"
    elif [ "$CODESIGN" = true ]; then
        # Sign existing binary if requested
        BINARY_PATH="$EXTENSION_DIR/bin/$BINARY_NAME"
        codesign_binary "$BINARY_PATH"

        if [ "$NOTARIZE" = true ]; then
            if notarize_binary "$BINARY_PATH"; then
                verify_binary "$BINARY_PATH"
            else
                log_error "Notarization failed. Build will continue but binary is not notarized."
            fi
        else
            verify_binary "$BINARY_PATH"
        fi
    fi
fi

echo ""

# ============================================================
# STEP 2: Install pnpm dependencies
# ============================================================

if [ "$SKIP_INSTALL" = false ]; then
    log_step "Installing pnpm dependencies..."
    cd "$EXTENSION_DIR"
    $PNPM install
    log_info "Dependencies installed"
else
    log_info "Skipping dependency installation (--skip-install)"
fi

# ============================================================
# STEP 3: Compile TypeScript
# ============================================================

log_step "Compiling TypeScript..."
cd "$EXTENSION_DIR"
$PNPM run compile
log_info "TypeScript compiled"

# ============================================================
# STEP 4: Validate binary before packaging
# ============================================================

BINARY_PATH="$EXTENSION_DIR/bin/fleet-schema-gen-$BINARY_SUFFIX"
if ! validate_binary "$BINARY_PATH"; then
    log_error "Binary validation failed. Aborting."
    exit 1
fi

echo ""

# ============================================================
# STEP 5: Package extension
# ============================================================

log_step "Packaging extension..."
cd "$EXTENSION_DIR"

# Remove old vsix if exists
rm -f "$EXTENSION_DIR"/*.vsix

# Package with vsce
$PNPM exec vsce package --no-dependencies --allow-missing-repository

if [ ! -f "$EXTENSION_DIR/$VSIX_FILE" ]; then
    log_error "Failed to create $VSIX_FILE"
    exit 1
fi

log_info "Extension packaged: $VSIX_FILE"

# ============================================================
# STEP 6: Copy to dist directory
# ============================================================

log_step "Copying to dist directory..."
mkdir -p "$DIST_DIR"
cp "$EXTENSION_DIR/$VSIX_FILE" "$DIST_DIR/"

# Create checksums
cd "$DIST_DIR"
shasum -a 256 "$VSIX_FILE" > "${VSIX_FILE}.sha256"
log_info "Checksum created"

echo ""
echo "============================================================"
log_info "Build complete!"
echo "============================================================"
echo ""
echo "Output files:"
echo "  $DIST_DIR/$VSIX_FILE"
echo "  $DIST_DIR/${VSIX_FILE}.sha256"
echo ""
ls -lh "$DIST_DIR/$VSIX_FILE"
echo ""

# ============================================================
# STEP 7: Create GitHub Release (if requested)
# ============================================================

if [ "$CREATE_RELEASE" = true ]; then
    echo "============================================================"
    log_step "Creating GitHub pre-release..."
    echo "============================================================"
    echo ""

    cd "$PROJECT_ROOT"

    # Check if we're in a git repo
    if ! git rev-parse --git-dir > /dev/null 2>&1; then
        log_error "Not in a git repository"
        exit 1
    fi

    # Check/Create git tag
    log_info "Checking git tag: $TAG_NAME"
    if git rev-parse "$TAG_NAME" >/dev/null 2>&1; then
        log_info "Tag $TAG_NAME already exists"
    else
        log_info "Creating tag $TAG_NAME..."
        git tag "$TAG_NAME"
        log_info "Tag created locally"

        read -p "Push tag to remote? [Y/n] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Nn]$ ]]; then
            git push origin "$TAG_NAME"
            log_info "Tag pushed to remote"
        else
            log_warn "Tag not pushed. You can push it later with: git push origin $TAG_NAME"
        fi
    fi

    # Check if release exists
    RELEASE_EXISTS=false
    if $GH_CLI release view "$TAG_NAME" >/dev/null 2>&1; then
        RELEASE_EXISTS=true
        log_info "Release $TAG_NAME exists"

        if [ "$FORCE" = true ]; then
            log_warn "Force mode: deleting existing release..."
            $GH_CLI release delete "$TAG_NAME" --yes
            RELEASE_EXISTS=false
            log_info "Existing release deleted"
        fi
    fi

    if [ "$RELEASE_EXISTS" = false ]; then
        log_info "Creating pre-release $TAG_NAME..."
        $GH_CLI release create "$TAG_NAME" \
            --title "Release $TAG_NAME" \
            --prerelease \
            --generate-notes
        log_info "Pre-release created"
    fi

    # Upload artifacts
    log_info "Uploading artifacts..."
    $GH_CLI release upload "$TAG_NAME" "$DIST_DIR/$VSIX_FILE" --clobber
    $GH_CLI release upload "$TAG_NAME" "$DIST_DIR/${VSIX_FILE}.sha256" --clobber

    echo ""
    echo "============================================================"
    log_info "Pre-release published successfully!"
    echo "============================================================"
    echo ""
    echo "Release URL:"
    $GH_CLI release view "$TAG_NAME" --json url -q .url
    echo ""
    echo "Release assets:"
    $GH_CLI release view "$TAG_NAME" --json assets -q '.assets[].name'
    echo ""
    echo "To view the release:"
    echo "  gh release view $TAG_NAME --web"
    echo ""
    echo "To install the extension locally:"
    echo "  code --install-extension $DIST_DIR/$VSIX_FILE"
    echo ""
else
    echo "To install the extension locally:"
    echo "  code --install-extension $DIST_DIR/$VSIX_FILE"
    echo ""
    echo "To create a GitHub pre-release, run:"
    echo "  $(basename "$0") --release"
    echo ""
fi
