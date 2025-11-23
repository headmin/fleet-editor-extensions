#!/usr/bin/env zsh
# Build script for macOS signed binary
# Creates optimized, signed binaries for Apple Silicon

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
BINARY_NAME="fleet-schema-gen"
VERSION=$(grep '^version' Cargo.toml | head -n1 | cut -d'"' -f2)
BUILD_DIR="target"
DIST_DIR="dist"

echo "${BLUE}=== Fleet Schema Gen - macOS Build ===${NC}"
echo "Version: ${GREEN}${VERSION}${NC}"
echo ""

# Check for required tools
command -v cargo >/dev/null 2>&1 || { echo "${RED}Error: cargo not found${NC}"; exit 1; }

# Parse arguments
SIGN=false
STRIP=true

while [[ $# -gt 0 ]]; do
    case $1 in
        --sign)
            SIGN=true
            shift
            ;;
        --no-strip)
            STRIP=false
            shift
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --sign        Sign the binary with Apple Developer ID"
            echo "  --no-strip    Don't strip debug symbols"
            echo "  --help        Show this help message"
            echo ""
            echo "This builds for Apple Silicon (aarch64) only."
            echo "For universal binaries (Intel + Apple Silicon), use build-universal.zsh"
            exit 0
            ;;
        *)
            echo "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Clean previous builds
echo "${YELLOW}→${NC} Cleaning previous builds..."
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

# Check if target is installed
if ! rustup target list | grep -q "^aarch64-apple-darwin (installed)"; then
    echo "${YELLOW}→${NC} Installing aarch64-apple-darwin target..."
    rustup target add aarch64-apple-darwin
fi

# Build for Apple Silicon
echo "${YELLOW}→${NC} Building for Apple Silicon (aarch64)..."
cargo build --release --target aarch64-apple-darwin
cp "${BUILD_DIR}/aarch64-apple-darwin/release/${BINARY_NAME}" "${DIST_DIR}/"

# Strip debug symbols
if [[ "$STRIP" == true ]]; then
    echo "${YELLOW}→${NC} Stripping debug symbols..."
    strip "${DIST_DIR}/${BINARY_NAME}"
fi

# Get binary info
BINARY_SIZE=$(du -h "${DIST_DIR}/${BINARY_NAME}" | cut -f1)
echo "${GREEN}✓${NC} Binary built: ${BINARY_SIZE}"

# Check architectures in binary
echo "${YELLOW}→${NC} Binary architectures:"
lipo -info "${DIST_DIR}/${BINARY_NAME}" | sed 's/^/  /'

# Sign binary if requested
if [[ "$SIGN" == true ]]; then
    echo ""
    echo "${YELLOW}→${NC} Signing binary..."
    ./scripts/sign-macos.zsh "${DIST_DIR}/${BINARY_NAME}"
fi

# Create checksum
echo "${YELLOW}→${NC} Creating checksum..."
cd "${DIST_DIR}"
shasum -a 256 "${BINARY_NAME}" > "${BINARY_NAME}.sha256"
cd - > /dev/null

# Create tarball for distribution
echo "${YELLOW}→${NC} Creating distribution archive..."
ARCHIVE_NAME="${BINARY_NAME}-${VERSION}-macos-aarch64.tar.gz"
tar -czf "${DIST_DIR}/${ARCHIVE_NAME}" -C "${DIST_DIR}" \
    "${BINARY_NAME}" \
    "${BINARY_NAME}.sha256"

ARCHIVE_SIZE=$(du -h "${DIST_DIR}/${ARCHIVE_NAME}" | cut -f1)
echo "${GREEN}✓${NC} Archive created: $ARCHIVE_SIZE"

echo ""
echo "${GREEN}=== Build Complete ===${NC}"
echo "Binary: ${DIST_DIR}/${BINARY_NAME}"
echo "Size: ${BINARY_SIZE}"
echo "Archive: ${DIST_DIR}/${ARCHIVE_NAME} ($ARCHIVE_SIZE)"
echo "Checksum: ${DIST_DIR}/${BINARY_NAME}.sha256"

# Test the binary
echo ""
echo "${YELLOW}→${NC} Testing binary..."
if "${DIST_DIR}/${BINARY_NAME}" --version; then
    echo "${GREEN}✓${NC} Binary works!"
else
    echo "${RED}✗${NC} Binary test failed"
    exit 1
fi

echo ""
echo "${GREEN}Done!${NC}"
