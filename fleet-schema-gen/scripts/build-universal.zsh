#!/usr/bin/env zsh
# Build universal binary for macOS (Intel + Apple Silicon)
# Creates a single binary that works on both architectures

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "${BLUE}=== Universal Binary Builder ===${NC}"
echo ""

# Configuration
BINARY_NAME="fleet-schema-gen"
VERSION=$(grep '^version' Cargo.toml | head -n1 | cut -d'"' -f2)
BUILD_DIR="target"
DIST_DIR="dist"

echo "Version: ${GREEN}${VERSION}${NC}"
echo "Building for: Intel (x86_64) + Apple Silicon (aarch64)"
echo ""

# Check for Rust targets
echo "${YELLOW}→${NC} Checking Rust targets..."

TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
for TARGET in "${TARGETS[@]}"; do
    if ! rustup target list | grep -q "^${TARGET} (installed)"; then
        echo "  ${YELLOW}Installing ${TARGET}...${NC}"
        rustup target add "$TARGET"
    else
        echo "  ${GREEN}✓${NC} ${TARGET}"
    fi
done

# Clean previous builds
echo "${YELLOW}→${NC} Cleaning previous builds..."
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

# Build for Apple Silicon
echo ""
echo "${BLUE}=== Building for Apple Silicon (aarch64) ===${NC}"
cargo build --release --target aarch64-apple-darwin

ARM_BINARY="${BUILD_DIR}/aarch64-apple-darwin/release/${BINARY_NAME}"
ARM_SIZE=$(du -h "$ARM_BINARY" | cut -f1)
echo "${GREEN}✓${NC} Built: $ARM_SIZE"

# Build for Intel
echo ""
echo "${BLUE}=== Building for Intel (x86_64) ===${NC}"
cargo build --release --target x86_64-apple-darwin

INTEL_BINARY="${BUILD_DIR}/x86_64-apple-darwin/release/${BINARY_NAME}"
INTEL_SIZE=$(du -h "$INTEL_BINARY" | cut -f1)
echo "${GREEN}✓${NC} Built: $INTEL_SIZE"

# Create universal binary
echo ""
echo "${BLUE}=== Creating Universal Binary ===${NC}"
echo "${YELLOW}→${NC} Combining architectures..."

UNIVERSAL_BINARY="${DIST_DIR}/${BINARY_NAME}"

lipo -create \
    "$ARM_BINARY" \
    "$INTEL_BINARY" \
    -output "$UNIVERSAL_BINARY"

UNIVERSAL_SIZE=$(du -h "$UNIVERSAL_BINARY" | cut -f1)
echo "${GREEN}✓${NC} Universal binary created: $UNIVERSAL_SIZE"

# Verify architectures
echo ""
echo "${YELLOW}→${NC} Verifying architectures..."
lipo -info "$UNIVERSAL_BINARY" | sed 's/^/  /'

# Strip debug symbols
echo "${YELLOW}→${NC} Stripping debug symbols..."
strip "$UNIVERSAL_BINARY"
STRIPPED_SIZE=$(du -h "$UNIVERSAL_BINARY" | cut -f1)
echo "${GREEN}✓${NC} Stripped: $STRIPPED_SIZE"

# Test on both architectures
echo ""
echo "${YELLOW}→${NC} Testing binary..."

# Test on current arch
if "$UNIVERSAL_BINARY" --version >/dev/null 2>&1; then
    CURRENT_ARCH=$(uname -m)
    echo "${GREEN}✓${NC} Works on $CURRENT_ARCH"
else
    echo "${RED}✗${NC} Binary test failed"
    exit 1
fi

# Create checksum
echo "${YELLOW}→${NC} Creating checksum..."
cd "${DIST_DIR}"
shasum -a 256 "${BINARY_NAME}" > "${BINARY_NAME}.sha256"
cd - > /dev/null

# Create tarball for distribution
echo "${YELLOW}→${NC} Creating distribution archive..."
ARCHIVE_NAME="${BINARY_NAME}-${VERSION}-macos-universal.tar.gz"
tar -czf "${DIST_DIR}/${ARCHIVE_NAME}" -C "${DIST_DIR}" \
    "${BINARY_NAME}" \
    "${BINARY_NAME}.sha256"

ARCHIVE_SIZE=$(du -h "${DIST_DIR}/${ARCHIVE_NAME}" | cut -f1)
echo "${GREEN}✓${NC} Archive created: $ARCHIVE_SIZE"

# Summary
echo ""
echo "${GREEN}=== Build Complete ===${NC}"
echo ""
echo "Universal Binary:"
echo "  Path: ${DIST_DIR}/${BINARY_NAME}"
echo "  Size: $STRIPPED_SIZE"
echo "  Architectures: Intel (x86_64) + Apple Silicon (aarch64)"
echo ""
echo "Distribution:"
echo "  Archive: ${DIST_DIR}/${ARCHIVE_NAME} ($ARCHIVE_SIZE)"
echo "  Checksum: ${DIST_DIR}/${BINARY_NAME}.sha256"
echo ""
echo "Next steps:"
echo "  1. Sign: ./scripts/sign-macos.zsh ${DIST_DIR}/${BINARY_NAME}"
echo "  2. Notarize: ./scripts/notarize-macos.zsh ${DIST_DIR}/${BINARY_NAME}"
echo "  3. Install: ./scripts/install-macos.zsh --binary ${DIST_DIR}/${BINARY_NAME}"
echo ""
echo "Or build and sign in one go:"
echo "  ./scripts/release-macos.zsh"
