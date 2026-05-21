#!/bin/bash
# DarkWow Public Testnet Node — Entrypoint
#
# Single entrypoint dispatching on MODE environment variable.
#   MODE=native  — dwowd + xmrig (solo RandomX mining)
#   MODE=merge   — monerod + dwowd + p2pool + xmrig (merge mining)
#   MODE=lilith  — standalone P2P seed node
#
# Usage:
#   docker run --network=host -e MODE=native darkwow-node/testnet
#   docker run --network=host -e MODE=merge darkwow-node/testnet
#   docker run --network=host -e MODE=lilith darkwow-node/testnet

set -e

# --- Mode selection ---
# Backward compatibility: if MODE is unset but ROLE is set, infer MODE from ROLE
if [ -z "${MODE:-}" ] && [ -n "${ROLE:-}" ]; then
    MODE="$ROLE"
fi
MODE="${MODE:-native}"

# --- Common defaults ---
NETWORK="${NETWORK:-darkwow-testnet}"
P2P_PORT="${P2P_PORT:-31342}"
RPC_PORT="${RPC_PORT:-31345}"
STRATUM_PORT="${STRATUM_PORT:-31347}"
MANAGEMENT_PORT="${MANAGEMENT_PORT:-31346}"
SEED_ADDR="${SEED_ADDR:-lilith0.dark.fi:31340,lilith1.dark.fi:31340}"
EXTERNAL_ADDR="${EXTERNAL_ADDR:-}"
THRESHOLD="${THRESHOLD:-3}"
TARGET_BLOCK_TIME="${TARGET_BLOCK_TIME:-120}"
SKIP_SYNC="${SKIP_SYNC:-false}"
SKIP_FEES="${SKIP_FEES:-false}"
LOCALNET="${LOCALNET:-false}"
WALLET_ADDRESS="${WALLET_ADDRESS:-}"
WALLET_SECRET="${WALLET_SECRET:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
MINING_THREADS="${MINING_THREADS:-1}"
RANDOMX_MAX_THREADS="${RANDOMX_MAX_THREADS:-0}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dwowd/${NETWORK}}"

echo "=== DarkWow Public Testnet Node ==="
echo "  MODE=$MODE  NETWORK=$NETWORK"

# --- Derive magic bytes from NETWORK name ---
derive_magic_bytes() {
    local network="${1:-}"
    if command -v b3sum >/dev/null 2>&1; then
        local net_hash
        net_hash=$(echo -n "$network" | b3sum --no-names | head -c 8)
        local b0=$((16#${net_hash:0:2}))
        local b1=$((16#${net_hash:2:2}))
        local b2=$((16#${net_hash:4:2}))
        local b3=$((16#${net_hash:6:2}))
        echo "$b0, $b1, $b2, $b3"
    elif command -v openssl >/dev/null 2>&1; then
        echo -n "$network" | openssl dgst -blake2b512 -binary | head -c 4 | \
            od -A n -t u1 -w4 | head -1 | tr -s ' ' | sed 's/^ //; s/ /, /g'
    else
        local sum=0
        for ((i=0; i<${#network}; i++)); do
            sum=$(( (sum + $(printf '%d' "'${network:$i:1}")) % 256 ))
        done
        echo "$sum, $(( (sum * 7 + 13) % 256 )), $(( (sum * 31 + 37) % 256 )), $(( (sum * 127 + 73) % 256 ))"
    fi
}

# --- Derive magic bytes from NETWORK if not explicitly set ---
if [ -z "$MAGIC_BYTES" ]; then
    MAGIC_BYTES=$(derive_magic_bytes "$NETWORK")
    if ! command -v b3sum >/dev/null 2>&1 && ! command -v openssl >/dev/null 2>&1; then
        echo "  WARNING: No b3sum/openssl, magic bytes from simple hash: [$MAGIC_BYTES]"
    fi
fi
echo "  Magic bytes: [$MAGIC_BYTES]"

# ============================================================================
# Helper: generate dwowd config
# ============================================================================
generate_dwowd_config() {
    local configfile="${1:-/root/.config/dwow/dwowd_config.toml}"
    local merge_mining="${2:-false}"
    local mm_rpc_port="${3:-31348}"

    CONFIGDIR=$(dirname "$configfile")
    mkdir -p "$CONFIGDIR" "$DATADIR"

    # Parse seed list
    local seeds_line=""
    if [ -n "$SEED_ADDR" ]; then
        local seed_list=""
        IFS=',' read -ra SEEDS <<< "$SEED_ADDR"
        for seed in "${SEEDS[@]}"; do
            seed=$(echo "$seed" | xargs)
            if [ -z "$seed_list" ]; then
                seed_list="\"tcp+tls://${seed}\""
            else
                seed_list="${seed_list}, \"tcp+tls://${seed}\""
            fi
        done
        seeds_line="seeds = [${seed_list}]"
    fi

    local external_line=""
    if [ -n "$EXTERNAL_ADDR" ]; then
        external_line="external_addrs = [\"tcp+tls://${EXTERNAL_ADDR}\"]"
    fi

    cat > "$configfile" << DWOWEOF
network = "${NETWORK}"

[network_config."${NETWORK}"]
database = "${DATADIR}"
threshold = ${THRESHOLD}
max_forks = 8
pow_target = ${TARGET_BLOCK_TIME}
skip_sync = ${SKIP_SYNC}
skip_fees = ${SKIP_FEES}
txs_batch_size = 50

[network_config."${NETWORK}".pow]
target_block_time = ${TARGET_BLOCK_TIME}
initial_target = 16777215
min_target = 1
max_target = 4294967295
min_block_interval = 10
randomx_max_threads = ${RANDOMX_MAX_THREADS:-0}

[network_config."${NETWORK}".rpc]
rpc_listen = "tcp://0.0.0.0:${RPC_PORT}"

[network_config."${NETWORK}".stratum_rpc]
rpc_listen = "tcp://0.0.0.0:${STRATUM_PORT}"

[network_config."${NETWORK}".management_rpc]
rpc_listen = "tcp://127.0.0.1:${MANAGEMENT_PORT}"

[network_config."${NETWORK}".net]
localnet = ${LOCALNET}
active_profiles = ["tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
magic_bytes = [${MAGIC_BYTES}]
hostlist = "${DATADIR}/hostlist.tsv"
${seeds_line}
${external_line}

[network_config."${NETWORK}".net.profiles."tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
DWOWEOF

    if [ "$merge_mining" = "true" ]; then
        cat >> "$configfile" << DWOWEOF

[network_config."${NETWORK}".mm_rpc]
rpc_listen = "tcp://0.0.0.0:${mm_rpc_port}"
DWOWEOF
    fi

    echo "  Config written to $configfile"
}

# ============================================================================
# Helper: pre-seed mining keypair
# ============================================================================
preseed_wallet() {
    local miner_addr_file="${DATADIR}/mining_address"
    local miner_secret_file="${DATADIR}/mining_secret"
    mkdir -p "$DATADIR"

    local resolved_secret=""
    if [ -n "$WALLET_SECRET_FILE" ] && [ -f "$WALLET_SECRET_FILE" ]; then
        resolved_secret=$(cat "$WALLET_SECRET_FILE")
    elif [ -n "$WALLET_SECRET" ]; then
        echo "  WARNING: WALLET_SECRET from environment is visible in docker inspect."
        echo "  Use WALLET_SECRET_FILE instead for production deployments."
        resolved_secret="$WALLET_SECRET"
    fi

    if [ -n "$WALLET_ADDRESS" ] && [ -n "$resolved_secret" ]; then
        if [ ! -f "$miner_addr_file" ] || [ ! -f "$miner_secret_file" ]; then
            echo "  Pre-seeding mining keypair..."
            echo "$WALLET_ADDRESS" > "$miner_addr_file"
            echo "$resolved_secret" > "$miner_secret_file"
        fi
    elif [ -z "$resolved_secret" ] && [ ! -f "$miner_secret_file" ]; then
        echo "  No wallet secret provided — dwowd will auto-generate a random mining keypair."
        echo "  Mining rewards go to an address whose secret exists only in this container."
        echo "  To use a pre-configured wallet, set WALLET_SECRET_FILE."
    fi
}

# ============================================================================
# Helper: wait for dwowd to produce a mining address, then start xmrig
# ============================================================================
start_xmrig_native() {
    local miner_addr_file="${DATADIR}/mining_address"

    if [ -n "$WALLET_ADDRESS" ]; then
        echo "  Using provided WALLET_ADDRESS: $WALLET_ADDRESS"
    elif [ -f "$miner_addr_file" ]; then
        WALLET_ADDRESS=$(cat "$miner_addr_file")
        echo "  Using persisted mining address: $WALLET_ADDRESS"
    else
        echo "  Waiting for dwowd to generate mining address..."
        for i in $(seq 1 30); do
            if [ -f "$miner_addr_file" ]; then
                WALLET_ADDRESS=$(cat "$miner_addr_file")
                echo "  Generated mining address: $WALLET_ADDRESS"
                break
            fi
            sleep 1
        done
    fi

    if [ -n "$WALLET_ADDRESS" ]; then
        echo "  Starting xmrig (stratum+tcp://127.0.0.1:$STRATUM_PORT, $MINING_THREADS threads)..."
        xmrig \
            -o "stratum+tcp://127.0.0.1:${STRATUM_PORT}" \
            -u "$WALLET_ADDRESS" \
            -a rx/0 \
            -t "$MINING_THREADS" \
            --keepalive &
    else
        echo "  WARNING: No mining address available after 30s, xmrig not started"
    fi
}

# ============================================================================
# Source-only guard — allow tests to load functions without executing main flow
# ============================================================================
if [ -n "${ENTRYPOINT_SOURCE_ONLY:-}" ]; then
    return 0 2>/dev/null || exit 0
fi

# ============================================================================
# MODE: lilith — standalone P2P seed node
# ============================================================================
if [ "$MODE" = "lilith" ]; then
    echo "=== Mode: lilith (P2P seed) ==="
    LILITH_RPC_PORT="${LILITH_RPC_PORT:-18927}"
    LILITH_DATADIR="${LILITH_DATADIR:-/root/.local/share/dwow/lilith/${NETWORK}}"

    mkdir -p "$(dirname "$LILITH_DATADIR")"

    cat > /tmp/lilith.toml << LILITHEOF
[rpc]
rpc_listen = "tcp://127.0.0.1:${LILITH_RPC_PORT}"

[network."${NETWORK}"]
accept_addrs = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
seeds = []
peers = []
version = "0.5.0"
app_name = "dwowd"
localnet = ${LOCALNET}
hostlist = "${LILITH_DATADIR}/hostlist.tsv"
datastore = "${LILITH_DATADIR}"
magic_bytes = [${MAGIC_BYTES}]
LILITHEOF

    echo "  P2P accept: tcp+tls://0.0.0.0:${P2P_PORT}"
    echo "  Config: /tmp/lilith.toml"
    exec /app/lilith --config /tmp/lilith.toml
fi

# ============================================================================
# MODE: native (default) — dwowd + xmrig solo mining
# ============================================================================
if [ "$MODE" = "native" ]; then
    echo "=== Mode: native (solo RandomX mining) ==="
    echo "  P2P=$P2P_PORT  RPC=$RPC_PORT  STRATUM=$STRATUM_PORT"
    echo "  Seeds: $SEED_ADDR"

    CONFIGFILE="/root/.config/dwow/dwowd_config.toml"
    generate_dwowd_config "$CONFIGFILE" "false"
    preseed_wallet

    echo "  Starting dwowd..."
    /app/dwowd &
    DWOWD_PID=$!

    start_xmrig_native

    wait $DWOWD_PID
fi

# ============================================================================
# MODE: merge — monerod + dwowd + p2pool + xmrig
# ============================================================================
if [ "$MODE" = "merge" ]; then
    echo "=== Mode: merge (Monero merge mining via p2pool) ==="

    # --- Merge mining env vars ---
    MM_RPC_PORT="${MM_RPC_PORT:-31348}"
    MONERO_OFFLINE="${MONERO_OFFLINE:-false}"
    MONERO_NETWORK="${MONERO_NETWORK:-testnet}"
    MONERO_RPC_PORT="${MONERO_RPC_PORT:-28081}"
    MONERO_ZMQ_PORT="${MONERO_ZMQ_PORT:-28083}"
    MONERO_FIXED_DIFFICULTY="${MONERO_FIXED_DIFFICULTY:-20000}"
    MONERO_ADD_PEERS="${MONERO_ADD_PEERS:-125.229.105.12:28081,37.187.74.171:28089}"
    P2POOL_STRATUM_PORT="${P2POOL_STRATUM_PORT:-3333}"
    MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"
    XMERGE_THREADS="${XMERGE_THREADS:-2}"

    echo "  P2P=$P2P_PORT  RPC=$RPC_PORT  MM_RPC=$MM_RPC_PORT"
    echo "  Monero: offline=$MONERO_OFFLINE network=$MONERO_NETWORK"
    echo "  Seeds: $SEED_ADDR"

    # --- 1. Start monerod ---
    echo
    echo "  [1/4] Starting monerod..."
    MONERO_ARGS="--non-interactive --no-igd --data-dir /root/.bitmonero --log-level 1 --hide-my-port"
    MONERO_ARGS="$MONERO_ARGS --zmq-pub tcp://0.0.0.0:${MONERO_ZMQ_PORT}"
    MONERO_ARGS="$MONERO_ARGS --rpc-bind-ip 0.0.0.0 --rpc-bind-port ${MONERO_RPC_PORT} --confirm-external-bind"

    if [ "$MONERO_OFFLINE" = "true" ]; then
        echo "  monerod: offline mode (fixed difficulty $MONERO_FIXED_DIFFICULTY)"
        MONERO_ARGS="$MONERO_ARGS --offline --fixed-difficulty ${MONERO_FIXED_DIFFICULTY} --disable-rpc-ban"
    else
        echo "  monerod: syncing Monero $MONERO_NETWORK"
        if [ "$MONERO_NETWORK" = "testnet" ]; then
            MONERO_ARGS="$MONERO_ARGS --testnet"
        fi
        if [ -n "$MONERO_ADD_PEERS" ]; then
            IFS=',' read -ra PEERS <<< "$MONERO_ADD_PEERS"
            for peer in "${PEERS[@]}"; do
                peer=$(echo "$peer" | xargs)
                MONERO_ARGS="$MONERO_ARGS --add-peer $peer"
            done
            echo "  monerod: bootstrap peers: $MONERO_ADD_PEERS"
        fi
    fi

    monerod $MONERO_ARGS &
    MONEROD_PID=$!

    # Wait for monerod RPC
    echo "  Waiting for monerod RPC (port $MONERO_RPC_PORT)..."
    for i in $(seq 1 60); do
        if curl -s http://127.0.0.1:${MONERO_RPC_PORT}/get_height -X POST \
            -H 'Content-Type: application/json' -d '{"jsonrpc":"2.0","id":"0","method":"get_height"}' \
            >/dev/null 2>&1; then
            echo "  monerod RPC ready"
            break
        fi
        sleep 2
    done

    # --- 2. Start dwowd with merge mining ---
    echo
    echo "  [2/4] Starting dwowd (merge mining enabled)..."
    CONFIGFILE="/root/.config/dwow/dwowd_config.toml"
    generate_dwowd_config "$CONFIGFILE" "true" "$MM_RPC_PORT"
    preseed_wallet

    /app/dwowd &
    DWOWD_PID=$!

    # Wait for dwowd mm_rpc
    echo "  Waiting for dwowd mm_rpc (port $MM_RPC_PORT)..."
    for i in $(seq 1 30); do
        if timeout 2 bash -c "exec 3<>/dev/tcp/127.0.0.1/$MM_RPC_PORT" 2>/dev/null; then
            echo "  dwowd mm_rpc ready"
            break
        fi
        sleep 2
    done

    # --- 3. Start p2pool ---
    echo
    echo "  [3/4] Starting p2pool..."

    if [ -z "$MONERO_WALLET_ADDRESS" ]; then
        echo "  WARNING: MONERO_WALLET_ADDRESS not set — using dummy address"
        MONERO_WALLET_ADDRESS="9wenrVcFffvbTR4nEQ7KAbDMw7bq6B7uwgsraJzFVLkq9SiqMYFf72544RyXLaXKmZYfYNdcdZWpKaBv5dD8xkpS5djBZPM"
    fi

    MONERO_NET_FLAG=""
    if [ "$MONERO_NETWORK" = "testnet" ]; then
        MONERO_NET_FLAG="--testnet --mini"
    fi

    p2pool \
        --host 127.0.0.1 \
        --rpc-port "$MONERO_RPC_PORT" \
        --zmq-port "$MONERO_ZMQ_PORT" \
        --wallet "$MONERO_WALLET_ADDRESS" \
        --stratum "0.0.0.0:${P2POOL_STRATUM_PORT}" \
        --data-dir /root/.p2pool \
        --no-igd \
        $MONERO_NET_FLAG \
        --merge-mine "127.0.0.1:${MM_RPC_PORT}" "${WALLET_ADDRESS}" &
    P2POOL_PID=$!

    # Wait for p2pool stratum
    echo "  Waiting for p2pool stratum (port $P2POOL_STRATUM_PORT)..."
    for i in $(seq 1 30); do
        if timeout 2 bash -c "exec 3<>/dev/tcp/127.0.0.1/$P2POOL_STRATUM_PORT" 2>/dev/null; then
            echo "  p2pool stratum ready"
            break
        fi
        sleep 2
    done

    # --- 4. Start xmrig ---
    echo
    echo "  [4/4] Starting xmrig ($XMERGE_THREADS threads, p2pool stratum 127.0.0.1:$P2POOL_STRATUM_PORT)..."
    xmrig \
        -o "127.0.0.1:${P2POOL_STRATUM_PORT}" \
        -a rx/0 \
        -t "$XMERGE_THREADS" \
        --keepalive &
    XMRIG_PID=$!

    echo
    echo "=== All services started ==="
    echo "  monerod PID:   $MONEROD_PID"
    echo "  dwowd PID:     $DWOWD_PID"
    echo "  p2pool PID:    $P2POOL_PID"
    echo "  xmrig PID:     $XMRIG_PID"
    echo
    echo "  Logs: docker logs -f <container>"
    echo "  RPC:  curl -s http://127.0.0.1:$RPC_PORT -X POST -H 'Content-Type: application/json' -d '{\"method\":\"blockchain.info\",\"params\":[],\"id\":1}'"
    echo "  Stop: docker stop <container>"

    # Wait on all; restart any that die
    while true; do
        if ! kill -0 $DWOWD_PID 2>/dev/null; then
            echo "  ERROR: dwowd died — exiting"
            exit 1
        fi
        if ! kill -0 $MONEROD_PID 2>/dev/null; then
            echo "  ERROR: monerod died — exiting"
            exit 1
        fi
        if ! kill -0 $P2POOL_PID 2>/dev/null; then
            echo "  WARNING: p2pool died — restarting..."
            p2pool \
                --host 127.0.0.1 \
                --rpc-port "$MONERO_RPC_PORT" \
                --zmq-port "$MONERO_ZMQ_PORT" \
                --wallet "$MONERO_WALLET_ADDRESS" \
                --stratum "0.0.0.0:${P2POOL_STRATUM_PORT}" \
                --data-dir /root/.p2pool \
                --no-igd \
                $MONERO_NET_FLAG \
                --merge-mine "127.0.0.1:${MM_RPC_PORT}" "${WALLET_ADDRESS}" &
            P2POOL_PID=$!
        fi
        if ! kill -0 $XMRIG_PID 2>/dev/null; then
            echo "  WARNING: xmrig died — restarting..."
            xmrig \
                -o "127.0.0.1:${P2POOL_STRATUM_PORT}" \
                -a rx/0 \
                -t "$XMERGE_THREADS" \
                --keepalive &
            XMRIG_PID=$!
        fi
        sleep 10
    done
fi

# --- Unknown MODE ---
echo "ERROR: Unknown MODE='$MODE'. Valid modes: native, merge, lilith"
exit 1
