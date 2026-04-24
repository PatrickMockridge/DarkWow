#!/bin/bash
# Mine a block on the 5-node linear-testnet via RPC
# Usage: ./mine.sh [node_index] [reward]
# Defaults: node_index=0, reward=100000000

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR/.."

NODE_INDEX=${1:-0}
REWARD=${2:-100000000}

# RPC port mapping
case $NODE_INDEX in
    0) RPC_PORT=28345 ;;
    1) RPC_PORT=28346 ;;
    2) RPC_PORT=28347 ;;
    3) RPC_PORT=28348 ;;
    4) RPC_PORT=28349 ;;
    *) echo "Invalid node index: $NODE_INDEX (must be 0-4)"; exit 1 ;;
esac

# Default wallet address
WALLET_ADDR=${WALLET_ADDR:-DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf}

echo "=== Mining block on node$NODE_INDEX (port $RPC_PORT) ==="
echo "Reward: $REWARD"
echo "Wallet: $WALLET_ADDR"
echo ""

# Check if node is running
if ! curl -s -f -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' > /dev/null 2>&1; then
    echo "ERROR: Node$NODE_INDEX is not responding on port $RPC_PORT"
    echo "Is the stack running? Try: ./scripts/start.sh"
    exit 1
fi

# Get current block height
HEIGHT=$(curl -s -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"blockchain.best_fork_next_block_height","params":[],"id":1}' \
    | jq -r '.result.height // 0')

echo "Current height: $HEIGHT"

# Mine a block
echo "Submitting mine request..."
RESULT=$(curl -s -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d "{\"jsonrpc\": \"2.0\", \"method\": \"miner.mine_linear\", \"params\": [\"$WALLET_ADDR\", $REWARD], \"id\": 1}")

echo "Result: $RESULT"

# Check new height
sleep 2
NEW_HEIGHT=$(curl -s -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"blockchain.best_fork_next_block_height","params":[],"id":1}' \
    | jq -r '.result.height // 0')

echo ""
echo "New height: $NEW_HEIGHT"

if [ "$NEW_HEIGHT" -gt "$HEIGHT" ]; then
    echo "SUCCESS: Block mined!"
else
    echo "Note: Block may take time to be confirmed"
fi