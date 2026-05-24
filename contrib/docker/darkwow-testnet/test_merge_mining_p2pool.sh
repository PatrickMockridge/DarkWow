#!/bin/bash
# DarkWow Merge Mining with P2Pool — Local Test
#
# One metric: does p2pool merge mining produce a darkwow block? Yes or no.
#
# Setup per upstream reference:
#   /tmp/darkfi/doc/src/testnet/merge-mining.md
#
#   xmrig --> p2pool --[merge-mine]--> dwowd (mm_rpc)
#                    \--[monerod RPC]--> monerod (offline, fixed-difficulty)
#
# Prerequisite (one-time): monerod synced from public testnet via fast-block-sync.
# Once synced, the persistent data dir is reused for all subsequent runs.
#
# Usage:
#   ./test_merge_mining_p2pool.sh              # full test
#   ./test_merge_mining_p2pool.sh --no-build   # skip cargo build

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
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }

NO_BUILD=false
if [ "${1:-}" = "--no-build" ]; then
    NO_BUILD=true
fi

cleanup() {
    info "Cleaning up..."
    kill $DWOWD_PID 2>/dev/null || true
    kill $MONEROD_PID 2>/dev/null || true
    kill $P2POOL_PID 2>/dev/null || true
    kill $XMING_PID 2>/dev/null || true
    wait 2>/dev/null || true
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

# ==============================================================================
# Phase 1: Prerequisites
# ==============================================================================
echo "=== Merge Mining P2Pool Test ==="

info "Phase 1: Prerequisites..."

mkdir -p "$TEST_DIR" "$DATADIR" "$MONERO_DATADIR" "$P2POOL_DATADIR"

# --- dwowd ---
DWOWD_BIN="$REPO_ROOT/target/release/dwowd"
if [ ! -x "$DWOWD_BIN" ] && [ "$NO_BUILD" = false ]; then
    info "Building dwowd..."
    (cd "$REPO_ROOT" && RAYON_NUM_THREADS=10 cargo build -p dwowd --release 2>&1 | tail -3)
fi
if [ ! -x "$DWOWD_BIN" ]; then
    echo -e "${RED}[FAIL]${NC} dwowd binary not found at $DWOWD_BIN"
    exit 1
fi
info "dwowd: $DWOWD_BIN"

# --- monerod ---
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
info "monerod: $MONEROD_BIN"

# --- p2pool ---
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
info "p2pool: $P2POOL_BIN"

# --- xmrig ---
# xmrig 6.21.1 has a RandomX buffer overflow bug on some systems.
# Require >= 6.22.2, downloading if necessary.
XMRIG_BIN="$HOME/.local/bin/xmrig-6.22.2"
XMRIG_MIN_VER="6.22.2"

need_xmrig_dl=false
if [ -x "$XMRIG_BIN" ]; then
    XMRIG_VER=$("$XMRIG_BIN" --version 2>&1 | grep -oP 'XMRig \K[\d.]+' || echo "0")
    if [ "$(printf '%s\n' "$XMRIG_MIN_VER" "$XMRIG_VER" | sort -V | head -1)" = "$XMRIG_MIN_VER" ]; then
        info "xmrig: $XMRIG_BIN (v$XMRIG_VER)"
    else
        need_xmrig_dl=true
    fi
elif command -v xmrig >/dev/null 2>&1; then
    XMRIG_VER=$(xmrig --version 2>&1 | grep -oP 'XMRig \K[\d.]+' || echo "0")
    if [ "$(printf '%s\n' "$XMRIG_MIN_VER" "$XMRIG_VER" | sort -V | head -1)" = "$XMRIG_MIN_VER" ]; then
        XMRIG_BIN="$(command -v xmrig)"
        info "xmrig: $XMRIG_BIN (v$XMRIG_VER)"
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
    info "xmrig: $XMRIG_BIN (v$XMRIG_MIN_VER)"
fi

# Quick smoke test: xmrig benchmark must not crash
XMRIG_SMOKE=$(timeout 5 "$XMRIG_BIN" -a rx/0 --benchmark=1M -t 1 2>&1 || true)
if echo "$XMRIG_SMOKE" | grep -q "buffer overflow\|SIGABRT\|dumped core"; then
    echo -e "${RED}[FAIL]${NC} xmrig benchmark crashed — RandomX is broken on this system"
    exit 1
fi

# ==============================================================================
# Phase 2: Config
# ==============================================================================
info "Phase 2: Config..."

mkdir -p "$(dirname "$CONFIG_FILE")"
"$DWOWD_BIN" --config "$CONFIG_FILE" > /dev/null 2>&1 || true

if [ ! -s "$CONFIG_FILE" ]; then
    echo -e "${RED}[FAIL]${NC} dwowd did not generate config template"
    exit 1
fi

cat >> "$CONFIG_FILE" << 'EOF'

## Linear-testnet merge mining JSON-RPC settings (for p2pool)
[network_config."linear-testnet".mm_rpc]
rpc_listen = "http+tcp://127.0.0.1:31348"
EOF

sed -i "s|~/.local/share/dwow/dwowd/linear-testnet|$DATADIR|g" "$CONFIG_FILE"
info "config: mm_rpc injected, data paths redirected"

# ==============================================================================
# Phase 3: Precondition — is monerod synced?
# ==============================================================================
info "Phase 3: Checking monerod sync state..."

MONERO_SYNC_CMD="$MONEROD_BIN \\
    --testnet --no-igd --data-dir $MONERO_DATADIR \\
    --log-level 0 --hide-my-port \\
    --add-peer 125.229.105.12:28081 --add-peer 37.187.74.171:28089 \\
    --fast-block-sync=1 \\
    --zmq-pub tcp://127.0.0.1:$MONERO_ZMQ_PORT \\
    --rpc-bind-ip 127.0.0.1 --rpc-bind-port $MONERO_RPC_PORT \\
    --confirm-external-bind --non-interactive"

if [ ! -f "$MONERO_DATADIR/lmdb/data.mdb" ] && [ ! -f "$MONERO_DATADIR/testnet/lmdb/data.mdb" ]; then
    echo -e "${RED}[FAIL]${NC} monerod data dir not found — needs one-time sync first."
    echo ""
    echo "  Run this command and wait for 'Synced' in the output:"
    echo ""
    echo "    $MONERO_SYNC_CMD"
    echo ""
    echo "  Once synced, re-run this test. The data dir persists at:"
    echo "    $MONERO_DATADIR"
    exit 1
fi

# Query stored chain height via offline monerod
"$MONEROD_BIN" \
    --testnet --offline \
    --data-dir "$MONERO_DATADIR" \
    --log-level 0 --hide-my-port --disable-rpc-ban \
    --rpc-bind-ip 127.0.0.1 --rpc-bind-port "$MONERO_RPC_PORT" \
    --confirm-external-bind --non-interactive \
    > "$TEST_DIR/monerod_check.log" 2>&1 &
MONERO_CHECK_PID=$!

STORED_HEIGHT=0
for i in $(seq 1 15); do
    H=$(curl -s -X POST "http://127.0.0.1:$MONERO_RPC_PORT/json_rpc" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"get_info","params":[],"id":1}' 2>/dev/null \
        | grep -Po '"height"\s*:\s*\K\d+' || echo "0")
    if [ "$H" -gt 1 ] 2>/dev/null; then
        STORED_HEIGHT=$H
        break
    fi
    sleep 1
done
kill $MONERO_CHECK_PID 2>/dev/null || true
wait $MONERO_CHECK_PID 2>/dev/null || true

if [ "$STORED_HEIGHT" -le 1 ] 2>/dev/null; then
    echo -e "${RED}[FAIL]${NC} monerod data dir exists but chain is empty (height=$STORED_HEIGHT)."
    echo ""
    echo "  The sync may have been interrupted. Re-run the sync command:"
    echo ""
    echo "    $MONERO_SYNC_CMD"
    echo ""
    echo "  Once synced, re-run this test."
    exit 1
fi

info "monerod synced: height=$STORED_HEIGHT"

# ==============================================================================
# Phase 4: Start services
# ==============================================================================
info "Phase 4: Starting services..."

DUMMY_MONERO_WALLET="9wenrVcFffvbTR4nEQ7KAbDMw7bq6B7uwgsraJzFVLkq9SiqMYFf72544RyXLaXKmZYfYNdcdZWpKaBv5dD8xkpS5djBZPM"
DUMMY_DARKFI_WALLET="fTwfhzTmzupKdU1XQM1zgNGP4CG6HRnNiNZDWvm1HVyDyFFwRPygmqj1"

# --- monerod (offline, fixed-difficulty) ---
# Per upstream merge-mining.md line 239
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

# Wait for monerod RPC
for i in $(seq 1 30); do
    RESP=$(curl -s -X POST "http://127.0.0.1:$MONERO_RPC_PORT/json_rpc" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"get_info","params":[],"id":1}' 2>/dev/null || true)
    if echo "$RESP" | grep -q "OK\|status\|height"; then
        info "monerod RPC ready (attempt $i)"
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

for i in $(seq 1 30); do
    if exec 3<>/dev/tcp/127.0.0.1/$DWOWD_RPC_PORT 2>/dev/null; then
        echo '{"jsonrpc":"2.0","method":"ping","params":[],"id":1}' >&3
        if timeout 2 cat <&3 2>/dev/null | grep -q "pong"; then
            exec 3>&-
            info "dwowd RPC ready (attempt $i)"
            break
        fi
    fi
    exec 3>&- 2>/dev/null || true
    if [ "$i" -eq 30 ]; then
        echo -e "${RED}[FAIL]${NC} dwowd RPC did not start"
        tail -20 "$TEST_DIR/dwowd.log"
        exit 1
    fi
    sleep 1
done

# Verify mm_rpc endpoint
info "Checking mm_rpc endpoint..."
for i in $(seq 1 10); do
    RESP=$(curl -s -X POST "http://127.0.0.1:$MM_RPC_PORT" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","params":[],"id":1}' 2>/dev/null || true)
    if echo "$RESP" | grep -q "chain_id"; then
        info "mm_rpc endpoint ready (attempt $i)"
        break
    fi
    if [ "$i" -eq 10 ]; then
        echo -e "${RED}[FAIL]${NC} mm_rpc endpoint not responding"
        exit 1
    fi
    sleep 1
done

# --- p2pool (merge mining mode) ---
# Per upstream merge-mining.md line 245
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

# Wait for p2pool stratum
info "Waiting for p2pool stratum..."
P2POOL_READY=false
for i in $(seq 1 60); do
    P2POOL_LOGS=$(cat "$TEST_DIR/p2pool.log" 2>/dev/null || true)
    if echo "$P2POOL_LOGS" | grep -qi "StratumServer\|stratum server"; then
        info "p2pool stratum active (attempt $i)"
        P2POOL_READY=true
        break
    fi
    if echo "$P2POOL_LOGS" | grep -qi "not synchronized"; then
        echo -e "${RED}[FAIL]${NC} p2pool reports monerod not synchronized — sync may be incomplete."
        echo "  Re-run the sync command:"
        echo "    $MONERO_SYNC_CMD"
        tail -5 "$TEST_DIR/p2pool.log"
        exit 1
    fi
    sleep 1
done

if ! $P2POOL_READY; then
    echo -e "${RED}[FAIL]${NC} p2pool stratum did not start within 60s"
    tail -20 "$TEST_DIR/p2pool.log"
    exit 1
fi

# --- xmrig ---
# Per upstream merge-mining.md line 251
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

# ==============================================================================
# Phase 5: Wait for merge mined darkwow block
# ==============================================================================
info "Phase 5: Waiting for merge mined darkwow block..."

# The ONE metric: does a merge mining submission reach dwowd and get accepted?
# Evidence: dwowd log contains "submit_solution" or "BLOCK ACCEPTED" from RPC-MM.
MONERO_HEIGHT=$(curl -s -X POST "http://127.0.0.1:$MONERO_RPC_PORT/json_rpc" \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"get_info","params":[],"id":1}' 2>/dev/null \
    | grep -Po '"height"\s*:\s*\K\d+' || echo "0")
info "Starting monerod height: $MONERO_HEIGHT"

BLOCK_PRODUCED=false
for i in $(seq 1 300); do
    sleep 1

    # Check for merge mining submission in dwowd log
    MM_SUBMIT=$(grep -c "RPC-MM.*submit_solution\|BLOCK ACCEPTED" "$TEST_DIR/dwowd.log" 2>/dev/null || echo "0")

    if [ "$MM_SUBMIT" -gt 0 ] 2>/dev/null; then
        echo ""
        echo -e "${GREEN}[PASS]${NC} Merge mined darkwow block produced ($MM_SUBMIT submission(s) in dwowd log)"
        grep "RPC-MM.*submit_solution\|BLOCK ACCEPTED" "$TEST_DIR/dwowd.log" | tail -5
        BLOCK_PRODUCED=true
        break
    fi

    # Progress indicator
    if [ $((i % 30)) -eq 0 ]; then
        MONERO_CURRENT=$(curl -s -X POST "http://127.0.0.1:$MONERO_RPC_PORT/json_rpc" \
            -H "Content-Type: application/json" \
            -d '{"jsonrpc":"2.0","method":"get_info","params":[],"id":1}' 2>/dev/null \
            | grep -Po '"height"\s*:\s*\K\d+' || echo "0")
        P2POOL_SHARES=$(grep -ci "found share\|block found" "$TEST_DIR/p2pool.log" 2>/dev/null || echo "0")
        info "  ${i}s elapsed, monerod height=$MONERO_CURRENT, p2pool shares=$P2POOL_SHARES, dwowd submissions=$MM_SUBMIT"
    fi
done

if ! $BLOCK_PRODUCED; then
    echo ""
    echo -e "${RED}[FAIL]${NC} No merge mined darkwow block produced within 300s."
    echo ""
    echo "  Last 15 lines of dwowd log:"
    tail -15 "$TEST_DIR/dwowd.log"
    echo ""
    echo "  Last 15 lines of p2pool log:"
    tail -15 "$TEST_DIR/p2pool.log"
    echo ""
    echo "  Last 15 lines of xmrig log:"
    tail -15 "$TEST_DIR/xmrig.log"
    exit 1
fi
