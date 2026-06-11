#!/usr/bin/env bash
# L4: otc_swap contract — wallet capability verification
set -euo pipefail; SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
CONTRACT="otc_swap"
echo "=== L4: $CONTRACT ==="
deploy_contract "$CONTRACT"
CID=$(wal 1 deploy get-contract-id "$CONTRACT" 2>&1 | tail -1)
register_contract 1 "$CONTRACT" "$CID"
echo "  Initiating swap..."; call_contract 1 "$CID" "InitiateSwap" --token-x DRKW --token-y DRKW --amount-x 100 --amount-y 100
wait_for_blocks 1; scan_wallet 1
ACTUAL=$(get_position 1); EXPECTED=$(oracle_fixture "$CONTRACT" "initiate")
assert_capability_match "$CONTRACT:proposer" "$EXPECTED" "$ACTUAL"
echo "  $CONTRACT: complete"
