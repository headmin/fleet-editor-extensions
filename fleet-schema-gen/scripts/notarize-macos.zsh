#!/usr/bin/env zsh
# Notarization script for macOS binaries
# Submits binaries to Apple for notarization

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "${BLUE}=== macOS Notarization ===${NC}"

# Check arguments
if [[ $# -lt 1 ]]; then
    echo "${RED}Error: Binary path required${NC}"
    echo "Usage: $0 <binary-path> [apple-id] [team-id]"
    echo ""
    echo "Example:"
    echo "  $0 dist/fleet-schema-gen"
    echo "  $0 dist/fleet-schema-gen you@example.com TEAMID123"
    echo ""
    echo "Prerequisites:"
    echo "  1. Apple Developer account"
    echo "  2. App-specific password stored in keychain:"
    echo "     xcrun notarytool store-credentials fleet-schema-gen \\"
    echo "       --apple-id you@example.com \\"
    echo "       --team-id TEAMID123 \\"
    echo "       --password xxxx-xxxx-xxxx-xxxx"
    exit 1
fi

BINARY_PATH="$1"
APPLE_ID="${2:-}"
TEAM_ID="${3:-}"
KEYCHAIN_PROFILE="fleet-schema-gen"

# Check if binary exists
if [[ ! -f "$BINARY_PATH" ]]; then
    echo "${RED}Error: Binary not found: $BINARY_PATH${NC}"
    exit 1
fi

# Check if binary is signed
if ! codesign -dv "$BINARY_PATH" 2>/dev/null; then
    echo "${RED}Error: Binary must be signed first${NC}"
    echo "Run: ./scripts/sign-macos.zsh $BINARY_PATH"
    exit 1
fi

# Check for notarytool
if ! command -v xcrun >/dev/null 2>&1; then
    echo "${RED}Error: Xcode command line tools not installed${NC}"
    echo "Install with: xcode-select --install"
    exit 1
fi

# Get binary name and version
BINARY_NAME=$(basename "$BINARY_PATH")
VERSION=$(./target/release/fleet-schema-gen --version 2>/dev/null | awk '{print $2}' || echo "unknown")
ZIP_NAME="${BINARY_NAME}-${VERSION}.zip"

echo "Binary: $BINARY_NAME"
echo "Version: $VERSION"
echo ""

# Create a zip file (required for notarization)
echo "${YELLOW}→${NC} Creating archive for notarization..."
rm -f "$ZIP_NAME"
ditto -c -k --keepParent "$BINARY_PATH" "$ZIP_NAME"
echo "${GREEN}✓${NC} Created: $ZIP_NAME"

# Check if credentials are stored
echo "${YELLOW}→${NC} Checking credentials..."
if ! xcrun notarytool history --keychain-profile "$KEYCHAIN_PROFILE" >/dev/null 2>&1; then
    echo "${YELLOW}⚠${NC}  Credentials not found in keychain"

    if [[ -z "$APPLE_ID" ]] || [[ -z "$TEAM_ID" ]]; then
        echo ""
        echo "${RED}Error: Apple ID and Team ID required${NC}"
        echo ""
        echo "Store credentials first:"
        echo "  xcrun notarytool store-credentials $KEYCHAIN_PROFILE \\"
        echo "    --apple-id your@email.com \\"
        echo "    --team-id TEAMID123 \\"
        echo "    --password xxxx-xxxx-xxxx-xxxx"
        echo ""
        echo "Or provide them as arguments:"
        echo "  $0 $BINARY_PATH your@email.com TEAMID123"
        exit 1
    fi

    echo ""
    echo "Store credentials interactively? (y/n)"
    read -q REPLY
    echo ""
    if [[ $REPLY == "y" ]]; then
        xcrun notarytool store-credentials "$KEYCHAIN_PROFILE" \
            --apple-id "$APPLE_ID" \
            --team-id "$TEAM_ID"
    else
        exit 1
    fi
fi

# Submit for notarization
echo "${YELLOW}→${NC} Submitting to Apple for notarization..."
echo "  This may take several minutes..."
echo ""

SUBMISSION_ID=$(xcrun notarytool submit "$ZIP_NAME" \
    --keychain-profile "$KEYCHAIN_PROFILE" \
    --wait 2>&1 | tee /dev/stderr | grep "id:" | head -n1 | awk '{print $2}')

if [[ -z "$SUBMISSION_ID" ]]; then
    echo ""
    echo "${RED}✗${NC} Notarization submission failed"
    echo ""
    echo "Check your credentials and try again"
    exit 1
fi

echo ""
echo "${GREEN}✓${NC} Submission ID: $SUBMISSION_ID"

# Wait for result and check status
echo "${YELLOW}→${NC} Checking notarization status..."
xcrun notarytool info "$SUBMISSION_ID" --keychain-profile "$KEYCHAIN_PROFILE"

# Get the log if available
echo ""
echo "${YELLOW}→${NC} Fetching notarization log..."
xcrun notarytool log "$SUBMISSION_ID" --keychain-profile "$KEYCHAIN_PROFILE" \
    "${BINARY_NAME}-notarization.log" 2>/dev/null || true

if [[ -f "${BINARY_NAME}-notarization.log" ]]; then
    echo "${GREEN}✓${NC} Log saved: ${BINARY_NAME}-notarization.log"

    # Check for issues in the log
    if grep -q "status.*Accepted" "${BINARY_NAME}-notarization.log" 2>/dev/null; then
        echo "${GREEN}✓${NC} Notarization accepted!"
    elif grep -q "status.*Invalid" "${BINARY_NAME}-notarization.log" 2>/dev/null; then
        echo "${RED}✗${NC} Notarization rejected"
        echo ""
        echo "Check the log for details:"
        echo "  cat ${BINARY_NAME}-notarization.log"
        exit 1
    fi
fi

# Staple the notarization ticket
echo ""
echo "${YELLOW}→${NC} Stapling notarization ticket..."
if xcrun stapler staple "$BINARY_PATH" 2>&1; then
    echo "${GREEN}✓${NC} Ticket stapled successfully"
else
    echo "${YELLOW}⚠${NC}  Could not staple ticket (this is ok for command-line tools)"
fi

# Verify
echo ""
echo "${YELLOW}→${NC} Final verification..."
if spctl --assess --type execute --verbose "$BINARY_PATH" 2>&1 | grep -q "accepted"; then
    echo "${GREEN}✓${NC} Gatekeeper will accept this binary"
else
    echo "${YELLOW}⚠${NC}  Gatekeeper status unknown"
fi

# Clean up
echo ""
echo "${YELLOW}→${NC} Cleaning up..."
rm -f "$ZIP_NAME"

echo ""
echo "${GREEN}=== Notarization Complete ===${NC}"
echo "Your binary is now notarized and ready for distribution!"
echo ""
echo "Distribution:"
echo "  1. Share the signed binary: $BINARY_PATH"
echo "  2. Users can download and run without Gatekeeper warnings"
echo "  3. Checksum: ${BINARY_PATH}.sha256"
