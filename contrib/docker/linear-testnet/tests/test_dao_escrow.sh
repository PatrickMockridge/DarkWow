#!/bin/bash
# DAO-Escrow Contract Tests
# Tests InitializeV1, PayPremiumV1, WithdrawV1, EndowmentWithdrawV1, TreasurySpendV1
#
# Usage: ./test_dao_escrow.sh [node_index]
# Default: node_index=0 (wallet0)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$SCRIPT_DIR/../.."
TESTNET_DIR="$SCRIPT_DIR/.."

NODE_INDEX=${1:-0}
RPC_PORT=$((28345 + NODE_INDEX))
RPC_URL="http://localhost:$RPC_PORT"

echo "=== DAO-Escrow Contract Tests (Node $NODE_INDEX) ==="
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

echo ""
echo "[Setup] Wallet addresses:"
echo "  wallet0: $WALLET0_ADDR"
echo "  wallet1: $WALLET1_ADDR"
echo "  wallet2: $WALLET2_ADDR"

# Check balances
echo ""
echo "[Setup] Checking balances..."
BALANCE0=$(get_balance "$WALLET0_ADDR")
echo "  wallet0 balance: $BALANCE0"

if [ "$BALANCE0" -lt 100000000 ]; then
    echo "  WARNING: wallet0 balance low, minting tokens..."
    "$TESTNET_DIR/scripts/mint.sh" 0 1000000000
    BALANCE0=$(get_balance "$WALLET0_ADDR")
    echo "  new balance: $BALANCE0"
fi

# ============================================================
# Test 3.1: Create DAO-Escrow Endowment
# ============================================================

echo ""
echo "=========================================="
echo "[Test 3.1] Initialize DAO-Escrow Endowment"
echo "=========================================="

# Parameters for InitializeV1:
# - dao_bulla: A unique identifier for the DAO
# - owner_pubkey: Public key of the endowment owner
# - endowment_token_id: Token ID for the endowment
# - bulla_blind: Random blind factor for bulla
# - enable_drain_protection: Boolean

# Generate random bulla_blind (placeholder - real implementation needs proper random)
BULLA_BLIND="12345678901234567890"

# Build InitializeV1 call
# Note: This requires ZK proof generation for init_proof
# The actual implementation would use drk to build the proof

echo "[Test 3.1] Preparing InitializeV1 call..."
echo "  owner_pubkey: $WALLET0_ADDR"
echo "  endowment_token_id: NATIVE_TOKEN (placeholder)"
echo "  bulla_blind: $BULLA_BLIND"
echo "  enable_drain_protection: false"

# For testing via RPC (dry run - shows calldata):
echo ""
echo "[Test 3.1] Simulating InitializeV1 (dry run)..."
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
                "bulla_blind": "'"$BULLA_BLIND"'",
                "enable_drain_protection": false
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 3.2: Member Pays Premium
# ============================================================

echo ""
echo "=========================================="
echo "[Test 3.2] Pay Premium (Membership)"
echo "=========================================="

# Parameters for PayPremiumV1:
# - dao_escrow_bulla: The endowment's bulla
# - membership_note: Commitment to membership
# - value_commit: Pedersen commitment to value
# - value: Amount being paid
# - token_id: Token being used
# - expiry: Membership expiry block
# - membership_blind: Random blind for membership note
# - value_blind: Random blind for value commitment
# - member_pubkey: Member's public key

echo "[Test 3.2] Preparing PayPremiumV1 call..."
echo "  dao_escrow_bulla: <from Test 3.1>"
echo "  member_pubkey: $WALLET1_ADDR"
echo "  value: 100000000 (1 token)"

# Simulate pay premium
RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dao_escrow",
            "function": "PayPremiumV1",
            "params": {
                "dao_escrow_bulla": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "membership_note": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "value_commit": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "value": 100000000,
                "token_id": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "expiry": 1000000,
                "membership_blind": "1234567890",
                "value_blind": "1234567890",
                "member_pubkey": "'"$WALLET1_ADDR"'"
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 3.3: Withdraw from Endowment
# ============================================================

echo ""
echo "=========================================="
echo "[Test 3.3] Withdraw from Endowment"
echo "=========================================="

# Parameters for WithdrawV1:
# - dao_escrow_bulla: The endowment's bulla
# - value: Amount to withdraw
# - recipient_pubkey: Recipient's public key
# Note: REQUIRES money_v3::transfer_v1 child call

echo "[Test 3.3] Preparing WithdrawV1 call..."
echo "  dao_escrow_bulla: <from Test 3.1>"
echo "  value: 50000000 (0.5 token)"
echo "  recipient_pubkey: $WALLET0_ADDR"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dao_escrow",
            "function": "WithdrawV1",
            "params": {
                "dao_escrow_bulla": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "value": 50000000,
                "recipient_pubkey": "'"$WALLET0_ADDR"'"
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 3.4: Endowment Withdraw (DAO Vote Required)
# ============================================================

echo ""
echo "=========================================="
echo "[Test 3.4] Endowment Withdraw (requires DAO vote)"
echo "=========================================="

# Parameters for EndowmentWithdrawV1:
# - dao_escrow_bulla: The endowment's bulla
# - claim_id: Approved claim identifier
# - recipient_pubkey: Recipient's public key
# - value: Amount to withdraw
# Note: REQUIRES money_v3::transfer_v1 child call

echo "[Test 3.4] Preparing EndowmentWithdrawV1 call..."
echo "  dao_escrow_bulla: <from Test 3.1>"
echo "  claim_id: <approved claim>"
echo "  value: 100000000"
echo "  recipient_pubkey: $WALLET2_ADDR"

RESULT=$(curl -s -X POST "$RPC_URL" -H "Content-Type: application/json" \
    -d '{
        "jsonrpc": "2.0",
        "method": "contract.invoke",
        "params": {
            "contract_id": "dao_escrow",
            "function": "EndowmentWithdrawV1",
            "params": {
                "dao_escrow_bulla": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "claim_id": "DZnsGMCvZU5CEzvpuExnxbvz6SEhE2rn89sMcuHsppFE6TjL4SBTrKkf",
                "recipient_pubkey": "'"$WALLET2_ADDR"'",
                "value": 100000000
            },
            "dry_run": true
        },
        "id": 1
    }')

echo "  Result: $RESULT"

# ============================================================
# Test 3.5: Treasury Spend
# ============================================================

echo ""
echo "=========================================="
echo "[Test 3.5] Treasury Spend (Treasury mode only)"
echo "=========================================="

# Parameters for TreasurySpendV1:
# - dao_escrow_bulla: The endowment's bulla (must be Treasury/TreasuryEndowment mode)
# - proposal_id: Approved proposal identifier
# - recipient_pubkey: Recipient's public key
# - value: Amount to spend
# Note: REQUIRES money_v3::transfer_v1 child call

echo "[Test 3.5] Preparing TreasurySpendV1 call..."
echo "  dao_escrow_bulla: <Treasury mode endowment>"
echo "  proposal_id: <approved proposal>"
echo "  value: 50000000"
echo "  recipient_pubkey: $WALLET1_ADDR"

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

# ============================================================
# Test Summary
# ============================================================

echo ""
echo "=========================================="
echo "=== DAO-Escrow Test Summary ==="
echo "=========================================="
echo ""
echo "Tests executed with dry_run=true (simulated)"
echo "Full testing requires:"
echo "  1. ZK proof generation for init_zk and premium_zk"
echo "  2. Transaction broadcast via drk wallet"
echo "  3. Block mining to confirm transactions"
echo ""
echo "Next steps:"
echo "  1. Deploy contract: ./setup.sh"
echo "  2. Run full test: RPC calls with actual ZK proofs"
echo "  3. Verify state: blockchain.get_contract_state"
echo ""
echo "Reference: src/contract/dao_escrow/src/entrypoint.rs"
echo "  - InitializeV1: Creates new endowment (requires init_zk proof)"
echo "  - PayPremiumV1: Member joins with premium (requires premium_zk proof)"
echo "  - WithdrawV1: Owner withdraws (requires money::transfer child call)"
echo "  - EndowmentWithdrawV1: DAO-approved claim execution"
echo "  - TreasurySpendV1: DAO treasury spending (Treasury mode)"
echo ""