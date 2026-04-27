#!/bin/bash
# Cross-Contract Integration Tests
# Tests interactions between dao_escrow, stablecoin, and dex contracts
#
# Usage: ./test_cross.sh [node_index]
# Default: node_index=3 (wallet3)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../.."
TESTNET_DIR="$SCRIPT_DIR/.."

NODE_INDEX=${1:-3}
RPC_PORT=$((28345 + NODE_INDEX))
RPC_URL="http://localhost:$RPC_PORT"

echo "=== Cross-Contract Integration Tests (Node $NODE_INDEX) ==="
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

# Mint tokens to all wallets if needed
echo ""
echo "[Setup] Ensuring all wallets have tokens..."
for i in 0 1 2 3 4; do
    BALANCE=$(get_balance "$(get_wallet_addr $i)")
    if [ "$BALANCE" -lt 500000000 ]; then
        echo "  Minting to wallet$i..."
        "$TESTNET_DIR/scripts/mint.sh" $i 1000000000
    else
        echo "  wallet$i: $BALANCE (OK)"
    fi
done

# ============================================================
# Test 6.1: DAO-Escrow uses Stablecoin for Premium Payment
# ============================================================

echo ""
echo "=========================================="
echo "[Test 6.1] DAO-Escrow Premium Payment in Stablecoin"
echo "=========================================="

# Scenario: A DAO-Escrow accepts stablecoin as premium token
# Members pay premiums in stablecoin instead of native token
#
# Flow:
# 1. Deploy stablecoin contract
# 2. Create DAO-Escrow endowment with stablecoin as premium token
# 3. Member deposits collateral into stablecoin (opens CDP)
# 4. Member mints stablecoins
# 5. Member pays premium to DAO-Escrow in stablecoins

echo "[Test 6.1] Simulating cross-contract flow..."
echo "  1. Deploy stablecoin (if not already deployed)"
echo "  2. Create DAO-Escrow with stablecoin premium token"
echo "  3. Member opens stablecoin position"
echo "  4. Member mints stablecoins"
echo "  5. Member pays DAO-Escrow premium in stablecoin"

# Step 1: Check stablecoin is available
echo ""
echo "[Test 6.1 Step 1] Checking stablecoin contract..."
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
                "proof": "placeholder",
                "zk_public_inputs": ["DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf"]
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# Step 2: Create DAO-Escrow with stablecoin premium
echo ""
echo "[Test 6.1 Step 2] Creating DAO-Escrow with stablecoin premium..."

# Note: In real implementation, would set endowment_token_id to stablecoin's token ID
RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dao_escrow",
            "function": "InitializeV1",
            "params": {
                "dao_bulla": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "owner_pubkey": "'"$WALLET0_ADDR"'",
                "endowment_token_id": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "bulla_blind": "12345678901234567890",
                "enable_drain_protection": false
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

echo ""
echo "  NOTE: Full cross-contract flow requires:"
echo "    - Deployooor deployment of both contracts to same network"
echo "    - ZK proof coordination between contracts"
echo "    - Shared token/merkle state verification"

# ============================================================
# Test 6.2: DEX trades DAO-Token / Stablecoin
# ============================================================

echo ""
echo "=========================================="
echo "[Test 6.2] DEX Atomic Swap for DAO-Token / Stablecoin"
echo "=========================================="

# Scenario: Trade DAO governance tokens or stablecoin via DEX
#
# Flow:
# 1. Alice creates swap: offers DAO tokens, wants stablecoin
# 2. Bob accepts swap with stablecoin
# 3. Execute atomic swap

echo "[Test 6.2] Simulating DEX swap for DAO-Token / Stablecoin..."
echo "  Alice (wallet0): offers 100 DAO governance tokens"
echo "  Bob (wallet1): offers 100 stablecoins"
echo "  Both locks funds, either can execute atomically"

# Create swap
echo ""
echo "[Test 6.2 Step 1] Alice creates swap (DAO token <-> stablecoin)..."
RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "CreateSwapV1",
            "params": {
                "swap_id": "1111111111abcdef1111111111abcdef1111111111abcdef1111111111abcdef",
                "offer_token": "dao_governance_token_id_placeholder",
                "offer_amount": 100000000,
                "request_token": "stablecoin_token_id_placeholder",
                "request_amount": 100000000,
                "signature": "placeholder",
                "alice_lock": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "expires_at": 1000000,
                "open_execution": false
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# Accept swap
echo ""
echo "[Test 6.2 Step 2] Bob accepts swap..."
RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "AcceptSwapV1",
            "params": {
                "swap_id": "1111111111abcdef1111111111abcdef1111111111abcdef1111111111abcdef",
                "bob_lock": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "signature": "placeholder"
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# Execute swap
echo ""
echo "[Test 6.2 Step 3] Execute atomic swap..."
RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dex",
            "function": "ExecuteSwapV1",
            "params": {
                "swap_id": "1111111111abcdef1111111111abcdef1111111111abcdef1111111111abcdef",
                "alice_secret": "alice_secret",
                "bob_secret": "bob_secret"
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 6.3: DAO-Escrow Treasury Spend via Multisig
# ============================================================

echo ""
echo "=========================================="
echo "[Test 6.3] DAO-Escrow Treasury Spend"
echo "=========================================="

# Scenario: DAO treasury sends funds to multiple recipients
# Requires: Treasury mode DAO-Escrow + DAO vote approval
#
# Flow:
# 1. Create Treasury-mode DAO-Escrow
# 2. Fund treasury (members pay fees)
# 3. Propose treasury spend
# 4. DAO vote (approve/reject)
# 5. Execute treasury spend

echo "[Test 6.3] Treasury spend flow..."
echo "  NOTE: TreasurySpendV1 requires Treasury or TreasuryEndowment mode"
echo "  Also requires money_v3::transfer_v1 child call for actual transfer"

# Check if endowment is in treasury mode
echo ""
echo "[Test 6.3 Step 1] Checking DAO-Escrow treasury mode..."
RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dao_escrow",
            "function": "TreasurySpendV1",
            "params": {
                "dao_escrow_bulla": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "proposal_id": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "recipient_pubkey": "'"$WALLET1_ADDR"'",
                "value": 50000000
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

echo ""
echo "  NOTE: Treasury spend requires:"
echo "    - DAO-Escrow in Treasury or TreasuryEndowment mode"
echo "    - Proposal approved via DAO vote"
echo "    - money_v3::transfer_v1 child call bundled"

# ============================================================
# Test Summary
# ============================================================

echo ""
echo "=========================================="
echo "=== Cross-Contract Test Summary ==="
echo "=========================================="
echo ""
echo "Tests executed with dry_run=true (simulated)"
echo ""
echo "Cross-Contract Patterns Demonstrated:"
echo ""
echo "1. Stablecoin Premium (6.1):"
echo "   - DAO-Escrow accepts stablecoin as endowment token"
echo "   - Members use CDP positions to acquire stablecoins"
echo "   - Premiums paid in stablecoin instead of native token"
echo ""
echo "2. DEX Token Swap (6.2):"
echo "   - Atomic swap between DAO governance token and stablecoin"
echo "   - Uses money::lock for fund locking in DEX"
echo "   - ZK proofs verify lock reveal for execution"
echo ""
echo "3. Treasury Spend (6.3):"
echo "   - DAO-Escrow in Treasury mode controls funds"
echo "   - Proposal -> Vote -> Execute flow"
echo "   - money::transfer child call for actual disbursement"
echo ""
echo "Contract Dependencies:"
echo "  - dao_escrow: needs money for transfers"
echo "  - stablecoin: needs money for mint/burn"
echo "  - dex: needs money for lock/unlock"
echo ""
echo "Shared Infrastructure:"
echo "  - Merkle trees for commitment verification"
echo "  - Intent nullifiers for state tracking"
echo "  - ZK proof coordination for privacy"
echo ""
echo "Full testing requires:"
echo "  1. Deploy all contracts via Deployooor"
echo "  2. Initialize contract states"
echo "  3. ZK proof generation for all operations"
echo "  4. Transaction coordination across contracts"
echo "  5. Block mining to finalize state changes"