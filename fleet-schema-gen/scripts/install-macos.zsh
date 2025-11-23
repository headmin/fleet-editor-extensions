#!/usr/bin/env zsh
# Installation script for macOS
# Installs fleet-schema-gen to system or user bin directory

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "${BLUE}=== Fleet Schema Gen - Install ===${NC}"
echo ""

# Configuration
BINARY_NAME="fleet-schema-gen"
DEFAULT_INSTALL_DIR="/usr/local/bin"
USER_INSTALL_DIR="$HOME/.local/bin"

# Parse arguments
INSTALL_DIR=""
BINARY_PATH=""
SYSTEM_INSTALL=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --system)
            SYSTEM_INSTALL=true
            shift
            ;;
        --user)
            INSTALL_DIR="$USER_INSTALL_DIR"
            shift
            ;;
        --dir)
            INSTALL_DIR="$2"
            shift 2
            ;;
        --binary)
            BINARY_PATH="$2"
            shift 2
            ;;
        --help)
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --system      Install to /usr/local/bin (requires sudo)"
            echo "  --user        Install to ~/.local/bin (default)"
            echo "  --dir <path>  Install to custom directory"
            echo "  --binary <path>  Path to binary (default: dist/fleet-schema-gen)"
            echo "  --help        Show this help message"
            echo ""
            echo "Examples:"
            echo "  $0                    # Install to ~/.local/bin"
            echo "  $0 --system           # Install to /usr/local/bin"
            echo "  $0 --dir ~/bin        # Install to ~/bin"
            exit 0
            ;;
        *)
            echo "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

# Determine install directory
if [[ -z "$INSTALL_DIR" ]]; then
    if [[ "$SYSTEM_INSTALL" == true ]]; then
        INSTALL_DIR="$DEFAULT_INSTALL_DIR"
    else
        INSTALL_DIR="$USER_INSTALL_DIR"
    fi
fi

# Find binary
if [[ -z "$BINARY_PATH" ]]; then
    if [[ -f "dist/${BINARY_NAME}" ]]; then
        BINARY_PATH="dist/${BINARY_NAME}"
    elif [[ -f "target/release/${BINARY_NAME}" ]]; then
        BINARY_PATH="target/release/${BINARY_NAME}"
    else
        echo "${RED}Error: Binary not found${NC}"
        echo ""
        echo "Build it first:"
        echo "  ./scripts/build-macos.zsh"
        exit 1
    fi
fi

# Check if binary exists
if [[ ! -f "$BINARY_PATH" ]]; then
    echo "${RED}Error: Binary not found: $BINARY_PATH${NC}"
    exit 1
fi

# Get binary version
VERSION=$("$BINARY_PATH" --version 2>/dev/null | awk '{print $2}' || echo "unknown")

echo "Binary: $BINARY_PATH"
echo "Version: $VERSION"
echo "Install directory: $INSTALL_DIR"
echo ""

# Check if installation directory exists
if [[ ! -d "$INSTALL_DIR" ]]; then
    echo "${YELLOW}→${NC} Creating installation directory..."
    mkdir -p "$INSTALL_DIR"
fi

# Check if we need sudo
NEEDS_SUDO=false
if [[ ! -w "$INSTALL_DIR" ]]; then
    NEEDS_SUDO=true
    echo "${YELLOW}⚠${NC}  Installation directory requires sudo access"
fi

# Install binary
INSTALL_PATH="${INSTALL_DIR}/${BINARY_NAME}"

echo "${YELLOW}→${NC} Installing to $INSTALL_PATH..."

if [[ "$NEEDS_SUDO" == true ]]; then
    sudo cp "$BINARY_PATH" "$INSTALL_PATH"
    sudo chmod 755 "$INSTALL_PATH"
else
    cp "$BINARY_PATH" "$INSTALL_PATH"
    chmod 755 "$INSTALL_PATH"
fi

echo "${GREEN}✓${NC} Installed"

# Verify installation
echo "${YELLOW}→${NC} Verifying installation..."
if command -v "$BINARY_NAME" >/dev/null 2>&1; then
    INSTALLED_VERSION=$("$BINARY_NAME" --version 2>/dev/null | awk '{print $2}' || echo "unknown")
    echo "${GREEN}✓${NC} $BINARY_NAME is in PATH (version: $INSTALLED_VERSION)"
else
    echo "${YELLOW}⚠${NC}  $BINARY_NAME is not in PATH"
    echo ""
    echo "Add to your PATH by adding this to your ~/.zshrc:"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    echo ""
    echo "Then reload your shell:"
    echo "  source ~/.zshrc"
fi

# Test the installed binary
echo ""
echo "${YELLOW}→${NC} Testing installation..."
if "$INSTALL_PATH" --version >/dev/null 2>&1; then
    echo "${GREEN}✓${NC} Binary works!"
else
    echo "${RED}✗${NC} Binary test failed"
    exit 1
fi

# Show signature info if signed
echo ""
if codesign -dv "$INSTALL_PATH" 2>/dev/null; then
    echo "${GREEN}=== Code Signature ===${NC}"
    codesign -dv "$INSTALL_PATH" 2>&1 | grep -E "Authority|Identifier" | sed 's/^/  /'
fi

echo ""
echo "${GREEN}=== Installation Complete ===${NC}"
echo "Installed: $INSTALL_PATH"
echo "Version: $VERSION"
echo ""
echo "Usage:"
echo "  $BINARY_NAME --help"
echo "  $BINARY_NAME generate --help"
echo ""
echo "Example:"
echo "  $BINARY_NAME generate --source go --output .vscode/"
