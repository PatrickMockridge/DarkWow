#!/bin/bash
# Minimal reproduce script: xmrig -> p2pool -> dwowd (merge mining)
#
# Prerequisites (one-time):
#   - dwowd built: cargo build -p dwowd --release
#   - monerod synced from public testnet (data at ~/.cache/dwow_merge_testnet_monero)
#   - p2pool and xmrig-6.22.2 at ~/.local/bin/
#
# See test_merge_mining_p2pool.sh for the full version with auto-download and checks.

set -e

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TEST_DIR="/tmp/dwow_merge_test"

DWOWD_BIN="$REPO_ROOT/target/release/dwowd"
MONEROD_BIN="$HOME/.local/bin/monerod"
P2POOL_BIN="$HOME/.local/bin/p2pool"
XMRIG_BIN="$HOME/.local/bin/xmrig-6.22.2"

MONERO_RPC_PORT=28081
MONERO_ZMQ_PORT=28083
MM_RPC_PORT=31348
P2POOL_STRATUM_PORT=3333

DUMMY_MONERO_WALLET="9wenrVcFffvbTR4nEQ7KAbDMw7bq6B7uwgsraJzFVLkq9SiqMYFf72544RyXLaXKmZYfYNdcdZWpKaBv5dD8xkpS5djBZPM"
DUMMY_DARKFI_WALLET="fTwfhzTmzupKdU1XQM1zgNGP4CG6HRnNiNZDWvm1HVyDyFFwRPygmqj1"

MONERO_DATADIR="$HOME/.cache/dwow_merge_testnet_monero"

PIDS=""

cleanup() {
    kill $PIDS 2>/dev/null || true
    wait 2>/dev/null || true
    rm -rf "$TEST_DIR"
}
trap cleanup EXIT

# ---- Setup ----
rm -rf "$TEST_DIR"
mkdir -p "$TEST_DIR" "$TEST_DIR/data" "$TEST_DIR/p2pool_data"

# Generate config
export XDG_CONFIG_HOME="$TEST_DIR"
CONFIG_FILE="$XDG_CONFIG_HOME/dwow/dwowd_config.toml"
mkdir -p "$(dirname "$CONFIG_FILE")"
"$DWOWD_BIN" --config "$CONFIG_FILE" > /dev/null 2>&1 || true

# Inject mm_rpc
cat >> "$CONFIG_FILE" << 'EOF'

[network_config."darkwow-devnet".mm_rpc]
rpc_listen = "http+tcp://127.0.0.1:31348"
EOF

sed -i "s|~/.local/share/dwow/dwowd/darkwow-devnet|$TEST_DIR/data|g" "$CONFIG_FILE"

# ---- Start monerod (offline) ----
"$MONEROD_BIN" \
    --testnet --offline --fixed-difficulty 20000 \
    --data-dir "$MONERO_DATADIR" \
    --log-level 1 --hide-my-port --disable-rpc-ban \
    --zmq-pub "tcp://127.0.0.1:$MONERO_ZMQ_PORT" \
    --rpc-bind-ip 127.0.0.1 --rpc-bind-port "$MONERO_RPC_PORT" \
    --confirm-external-bind --non-interactive \
    > "$TEST_DIR/monerod.log" 2>&1 &
PIDS="$PIDS $!"

for i in $(seq 1 30); do
    RESP=$(curl -s -X POST "http://127.0.0.1:$MONERO_RPC_PORT/json_rpc" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"get_info","params":[],"id":1}' 2>/dev/null || true)
    echo "$RESP" | grep -q "OK\|height" && break
    sleep 1
done

# ---- Start dwowd ----
ulimit -s 131072
export RUST_MIN_STACK=67108864
"$DWOWD_BIN" > "$TEST_DIR/dwowd.log" 2>&1 &
PIDS="$PIDS $!"

for i in $(seq 1 30); do
    RESP=$(curl -s -X POST "http://127.0.0.1:$MM_RPC_PORT" \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","params":[],"id":1}' 2>/dev/null || true)
    echo "$RESP" | grep -q "chain_id" && break
    sleep 1
done

# ---- Start p2pool (merge mining) ----
"$P2POOL_BIN" \
    --host 127.0.0.1 \
    --rpc-port "$MONERO_RPC_PORT" \
    --zmq-port "$MONERO_ZMQ_PORT" \
    --wallet "$DUMMY_MONERO_WALLET" \
    --stratum "0.0.0.0:$P2POOL_STRATUM_PORT" \
    --data-dir "$TEST_DIR/p2pool_data" \
    --no-igd --mini --no-upnp \
    --merge-mine "127.0.0.1:$MM_RPC_PORT" "$DUMMY_DARKFI_WALLET" \
    > "$TEST_DIR/p2pool.log" 2>&1 &
PIDS="$PIDS $!"

for i in $(seq 1 60); do
    grep -qi "StratumServer\|stratum server" "$TEST_DIR/p2pool.log" 2>/dev/null && break
    sleep 1
done

# ---- Start xmrig ----
"$XMRIG_BIN" \
    -o "127.0.0.1:$P2POOL_STRATUM_PORT" \
    -u x -p 20000 -a rx/0 -t 1 --keepalive --print-time=5 \
    > "$TEST_DIR/xmrig.log" 2>&1 &
PIDS="$PIDS $!"
sleep 2

# ---- Wait for merge mined block ----
echo "Waiting for merge mined darkwow block..."
for i in $(seq 1 300); do
    sleep 1
    N=$(grep -c "RPC-MM.*submit_solution\|BLOCK ACCEPTED" "$TEST_DIR/dwowd.log" 2>/dev/null || echo 0)
    if [ "$N" -gt 0 ] 2>/dev/null; then
        echo "PASS: $N merge mining submission(s) in dwowd log"
        grep "RPC-MM.*submit_solution\|BLOCK ACCEPTED" "$TEST_DIR/dwowd.log" | tail -5
        exit 0
    fi
done

echo "FAIL: no merge mined block within 300s"
tail -15 "$TEST_DIR/dwowd.log"
tail -15 "$TEST_DIR/p2pool.log"
exit 1
