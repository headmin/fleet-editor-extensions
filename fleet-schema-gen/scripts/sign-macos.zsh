#!/usr/bin/env zsh
# Code signing script for macOS binaries
# Signs binaries with Apple Developer ID certificate

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "${BLUE}=== macOS Code Signing ===${NC}"

# Check arguments
if [[ $# -lt 1 ]]; then
    echo "${RED}Error: Binary path required${NC}"
    echo "Usage: $0 <binary-path> [identity]"
    echo ""
    echo "Example:"
    echo "  $0 dist/fleet-schema-gen"
    echo "  $0 dist/fleet-schema-gen \"Developer ID Application: Your Name (TEAMID)\""
    exit 1
fi

BINARY_PATH="$1"
IDENTITY="${2:-}"

# Check if binary exists
if [[ ! -f "$BINARY_PATH" ]]; then
    echo "${RED}Error: Binary not found: $BINARY_PATH${NC}"
    exit 1
fi

# Auto-detect signing identity if not provided
if [[ -z "$IDENTITY" ]]; then
    echo "${YELLOW}→${NC} Auto-detecting signing identity..."

    # Try to find Developer ID Application certificate
    IDENTITY=$(security find-identity -v -p codesigning | \
        grep "Developer ID Application" | \
        head -n1 | \
        sed -E 's/.*"(.*)"/\1/')

    if [[ -z "$IDENTITY" ]]; then
        echo "${RED}Error: No Developer ID Application certificate found${NC}"
        echo ""
        echo "Available certificates:"
        security find-identity -v -p codesigning
        echo ""
        echo "To create a certificate, visit:"
        echo "  https://developer.apple.com/account/resources/certificates/list"
        exit 1
    fi

    echo "  ${GREEN}Found: $IDENTITY${NC}"
fi

# Check if already signed
if codesign -dv "$BINARY_PATH" 2>/dev/null; then
    echo "${YELLOW}→${NC} Binary is already signed, removing old signature..."
    codesign --remove-signature "$BINARY_PATH"
fi

# Sign the binary
echo "${YELLOW}→${NC} Signing binary..."
echo "  Identity: $IDENTITY"
echo "  Binary: $BINARY_PATH"

codesign \
    --sign "$IDENTITY" \
    --force \
    --options runtime \
    --timestamp \
    --verbose \
    "$BINARY_PATH"

# Verify signature
echo "${YELLOW}→${NC} Verifying signature..."
if codesign --verify --deep --strict --verbose=2 "$BINARY_PATH" 2>&1; then
    echo "${GREEN}✓${NC} Signature valid"
else
    echo "${RED}✗${NC} Signature verification failed"
    exit 1
fi

# Display signing information
echo ""
echo "${GREEN}=== Signature Information ===${NC}"
codesign -dv "$BINARY_PATH" 2>&1 | grep -E "Authority|Identifier|TeamIdentifier|Timestamp" | sed 's/^/  /'

# Check Gatekeeper
echo ""
echo "${YELLOW}→${NC} Checking Gatekeeper..."
if spctl --assess --type execute --verbose "$BINARY_PATH" 2>&1 | grep -q "accepted"; then
    echo "${GREEN}✓${NC} Gatekeeper will accept this binary"
else
    echo "${YELLOW}⚠${NC}  Gatekeeper may require notarization"
    echo "  Run: ./scripts/notarize-macos.zsh $BINARY_PATH"
fi

echo ""
echo "${GREEN}Signing complete!${NC}"
