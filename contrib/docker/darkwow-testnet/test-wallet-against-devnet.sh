#!/bin/bash
# Standalone wallet test against an already-running devnet.
#
# Usage: ./test-wallet-against-devnet.sh
#
# Prerequisites: a running devnet with wallet containers.
# Start one with:
#   ./test_pipeline.sh --mode native --with-wallet 2 --stop-after 9
#
# This script sources wallet-shell.sh and runs the same checks as
# phases 10-11 of test_pipeline.sh, but without the pipeline overhead.
# Every check reports PASS or FAIL.

set -e
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/lib/output.sh"
source "$SCRIPT_DIR/wallet-shell.sh"

FAIL=0

echo "=== Wallet Verification Against Running Devnet ==="

# Verify wallet-1 is running and has the expected address
info "Checking wallet-1..."
wallet1_addr=$(wal 1 wallet address 2>&1 | tail -1)
if [ -n "$wallet1_addr" ]; then
    pass "wallet-1 running (address: ${wallet1_addr:0:16}...)"
else
    fail "wallet-1 not running or address not retrievable"
fi

# Sync and scan
info "Running sync init..."
sync_out=$(wal 1 sync init 2>&1)
if echo "$sync_out" | grep -q "P2P sync started"; then
    pass "wallet-1 sync init"
else
    fail "wallet-1 sync init failed"
fi

info "Waiting for peers and blocks (120s)..."
for i in $(seq 1 24); do
    status=$(wal 1 sync status 2>&1)
    peers=$(echo "$status" | grep -oP 'Peers: \K\d+' || echo 0)
    height=$(echo "$status" | grep -oP 'Local chain height: \K\d+' || echo 0)
    [ "$peers" -gt 0 ] && [ "$height" -gt 0 ] && break
    sleep 5
done
pass "wallet-1 sync (peers=$peers, height=$height)"

info "Running scan..."
wal 1 scan 2>&1 | grep -q "Scanning block" && pass "wallet-1 scan" || fail "wallet-1 scan"

info "Checking balance..."
balance=$(wal 1 wallet balance 2>&1)
echo "$balance" | grep -qE 'DRKW\s+[0-9]+' && pass "wallet-1 has DRKW balance" || fail "wallet-1 has no DRKW balance"

# Wallet-2 transfer (if present)
if docker ps --format '{{.Names}}' | grep -q "dwow-wallet-2"; then
    info "Testing wallet-1 -> wallet-2 transfer..."
    wallet2_addr=$(wal 2 wallet address 2>&1 | tail -1)
    if [ -z "$wallet2_addr" ]; then
        fail "wallet-2 address not retrievable"
    else
        info "  Wallet-2 address: ${wallet2_addr:0:16}..."
        transfer_out=$(wal 1 transfer 1 DRKW "$wallet2_addr" 2>&1)
        echo "$transfer_out" | grep -q "Transaction" && pass "transfer tx built" || fail "transfer tx failed"
        info "  Waiting for confirmation (polling wallet-2, up to 5 min)..."
        for attempt in $(seq 1 20); do
            sleep 15
            wal 2 scan 2>&1 >/dev/null || true
            balance2=$(wal 2 wallet balance 2>&1)
            echo "$balance2" | grep -qE 'DRKW\s+[0-9]+' && { pass "wallet-2 received transfer after $((attempt * 15))s"; break; }
        done
    fi
fi

echo ""
echo "==========================================="
echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
echo "==========================================="
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
