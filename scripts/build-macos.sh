#!/bin/bash
set -e

# Cougar macOS Build Script
# Builds, code signs, and notarizes the Cougar binary for macOS
#
# Prerequisites:
# - Valid Apple Developer ID Application certificate in keychain
# - App-specific password stored in keychain for notarization
# - Xcode command line tools installed
#
# Environment Variables:
# - CODESIGN_IDENTITY: Developer ID Application certificate name (required)
# - NOTARIZATION_APPLE_ID: Apple ID for notarization (required)
# - NOTARIZATION_TEAM_ID: Team ID for notarization (required)
# - NOTARIZATION_PASSWORD: App-specific password (stored in keychain as @keychain:AC_PASSWORD)

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TARGET_DIR="$PROJECT_ROOT/target/release"
DIST_DIR="$PROJECT_ROOT/dist"
VERSION=$(grep '^version = ' "$PROJECT_ROOT/Cargo.toml" | sed 's/version = "\(.*\)"/\1/')

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check required environment variables
check_prerequisites() {
    log_info "Checking prerequisites..."

    if [ -z "$CODESIGN_IDENTITY" ]; then
        log_error "CODESIGN_IDENTITY environment variable not set"
        log_info "Example: export CODESIGN_IDENTITY='Developer ID Application: Your Name (TEAM_ID)'"
        exit 1
    fi

    if [ -z "$NOTARIZATION_APPLE_ID" ]; then
        log_error "NOTARIZATION_APPLE_ID environment variable not set"
        exit 1
    fi

    if [ -z "$NOTARIZATION_TEAM_ID" ]; then
        log_error "NOTARIZATION_TEAM_ID environment variable not set"
        exit 1
    fi

    # Check if password is in keychain
    if ! security find-generic-password -a "$NOTARIZATION_APPLE_ID" -s "AC_PASSWORD" &>/dev/null; then
        log_error "Notarization password not found in keychain"
        log_info "Store it with: security add-generic-password -a '$NOTARIZATION_APPLE_ID' -w 'your-app-specific-password' -s 'AC_PASSWORD'"
        exit 1
    fi

    # Check if certificate exists
    if ! security find-identity -v | grep -q "$CODESIGN_IDENTITY"; then
        log_error "Certificate '$CODESIGN_IDENTITY' not found in keychain"
        exit 1
    fi

    log_info "Prerequisites check passed"
}

# Build the binary
build_binary() {
    log_info "Building Cougar v$VERSION for macOS (Apple Silicon only)..."

    cd "$PROJECT_ROOT"

    # Build for aarch64 (Apple Silicon)
    log_info "Building for aarch64 (Apple Silicon)..."
    cargo build --release --target aarch64-apple-darwin

    # Copy to dist directory
    mkdir -p "$DIST_DIR"
    cp "$PROJECT_ROOT/target/aarch64-apple-darwin/release/cougar" "$DIST_DIR/cougar"

    log_info "Apple Silicon binary created successfully"
    file "$DIST_DIR/cougar"
}

# Code sign the binary
codesign_binary() {
    log_info "Code signing binary..."

    codesign --force \
        --options runtime \
        --timestamp \
        --sign "$CODESIGN_IDENTITY" \
        "$DIST_DIR/cougar"

    # Verify signature
    codesign -vvv --deep --strict "$DIST_DIR/cougar"

    log_info "Code signing completed successfully"
}

# Create ZIP archive for notarization
create_archive() {
    log_info "Creating distribution archive..."

    cd "$DIST_DIR"
    ZIP_NAME="cougar-${VERSION}-macos-arm64.zip"

    # Create ZIP with ditto (preserves code signature)
    ditto -c -k --keepParent cougar "$ZIP_NAME"

    log_info "Archive created: $ZIP_NAME"
    echo "$ZIP_NAME"
}

# Submit for notarization
notarize_binary() {
    local ZIP_FILE="$1"

    log_info "Submitting for notarization..."

    # Submit for notarization
    SUBMISSION_OUTPUT=$(xcrun notarytool submit "$DIST_DIR/$ZIP_FILE" \
        --apple-id "$NOTARIZATION_APPLE_ID" \
        --team-id "$NOTARIZATION_TEAM_ID" \
        --password "@keychain:AC_PASSWORD" \
        --wait)

    echo "$SUBMISSION_OUTPUT"

    # Extract submission ID
    SUBMISSION_ID=$(echo "$SUBMISSION_OUTPUT" | grep "id:" | head -1 | awk '{print $2}')

    if echo "$SUBMISSION_OUTPUT" | grep -q "status: Accepted"; then
        log_info "Notarization successful!"

        # Get notarization log for record
        log_info "Fetching notarization log..."
        xcrun notarytool log "$SUBMISSION_ID" \
            --apple-id "$NOTARIZATION_APPLE_ID" \
            --team-id "$NOTARIZATION_TEAM_ID" \
            --password "@keychain:AC_PASSWORD" \
            "$DIST_DIR/notarization-log-${VERSION}.json"

        return 0
    else
        log_error "Notarization failed"

        # Fetch detailed log
        xcrun notarytool log "$SUBMISSION_ID" \
            --apple-id "$NOTARIZATION_APPLE_ID" \
            --team-id "$NOTARIZATION_TEAM_ID" \
            --password "@keychain:AC_PASSWORD"

        return 1
    fi
}

# Staple the notarization ticket
staple_ticket() {
    log_info "Stapling notarization ticket..."

    # Note: Can't staple to a bare executable, only to .app bundles or .pkg
    # For CLI tools, the notarization is valid but can't be stapled
    log_warn "Notarization ticket cannot be stapled to bare executables"
    log_info "Users will need internet connection on first run to verify notarization"
}

# Verify the final binary
verify_binary() {
    log_info "Verifying final binary..."

    # Check code signature
    codesign -dvv "$DIST_DIR/cougar" 2>&1 | grep -E "(Identifier|Authority|Timestamp)"

    # Check notarization
    spctl -a -vv -t install "$DIST_DIR/cougar"

    log_info "Verification complete"
}

# Create checksums
create_checksums() {
    log_info "Creating checksums..."

    cd "$DIST_DIR"
    shasum -a 256 cougar-*.zip > checksums.txt
    shasum -a 256 cougar >> checksums.txt

    log_info "Checksums saved to checksums.txt"
    cat checksums.txt
}

# Main build process
main() {
    log_info "Starting Cougar macOS build process..."
    log_info "Version: $VERSION"

    check_prerequisites
    build_binary
    codesign_binary

    ZIP_FILE=$(create_archive)

    # Ask if user wants to notarize
    read -p "Do you want to notarize the binary? (requires Apple Developer account) [y/N] " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        if notarize_binary "$ZIP_FILE"; then
            staple_ticket
            verify_binary
        else
            log_error "Notarization failed. Build completed but not notarized."
            exit 1
        fi
    else
        log_warn "Skipping notarization. Binary is signed but not notarized."
    fi

    create_checksums

    log_info "Build complete!"
    log_info "Distribution files in: $DIST_DIR"
    log_info ""
    log_info "Files created:"
    ls -lh "$DIST_DIR" | grep -E "(cougar|zip|txt)"
}

main "$@"
