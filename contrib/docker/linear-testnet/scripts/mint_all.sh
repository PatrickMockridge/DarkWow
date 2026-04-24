#!/bin/bash
# Mint tokens to ALL 5 wallets to bootstrap them
# Usage: ./mint_all.sh [reward_value]
# Defaults: reward_value=100000000 (1 token with 8 decimals)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

REWARD=${1:-100000000}

echo "=== Minting to ALL 5 Wallets ==="
echo "Reward per mint: $REWARD"
echo ""

for i in 0 1 2 3 4; do
    echo "--- Minting to wallet $i ---"
    "$SCRIPT_DIR/mint.sh" $i $REWARD
    echo ""
done

echo "=== All Wallets Minted ==="
echo ""
echo "To check balances:"
for i in 0 1 2 3 4; do
    echo "Wallet $i: curl -X POST http://localhost:2834${i} -H 'Content-Type: application/json' \\"
    echo "  -d '{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_balance\",\"params\":[\"<addr>\"],\"id\":1}'"
done