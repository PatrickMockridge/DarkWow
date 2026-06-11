#!/usr/bin/env bash
# L4: subscription contract — wallet capability verification
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

CONTRACT="subscription"
echo "=== L4: $CONTRACT ==="

# Phase 1: Deploy
deploy_contract "$CONTRACT"
CID=$(wal 1 deploy get-contract-id "$CONTRACT" 2>&1 | tail -1)
register_contract 1 "$CONTRACT" "$CID"

# Phase 2: Subscribe (wallet-1 = subscriber)
echo "  Subscribing to plan 1..."
call_contract 1 "$CID" "Subscribe" --plan-id 1 --payment-amount 500

# Phase 3: Wait
wait_for_blocks 1

# Phase 4: Scan
scan_wallet 1

# Phase 5: Position
ACTUAL=$(get_position 1)

# Phase 6: Verify
EXPECTED=$(oracle_fixture "$CONTRACT" "subscribe")
assert_capability_match "$CONTRACT:subscriber" "$EXPECTED" "$ACTUAL"
echo "  $CONTRACT: complete"
