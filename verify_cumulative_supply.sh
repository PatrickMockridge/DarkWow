#!/bin/bash
# verify_cumulative_supply.sh — end-to-end cumulative supply audit
#
# Queries the DarkWow node's cumulative supply state via RPC and verifies
# that total supply matches the emission schedule. Full Pedersen commitment
# chain verification is demonstrated by the Python model (sim/tests/).
#
# Usage:
#   ./verify_cumulative_supply.sh [container_name]
#
# Default container: dwow-test-node0

set -euo pipefail

CONTAINER="${1:-dwow-test-node0}"
RPC_PORT="${2:-31345}"
RPC_HOST="${3:-127.0.0.1}"

echo "=== Cumulative Supply Audit ==="
echo "Container: $CONTAINER  Port: $RPC_PORT"

# ---- 1. Query cumulative state from node RPC ----
RPC_REQ='{"jsonrpc":"2.0","method":"blockchain.get_cumulative_supply","params":[],"id":1}'
RESP=$(docker exec "$CONTAINER" bash -c \
    "exec 3<>/dev/tcp/$RPC_HOST/$RPC_PORT;
     echo '$RPC_REQ' >&3;
     timeout 5 cat <&3" 2>/dev/null) || true

if [ -z "$RESP" ]; then
    echo "FAIL: No response from RPC endpoint"
    exit 1
fi

if echo "$RESP" | grep -q '"error"'; then
    echo "FAIL: RPC returned error:"
    echo "$RESP" | grep -o '"message":"[^"]*"'
    exit 1
fi

HEIGHT=$(echo "$RESP" | grep -o '"height":[0-9]*' | grep -o '[0-9]*')
TOTAL_SUPPLY=$(echo "$RESP" | grep -o '"total_supply":[0-9]*' | grep -o '[0-9]*')

if [ -z "$HEIGHT" ] || [ "$HEIGHT" -eq 0 ] 2>/dev/null; then
    echo "FAIL: No blocks mined yet (height=${HEIGHT:-null})"
    echo "Raw response: $RESP"
    exit 1
fi

echo "  Height:       $HEIGHT"
echo "  Total Supply: $TOTAL_SUPPLY"

# ---- 2. Verify total supply against emission schedule ----
EXPECTED=$(python3 -c "
import sys; sys.path.insert(0, '.')
from sim.crypto import expected_cumulative_supply
print(expected_cumulative_supply($HEIGHT))
")

if [ "$TOTAL_SUPPLY" != "$EXPECTED" ]; then
    echo "FAIL: Supply mismatch at height $HEIGHT"
    echo "  Stored:  $TOTAL_SUPPLY"
    echo "  Expected: $EXPECTED"
    exit 1
fi

echo ""
echo "=== PASS: Cumulative supply verified at height $HEIGHT ==="
echo "  Total supply $TOTAL_SUPPLY matches emission schedule"
echo ""
echo "Note: Full Pedersen commitment chain verification (S_H = S_{H-1} + C_H)"
echo "is demonstrated by the Python model. Run:"
echo "  python3 -c 'import sys; sys.path.insert(0,\".\"); from sim.tests.test_cumulative_supply_chain import run_all; run_all()'"
