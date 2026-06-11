#!/usr/bin/env bash
# L4: bearer_bond contract — wallet capability verification
set -euo pipefail; SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
CONTRACT="bearer_bond"
echo "=== L4: $CONTRACT ==="
deploy_contract "$CONTRACT"
CID=$(wal 1 deploy get-contract-id "$CONTRACT" 2>&1 | tail -1)
register_contract 1 "$CONTRACT" "$CID"
echo "  Issuing stake..."; call_contract 1 "$CID" "IssueStake" --principal 10000 --maturity 1000 --rate 500
wait_for_blocks 1; scan_wallet 1
ACTUAL=$(get_position 1); EXPECTED=$(oracle_fixture "$CONTRACT" "issue")
assert_capability_match "$CONTRACT:holder" "$EXPECTED" "$ACTUAL"
echo "  $CONTRACT: complete"
