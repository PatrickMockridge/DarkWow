#!/bin/bash
# DarkFi Testnet Upgrade Script
# Usage: ./upgrade.sh

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "=========================================="
echo "  DarkFi Testnet Upgrade"
echo "=========================================="
echo ""

# Check if we're in a git repo
if [ ! -d ".git" ]; then
    echo -e "${RED}Error: Not in DarkFi source directory${NC}"
    echo "Please run from the DarkFi source root:"
    echo "  cd /path/to/darkfi"
    echo "  ./contrib/testnet/upgrade.sh"
    exit 1
fi

# Stop daemon if running
if [ -f "$HOME/.local/share/darkfi/darkfid/testnet.pid" ]; then
    echo "Stopping darkfid before upgrade..."
    ./stop.sh
    sleep 2
fi

echo "Upgrading DarkFi..."
echo ""

# Pull latest changes
echo "[1/3] Pulling latest changes..."
git pull origin master

echo ""
echo "[2/3] Building darkfid..."
cargo build --release -p darkfid

echo ""
echo "[3/3] Building drk wallet..."
cargo build --release -p drk

echo ""
echo -e "${GREEN}Upgrade complete!${NC}"
echo ""
echo "To restart:"
echo "  ./start.sh"