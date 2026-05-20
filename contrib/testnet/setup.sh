#!/bin/bash
# DarkFi Testnet Setup Script
# Usage: ./setup.sh

set -e

DWOW_HOME="${DWOW_HOME:-$HOME/.local/share/dwow}"
DARKFID_HOME="$DWOW_HOME/darkfid"
CONFIG_DIR="$DARKFID_HOME"
CONFIG_FILE="$CONFIG_DIR/dwowd_config.toml"
NETWORK="testnet"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "=========================================="
echo "  DarkFi Testnet Node Setup"
echo "=========================================="
echo ""

# Check prerequisites
echo "[1/5] Checking prerequisites..."

if ! command -v dwowd &> /dev/null; then
    echo -e "${RED}ERROR: dwowd not found in PATH${NC}"
    echo "Please build dwowd first:"
    echo "  cargo build --release -p dwowd"
    exit 1
fi

echo -e "${GREEN}  dwowd found${NC}"

# Create directories
echo ""
echo "[2/5] Creating directories..."
mkdir -p "$DARKFID_HOME/$NETWORK"
echo -e "${GREEN}  Created $DARKFID_HOME/$NETWORK/${NC}"

# Copy or create config
echo ""
echo "[3/5] Configuring dwowd..."

if [ -f "$CONFIG_FILE" ]; then
    echo "  Using existing config: $CONFIG_FILE"
    # Verify testnet section exists
    if grep -q "^\[network_config.\"testnet\"\]" "$CONFIG_FILE"; then
        echo -e "${GREEN}  Config already has testnet section${NC}"
    else
        echo -e "${YELLOW}  Warning: Config exists but missing testnet section${NC}"
    fi
else
    echo "  No config found, copying default..."
    if [ -f "$(dirname $0)/../bin/dwowd/dwowd_config.toml" ]; then
        cp "$(dirname $0)/../bin/dwowd/dwowd_config.toml" "$CONFIG_FILE"
    elif [ -f "$HOME/dwow/bin/dwowd/dwowd_config.toml" ]; then
        cp "$HOME/dwow/bin/dwowd/dwowd_config.toml" "$CONFIG_FILE"
    else
        echo -e "${RED}ERROR: Could not find default config to copy${NC}"
        echo "Please copy your dwowd_config.toml to $CONFIG_FILE"
        exit 1
    fi
    echo -e "${GREEN}  Config created${NC}"
fi

# Check genesis block
echo ""
echo "[4/5] Checking genesis block..."

GENESIS_FILE="$DARKFID_HOME/$NETWORK/genesis_block"
if [ -f "$GENESIS_FILE" ]; then
    echo -e "${GREEN}  Genesis block found${NC}"
else
    echo "  Genesis block not found (will sync from network)"
    echo -e "${YELLOW}  If this is a fresh install, sync may take time${NC}"
fi

# Summary
echo ""
echo "[5/5] Setup complete!"
echo ""
echo "=========================================="
echo "  Next Steps"
echo "=========================================="
echo ""
echo "  Start the daemon:"
echo -e "    ${GREEN}./start.sh${NC}"
echo ""
echo "  Or manually:"
echo -e "    ${GREEN}dwowd --network testnet${NC}"
echo ""
echo "  Check status:"
echo -e "    ${GREEN}./status.sh${NC}"
echo ""
echo "  View logs:"
echo -e "    ${GREEN}./logs.sh${NC}"
echo ""
echo "  Stop the daemon:"
echo -e "    ${GREEN}./stop.sh${NC}"
echo ""
echo "  For wallet operations, see:"
echo -e "    ${GREEN}./wallet-init.sh${NC}"
echo ""
echo "=========================================="