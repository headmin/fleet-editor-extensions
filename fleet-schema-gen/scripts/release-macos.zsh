#!/usr/bin/env zsh
# Complete release pipeline for macOS
# Builds, signs, and optionally notarizes the binary

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "${BLUE}╔════════════════════════════════════════════╗${NC}"
echo "${BLUE}║  Fleet Schema Gen - macOS Release Build   ║${NC}"
echo "${BLUE}╚════════════════════════════════════════════╝${NC}"
echo ""

# Configuration
VERSION=$(grep '^version' Cargo.toml | head -n1 | cut -d'"' -f2)
echo "Version: ${GREEN}${VERSION}${NC}"
echo ""

# Parse arguments
NOTARIZE=false
INSTALL=false
SKIP_TESTS=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --notarize)
            NOTARIZE=true
            shift
            ;;
        --install)
            INSTALL=true
            shift
            ;;
        --skip-tests)
            SKIP_TESTS=true
            shift
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --notarize     Submit to Apple for notarization"
            echo "  --install      Install after building"
            echo "  --skip-tests   Skip running tests"
            echo "  --help         Show this help message"
            echo ""
            echo "This script will:"
            echo "  1. Run tests (unless --skip-tests)"
            echo "  2. Build for Apple Silicon (aarch64)"
            echo "  3. Sign the binary"
            echo "  4. Optionally notarize (--notarize)"
            echo "  5. Optionally install (--install)"
            exit 0
            ;;
        *)
            echo "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Step 1: Run tests
if [[ "$SKIP_TESTS" == false ]]; then
    echo "${BLUE}[1/5]${NC} ${YELLOW}Running tests...${NC}"
    cargo test --release
    echo "${GREEN}✓${NC} All tests passed"
    echo ""
else
    echo "${BLUE}[1/5]${NC} ${YELLOW}Skipping tests${NC}"
    echo ""
fi

# Step 2: Build for Apple Silicon
echo "${BLUE}[2/5]${NC} ${YELLOW}Building for Apple Silicon...${NC}"
./scripts/build-macos.zsh
echo ""

# Step 3: Sign binary
echo "${BLUE}[3/5]${NC} ${YELLOW}Signing binary...${NC}"
./scripts/sign-macos.zsh dist/fleet-schema-gen
echo ""

# Step 4: Notarize (optional)
if [[ "$NOTARIZE" == true ]]; then
    echo "${BLUE}[4/5]${NC} ${YELLOW}Notarizing binary...${NC}"
    ./scripts/notarize-macos.zsh dist/fleet-schema-gen
    echo ""
else
    echo "${BLUE}[4/5]${NC} ${YELLOW}Skipping notarization${NC}"
    echo "  (Run with --notarize to submit to Apple)"
    echo ""
fi

# Step 5: Install (optional)
if [[ "$INSTALL" == true ]]; then
    echo "${BLUE}[5/5]${NC} ${YELLOW}Installing...${NC}"
    ./scripts/install-macos.zsh --binary dist/fleet-schema-gen
    echo ""
else
    echo "${BLUE}[5/5]${NC} ${YELLOW}Skipping installation${NC}"
    echo "  (Run with --install to install automatically)"
    echo ""
fi

# Summary
echo "${GREEN}╔════════════════════════════════════════════╗${NC}"
echo "${GREEN}║          Release Build Complete!           ║${NC}"
echo "${GREEN}╚════════════════════════════════════════════╝${NC}"
echo ""
echo "Version: ${GREEN}${VERSION}${NC}"
echo "Binary: ${BLUE}dist/fleet-schema-gen${NC}"
echo ""

# Show file info
BINARY_SIZE=$(du -h dist/fleet-schema-gen | cut -f1)
echo "Size: $BINARY_SIZE"

# Show architectures
echo "Architectures:"
lipo -info dist/fleet-schema-gen | sed 's/^/  /'

# Show signature status
echo ""
if codesign -dv dist/fleet-schema-gen 2>/dev/null; then
    echo "Status: ${GREEN}Signed${NC}"
    codesign -dv dist/fleet-schema-gen 2>&1 | grep -E "Authority" | head -n1 | sed 's/^/  /'
fi

# Check notarization
echo ""
if spctl --assess --type execute --verbose dist/fleet-schema-gen 2>&1 | grep -q "accepted"; then
    echo "Gatekeeper: ${GREEN}Accepted${NC}"
elif [[ "$NOTARIZE" == false ]]; then
    echo "Gatekeeper: ${YELLOW}Not notarized${NC}"
    echo "  Run with --notarize to submit to Apple"
else
    echo "Gatekeeper: ${GREEN}Notarized${NC}"
fi

# Distribution files
echo ""
echo "${BLUE}Distribution files:${NC}"
ls -lh dist/ | grep fleet-schema-gen | awk '{print "  " $9 " (" $5 ")"}'

echo ""
echo "${GREEN}Next steps:${NC}"
if [[ "$INSTALL" == false ]]; then
    echo "  Install: ./scripts/install-macos.zsh"
fi
echo "  Test: dist/fleet-schema-gen --version"
echo "  Distribute: dist/fleet-schema-gen-${VERSION}-macos-aarch64.tar.gz"
echo ""
echo "${GREEN}Done!${NC}"
