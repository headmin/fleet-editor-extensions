#!/usr/bin/env zsh
# Uninstall script for macOS
# Removes fleet-schema-gen from system

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "${BLUE}=== Fleet Schema Gen - Uninstall ===${NC}"
echo ""

BINARY_NAME="fleet-schema-gen"

# Find installed locations
INSTALLED_PATHS=()

# Check common locations
LOCATIONS=(
    "/usr/local/bin/${BINARY_NAME}"
    "$HOME/.local/bin/${BINARY_NAME}"
    "$HOME/bin/${BINARY_NAME}"
    "/opt/homebrew/bin/${BINARY_NAME}"
)

for LOC in "${LOCATIONS[@]}"; do
    if [[ -f "$LOC" ]]; then
        INSTALLED_PATHS+=("$LOC")
    fi
done

# Also check PATH
if command -v "$BINARY_NAME" >/dev/null 2>&1; then
    PATH_LOCATION=$(which "$BINARY_NAME")
    # Add to array if not already there
    if [[ ! " ${INSTALLED_PATHS[@]} " =~ " ${PATH_LOCATION} " ]]; then
        INSTALLED_PATHS+=("$PATH_LOCATION")
    fi
fi

# Check if anything to uninstall
if [[ ${#INSTALLED_PATHS[@]} -eq 0 ]]; then
    echo "${YELLOW}No installations found${NC}"
    echo ""
    echo "Checked locations:"
    for LOC in "${LOCATIONS[@]}"; do
        echo "  - $LOC"
    done
    exit 0
fi

# Show what will be removed
echo "Found ${GREEN}${#INSTALLED_PATHS[@]}${NC} installation(s):"
echo ""
for PATH_ITEM in "${INSTALLED_PATHS[@]}"; do
    VERSION=$("$PATH_ITEM" --version 2>/dev/null | awk '{print $2}' || echo "unknown")
    echo "  ${YELLOW}→${NC} $PATH_ITEM (version: $VERSION)"
done

echo ""
echo "${RED}This will remove all installations.${NC}"
echo "Continue? (y/N)"
read -q REPLY
echo ""

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "${YELLOW}Cancelled${NC}"
    exit 0
fi

# Remove each installation
for PATH_ITEM in "${INSTALLED_PATHS[@]}"; do
    echo "${YELLOW}→${NC} Removing $PATH_ITEM..."

    if [[ -w "$PATH_ITEM" ]]; then
        rm "$PATH_ITEM"
        echo "${GREEN}✓${NC} Removed"
    elif [[ -w "$(dirname "$PATH_ITEM")" ]]; then
        rm "$PATH_ITEM"
        echo "${GREEN}✓${NC} Removed"
    else
        echo "  ${YELLOW}Requires sudo${NC}"
        sudo rm "$PATH_ITEM"
        echo "${GREEN}✓${NC} Removed"
    fi
done

# Verify uninstallation
echo ""
echo "${YELLOW}→${NC} Verifying..."
if command -v "$BINARY_NAME" >/dev/null 2>&1; then
    echo "${YELLOW}⚠${NC}  $BINARY_NAME is still in PATH"
    echo "  Location: $(which "$BINARY_NAME")"
    echo "  You may need to restart your shell"
else
    echo "${GREEN}✓${NC} $BINARY_NAME removed from system"
fi

echo ""
echo "${GREEN}Uninstall complete!${NC}"
