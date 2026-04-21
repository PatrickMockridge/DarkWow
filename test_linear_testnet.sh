#!/bin/bash
# Linear-testnet test script
# This verifies linear-testnet functionality once darkfid config issue is fixed

set -e

BIN="./target/debug/darkfid"
RPC_PORT="${RPC_PORT:-28345}"
NETWORK="linear-testnet"
CONFIG="linear_testnet.toml"

echo "=== Linear-Testnet Test Script ==="

# Check if darkfid exists
if [ ! -f "$BIN" ]; then
    echo "ERROR: darkfid not built. Run: cargo build -p darkfid"
    exit 1
fi

# Check if config exists
if [ ! -f "$CONFIG" ]; then
    echo "ERROR: $CONFIG not found"
    exit 1
fi

# Kill any existing darkfid processes
pkill -f "darkfid.*$NETWORK" 2>/dev/null || true
sleep 1

echo ""
echo "=== Step 1: Starting darkfid in linear-testnet mode ==="
$BIN -c $CONFIG -n $NETWORK &
DARKFID_PID=$!
echo "darkfid PID: $DARKFID_PID"

# Wait for startup
sleep 5

# Check if process is still running
if ! kill -0 $DARKFID_PID 2>/dev/null; then
    echo "ERROR: darkfid failed to start"
    exit 1
fi

echo ""
echo "=== Step 2: Check RPC connection ==="
curl -s -X POST http://localhost:$RPC_PORT -H "Content-Type: application/json" \
    -d '{"jsonrpc": "2.0", "method": "ping", "params": [], "id": 1}' || {
    echo "ERROR: RPC not responding"
    kill $DARKFID_PID 2>/dev/null || true
    exit 1
}

echo ""
echo "=== Step 3: Create wallet and get address ==="
ADDR=$(./target/debug/drk -c drk.toml -n $NETWORK wallet address 2>/dev/null | head -1)
if [ -z "$ADDR" ]; then
    echo "Using placeholder address for testing"
    ADDR="4Rwqa7syEBV3BtP2DrJvQKxE2vXmPNbxqLB3PkMXMRX8"
fi
echo "Wallet address: $ADDR"

echo ""
echo "=== Step 4: Mine genesis block via RPC ==="
RESULT=$(curl -s -X POST http://localhost:$RPC_PORT -H "Content-Type: application/json" \
    -d "{\"jsonrpc\": \"2.0\", \"method\": \"miner.mine_linear\", \"params\": [\"$ADDR\", 100000000], \"id\": 1}")
echo "Mine result: $RESULT"

echo ""
echo "=== Step 5: Get block info ==="
curl -s -X POST http://localhost:$RPC_PORT -H "Content-Type: application/json" \
    -d '{"jsonrpc": "2.0", "method": "blockchain.best_fork_next_block_height", "params": [], "id": 1}'

echo ""
echo "=== Test complete ==="
kill $DARKFID_PID 2>/dev/null || true
echo "darkfid stopped"