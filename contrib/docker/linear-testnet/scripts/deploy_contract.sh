#!/bin/bash
# Deploy WASM contract to linear-testnet via RPC
# This script runs inside the darkfi-linear-node0 container

set -e

WASM_FILE="$1"
CONTRACT_ID="$2"
RPC_URL="${3:-http://localhost:28345}"

if [ -z "$WASM_FILE" ] || [ -z "$CONTRACT_ID" ]; then
    echo "Usage: deploy_contract.sh <wasm_file> <contract_id> [rpc_url]"
    exit 1
fi

if [ ! -f "$WASM_FILE" ]; then
    echo "ERROR: WASM file not found: $WASM_FILE"
    exit 1
fi

echo "Deploying $WASM_FILE as $CONTRACT_ID to $RPC_URL..."

# Read WASM file and encode as base64
WASM_B64=$(base64 -w 0 "$WASM_FILE")

# Create JSON payload
JSON_PAYLOAD="{\"jsonrpc\":\"2.0\",\"method\":\"contract.deploy\",\"params\":{\"wasm\":\"$WASM_B64\",\"contract_id\":\"$CONTRACT_ID\"},\"id\":1}"

# Write to temp file to avoid issues with command line length
TEMP_FILE=$(mktemp)
echo "$JSON_PAYLOAD" > "$TEMP_FILE"

# Execute curl - note: using stdin redirect with -
curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" -d @- < "$TEMP_FILE"

rm "$TEMP_FILE"