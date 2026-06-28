#!/bin/bash
# Wallet P2P Wire Compatibility Canary
#
# Verifies the wallet binary can connect to lilith at the wire level.
# Runs in ~30 seconds — catches protocol mismatches, magic byte errors,
# varint framing bugs, and VersionMessage failures before the 2-hour
# Docker pipeline ever runs.
#
# Usage:
#   ./contrib/tests/test_wallet_p2p_compat.sh
#   ./contrib/tests/test_wallet_p2p_compat.sh --network darkwow-testnet
#
# Exit: 0 if wallet connects and gets peers > 0 within timeout.
#       1 if connection fails or diagnostic output recommends.

set -euo pipefail
NETWORK="${1:-darkwow-testnet}"
TIMEOUT="${2:-30}"
SEED="tcp+tls://127.0.0.1:31340"

echo "=== Wallet P2P Wire Canary ==="
echo "  Network: $NETWORK  Seed: $SEED  Timeout: ${TIMEOUT}s"

# Check binary exists
BIN="./target/release/dwow_wallet"
if [ ! -x "$BIN" ]; then
    BIN="./target/debug/dwow_wallet"
fi
if [ ! -x "$BIN" ]; then
    echo "[SKIP] dwow_wallet binary not built — run 'cargo build -p dwow_wallet' first"
    exit 0
fi

# Check lilith is reachable
if ! command -v nc &>/dev/null; then
    echo "[SKIP] nc not available — cannot test seed reachability"
else
    SEED_HOST=$(echo "$SEED" | sed 's|tcp+tls://||;s|:.*||')
    SEED_PORT=$(echo "$SEED" | sed 's|.*:||')
    if ! timeout 3 nc -z "$SEED_HOST" "$SEED_PORT" 2>/dev/null; then
        echo "[SKIP] Seed $SEED not reachable — no network to test against"
        exit 0
    fi
    echo "[PASS] Seed reachable: $SEED_HOST:$SEED_PORT"
fi

# Generate temp config pointing at the seed
TMPDIR=$(mktemp -d)
CONFIG="$TMPDIR/dww_config.toml"
DBDIR="$TMPDIR/data"
mkdir -p "$DBDIR"

cat > "$CONFIG" <<TOML
network = "$NETWORK"
[network_config."$NETWORK"]
chain_path = "$DBDIR/database"
cache_path = "$DBDIR/cache"
wallet_path = "$DBDIR/wallet.db"
wallet_pass = "canary-test"
history_path = "$DBDIR/history.txt"

[network_config."$NETWORK".net]
seeds = [{ url = "$SEED" }]
localnet = true
TOML

echo "  Config written to $CONFIG"

# Initialize wallet
echo "  Initializing wallet..."
"$BIN" --config "$CONFIG" wallet initialize 2>&1 >/dev/null || {
    echo "[FAIL] Wallet initialization failed"
    rm -rf "$TMPDIR"
    exit 1
}

# Start daemon in background
echo "  Starting daemon..."
"$BIN" --config "$CONFIG" daemon &
DAEMON_PID=$!
trap 'kill $DAEMON_PID 2>/dev/null; rm -rf "$TMPDIR"' EXIT

# Poll sync status
echo "  Polling for peers (timeout=${TIMEOUT}s)..."
elapsed=0
peers=0
while [ "$elapsed" -lt "$TIMEOUT" ]; do
    status=$("$BIN" --config "$CONFIG" sync status 2>&1 || true)
    peers=$(echo "$status" | grep -oP 'Peers: \K\d+' || echo 0)
    [ "$peers" -gt 0 ] && break
    sleep 2
    elapsed=$((elapsed + 2))
done

if [ "$peers" -gt 0 ]; then
    echo "[PASS] Wallet connected — $peers peer(s) within ${elapsed}s"
    kill $DAEMON_PID 2>/dev/null || true
    rm -rf "$TMPDIR"
    exit 0
else
    echo "[FAIL] Wallet got 0 peers after ${TIMEOUT}s"
    echo "  Last sync status:"
    "$BIN" --config "$CONFIG" sync status 2>&1 || true
    echo "  Diagnostic:"
    "$BIN" --config "$CONFIG" diagnostic 2>&1 || true
    kill $DAEMON_PID 2>/dev/null || true
    rm -rf "$TMPDIR"
    exit 1
fi
