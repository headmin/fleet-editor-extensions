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
DIST_DIR="$PROJECT_ROOT/dist"

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

show_usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Build the Fleet GitOps VS Code extension and optionally create a GitHub pre-release.

OPTIONS:
    -r, --release          Create GitHub pre-release after building
    -t, --tag TAG          Release tag (default: v{version} from package.json)
    -f, --force            Force recreate release if exists
    --skip-install         Skip pnpm install (use if dependencies are already installed)
    -h, --help             Show this help message

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
SKIP_INSTALL=false

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
        --skip-install)
            SKIP_INSTALL=true
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
echo "============================================================"
echo ""

# ============================================================
# STEP 1: Install dependencies
# ============================================================

if [ "$SKIP_INSTALL" = false ]; then
    log_step "Installing dependencies..."
    cd "$EXTENSION_DIR"
    $PNPM install
    log_info "Dependencies installed"
else
    log_info "Skipping dependency installation (--skip-install)"
fi

# ============================================================
# STEP 2: Compile TypeScript
# ============================================================

log_step "Compiling TypeScript..."
cd "$EXTENSION_DIR"
$PNPM run compile
log_info "TypeScript compiled"

# ============================================================
# STEP 3: Package extension
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
# STEP 4: Copy to dist directory
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
# STEP 5: Create GitHub Release (if requested)
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
