#!/bin/bash
# Linear-testnet test script
# This verifies linear-testnet functionality once dwowd config issue is fixed

set -e

BIN="./target/debug/dwowd"
RPC_PORT="${RPC_PORT:-28345}"
NETWORK="linear-testnet"
CONFIG="bin/dwowd/dwowd_config.toml"

echo "=== Linear-Testnet Test Script ==="

# Check if dwowd exists
if [ ! -f "$BIN" ]; then
    echo "ERROR: dwowd not built. Run: cargo build -p dwowd"
    exit 1
fi

# Check if config exists
if [ ! -f "$CONFIG" ]; then
    echo "ERROR: $CONFIG not found"
    exit 1
fi

# Kill any existing dwowd processes
pkill -f "dwowd.*$NETWORK" 2>/dev/null || true
sleep 1

echo ""
echo "=== Step 1: Starting dwowd in linear-testnet mode ==="
$BIN -c $CONFIG -n $NETWORK &
DARKFID_PID=$!
echo "dwowd PID: $DARKFID_PID"

# Wait for startup
sleep 5

# Check if process is still running
if ! kill -0 $DARKFID_PID 2>/dev/null; then
    echo "ERROR: dwowd failed to start"
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
ADDR=$(./target/debug/dwow_wallet -c drk.toml -n $NETWORK wallet address 2>/dev/null | head -1)
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
    -d '{"jsonrpc": "2.0", "method": "blockchain.get_difficulty_linear", "params": [], "id": 1}'

echo ""
echo "=== Test complete ==="
kill $DARKFID_PID 2>/dev/null || true
echo "dwowd stopped"