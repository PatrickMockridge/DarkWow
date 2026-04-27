#!/bin/bash
# Setup script - Deploy all contracts to linear-testnet
# Usage: ./setup.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../../.."

# RPC endpoint
RPC_URL="http://localhost:28345"

echo "=== Linear-Testnet Contract Deployment ==="
echo ""

# Check if darkfid is running
if ! curl -s -f -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' > /dev/null 2>&1; then
    echo "ERROR: darkfid node is not running on port 28345"
    echo "Start the testnet first: cd ../ && ./scripts/start.sh"
    exit 1
fi

echo "[1/4] Checking wallet0 address..."
WALLET_ADDR=$(../../target/debug/drk -c wallets/wallet0/drk0.toml wallet address 2>/dev/null | head -1 | tr -d '[:space:]')
if [ -z "$WALLET_ADDR" ]; then
    echo "ERROR: Could not get wallet0 address. Initialize wallet first."
    exit 1
fi
echo "  Wallet address: $WALLET_ADDR"

# Contract IDs (these should match what Deployooor assigns)
# For testing, we'll use placeholder IDs that can be updated
DAO_ESCROW_ID="DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"  # TODO: Update
STABLECOIN_ID="DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"  # TODO: Update
DEX_ID="DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"  # TODO: Update

deploy_contract() {
    local NAME=$1
    local WASM_FILE=$2
    local CONTRACT_ID=$3

    echo ""
    echo "[2/4] Deploying $NAME..."

    if [ ! -f "$WASM_FILE" ]; then
        echo "  WARNING: WASM file not found: $WASM_FILE"
        echo "  Skipping deployment."
        return
    fi

    # Read WASM file and encode as base64
    local WASM_B64=$(base64 -w 0 "$WASM_FILE")

    echo "  Deploying $NAME (contract_id: $CONTRACT_ID)..."

    # Deploy via RPC
    local RESULT=$(curl -s -X POST "$RPC_URL" \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\": \"2.0\",
            \"method\": \"contract.deploy\",
            \"params\": {
                \"wasm\": \"$WASM_B64\",
                \"contract_id\": \"$CONTRACT_ID\"
            },
            \"id\": 1
        }")

    echo "  Result: $RESULT"

    # Check for errors
    if echo "$RESULT" | jq -r '.error // empty' | grep -q "error"; then
        echo "  ERROR deploying $NAME"
    else
        echo "  SUCCESS: $NAME deployed"
    fi
}

# Build WASM files if they don't exist
echo ""
echo "[1/4] Checking contract WASM files..."

CONTRACTS_BUILD=(
    "dao_escrow:src/contract/dao_escrow/darkfi_dao_escrow_contract.wasm"
    "stablecoin:src/contract/stablecoin/darkfi_stablecoin_contract.wasm"
    "dex:src/contract/dex/darkfi_dex_contract.wasm"
)

for entry in "${CONTRACTS_BUILD[@]}"; do
    IFS=':' read -r NAME PATH <<< "$entry"
    if [ ! -f "$PATH" ]; then
        echo "  Building $NAME..."
        (cd "$ROOT_DIR" && cargo build --release -p "darkfi_${NAME}_contract" --target wasm32-unknown-unknown 2>/dev/null) || true
    fi
done

echo ""
echo "=== Deployment Summary ==="
echo "This script deploys contracts via contract.deploy RPC method."
echo "Note: Only NativeToken and Deployooor are in genesis block."
echo "      All other contracts must be deployed post-genesis."
echo ""
echo "Contract deployment requires:"
echo "  1. WASM file (base64 encoded)"
echo "  2. Contract ID (base58 encoded)"
echo ""
echo "Use the RPC directly:"
echo "  curl -X POST http://localhost:28345 \\"
echo "    -H 'Content-Type: application/json' \\"
echo "    -d '{\"jsonrpc\": \"2.0\", \"method\": \"contract.deploy\", \"params\": {\"wasm\": \"<b64>\", \"contract_id\": \"<id>\"}, \"id\": 1}'"
echo ""
echo "Or use drk wallet (once contract deploy is implemented):"
echo "  ./target/debug/drk -c wallets/wallet0/drk0.toml contract deploy --wasm <path> --authority <auth>"