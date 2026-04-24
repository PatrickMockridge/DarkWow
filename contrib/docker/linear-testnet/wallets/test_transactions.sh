#!/bin/bash
# Create a test transaction to verify wallets work
# Usage: ./test_transactions.sh [from_wallet] [to_wallet] [amount]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WALLETS_DIR="$SCRIPT_DIR"

FROM_WALLET=${1:-0}
TO_WALLET=${2:-1}
AMOUNT=${3:-1000000}  # Default 0.01 with 8 decimals

FROM_DIR="$WALLETS_DIR/wallet${FROM_WALLET}"
TO_DIR="$WALLETS_DIR/wallet${TO_WALLET}"

# Get addresses
FROM_ADDR=$(./target/debug/drk -c "$FROM_DIR/drk${FROM_WALLET}.toml" wallet address 2>/dev/null | head -1 | tr -d '[:space:]')
TO_ADDR=$(./target/debug/drk -c "$TO_DIR/drk${TO_WALLET}.toml" wallet address 2>/dev/null | head -1 | tr -d '[:space:]')

echo "=== Testing Transaction ==="
echo "From: wallet${FROM_WALLET} (${FROM_ADDR})"
echo "To:   wallet${TO_WALLET} (${TO_ADDR})"
echo "Amount: $AMOUNT"
echo ""

# Get RPC port for source wallet
PORTS=(28345 28346 28347 28348 28349)
RPC_PORT=${PORTS[$FROM_WALLET]}

# Check balance before
echo "Checking balance..."
BALANCE=$(curl -s -X POST "http://localhost:$RPC_PORT" -H "Content-Type: application/json" \
    -d "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_balance\",\"params\":[\"$FROM_ADDR\"],\"id\":1}" \
    | jq -r '.result.balance // "0"')
echo "Current balance: $BALANCE"

echo ""
echo "Note: For linear-testnet, minting happens via miner.mine_linear"
echo "      which automatically creates coinbase outputs to the WALLET_ADDR"
echo ""
echo "To mint new tokens (mine blocks):"
echo "  curl -X POST http://localhost:$RPC_PORT \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"jsonrpc\": \"2.0\", \"method\": \"miner.mine_linear\","
echo "        \"params\": [\"$FROM_ADDR\", $AMOUNT], \"id\": 1}'"