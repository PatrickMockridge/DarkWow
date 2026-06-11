#!/usr/bin/env bash
# L4: game_room contract — wallet capability verification
set -euo pipefail; SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/common.sh"
CONTRACT="game_room"
echo "=== L4: $CONTRACT ==="
deploy_contract "$CONTRACT"
CID=$(wal 1 deploy get-contract-id "$CONTRACT" 2>&1 | tail -1)
register_contract 1 "$CONTRACT" "$CID"
echo "  Creating room..."; call_contract 1 "$CID" "CreateRoom"
wait_for_blocks 1; scan_wallet 1
ACTUAL=$(get_position 1); EXPECTED=$(oracle_fixture "$CONTRACT" "create")
assert_capability_match "$CONTRACT:host" "$EXPECTED" "$ACTUAL"
echo "  $CONTRACT: complete"
