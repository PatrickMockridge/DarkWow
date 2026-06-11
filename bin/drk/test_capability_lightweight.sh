#!/bin/bash
# DarkWow — Wallet Capability Resolution Lightweight Tests (Level 1)
#
# Tests CLI integration of `dwow_wallet position` — the capability-based position
# resolver exposed through the wallet binary.
#
# Level 1 scope (no ZK proofs, no Docker):
#   - Binary compilation and subcommand existence
#   - CLI error handling (missing config, missing node, corrupt wallet)
#   - Basic end-to-end: start dwowd, mine blocks, scan, verify position output
#
# Run: RAYON_NUM_THREADS=10 bash bin/drk/test_capability_lightweight.sh

set -e
set -E
trap 'echo "[FATAL] Test failed at line $LINENO — exit code $?" >&2' ERR
trap 'cleanup' EXIT

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TMPDIR="${TMPDIR:-/tmp}/dww_capability_test_$$"
DWW="${REPO_ROOT}/target/debug/dwow_wallet"
DWOWD="${REPO_ROOT}/target/debug/dwowd"
RPC_PORT=38345
NETWORK="linear-testnet"
CONFIG="${TMPDIR}/drk.toml"
DWOWD_CONFIG="${REPO_ROOT}/bin/dwowd/dwowd_config.toml"

PASS=0
FAIL=0
DWOWD_PID=""

cleanup() {
    if [ -n "$DWOWD_PID" ] && kill -0 "$DWOWD_PID" 2>/dev/null; then
        kill "$DWOWD_PID" 2>/dev/null || true
        wait "$DWOWD_PID" 2>/dev/null || true
    fi
    rm -rf "$TMPDIR"
}

assert_contains() {
    local label="$1" output="$2" pattern="$3"
    if echo "$output" | grep -q "$pattern"; then
        echo "  PASS: $label"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $label — pattern '$pattern' not found in output"
        echo "  Output was: $output"
        FAIL=$((FAIL + 1))
    fi
}

assert_not_contains() {
    local label="$1" output="$2" pattern="$3"
    if echo "$output" | grep -qv "$pattern"; then
        echo "  PASS: $label"
        PASS=$((PASS + 1))
    else
        echo "  FAIL: $label — pattern '$pattern' found but should not be"
        FAIL=$((FAIL + 1))
    fi
}

echo "=== DarkWow Wallet Capability Lightweight Tests ==="
echo ""

# ── Build ────────────────────────────────────────────────────────────
echo "--- Build ---"
cd "$REPO_ROOT"
RAYON_NUM_THREADS=10 cargo build -p dwow_wallet -p dwowd 2>&1 | tail -3
echo ""

# ── Test 1: Subcommand existence ─────────────────────────────────────
echo "--- Test 1: Subcommand existence ---"
HELP_OUT=$("$DWW" --help 2>&1 || true)
assert_contains "position subcommand listed" "$HELP_OUT" "position"
assert_contains "help lists 'Show'" "$HELP_OUT" "Show"

# ── Test 2: Missing config file ──────────────────────────────────────
echo "--- Test 2: Missing config file ---"
MISSING_OUT=$("$DWW" -c /nonexistent/path/drk.toml position 2>&1 || true)
assert_contains "missing config produces error" "$MISSING_OUT" "Error"

# ── Test 3: Corrupt/invalid config ───────────────────────────────────
echo "--- Test 3: Corrupt config file ---"
mkdir -p "$TMPDIR"
echo "this is not valid toml {{{" > "$TMPDIR/corrupt.toml"
CORRUPT_OUT=$("$DWW" -c "$TMPDIR/corrupt.toml" position 2>&1 || true)
assert_contains "corrupt config produces error" "$CORRUPT_OUT" "Error"

# ── Test 4: No running node ──────────────────────────────────────────
echo "--- Test 4: No running node ---"
# Create a minimal valid config pointing to a port where nothing listens
cat > "$CONFIG" <<'TOML'
network = "linear-testnet"
cache_path = "/tmp/dww_cap_test_cache"
wallet_path = "/tmp/dww_cap_test_wallet"
wallet_pass = "testpass"
[network_config."linear-testnet"]
rpc_url = "http://127.0.0.1:19999"
TOML
NO_NODE_OUT=$("$DWW" -c "$CONFIG" position 2>&1 || true)
assert_contains "no node produces connection error" "$NO_NODE_OUT" "Error"

# ── Test 5: End-to-end — start node, mine, scan, position ────────────
echo "--- Test 5: End-to-end position resolution ---"

# Start dwowd in linear-testnet mode with a unique RPC port
echo "  Starting dwowd..."
"$DWOWD" -c "$DWOWD_CONFIG" -n "$NETWORK" --rpc-port "$RPC_PORT" &
DWOWD_PID=$!
sleep 3

if ! kill -0 "$DWOWD_PID" 2>/dev/null; then
    echo "  FAIL: dwowd failed to start"
    FAIL=$((FAIL + 1))
else
    echo "  dwowd running (PID $DWOWD_PID)"

    # Wait for RPC to respond
    for i in $(seq 1 10); do
        if curl -s -X POST "http://127.0.0.1:$RPC_PORT" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' 2>/dev/null | grep -q "pong"; then
            echo "  RPC responding"
            break
        fi
        sleep 2
    done

    # Create wallet config for this node
    cat > "$CONFIG" <<TOML
network = "linear-testnet"
cache_path = "$TMPDIR/cache"
wallet_path = "$TMPDIR/wallet.db"
wallet_pass = "testpass"
[network_config."linear-testnet"]
rpc_url = "http://127.0.0.1:$RPC_PORT"
TOML

    # Generate keypair and get address
    echo "  Generating keypair..."
    KEYPAIR_OUT=$("$DWW" -c "$CONFIG" wallet keygen 2>&1 || true)
    echo "  keygen output: $KEYPAIR_OUT"

    # Get wallet address for mining
    ADDR=$("$DWW" -c "$CONFIG" wallet address 2>&1 | head -1 || echo "")
    if [ -z "$ADDR" ]; then
        ADDR="4Rwqa7syEBV3BtP2DrJvQKxE2vXmPNbxqLB3PkMXMRX8"
    fi
    echo "  Wallet address: $ADDR"

    # Mine a few blocks to generate coinbase coins
    echo "  Mining blocks..."
    for i in $(seq 1 3); do
        RESULT=$(curl -s -X POST "http://127.0.0.1:$RPC_PORT" \
            -H "Content-Type: application/json" \
            -d "{\"jsonrpc\":\"2.0\",\"method\":\"miner.mine_linear\",\"params\":[\"$ADDR\",100000000],\"id\":$i}" 2>/dev/null || true)
        echo "  Block $i: $RESULT"
        sleep 1
    done

    # Initialize wallet and scan blocks
    echo "  Initializing wallet..."
    "$DWW" -c "$CONFIG" wallet init 2>&1 || true

    echo "  Scanning blocks..."
    "$DWW" -c "$CONFIG" scan 2>&1 || true

    # Run position resolution
    echo "  Running position resolution..."
    POS_OUT=$("$DWW" -c "$CONFIG" position 2>&1 || true)
    echo "  Output:"
    echo "$POS_OUT"

    # Verify output contains expected sections
    assert_contains "position shows capabilities" "$POS_OUT" "Capabilities"
    assert_contains "position shows actions section" "$POS_OUT" "Actions"
    assert_contains "position shows descriptors loaded" "$POS_OUT" "Descriptors loaded"

    # Verify coin capabilities surfaced (redesign: Path 1 + Path 2)
    assert_contains "position shows Coin worth" "$POS_OUT" "Coin worth"

    # Verify capability count is non-zero
    CAP_COUNT=$(echo "$POS_OUT" | grep -c "Coin worth" || true)
    if [ "$CAP_COUNT" -gt 0 ]; then
        echo "  [PASS] position shows $CAP_COUNT coin capabilities"
        PASS=$((PASS + 1))
    else
        echo "  [FAIL] position shows zero coin capabilities"
        FAIL=$((FAIL + 1))
    fi

    # Verify mined value appears in output
    assert_contains "mining reward value in position" "$POS_OUT" "100000000"

    # Kill dwowd
    kill "$DWOWD_PID" 2>/dev/null || true
    wait "$DWOWD_PID" 2>/dev/null || true
    DWOWD_PID=""
fi

# ── Report ────────────────────────────────────────────────────────────
echo ""
echo "=== Results ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
if [ "$FAIL" -gt 0 ]; then
    echo "FAILURE"
    exit 1
else
    echo "SUCCESS"
fi
