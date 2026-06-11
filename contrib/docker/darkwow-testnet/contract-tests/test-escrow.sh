#!/usr/bin/env bash
# L4: escrow contract — wallet capability verification
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"

CONTRACT="escrow"
echo "=== L4: $CONTRACT ==="

# Phase 1: Deploy
deploy_contract "$CONTRACT"
# deploy_contract prints the CID; capture it
CID=$(deploy_contract "$CONTRACT" 2>&1 | grep -oP 'Contract ID: \K\S+')
register_contract 1 "$CONTRACT" "$CID"

# Phase 2: Call cancel (non-ZK, parameterless — works universally)
echo "  Calling cancel on escrow $CID..."
call_contract 1 "$CID" "cancel"

# Phase 3: Wait
wait_for_blocks 1

# Phase 4: Scan
scan_wallet 1

# Phase 5: Position
ACTUAL=$(get_position 1)

# Phase 6: Verify
EXPECTED=$(oracle_fixture "$CONTRACT" "create")
assert_capability_match "$CONTRACT:cancel" "$EXPECTED" "$ACTUAL"

echo "  $CONTRACT: complete"
