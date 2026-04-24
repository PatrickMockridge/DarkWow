#!/bin/bash
# Mint tokens to any wallet by mining blocks
# Usage: ./mint.sh [wallet_index] [reward_value]
# Defaults: wallet_index=0, reward_value=100000000 (1 token with 8 decimals)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WALLETS_DIR="$SCRIPT_DIR/../wallets"

NODE_INDEX=${1:-0}
REWARD=${2:-100000000}

# Get wallet address
WALLET_DIR="$WALLETS_DIR/wallet${NODE_INDEX}"
WALLET_CONFIG="$WALLET_DIR/drk${NODE_INDEX}.toml"

# RPC port mapping
case $NODE_INDEX in
    0) RPC_PORT=28345 ;;
    1) RPC_PORT=28346 ;;
    2) RPC_PORT=28347 ;;
    3) RPC_PORT=28348 ;;
    4) RPC_PORT=28349 ;;
    *) echo "Invalid wallet index: $NODE_INDEX (must be 0-4)"; exit 1 ;;
esac

# Check if wallet config exists
if [ ! -f "$WALLET_CONFIG" ]; then
    echo "ERROR: Wallet config not found: $WALLET_CONFIG"
    echo "Run wallets/setup_wallets.sh first"
    exit 1
fi

# Get address from wallet
ADDR=$(../../target/debug/drk -c "$WALLET_CONFIG" wallet address 2>/dev/null | head -1 | tr -d '[:space:]')
if [ -z "$ADDR" ]; then
    echo "ERROR: Could not get wallet address"
    echo "Initialize wallet first: ../../target/debug/drk -c $WALLET_CONFIG wallet init"
    exit 1
fi

echo "=== Minting to Wallet $NODE_INDEX ==="
echo "Address: $ADDR"
echo "Reward:  $REWARD (smallest unit)"
echo "RPC:     localhost:$RPC_PORT"
echo ""

# Check if node is running
if ! curl -s -f -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' > /dev/null 2>&1; then
    echo "ERROR: Node $NODE_INDEX is not responding on port $RPC_PORT"
    echo "Is the stack running? Try: cd ../ && ./scripts/start.sh"
    exit 1
fi

# Get current block height
HEIGHT=$(curl -s -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"blockchain.best_fork_next_block_height","params":[],"id":1}' \
    | jq -r '.result.height // 0')

echo "Current height: $HEIGHT"
echo ""

# Mine a block with reward to this wallet
echo "Submitting mine request..."
RESULT=$(curl -s -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d "{\"jsonrpc\": \"2.0\", \"method\": \"miner.mine_linear\", \"params\": [\"$ADDR\", $REWARD], \"id\": 1}")

echo "Result: $RESULT"
echo ""

# Check new height after a short delay
sleep 2
NEW_HEIGHT=$(curl -s -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"blockchain.best_fork_next_block_height","params":[],"id":1}' \
    | jq -r '.result.height // 0')

echo "New height: $NEW_HEIGHT"

if [ "$NEW_HEIGHT" -gt "$HEIGHT" ]; then
    echo ""
    echo "SUCCESS: Block mined! Wallet $NODE_INDEX received $REWARD tokens."
else
    echo ""
    echo "Block may take time to be confirmed. Check status with:"
    echo "  curl -X POST http://localhost:$RPC_PORT -H 'Content-Type: application/json' \\"
    echo "    -d '{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_balance\",\"params\":[\"$ADDR\"],\"id\":1}'"
fi