#!/bin/bash
# DarkFi Testnet Wallet Initialization Script
# Usage: ./wallet-init.sh [wallet_name]

set -e

WALLET_NAME="${1:-testnet_wallet}"
WALLET_DIR="${DWOW_HOME:-$HOME/.local/share/dwow}/dww"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

echo "=========================================="
echo "  DarkFi Wallet Initialization"
echo "=========================================="
echo ""

# Check if dww CLI is available
if ! command -v dww &> /dev/null; then
    echo -e "${RED}ERROR: dww wallet CLI not found in PATH${NC}"
    echo "Please build dww first:"
    echo "  cargo build --release -p dww"
    exit 1
fi

echo -e "${GREEN}drk found${NC}"

# Create wallet directory
mkdir -p "$WALLET_DIR"
echo "Wallet directory: $WALLET_DIR"
echo ""

# Check if wallet exists
WALLET_PATH="$WALLET_DIR/$WALLET_NAME"
if [ -f "$WALLET_PATH.db" ]; then
    echo -e "${YELLOW}Wallet already exists: $WALLET_PATH${NC}"
    echo ""
    echo "To open the wallet, run:"
    echo "  dww --network testnet wallet --open $WALLET_NAME"
    exit 0
fi

echo "Creating new wallet: $WALLET_NAME"
echo ""
echo "You will be prompted to set a password."
echo ""

# Create wallet
dww --network testnet wallet --create --name "$WALLET_NAME"

echo ""
echo -e "${GREEN}Wallet created successfully!${NC}"
echo ""
echo "Next steps:"
echo "  1. Start the daemon: ./start.sh"
echo "  2. Open wallet: dww --network testnet wallet --open $WALLET_NAME"
echo "  3. Get address: dww address"
echo ""
echo "Remember to backup your seed phrase and password securely!"