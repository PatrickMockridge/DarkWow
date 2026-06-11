#!/usr/bin/env bash
# L4: auction contract — wallet capability verification
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

CONTRACT="auction"
echo "=== L4: $CONTRACT ==="

# Phase 1: Deploy
deploy_contract "$CONTRACT"
CID=$(wal 1 deploy get-contract-id "$CONTRACT" 2>&1 | tail -1)
register_contract 1 "$CONTRACT" "$CID"
register_contract 2 "$CONTRACT" "$CID"

# Phase 2a: Create auction (wallet-1 = seller)
echo "  Creating auction..."
call_contract 1 "$CID" "CreateAuction" --start-price 1000 --description "Test item"

# Phase 2b: Place bid (wallet-2 = bidder)
echo "  Placing bid..."
call_contract 2 "$CID" "PlaceBid" --auction-id "0x01" --amount 1500

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
assert_capability_match "$CONTRACT:wallet1_seller" "$EXPECTED" "$ACTUAL1"
echo "  $CONTRACT: complete"
