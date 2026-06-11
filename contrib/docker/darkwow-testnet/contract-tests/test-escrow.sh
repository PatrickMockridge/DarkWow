#!/usr/bin/env bash
# L4: escrow contract — wallet capability verification
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

CONTRACT="escrow"
echo "=== L4: $CONTRACT ==="

# Phase 1: Deploy
deploy_contract "$CONTRACT"
CID=$(wal 1 deploy get-contract-id "$CONTRACT" 2>&1 | tail -1)
register_contract 1 "$CONTRACT" "$CID"

# Phase 2: Create escrow with wallet-1 as buyer, wallet-2 as seller
SELLER_PK=$(get_address 2 2>&1 | tail -1)
echo "  Creating escrow: buyer=wallet-1, seller=wallet-2"
call_contract 1 "$CID" "CreateEscrow" --value 1000 --seller "$SELLER_PK"

# Phase 3: Wait
wait_for_blocks 1

# Phase 4: Scan
scan_wallet 1
scan_wallet 2

# Phase 5: Position
ACTUAL1=$(get_position 1)
ACTUAL2=$(get_position 2)

# Phase 6: Verify
EXPECTED=$(oracle_fixture "$CONTRACT" "create")
assert_capability_match "$CONTRACT:wallet1_buyer_created" "$EXPECTED" "$ACTUAL1"

echo "  $CONTRACT: complete"
