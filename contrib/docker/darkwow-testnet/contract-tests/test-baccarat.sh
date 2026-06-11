#!/usr/bin/env bash
set -euo pipefail; SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"; source "$SCRIPT_DIR/common.sh"
CONTRACT="baccarat"; echo "=== L4: $CONTRACT ==="
deploy_contract "$CONTRACT"; CID=$(deploy_contract "$CONTRACT" 2>&1 | grep -oP 'Contract ID: \K\S+'); register_contract 1 "$CONTRACT" "$CID"
call_contract 1 "$CID" "cancel"; scan_wallet 1; ACTUAL=$(get_position 1)
EXPECTED=$(oracle_fixture "$CONTRACT" "create"); assert_capability_match "$CONTRACT" "$EXPECTED" "$ACTUAL"
echo "  $CONTRACT: complete"
