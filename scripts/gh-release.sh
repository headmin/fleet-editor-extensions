#!/bin/bash
# ============================================================
# GitHub Release Script for Fleet GitOps VS Code Extension
# ============================================================
# Creates GitHub releases and uploads VS Code extension artifacts
# ============================================================

set -e

# ============================================================
# CONFIGURATION
# ============================================================

# GitHub CLI path - dynamic lookup
GH_CLI="${GH_CLI:-}"

if [ -z "$GH_CLI" ]; then
    if command -v gh &> /dev/null; then
        GH_CLI="$(command -v gh)"
    elif [ -d "$HOME/.local/share/mise/installs/gh" ]; then
        GH_CLI="$(find "$HOME/.local/share/mise/installs/gh" -name "gh" -type f -perm +111 2>/dev/null | head -1)"
    elif [ -f "$HOME/.local/bin/gh" ]; then
        GH_CLI="$HOME/.local/bin/gh"
    elif [ -f "/opt/homebrew/bin/gh" ]; then
        GH_CLI="/opt/homebrew/bin/gh"
    elif [ -f "/usr/local/bin/gh" ]; then
        GH_CLI="/usr/local/bin/gh"
    else
        GH_CLI="gh"
    fi
fi

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

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

Create a GitHub release and upload VS Code extension artifacts.

OPTIONS:
    -t, --tag TAG          Release tag (default: v{version} from package.json)
    -d, --draft            Create as draft release
    -p, --prerelease       Mark as prerelease (default)
    --release              Mark as full release (not prerelease)
    -n, --notes FILE       Release notes from file
    -a, --auto-notes       Generate release notes automatically (default)
    -f, --force            Force recreate release if exists
    --dist-dir DIR         Distribution directory (default: dist)
    --no-upload            Create release but don't upload artifacts
    -h, --help             Show this help message

EXAMPLES:
    # Create prerelease with auto-detected version
    $(basename "$0")

    # Create prerelease with specific tag
    $(basename "$0") --tag v1.2.3

    # Create draft release
    $(basename "$0") --draft

    # Create full release (not prerelease)
    $(basename "$0") --release

ARTIFACTS:
    The script uploads all .vsix files and checksums from the dist directory.

EOF
}

# ============================================================
# PARSE ARGUMENTS
# ============================================================

TAG_NAME=""
DRAFT=false
PRERELEASE=true
NOTES_FILE=""
AUTO_NOTES=true
FORCE=false
DIST_DIR="dist"
NO_UPLOAD=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--tag)
            TAG_NAME="$2"
            shift 2
            ;;
        -d|--draft)
            DRAFT=true
            shift
            ;;
        -p|--prerelease)
            PRERELEASE=true
            shift
            ;;
        --release)
            PRERELEASE=false
            shift
            ;;
        -n|--notes)
            NOTES_FILE="$2"
            AUTO_NOTES=false
            shift 2
            ;;
        -a|--auto-notes)
            AUTO_NOTES=true
            shift
            ;;
        -f|--force)
            FORCE=true
            shift
            ;;
        --dist-dir)
            DIST_DIR="$2"
            shift 2
            ;;
        --no-upload)
            NO_UPLOAD=true
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
# DETECT PROJECT INFO
# ============================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
EXTENSION_DIR="$PROJECT_ROOT/vscode-extension"

cd "$PROJECT_ROOT"

log_info "Detecting project information..."

# Find package.json
PACKAGE_JSON="$EXTENSION_DIR/package.json"
if [ ! -f "$PACKAGE_JSON" ]; then
    log_error "package.json not found at $PACKAGE_JSON"
    exit 1
fi

# Extract project name and version
EXTENSION_NAME=$(grep '"name"' "$PACKAGE_JSON" | head -1 | sed 's/.*"name": "\([^"]*\)".*/\1/')
VERSION=$(grep '"version"' "$PACKAGE_JSON" | head -1 | sed 's/.*"version": "\([^"]*\)".*/\1/')

if [ -z "$EXTENSION_NAME" ] || [ -z "$VERSION" ]; then
    log_error "Could not extract name or version from package.json"
    exit 1
fi

# Set tag name if not provided
if [ -z "$TAG_NAME" ]; then
    TAG_NAME="v${VERSION}"
fi

log_info "Extension: $EXTENSION_NAME"
log_info "Version: $VERSION"
log_info "Tag: $TAG_NAME"

# Check if we're in a git repo
if ! git rev-parse --git-dir > /dev/null 2>&1; then
    log_error "Not in a git repository"
    exit 1
fi

# ============================================================
# CHECK DISTRIBUTION DIRECTORY
# ============================================================

DIST_PATH="$PROJECT_ROOT/$DIST_DIR"

if [ ! -d "$DIST_PATH" ]; then
    log_error "Distribution directory not found: $DIST_PATH"
    log_info ""
    log_info "Please build the extension first:"
    log_info "  ./scripts/build-vscode-extension.sh"
    exit 1
fi

# Find artifacts (.vsix files and checksums)
ARTIFACTS=()
while IFS= read -r -d '' file; do
    ARTIFACTS+=("$file")
done < <(find "$DIST_PATH" -maxdepth 1 -type f \( -name "*.vsix" -o -name "*.sha256" -o -name "checksums.txt" \) -print0)

if [ ${#ARTIFACTS[@]} -eq 0 ]; then
    log_warn "No artifacts found in $DIST_PATH"
    if [ "$NO_UPLOAD" = false ]; then
        log_error "Cannot proceed without artifacts. Use --no-upload to create release without artifacts."
        log_info ""
        log_info "Build the extension first:"
        log_info "  ./scripts/build-vscode-extension.sh"
        exit 1
    fi
else
    log_info "Found ${#ARTIFACTS[@]} artifact(s):"
    for artifact in "${ARTIFACTS[@]}"; do
        echo "  - $(basename "$artifact")"
    done
fi

# ============================================================
# CHECK/CREATE GIT TAG
# ============================================================

echo ""
log_step "Checking git tag: $TAG_NAME"

TAG_EXISTS=false
if git rev-parse "$TAG_NAME" >/dev/null 2>&1; then
    TAG_EXISTS=true
    log_info "Tag $TAG_NAME already exists"
else
    log_info "Creating tag $TAG_NAME..."
    git tag "$TAG_NAME"
    log_info "✓ Tag created locally"

    read -p "Push tag to remote? [Y/n] " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Nn]$ ]]; then
        git push origin "$TAG_NAME"
        log_info "✓ Tag pushed to remote"
    else
        log_warn "Tag not pushed. You can push it later with: git push origin $TAG_NAME"
    fi
fi

# ============================================================
# CHECK/CREATE GITHUB RELEASE
# ============================================================

echo ""
log_step "Checking GitHub release..."

RELEASE_EXISTS=false
if $GH_CLI release view "$TAG_NAME" >/dev/null 2>&1; then
    RELEASE_EXISTS=true
    log_info "Release $TAG_NAME exists"

    if [ "$FORCE" = true ]; then
        log_warn "Force mode: deleting existing release..."
        $GH_CLI release delete "$TAG_NAME" --yes
        RELEASE_EXISTS=false
        log_info "✓ Existing release deleted"
    fi
fi

if [ "$RELEASE_EXISTS" = false ]; then
    log_info "Creating release $TAG_NAME..."

    # Build gh release create command
    CREATE_CMD=("$GH_CLI" "release" "create" "$TAG_NAME")

    # Add title
    CREATE_CMD+=("--title" "$EXTENSION_NAME $TAG_NAME")

    # Add draft flag
    if [ "$DRAFT" = true ]; then
        CREATE_CMD+=("--draft")
        log_info "  Mode: Draft"
    fi

    # Add prerelease flag
    if [ "$PRERELEASE" = true ]; then
        CREATE_CMD+=("--prerelease")
        log_info "  Mode: Prerelease"
    fi

    # Add notes
    if [ -n "$NOTES_FILE" ]; then
        if [ ! -f "$NOTES_FILE" ]; then
            log_error "Notes file not found: $NOTES_FILE"
            exit 1
        fi
        CREATE_CMD+=("--notes-file" "$NOTES_FILE")
        log_info "  Notes: From file $NOTES_FILE"
    elif [ "$AUTO_NOTES" = true ]; then
        CREATE_CMD+=("--generate-notes")
        log_info "  Notes: Auto-generated"
    else
        CREATE_CMD+=("--notes" "Release $TAG_NAME for $EXTENSION_NAME VS Code extension")
        log_info "  Notes: Default"
    fi

    # Execute create command
    "${CREATE_CMD[@]}"
    log_info "✓ Release created"
fi

# ============================================================
# UPLOAD ARTIFACTS
# ============================================================

if [ "$NO_UPLOAD" = true ]; then
    log_warn "Skipping artifact upload (--no-upload specified)"
elif [ ${#ARTIFACTS[@]} -gt 0 ]; then
    echo ""
    log_step "Uploading artifacts..."

    for artifact in "${ARTIFACTS[@]}"; do
        artifact_name=$(basename "$artifact")
        log_info "Uploading $artifact_name..."
        $GH_CLI release upload "$TAG_NAME" "$artifact" --clobber
    done

    log_info "✓ All artifacts uploaded"
fi

# ============================================================
# DISPLAY RELEASE INFO
# ============================================================

echo ""
echo "============================================================"
log_info "✓ Release $TAG_NAME ready!"
echo "============================================================"
echo ""
echo "Release URL:"
$GH_CLI release view "$TAG_NAME" --json url -q .url
echo ""

if [ ${#ARTIFACTS[@]} -gt 0 ]; then
    echo "Uploaded assets:"
    $GH_CLI release view "$TAG_NAME" --json assets -q '.assets[].name'
    echo ""
fi

echo "To view the release:"
echo "  $GH_CLI release view $TAG_NAME --web"
echo ""

if [ "$DRAFT" = true ]; then
    log_warn "Release is in DRAFT mode - remember to publish it!"
    echo "  $GH_CLI release edit $TAG_NAME --draft=false"
    echo ""
fi

if [ "$PRERELEASE" = true ]; then
    log_info "Release is marked as PRERELEASE"
    echo "To promote to full release:"
    echo "  $GH_CLI release edit $TAG_NAME --prerelease=false"
    echo ""
fi

echo "To install from release:"
echo "  Download the .vsix file and run:"
echo "  code --install-extension ${EXTENSION_NAME}-${VERSION}.vsix"
echo ""
