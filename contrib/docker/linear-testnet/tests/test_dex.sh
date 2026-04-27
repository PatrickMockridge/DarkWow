#!/bin/bash
# DEX Contract Tests
# Tests CreateSwapV1, AcceptSwapV1, ExecuteSwapV1, CancelSwapV1
#
# Usage: ./test_dex.sh [node_index]
# Default: node_index=2 (wallet2)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../.."
TESTNET_DIR="$SCRIPT_DIR/.."

NODE_INDEX=${1:-2}
RPC_PORT=$((28345 + NODE_INDEX))
RPC_URL="http://localhost:$RPC_PORT"

echo "=== DEX Contract Tests (Node $NODE_INDEX) ==="
echo "RPC: $RPC_URL"
echo ""

# ============================================================
# Helper Functions
# ============================================================

# Get wallet address
get_wallet_addr() {
    local idx=$1
    local config="$TESTNET_DIR/wallets/wallet${idx}/drk${idx}.toml"
    $ROOT_DIR/target/debug/drk -c "$config" wallet address 2>/dev/null | head -1 | tr -d '[:space:]'
}

# Check node health
check_node() {
    curl -s -f -X POST "$RPC_URL" -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' > /dev/null 2>&1
}

# Get balance
get_balance() {
    local addr=$1
    curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_balance\",\"params\":[\"$addr\"],\"id\":1}" \
        | jq -r '.result.balance // "0"'
}

# ============================================================
# Test Setup
# ============================================================

echo "[Setup] Checking node health..."
if ! check_node; then
    echo "ERROR: Node $NODE_INDEX is not responding on port $RPC_PORT"
    echo "Start the testnet first: cd $TESTNET_DIR && ./scripts/start.sh"
    exit 1
fi
echo "  Node is healthy"

# Get wallet addresses
WALLET0_ADDR=$(get_wallet_addr 0)
WALLET1_ADDR=$(get_wallet_addr 1)
WALLET2_ADDR=$(get_wallet_addr 2)
WALLET3_ADDR=$(get_wallet_addr 3)
WALLET4_ADDR=$(get_wallet_addr 4)

echo ""
echo "[Setup] Wallet addresses:"
echo "  wallet0: $WALLET0_ADDR"
echo "  wallet1: $WALLET1_ADDR"
echo "  wallet2: $WALLET2_ADDR"
echo "  wallet3: $WALLET3_ADDR"
echo "  wallet4: $WALLET4_ADDR"

# Check balances
echo ""
echo "[Setup] Checking balances..."
BALANCE2=$(get_balance "$WALLET2_ADDR")
echo "  wallet2 balance: $BALANCE2"

if [ "$BALANCE2" -lt 100000000 ]; then
    echo "  WARNING: wallet2 balance low, minting tokens..."
    "$TESTNET_DIR/scripts/mint.sh" 2 1000000000
    BALANCE2=$(get_balance "$WALLET2_ADDR")
    echo "  new balance: $BALANCE2"
fi

# ============================================================
# Test 5.1: Create Swap
# ============================================================

echo ""
echo "=========================================="
echo "[Test 5.1] Create Swap (Atomic Swap Proposal)"
echo "=========================================="

# Parameters for CreateSwapV1:
# - swap_id: Unique swap identifier
# - offer_token: Token Alice offers
# - offer_amount: Amount Alice offers
# - request_token: Token Alice wants in return
# - request_amount: Amount Alice wants
# - signature: Alice's signature on swap details
# - alice_lock: Commitment to Alice's funds (secret locked)
# - expires_at: Expiration block
# - open_execution: Whether anyone can execute

echo "[Test 5.1] Preparing CreateSwapV1 call..."
echo "  proposer: $WALLET2_ADDR (wallet2)"
echo "  offer: 100 TOKEN_A"
echo "  request: 100 TOKEN_B"
echo "  expires_at: 1000000"

# Simulate create swap
RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "CreateSwapV1",
            "params": {
                "swap_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "offer_token": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "offer_amount": 100000000,
                "request_token": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "request_amount": 100000000,
                "signature": "placeholder_signature_bytes",
                "alice_lock": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "expires_at": 1000000,
                "open_execution": false
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 5.2: Accept Swap
# ============================================================

echo ""
echo "=========================================="
echo "[Test 5.2] Accept Swap (Bob accepts)"
echo "=========================================="

# Parameters for AcceptSwapV1:
# - swap_id: Swap ID to accept
# - bob_lock: Commitment to Bob's funds (secret locked)
# - signature: Bob's signature on swap_id and lock

echo "[Test 5.2] Preparing AcceptSwapV1 call..."
echo "  acceptor: $WALLET3_ADDR (wallet3)"
echo "  swap_id: <from Test 5.1>"

# First ensure wallet3 has tokens
BALANCE3=$(get_balance "$WALLET3_ADDR")
if [ "$BALANCE3" -lt 100000000 ]; then
    echo "  Minting to wallet3..."
    "$TESTNET_DIR/scripts/mint.sh" 3 1000000000
fi

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "AcceptSwapV1",
            "params": {
                "swap_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "bob_lock": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "signature": "placeholder_signature_bytes"
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 5.3: Execute Swap (Atomic Settlement)
# ============================================================

echo ""
echo "=========================================="
echo "[Test 5.3] Execute Swap (Atomic Settlement)"
echo "=========================================="

# Parameters for ExecuteSwapV1:
# - swap_id: Swap ID to execute
# - alice_secret: Alice's secret to unlock her lock
# - bob_secret: Bob's secret to unlock his lock

echo "[Test 5.3] Preparing ExecuteSwapV1 call..."
echo "  executor: $WALLET0_ADDR (wallet0 - any party)"
echo "  swap_id: <from Test 5.1>"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "ExecuteSwapV1",
            "params": {
                "swap_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "alice_secret": "alice_secret_bytes_here",
                "bob_secret": "bob_secret_bytes_here"
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 5.4: Cancel Swap (Timeout/Refund)
# ============================================================

echo ""
echo "=========================================="
echo "[Test 5.4] Cancel Swap (Refund)"
echo "=========================================="

# Parameters for CancelSwapV1:
# - swap_id: Swap ID to cancel
# - secret: Secret to unlock the lock
# (Proposer cancels = refund to proposer, Acceptors cancel = refund to acceptor)

echo "[Test 5.4] Preparing CancelSwapV1 call..."
echo "  canceller: $WALLET2_ADDR (proposer) or $WALLET3_ADDR (acceptor)"
echo "  swap_id: <expired or unwanted swap>"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "CancelSwapV1",
            "params": {
                "swap_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "secret": "secret_bytes_here"
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 5.5: Execute Swap with Fee
# ============================================================

echo ""
echo "=========================================="
echo "[Test 5.5] Execute Swap with Fee"
echo "=========================================="

# Parameters for ExecuteSwapFeeV1:
# - swap_id: Swap ID to execute
# - alice_secret: Alice's secret
# - bob_secret: Bob's secret
# - fee_bps: Fee in basis points (e.g., 30 = 0.3%)

echo "[Test 5.5] Preparing ExecuteSwapFeeV1 call..."
echo "  swap_id: <from Test 5.1>"
echo "  fee_bps: 30 (0.3%)"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "ExecuteSwapFeeV1",
            "params": {
                "swap_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "alice_secret": "alice_secret_bytes_here",
                "bob_secret": "bob_secret_bytes_here",
                "fee_bps": 30
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 5.6: Execute Swap with Slippage Tolerance
# ============================================================

echo ""
echo "=========================================="
echo "[Test 5.6] Execute Swap with Slippage Tolerance"
echo "=========================================="

# Parameters for ExecuteSwapSlippageV1:
# - swap_id: Swap ID to execute
# - alice_secret: Alice's secret
# - bob_secret: Bob's secret
# - slippage_bps: Maximum slippage tolerance (e.g., 50 = 0.5%)

echo "[Test 5.6] Preparing ExecuteSwapSlippageV1 call..."
echo "  swap_id: <from Test 5.1>"
echo "  slippage_bps: 50 (0.5% tolerance)"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "ExecuteSwapSlippageV1",
            "params": {
                "swap_id": "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
                "alice_secret": "alice_secret_bytes_here",
                "bob_secret": "bob_secret_bytes_here",
                "slippage_bps": 50
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test Summary
# ============================================================

echo ""
echo "=========================================="
echo "=== DEX Test Summary ==="
echo "=========================================="
echo ""
echo "Tests executed with dry_run=true (simulated)"
echo "Full testing requires:"
echo "  1. ZK proof generation (CreateSwap, AcceptSwap, ExecuteSwap)"
echo "  2. Signature verification (Alice/Bob signatures)"
echo "  3. Lock/unlock secrets for atomic settlement"
echo "  4. Transaction broadcast via drk wallet"
echo "  5. Block mining to confirm transactions"
echo ""
echo "Atomic Swap Flow:"
echo "  1. Alice creates swap, locks her funds with secret_a"
echo "  2. Bob accepts swap, locks his funds with secret_b"
echo "  3. Either party executes atomically using secrets"
echo "  4. On timeout, proposer or acceptor can cancel to refund"
echo ""
echo "DEX Features:"
echo "  - CreateSwapV1: Propose an atomic swap (needs money::lock child)"
echo "  - AcceptSwapV1: Accept a swap (needs money::lock child)"
echo "  - ExecuteSwapV1: Execute atomic settlement (secrets reveal locks)"
echo "  - ExecuteSwapFeeV1: Execute with fee deduction"
echo "  - ExecuteSwapSlippageV1: Execute with slippage tolerance"
echo "  - CancelSwapV1: Cancel and refund"
echo ""
echo "Reference: src/contract/dex/src/entrypoint/"