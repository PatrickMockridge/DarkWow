#!/usr/bin/env bash
# L4: relayer_endowment contract — wallet capability verification
set -euo pipefail; SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
CONTRACT="relayer_endowment"
echo "=== L4: $CONTRACT ==="
deploy_contract "$CONTRACT"
CID=$(wal 1 deploy get-contract-id "$CONTRACT" 2>&1 | tail -1)
register_contract 1 "$CONTRACT" "$CID"
echo "  Registering relayer..."; call_contract 1 "$CID" "RegisterRelayer"
wait_for_blocks 1; scan_wallet 1
ACTUAL=$(get_position 1); EXPECTED=$(oracle_fixture "$CONTRACT" "register")
assert_capability_match "$CONTRACT:relayer" "$EXPECTED" "$ACTUAL"
echo "  $CONTRACT: complete"
