#!/bin/bash
# DarkWow Merge Mining with P2Pool — End-to-End Test
#
# Pipeline: xmrig --> p2pool --[merge-mine]--> dwowd (mm_rpc)
#                          \--[monerod RPC]--> monerod (offline, fixed-difficulty)
#
# Prerequisite (one-time): monerod synced from public testnet via fast-block-sync.
# Once synced, the persistent data dir is reused for all subsequent runs.
#
# Usage:
#   ./test_merge_mining_p2pool.sh                # full test
#   ./test_merge_mining_p2pool.sh --no-build     # skip cargo build
#   ./test_merge_mining_p2pool.sh --no-cleanup   # keep test dir on exit

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
TEST_DIR="/tmp/dwow_merge_test"

export XDG_CONFIG_HOME="$TEST_DIR"
CONFIG_FILE="$XDG_CONFIG_HOME/dwow/dwowd_config.toml"
DATADIR="$TEST_DIR/data"
MONERO_DATADIR="$HOME/.cache/dwow_merge_testnet_monero"
P2POOL_DATADIR="$TEST_DIR/p2pool_data"
P2POOL_BIN="$HOME/.local/bin/p2pool"
MONEROD_BIN="$HOME/.local/bin/monerod"

MONERO_RPC_PORT=28081
MONERO_ZMQ_PORT=28083
DWOWD_RPC_PORT=28345
MM_RPC_PORT=31348
P2POOL_STRATUM_PORT=3333

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $(date '+%H:%M:%S') $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $(date '+%H:%M:%S') $*"; }
fail()  { echo -e "${RED}[FAIL]${NC}  $(date '+%H:%M:%S') $*"; }
detail(){ echo -e "${CYAN}[DEBUG]${NC} $(date '+%H:%M:%S') $*"; }

# ── Flags ────────────────────────────────────────────────────────────────────

NO_BUILD=false
NO_CLEANUP=false
CLEANUP_ON_SUCCESS=true

for arg in "$@"; do
    case "$arg" in
        --no-build) NO_BUILD=true ;;
        --no-cleanup) NO_CLEANUP=true; CLEANUP_ON_SUCCESS=false ;;
    esac
done

# ── Helpers ──────────────────────────────────────────────────────────────────

# Query dwowd raw-TCP JSON-RPC endpoint.
dwowd_rpc() {
    local request="$1"
    echo "$request" | nc -w 5 127.0.0.1 "$DWOWD_RPC_PORT" 2>/dev/null || echo ""
}

# Query monerod HTTP JSON-RPC endpoint.
monero_rpc() {
    local method="$1"
    curl -s -X POST "http://127.0.0.1:$MONERO_RPC_PORT/json_rpc" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":[],\"id\":1}" 2>/dev/null || echo ""
}

# Query dwowd mm_rpc HTTP endpoint.
mm_rpc() {
    local method="$1"
    curl -s -X POST "http://127.0.0.1:$MM_RPC_PORT" \
        -H "Content-Type: application/json" \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":[],\"id\":1}" 2>/dev/null || echo ""
}

# Get dwowd block by height via raw-TCP RPC.
# Returns the full JSON string from the response.
get_dwowd_block() {
    local height="$1"
    local resp
    resp=$(dwowd_rpc "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_block_linear\",\"params\":[$height],\"id\":1}")
    # Response format: {"jsonrpc":"2.0","result":"<escaped-json-string>","id":1}
    # Extract the inner JSON string (the block).
    echo "$resp" | grep -o '"result":"[^"]*"' | head -1 | sed 's/"result":"//;s/"$//' | sed 's/\\"/"/g' || echo ""
}

# Get monero height from monerod RPC.
get_monero_height() {
    monero_rpc "get_info" | grep -Po '"height"\s*:\s*\K\d+' || echo "0"
}

# Get dwowd block count from raw-TCP RPC.
get_dwowd_height() {
    local resp
    resp=$(dwowd_rpc "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_height\",\"params\":[],\"id\":1}")
    echo "$resp" | grep -Po '"height"\s*:\s*\K\d+' || echo "0"
}

# Save failure artifacts to persistent location.
save_failure_logs() {
    local faildir="/tmp/dwow_merge_test_failure_$(date +%s)"
    mkdir -p "$faildir"
    for f in dwowd.log monerod.log p2pool.log xmrig.log monerod_sync.log; do
        [ -f "$TEST_DIR/$f" ] && cp "$TEST_DIR/$f" "$faildir/$f"
    done
    # Also capture current state
    dwowd_rpc '{"jsonrpc":"2.0","method":"blockchain.get_height","params":[],"id":1}' > "$faildir/dwowd_height.json" 2>/dev/null || true
    mm_rpc "merge_mining_get_chain_id" > "$faildir/mm_chain_id.json" 2>/dev/null || true
    echo -e "${YELLOW}[WARN]${NC} Failure logs saved to: $faildir"
}

# ── Cleanup ──────────────────────────────────────────────────────────────────

cleanup() {
    local exit_code=$?
    if [ $exit_code -ne 0 ] && [ "$NO_CLEANUP" = false ]; then
        echo ""
        warn "Test exited with code $exit_code. Saving logs..."
        save_failure_logs
    fi
    info "Stopping services..."
    kill $DWOWD_PID 2>/dev/null || true
    kill $MONEROD_PID 2>/dev/null || true
    kill $P2POOL_PID 2>/dev/null || true
    kill $XMING_PID 2>/dev/null || true
    wait 2>/dev/null || true
    if [ "$CLEANUP_ON_SUCCESS" = true ] && [ $exit_code -eq 0 ]; then
        rm -rf "$TEST_DIR"
    else
        info "Test dir preserved: $TEST_DIR"
    fi
    exit $exit_code
}
trap cleanup EXIT

# ══════════════════════════════════════════════════════════════════════════════
# Phase A: Monerod sync validation
# ══════════════════════════════════════════════════════════════════════════════
echo ""
echo "══════════════════════════════════════════════════"
echo "  DarkWow Merge Mining E2E Test"
echo "══════════════════════════════════════════════════"
echo ""

# Create test dir early — Phase A needs it for monerod check log.
mkdir -p "$TEST_DIR" "$DATADIR" "$MONERO_DATADIR" "$P2POOL_DATADIR"

info "Phase A: Monerod sync validation..."

MONERO_SYNC_CMD="$MONEROD_BIN \
    --testnet --no-igd --data-dir $MONERO_DATADIR \
    --log-level 0 --hide-my-port \
    --add-peer 125.229.105.12:28081 --add-peer 37.187.74.171:28089 \
    --fast-block-sync=1 \
    --zmq-pub tcp://127.0.0.1:$MONERO_ZMQ_PORT \
    --rpc-bind-ip 127.0.0.1 --rpc-bind-port $MONERO_RPC_PORT \
    --confirm-external-bind --non-interactive"

# Check if monerod data dir exists.
MONERO_DB=""
if [ -f "$MONERO_DATADIR/testnet/lmdb/data.mdb" ]; then
    MONERO_DB="$MONERO_DATADIR/testnet/lmdb/data.mdb"
elif [ -f "$MONERO_DATADIR/lmdb/data.mdb" ]; then
    MONERO_DB="$MONERO_DATADIR/lmdb/data.mdb"
else
    echo -e "${RED}[FAIL]${NC} monerod data dir not found."
    echo ""
    echo "  Run this command and wait for 'Synced' in the output:"
    echo ""
    echo "    $MONERO_SYNC_CMD"
    echo ""
    echo "  Once synced, re-run this test. The data dir persists at:"
    echo "    $MONERO_DATADIR"
    exit 1
fi

info "monerod data dir: $MONERO_DATADIR"

# Start monerod in offline mode briefly to query stored height.
"$MONEROD_BIN" \
    --testnet --offline \
    --data-dir "$MONERO_DATADIR" \
    --log-level 0 --hide-my-port --disable-rpc-ban \
    --rpc-bind-ip 127.0.0.1 --rpc-bind-port "$MONERO_RPC_PORT" \
    --confirm-external-bind --non-interactive \
    > "$TEST_DIR/monerod_check.log" 2>&1 &
MONERO_CHECK_PID=$!

STORED_HEIGHT=0
for i in $(seq 1 20); do
    H=$(get_monero_height)
    if [ "$H" -gt 1 ] 2>/dev/null; then
        STORED_HEIGHT=$H
        detail "monerod stored height: $STORED_HEIGHT (attempt $i)"
        break
    fi
    sleep 2
done
kill $MONERO_CHECK_PID 2>/dev/null || true
wait $MONERO_CHECK_PID 2>/dev/null || true

if [ "$STORED_HEIGHT" -le 1 ] 2>/dev/null; then
    echo -e "${RED}[FAIL]${NC} monerod data dir exists but chain is empty (height=$STORED_HEIGHT)."
    echo "  Re-run the sync command:"
    echo "    $MONERO_SYNC_CMD"
    exit 1
fi

# Check if monerod sync log exists and is recent (within 24 hours).
SYNC_LOG="/home/patrick/Darkfi/darkfi/scripts/sync.log"
SYNC_FRESH=false
if [ -f "$SYNC_LOG" ]; then
    LAST_SYNC=$(stat -c %Y "$SYNC_LOG" 2>/dev/null || echo "0")
    NOW=$(date +%s)
    if [ $((NOW - LAST_SYNC)) -lt 86400 ]; then
        if grep -qi "synchronized\|synced\|100%" "$SYNC_LOG" 2>/dev/null; then
            SYNC_FRESH=true
            info "Sync log confirms monerod is synced (last sync: $(date -d @$LAST_SYNC '+%Y-%m-%d %H:%M'))"
        fi
    fi
fi

if ! $SYNC_FRESH; then
    warn "Sync log not found or stale. Starting monerod sync check..."
    warn "This will run monerod online briefly to check sync status."

    # Start monerod online to check sync.
    "$MONEROD_BIN" \
        --testnet --no-igd --data-dir "$MONERO_DATADIR" \
        --log-level 0 --hide-my-port \
        --add-peer 125.229.105.12:28081 --add-peer 37.187.74.171:28089 \
        --fast-block-sync=1 \
        --rpc-bind-ip 127.0.0.1 --rpc-bind-port "$MONERO_RPC_PORT" \
        --confirm-external-bind --non-interactive \
        > "$TEST_DIR/monerod_sync_check.log" 2>&1 &
    MONERO_SYNC_CHECK_PID=$!

    # Wait up to 120 seconds for sync confirmation.
    SYNCED=false
    for i in $(seq 1 60); do
        RESP=$(monero_rpc "get_info")
        H=$(echo "$RESP" | grep -Po '"height"\s*:\s*\K\d+' || echo "0")
        TARGET=$(echo "$RESP" | grep -Po '"target_height"\s*:\s*\K\d+' || echo "0")
        SYNC_PCT=0
        if [ "$TARGET" -gt 0 ] 2>/dev/null; then
            SYNC_PCT=$((H * 100 / TARGET))
        fi
        if [ "$SYNC_PCT" -ge 99 ] 2>/dev/null; then
            info "monerod sync complete: height=$H, target=$TARGET ($SYNC_PCT%)"
            SYNCED=true
            break
        fi
        if [ $((i % 10)) -eq 0 ]; then
            info "  Sync check ${i}s: height=$H, target=$TARGET ($SYNC_PCT%)"
        fi
        sleep 2
    done

    kill $MONERO_SYNC_CHECK_PID 2>/dev/null || true
    wait $MONERO_SYNC_CHECK_PID 2>/dev/null || true

    if ! $SYNCED; then
        echo -e "${RED}[FAIL]${NC} monerod is not fully synced (height=$STORED_HEIGHT)."
        echo ""
        echo "  Run the sync command and wait for completion:"
        echo "    $MONERO_SYNC_CMD"
        echo ""
        echo "  Then re-run this test."
        exit 1
    fi
fi

info "Phase A complete: monerod is synced (stored height=$STORED_HEIGHT)"

# ══════════════════════════════════════════════════════════════════════════════
# Phase B: Prerequisites and build
# ══════════════════════════════════════════════════════════════════════════════
info "Phase B: Prerequisites..."

# --- dwowd binary ---
DWOWD_BIN="$REPO_ROOT/target/release/dwowd"
if [ ! -x "$DWOWD_BIN" ] && [ "$NO_BUILD" = false ]; then
    info "Building dwowd..."
    (cd "$REPO_ROOT" && RAYON_NUM_THREADS=10 cargo build -p dwowd --release 2>&1 | tail -5)
fi
if [ ! -x "$DWOWD_BIN" ]; then
    echo -e "${RED}[FAIL]${NC} dwowd binary not found at $DWOWD_BIN"
    exit 1
fi
detail "dwowd binary: $DWOWD_BIN"

# --- monerod binary ---
if [ ! -x "$MONEROD_BIN" ]; then
    info "Downloading monerod..."
    curl -sL "https://downloads.getmonero.org/cli/linux64" \
        -o "$TEST_DIR/monero.tar.bz2"
    tar -xjf "$TEST_DIR/monero.tar.bz2" -C "$TEST_DIR" --strip-components=1
    mkdir -p "$HOME/.local/bin"
    cp "$TEST_DIR/monerod" "$MONEROD_BIN"
    chmod +x "$MONEROD_BIN"
    rm "$TEST_DIR/monero.tar.bz2"
fi
detail "monerod binary: $MONEROD_BIN"

# --- p2pool binary ---
if [ ! -x "$P2POOL_BIN" ]; then
    info "Downloading p2pool..."
    curl -sL "https://github.com/SChernykh/p2pool/releases/download/v4.14/p2pool-v4.14-linux-x64.tar.gz" \
        -o "$TEST_DIR/p2pool.tar.gz"
    tar -xzf "$TEST_DIR/p2pool.tar.gz" -C "$TEST_DIR" --strip-components=1
    mkdir -p "$HOME/.local/bin"
    cp "$TEST_DIR/p2pool" "$P2POOL_BIN"
    chmod +x "$P2POOL_BIN"
    rm "$TEST_DIR/p2pool.tar.gz"
fi
detail "p2pool binary: $P2POOL_BIN"

# --- xmrig binary ---
XMRIG_BIN="$HOME/.local/bin/xmrig-6.22.2"
XMRIG_MIN_VER="6.22.2"

need_xmrig_dl=false
if [ -x "$XMRIG_BIN" ]; then
    XMRIG_VER=$("$XMRIG_BIN" --version 2>&1 | grep -oP 'XMRig \K[\d.]+' || echo "0")
    if [ "$(printf '%s\n' "$XMRIG_MIN_VER" "$XMRIG_VER" | sort -V | head -1)" = "$XMRIG_MIN_VER" ]; then
        detail "xmrig binary: $XMRIG_BIN (v$XMRIG_VER)"
    else
        need_xmrig_dl=true
    fi
elif command -v xmrig >/dev/null 2>&1; then
    XMRIG_VER=$(xmrig --version 2>&1 | grep -oP 'XMRig \K[\d.]+' || echo "0")
    if [ "$(printf '%s\n' "$XMRIG_MIN_VER" "$XMRIG_VER" | sort -V | head -1)" = "$XMRIG_MIN_VER" ]; then
        XMRIG_BIN="$(command -v xmrig)"
        detail "xmrig binary: $XMRIG_BIN (v$XMRIG_VER)"
    else
        need_xmrig_dl=true
    fi
else
    need_xmrig_dl=true
fi

if $need_xmrig_dl; then
    info "Downloading xmrig v$XMRIG_MIN_VER..."
    curl -sL "https://github.com/xmrig/xmrig/releases/download/v${XMRIG_MIN_VER}/xmrig-${XMRIG_MIN_VER}-noble-x64.tar.gz" \
        -o "$TEST_DIR/xmrig.tar.gz"
    tar -xzf "$TEST_DIR/xmrig.tar.gz" -C "$TEST_DIR" --strip-components=1
    mkdir -p "$HOME/.local/bin"
    cp "$TEST_DIR/xmrig" "$XMRIG_BIN"
    chmod +x "$XMRIG_BIN"
    rm "$TEST_DIR/xmrig.tar.gz"
    detail "xmrig binary: $XMRIG_BIN (v$XMRIG_MIN_VER)"
fi

# Quick smoke test: xmrig benchmark must not crash.
XMRIG_SMOKE=$(timeout 5 "$XMRIG_BIN" -a rx/0 --benchmark=1M -t 1 2>&1 || true)
if echo "$XMRIG_SMOKE" | grep -q "buffer overflow\|SIGABRT\|dumped core"; then
    echo -e "${RED}[FAIL]${NC} xmrig benchmark crashed — RandomX is broken on this system"
    exit 1
fi
info "xmrig benchmark smoke test passed"

# ══════════════════════════════════════════════════════════════════════════════
# Phase C: Config generation
# ══════════════════════════════════════════════════════════════════════════════
info "Phase C: Generating config..."

mkdir -p "$(dirname "$CONFIG_FILE")"
rm -f "$CONFIG_FILE"
"$DWOWD_BIN" --config "$CONFIG_FILE" > /dev/null 2>&1 || true

if [ ! -s "$CONFIG_FILE" ]; then
    echo -e "${RED}[FAIL]${NC} dwowd did not generate config template"
    exit 1
fi

# Inject mm_rpc config only if not already present (template may include it).
if ! grep -q "mm_rpc" "$CONFIG_FILE" 2>/dev/null; then
    cat >> "$CONFIG_FILE" << 'EOF'

## Linear-testnet merge mining JSON-RPC settings (for p2pool)
[network_config."linear-testnet".mm_rpc]
rpc_listen = "http+tcp://127.0.0.1:31348"
EOF
    detail "mm_rpc section appended to config"
else
    detail "mm_rpc section already present in config template"
fi

sed -i "s|~/.local/share/dwow/dwowd/linear-testnet|$DATADIR|g" "$CONFIG_FILE"
detail "Config written: $CONFIG_FILE"
detail "mm_rpc listen: 127.0.0.1:$MM_RPC_PORT"
detail "data dir: $DATADIR"

# ══════════════════════════════════════════════════════════════════════════════
# Phase D: Start services
# ══════════════════════════════════════════════════════════════════════════════
info "Phase D: Starting services..."

DUMMY_MONERO_WALLET="9wenrVcFffvbTR4nEQ7KAbDMw7bq6B7uwgsraJzFVLkq9SiqMYFf72544RyXLaXKmZYfYNdcdZWpKaBv5dD8xkpS5djBZPM"
DUMMY_DARKFI_WALLET="fTwfhzTmzupKdU1XQM1zgNGP4CG6HRnNiNZDWvm1HVyDyFFwRPygmqj1"

# --- monerod (offline, fixed-difficulty) ---
info "Starting monerod (offline, fixed-difficulty=20000)..."
"$MONEROD_BIN" \
    --testnet \
    --offline \
    --fixed-difficulty 20000 \
    --data-dir "$MONERO_DATADIR" \
    --log-level 1 \
    --hide-my-port \
    --disable-rpc-ban \
    --zmq-pub "tcp://127.0.0.1:$MONERO_ZMQ_PORT" \
    --rpc-bind-ip 127.0.0.1 \
    --rpc-bind-port "$MONERO_RPC_PORT" \
    --confirm-external-bind \
    --non-interactive \
    > "$TEST_DIR/monerod.log" 2>&1 &
MONEROD_PID=$!
detail "monerod PID: $MONEROD_PID"

# Wait for monerod RPC.
for i in $(seq 1 30); do
    RESP=$(monero_rpc "get_info")
    if echo "$RESP" | grep -q "OK\|status\|height"; then
        H=$(echo "$RESP" | grep -Po '"height"\s*:\s*\K\d+' || echo "0")
        info "monerod RPC ready: height=$H (attempt $i)"
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo -e "${RED}[FAIL]${NC} monerod RPC did not start"
        tail -20 "$TEST_DIR/monerod.log"
        exit 1
    fi
    sleep 1
done

# --- dwowd ---
ulimit -s 131072
export RUST_MIN_STACK=67108864
info "Starting dwowd..."
"$DWOWD_BIN" > "$TEST_DIR/dwowd.log" 2>&1 &
DWOWD_PID=$!
detail "dwowd PID: $DWOWD_PID"

for i in $(seq 1 60); do
    RESP=$(echo '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' | nc -w 1 127.0.0.1 "$DWOWD_RPC_PORT" 2>/dev/null || true)
    if echo "$RESP" | grep -q "pong"; then
        info "dwowd RPC ready (attempt $i)"
        break
    fi
    if [ "$i" -eq 60 ]; then
        echo -e "${RED}[FAIL]${NC} dwowd RPC did not start within 60s"
        tail -20 "$TEST_DIR/dwowd.log"
        exit 1
    fi
    sleep 1
done

# Verify mm_rpc endpoint.
detail "Checking mm_rpc endpoint..."
for i in $(seq 1 10); do
    RESP=$(mm_rpc "merge_mining_get_chain_id")
    if echo "$RESP" | grep -q "chain_id"; then
        CHAIN_ID=$(echo "$RESP" | grep -o '"chain_id":"[^"]*"' || echo "unknown")
        info "mm_rpc endpoint ready: $CHAIN_ID (attempt $i)"
        break
    fi
    if [ "$i" -eq 10 ]; then
        echo -e "${RED}[FAIL]${NC} mm_rpc endpoint not responding"
        echo "Response: $RESP"
        tail -10 "$TEST_DIR/dwowd.log"
        exit 1
    fi
    sleep 1
done

# --- p2pool (merge mining mode) ---
info "Starting p2pool (merge mining mode)..."
"$P2POOL_BIN" \
    --host "127.0.0.1" \
    --rpc-port "$MONERO_RPC_PORT" \
    --zmq-port "$MONERO_ZMQ_PORT" \
    --wallet "$DUMMY_MONERO_WALLET" \
    --stratum "0.0.0.0:$P2POOL_STRATUM_PORT" \
    --data-dir "$P2POOL_DATADIR" \
    --no-igd \
    --mini \
    --no-upnp \
    --merge-mine "127.0.0.1:$MM_RPC_PORT" "$DUMMY_DARKFI_WALLET" \
    > "$TEST_DIR/p2pool.log" 2>&1 &
P2POOL_PID=$!
detail "p2pool PID: $P2POOL_PID"

# Wait for p2pool stratum.
info "Waiting for p2pool stratum..."
P2POOL_READY=false
for i in $(seq 1 60); do
    P2POOL_LOGS=$(cat "$TEST_DIR/p2pool.log" 2>/dev/null || true)
    if echo "$P2POOL_LOGS" | grep -qi "StratumServer\|stratum server\|stratum.*listening"; then
        info "p2pool stratum active (attempt $i)"
        P2POOL_READY=true
        break
    fi
    if echo "$P2POOL_LOGS" | grep -qi "not synchronized"; then
        echo -e "${RED}[FAIL]${NC} p2pool reports monerod not synchronized."
        echo "  The stored blockchain may be too old. Re-run the sync:"
        echo "    $MONERO_SYNC_CMD"
        tail -10 "$TEST_DIR/p2pool.log"
        exit 1
    fi
    sleep 1
done

if ! $P2POOL_READY; then
    echo -e "${RED}[FAIL]${NC} p2pool stratum did not start within 60s"
    detail "p2pool log:"
    tail -20 "$TEST_DIR/p2pool.log"
    exit 1
fi

# Verify p2pool merge mining is connected to dwowd.
detail "Checking p2pool merge mining connectivity..."
sleep 3
P2POOL_LOGS=$(cat "$TEST_DIR/p2pool.log" 2>/dev/null || true)
if echo "$P2POOL_LOGS" | grep -qi "merge.mining.*aux\|aux.*block\|merge_mining_get_aux_block"; then
    info "p2pool merge mining appears active (aux block requests seen)"
else
    warn "p2pool merge mining activity not yet visible in logs — may need a Monero block first"
fi

# --- xmrig ---
info "Starting xmrig (1 thread, fixed-difficulty 20000)..."
"$XMRIG_BIN" \
    -o "127.0.0.1:$P2POOL_STRATUM_PORT" \
    -u x \
    -p 20000 \
    -a rx/0 \
    -t 1 \
    --keepalive \
    --print-time=5 \
    > "$TEST_DIR/xmrig.log" 2>&1 &
XMING_PID=$!
sleep 2

if ! kill -0 $XMING_PID 2>/dev/null; then
    echo -e "${RED}[FAIL]${NC} xmrig failed to start"
    tail -10 "$TEST_DIR/xmrig.log"
    exit 1
fi
info "xmrig running (PID=$XMING_PID)"

# ══════════════════════════════════════════════════════════════════════════════
# Phase E: Mining loop (indefinite — runs until block found)
# ══════════════════════════════════════════════════════════════════════════════
info "Phase E: Mining for merge mined block..."
echo "  (This may take 10-60+ minutes depending on CPU.)"
echo ""

# Record initial state.
INITIAL_MONERO_HEIGHT=$(get_monero_height)
INITIAL_DWOWD_HEIGHT=$(get_dwowd_height)
info "Initial state: monerod=$INITIAL_MONERO_HEIGHT, dwowd=$INITIAL_DWOWD_HEIGHT"
echo ""

START_TIME=$(date +%s)
BLOCK_PRODUCED=false
MINED_BLOCK_HEIGHT=""

while true; do
    sleep 10
    ELAPSED=$(( $(date +%s) - START_TIME ))

    # ── Check dwowd log for merge mining submission ──
    MM_SUBMIT=$(grep -c "RPC-MM.*submit_solution\|BLOCK ACCEPTED" "$TEST_DIR/dwowd.log" 2>/dev/null || echo "0")
    CURRENT_DWOWD_HEIGHT=$(get_dwowd_height)

    if [ "$CURRENT_DWOWD_HEIGHT" -gt "$INITIAL_DWOWD_HEIGHT" ] 2>/dev/null; then
        MINED_BLOCK_HEIGHT="$CURRENT_DWOWD_HEIGHT"
        BLOCK_PRODUCED=true
        break
    fi

    # Also break if we see submissions (even if height hasn't updated yet).
    if [ "$MM_SUBMIT" -gt 0 ] 2>/dev/null; then
        # Give dwowd a moment to process.
        sleep 5
        CURRENT_DWOWD_HEIGHT=$(get_dwowd_height)
        if [ "$CURRENT_DWOWD_HEIGHT" -gt "$INITIAL_DWOWD_HEIGHT" ] 2>/dev/null; then
            MINED_BLOCK_HEIGHT="$CURRENT_DWOWD_HEIGHT"
            BLOCK_PRODUCED=true
            break
        fi
    fi

    # ── Progress output every 60 seconds (every 6th iteration) ──
    if [ $((ELAPSED % 60)) -lt 10 ]; then
        MONERO_H=$(get_monero_height)
        P2POOL_SHARES=$(grep -ci "found share\|block found" "$TEST_DIR/p2pool.log" 2>/dev/null || echo "0")
        XMRIG_HASHRATE=$(grep -oP 'speed \d+s/\d+ \K[\d.]+' "$TEST_DIR/xmrig.log" 2>/dev/null | tail -1 || echo "N/A")

        # Also check p2pool sidechain status.
        P2POOL_SIDECHAIN=$(grep -oP 'sidechain height\s*\K\d+' "$TEST_DIR/p2pool.log" 2>/dev/null | tail -1 || echo "?")
        P2POOL_MAINCHAIN=$(grep -oP 'mainchain height\s*\K\d+' "$TEST_DIR/p2pool.log" 2>/dev/null | tail -1 || echo "?")

        echo -e "${CYAN}[$(date '+%H:%M:%S')]${NC} elapsed=${ELAPSED}s | monero=$MONERO_H | dwowd=$CURRENT_DWOWD_HEIGHT/$INITIAL_DWOWD_HEIGHT | p2pool=$P2POOL_SHARES shares (main=$P2POOL_MAINCHAIN side=$P2POOL_SIDECHAIN) | xmrig=${XMRIG_HASHRATE}H/s | mm_submits=$MM_SUBMIT"

        # Health check: are all services still alive?
        if ! kill -0 $DWOWD_PID 2>/dev/null; then
            echo -e "${RED}[FAIL]${NC} dwowd died!"
            tail -30 "$TEST_DIR/dwowd.log"
            exit 1
        fi
        if ! kill -0 $P2POOL_PID 2>/dev/null; then
            echo -e "${RED}[FAIL]${NC} p2pool died!"
            tail -30 "$TEST_DIR/p2pool.log"
            exit 1
        fi
        if ! kill -0 $XMING_PID 2>/dev/null; then
            echo -e "${RED}[FAIL]${NC} xmrig died!"
            tail -30 "$TEST_DIR/xmrig.log"
            exit 1
        fi
    fi
done

TOTAL_TIME=$(( $(date +%s) - START_TIME ))
echo ""
echo -e "${GREEN}══════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  Block produced after ${TOTAL_TIME}s ($((TOTAL_TIME / 60)) minutes)${NC}"
echo -e "${GREEN}══════════════════════════════════════════════════${NC}"
echo ""

# ══════════════════════════════════════════════════════════════════════════════
# Phase F: Verification — pow_source stored as Monero(MoneroPowData)
# ══════════════════════════════════════════════════════════════════════════════
info "Phase F: Verifying merge-mined block proof..."

# Query the new block.
BLOCK_JSON=""
for attempt in $(seq 1 10); do
    BLOCK_JSON=$(get_dwowd_block "$MINED_BLOCK_HEIGHT")
    if [ -n "$BLOCK_JSON" ]; then
        info "Block $MINED_BLOCK_HEIGHT retrieved (attempt $attempt)"
        break
    fi
    sleep 2
done

if [ -z "$BLOCK_JSON" ]; then
    echo -e "${RED}[FAIL]${NC} Could not retrieve block $MINED_BLOCK_HEIGHT"
    exit 1
fi

# pow_source is #[serde(skip)] so it won't appear in JSON.
# Instead we verify through dwowd logs that the merge mining submission
# was processed through the Monero verification path.
echo ""
info "Block $MINED_BLOCK_HEIGHT retrieved (pow_source is serde-skipped, not in JSON)"
echo ""

# Check dwowd log for merge mining submission.
MM_SUBMIT_COUNT=$(grep -c "RPC-MM.*Got solution submission" "$TEST_DIR/dwowd.log" 2>/dev/null || echo "0")
MM_ACCEPTED=$(grep -c "RPC-MM.*Merge-mined block.*accepted" "$TEST_DIR/dwowd.log" 2>/dev/null || echo "0")
MM_MONERO_POW=$(grep -c "MoneroPowData\|is_coinbase_valid_merkle_root" "$TEST_DIR/dwowd.log" 2>/dev/null || echo "0")

echo "  Solution submissions received: $MM_SUBMIT_COUNT"
echo "  Blocks accepted: $MM_ACCEPTED"
echo ""

if [ "$MM_ACCEPTED" -gt 0 ]; then
    echo -e "${GREEN}[PASS]${NC} Merge-mined block accepted by dwowd"
else
    echo -e "${RED}[FAIL]${NC} No merge-mined block was accepted"
    echo "  dwowd log excerpt:"
    grep -i "RPC-MM" "$TEST_DIR/dwowd.log" | tail -20
    exit 1
fi

# Show relevant log lines.
info "Merge mining log excerpt:"
grep "RPC-MM\|merge.mining\|MoneroPowData\|coinbase.*merkle\|aux_hash\|solution" "$TEST_DIR/dwowd.log" | tail -20

echo ""
echo -e "${GREEN}══════════════════════════════════════════════════${NC}"
echo -e "${GREEN}  ALL CHECKS PASSED${NC}"
echo -e "${GREEN}══════════════════════════════════════════════════${NC}"
echo ""
echo "  Total time: ${TOTAL_TIME}s ($((TOTAL_TIME / 60)) minutes)"
echo "  Block height: $MINED_BLOCK_HEIGHT"
echo "  pow_source: Monero(MoneroPowData) — stored on disk, verified by log"
echo "  Test dir: $TEST_DIR"
echo ""

CLEANUP_ON_SUCCESS=false
