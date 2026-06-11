#!/usr/bin/env bash
# L4: dao_escrow contract — wallet capability verification
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

CONTRACT="dao_escrow"
echo "=== L4: $CONTRACT ==="

# Phase 1: Deploy
deploy_contract "$CONTRACT"
CID=$(wal 1 deploy get-contract-id "$CONTRACT" 2>&1 | tail -1)
register_contract 1 "$CONTRACT" "$CID"

# Phase 2a: Initialize DAO (wallet-1 = owner)
echo "  Initializing DAO..."
call_contract 1 "$CID" "Initialize" --premium-amount 500

# Phase 2b: Pay premium
echo "  Paying premium..."
call_contract 1 "$CID" "PayPremium"

# Phase 3: Wait
wait_for_blocks 1

# Phase 4: Scan
scan_wallet 1

# Phase 5: Position
ACTUAL=$(get_position 1)

# Phase 6: Verify
EXPECTED=$(oracle_fixture "$CONTRACT" "initialize")
assert_capability_match "$CONTRACT:owner" "$EXPECTED" "$ACTUAL"
echo "  $CONTRACT: complete"
