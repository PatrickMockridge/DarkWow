#!/bin/bash
# Stablecoin (CDP) Contract Tests
# Tests OpenPositionV1, MintStableV1, RepayV1, LiquidateV1, AccrueInterestV1
#
# Usage: ./test_stablecoin.sh [node_index]
# Default: node_index=1 (wallet1)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../.."
TESTNET_DIR="$SCRIPT_DIR/.."

NODE_INDEX=${1:-1}
RPC_PORT=$((28345 + NODE_INDEX))
RPC_URL="http://localhost:$RPC_PORT"

echo "=== Stablecoin (CDP) Contract Tests (Node $NODE_INDEX) ==="
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

echo ""
echo "[Setup] Wallet addresses:"
echo "  wallet0: $WALLET0_ADDR"
echo "  wallet1: $WALLET1_ADDR"
echo "  wallet2: $WALLET2_ADDR"
echo "  wallet3: $WALLET3_ADDR"

# Check balances
echo ""
echo "[Setup] Checking balances..."
BALANCE1=$(get_balance "$WALLET1_ADDR")
echo "  wallet1 balance: $BALANCE1"

if [ "$BALANCE1" -lt 100000000 ]; then
    echo "  WARNING: wallet1 balance low, minting tokens..."
    "$TESTNET_DIR/scripts/mint.sh" 1 1000000000
    BALANCE1=$(get_balance "$WALLET1_ADDR")
    echo "  new balance: $BALANCE1"
fi

# ============================================================
# Test 4.1: Open CDP Position
# ============================================================

echo ""
echo "=========================================="
echo "[Test 4.1] Open CDP Position"
echo "=========================================="

# Parameters for OpenPositionV1:
# - deposit_commitment: Pedersen commitment to collateral
# - collateral_amount: Amount of collateral being deposited
# - collateral_type: XMR, DRK, or ETH
# - proof: ZK proof that deposit is valid
# - zk_public_inputs: [position_nullifier, position_commitment]

echo "[Test 4.1] Preparing OpenPositionV1 call..."
echo "  collateral_amount: 1000000000 (10 tokens)"
echo "  collateral_type: DRK"
echo "  owner: $WALLET1_ADDR"

# Simulate open position
RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "stablecoin",
            "function": "OpenPositionV1",
            "params": {
                "deposit_commitment": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "collateral_amount": 1000000000,
                "collateral_type": "Drk",
                "proof": "placeholder_proof_bytes",
                "zk_public_inputs": ["DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"]
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 4.2: Mint Stablecoin (against collateral)
# ============================================================

echo ""
echo "=========================================="
echo "[Test 4.2] Mint Stablecoin"
echo "=========================================="

# Parameters for MintStableV1:
# - position_commitment: Commitment to the position being debted
# - mint_amount: Amount of stablecoin to mint
# - proof: ZK proof that mint is valid
# - zk_public_inputs: [position_nullifier, debt_commitment, ...]

echo "[Test 4.2] Preparing MintStableV1 call..."
echo "  mint_amount: 500000000 (5 stablecoins)"
echo "  owner: $WALLET1_ADDR"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "stablecoin",
            "function": "MintStableV1",
            "params": {
                "position_commitment": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "mint_amount": 500000000,
                "proof": "placeholder_proof_bytes",
                "zk_public_inputs": ["DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"]
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 4.3: Repay Stablecoin (close position)
# ============================================================

echo ""
echo "=========================================="
echo "[Test 4.3] Repay Stablecoin"
echo "=========================================="

# Parameters for RepayStableV1:
# - repay_amount: Amount of stablecoin to repay
# - position_nullifier: Nullifier to prove ownership
# - proof: ZK proof that repay is valid
# - zk_public_inputs: [position_commitment, debt_nullifier, ...]

echo "[Test 4.3] Preparing RepayStableV1 call..."
echo "  repay_amount: 200000000 (2 stablecoins)"
echo "  owner: $WALLET1_ADDR"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "stablecoin",
            "function": "RepayStableV1",
            "params": {
                "repay_amount": 200000000,
                "position_nullifier": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "proof": "placeholder_proof_bytes",
                "zk_public_inputs": ["DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"]
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 4.4: Liquidate Underwater Position
# ============================================================

echo ""
echo "=========================================="
echo "[Test 4.4] Liquidate Underwater Position"
echo "=========================================="

# Parameters for LiquidateV1:
# - position_nullifier: Nullifier of position being liquidated
# - proof: ZK proof that liquidation is valid
# - zk_public_inputs: [collateral_commitment, debt_commitment, ...]

echo "[Test 4.4] Preparing LiquidateV1 call..."
echo "  target position: wallet3's position (if underwater)"
echo "  liquidator: $WALLET2_ADDR"

# First mint some tokens to wallet2 for liquidation
BALANCE2=$(get_balance "$WALLET2_ADDR")
if [ "$BALANCE2" -lt 100000000 ]; then
    echo "  Minting to wallet2 for liquidation..."
    "$TESTNET_DIR/scripts/mint.sh" 2 1000000000
fi

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "stablecoin",
            "function": "LiquidateV1",
            "params": {
                "position_nullifier": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "proof": "placeholder_proof_bytes",
                "zk_public_inputs": ["DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"]
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 4.5: Accrue Interest (PI Controller update)
# ============================================================

echo ""
echo "=========================================="
echo "[Test 4.5] Accrue Interest (PI Controller)"
echo "=========================================="

# Parameters for AccrueInterestV1:
# - governance_report: Oracle price report for TWAP calculation
# - proof: ZK proof that interest calculation is valid
# - zk_public_inputs: [total_debt_nullifier, ...]

echo "[Test 4.5] Preparing AccrueInterestV1 call..."
echo "  Updates redemption rate based on PI controller"
echo "  Oracle: price_feeds for collateral types"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "stablecoin",
            "function": "AccrueInterestV1",
            "params": {
                "governance_report": {
                    "price_feeds": [
                        {"collateral_type": "Drk", "twap": 100000000, "spread": 1000000}
                    ]
                },
                "proof": "placeholder_proof_bytes",
                "zk_public_inputs": ["DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"]
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
echo "=== Stablecoin Test Summary ==="
echo "=========================================="
echo ""
echo "Tests executed with dry_run=true (simulated)"
echo "Full testing requires:"
echo "  1. ZK proof generation (OpenPosition, MintStable, Repay, Liquidate)"
echo "  2. Transaction broadcast via drk wallet"
echo "  3. Block mining to confirm transactions"
echo ""
echo "Stablecoin Architecture (Pooled Debt):"
echo "  - All collateral backs all debt (no individual positions)"
echo "  - PI Controller adjusts redemption rate based on TWAP"
echo "  - AMM-based price oracle (NETHER/DRK constant-product pool)"
echo "  - Full privacy via Pedersen commitments + SMT"
echo ""
echo "Key functions:"
echo "  - OpenPositionV1: Deposit collateral, get position commitment"
echo "  - MintStableV1: Mint stablecoins against collateral (150%+ ratio)"
echo "  - RepayStableV1: Repay debt to release collateral"
echo "  - LiquidateV1: Liquidate underwater positions (pool health check)"
echo "  - AccrueInterestV1: Update PI controller and redemption rate"
echo ""
echo "Reference: src/contract/stablecoin/src/entrypoint.rs"