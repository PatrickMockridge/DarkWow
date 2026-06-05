#!/bin/bash
# DarkWow Testnet Entrypoint
# Generates config from environment variables at container start.
# Supports two roles: lilith (P2P seed) and dwowd (fullnode + optional miner).
#
# Usage:
#   docker compose up                              # 3-node local testnet
#   docker run --network=host -e ROLE=lilith ...   # standalone seed
#   docker run --network=host -e SEED_ADDR=...     # join existing devnet

set -e

# --- Configuration from environment ---
ROLE="${ROLE:-dwowd}"
NETWORK="${NETWORK:-darkwow-testnet}"
P2P_PORT="${P2P_PORT:-31342}"
RPC_PORT="${RPC_PORT:-31345}"
STRATUM_PORT="${STRATUM_PORT:-31347}"
MANAGEMENT_PORT="${MANAGEMENT_PORT:-31346}"
SEED_ADDR="${SEED_ADDR:-}"
PEER_ADDR="${PEER_ADDR:-}"
EXTERNAL_ADDR="${EXTERNAL_ADDR:-}"
IS_SEED="${IS_SEED:-false}"
FIXED_DIFFICULTY="${FIXED_DIFFICULTY:-}"
TARGET_BLOCK_TIME="${TARGET_BLOCK_TIME:-120}"
MINING_ENABLED="${MINING_ENABLED:-true}"
MINING_THREADS="${MINING_THREADS:-2}"
RANDOMX_MAX_THREADS="${RANDOMX_MAX_THREADS:-0}"
THRESHOLD="${THRESHOLD:-3}"
SKIP_SYNC="${SKIP_SYNC:-false}"
SKIP_FEES="${SKIP_FEES:-false}"
LOCALNET="${LOCALNET:-false}"
WALLET_ADDRESS="${WALLET_ADDRESS:-}"
WALLET_SECRET="${WALLET_SECRET:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
MERGE_MINING="${MERGE_MINING:-false}"
MM_RPC_PORT="${MM_RPC_PORT:-31348}"
FINALITY_MODE="${FINALITY_MODE:-always}"
FINALITY_DISABLE_CARIBINA="${FINALITY_DISABLE_CARIBINA:-false}"
FINALITY_ENABLE_MONERO="${FINALITY_ENABLE_MONERO:-false}"
MONERO_MIN_CONFIRMATIONS="${MONERO_MIN_CONFIRMATIONS:-3}"
MONEROD_RPC_URL="${MONEROD_RPC_URL:-}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dwowd/${NETWORK}}"
LILITH_DATADIR="${LILITH_DATADIR:-/root/.local/share/dwow/lilith/${NETWORK}}"

echo "=== DarkWow Testnet Node ==="
echo "  ROLE=$ROLE  NETWORK=$NETWORK"
echo "  P2P=$P2P_PORT  RPC=$RPC_PORT  STRATUM=$STRATUM_PORT"

# --- Derive magic bytes from NETWORK if not explicitly set ---
if [ -z "$MAGIC_BYTES" ]; then
    if command -v b3sum >/dev/null 2>&1; then
        NET_HASH=$(echo -n "$NETWORK" | b3sum --no-names | head -c 8)
        B0=$((16#${NET_HASH:0:2}))
        B1=$((16#${NET_HASH:2:2}))
        B2=$((16#${NET_HASH:4:2}))
        B3=$((16#${NET_HASH:6:2}))
        MAGIC_BYTES="$B0, $B1, $B2, $B3"
    elif command -v openssl >/dev/null 2>&1; then
        RAW=$(echo -n "$NETWORK" | openssl dgst -blake2b512 -binary | head -c 4 | \
            od -A n -t u1 -w4 | head -1 | sed 's/ /, /g')
        MAGIC_BYTES="$RAW"
    else
        SUM=0; for ((i=0; i<${#NETWORK}; i++)); do
            SUM=$(( (SUM + $(printf '%d' "'${NETWORK:$i:1}")) % 256 ))
        done
        MAGIC_BYTES="$SUM, $(( (SUM * 7 + 13) % 256 )), $(( (SUM * 31 + 37) % 256 )), $(( (SUM * 127 + 73) % 256 ))"
        echo "  WARNING: No b3sum/openssl, magic bytes from simple hash: [$MAGIC_BYTES]"
    fi
fi
echo "  Magic bytes: [$MAGIC_BYTES]"

# ============================================================================
# ROLE: lilith — P2P seed node
# ============================================================================
if [ "$ROLE" = "lilith" ]; then
    echo "  Mode: lilith seed node"
    LILITH_RPC_PORT="${LILITH_RPC_PORT:-18927}"

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

    echo "  Lilith config written to /tmp/lilith.toml"
    echo "  P2P accept: tcp+tls://0.0.0.0:${P2P_PORT}"
    echo "Starting lilith..."
    exec /app/lilith --config /tmp/lilith.toml
fi

# ============================================================================
# ROLE: dwowd (default) — fullnode with optional mining
# ============================================================================

CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"
CONFIGFILE="${CONFIGDIR}/dwowd_config.toml"

# --- Build seeds / peers / external_addrs config lines ---
SEEDS_LINE=""
PEERS_LINE=""
EXTERNAL_LINE=""

if [ "$IS_SEED" = "true" ]; then
    echo "  Mode: SEED (no upstream seeds configured)"
else
    if [ -n "$SEED_ADDR" ]; then
        SEED_LIST=""
        IFS=',' read -ra SEEDS <<< "$SEED_ADDR"
        for seed in "${SEEDS[@]}"; do
            seed=$(echo "$seed" | xargs)
            if [ -z "$SEED_LIST" ]; then
                SEED_LIST="\"tcp+tls://${seed}\""
            else
                SEED_LIST="${SEED_LIST}, \"tcp+tls://${seed}\""
            fi
        done
        SEEDS_LINE="seeds = [${SEED_LIST}]"
        echo "  Seeds: ${SEED_LIST}"
    else
        echo "  ================================================================"
        echo "  WARNING: No SEED_ADDR configured and IS_SEED is not true."
        echo "  This node will start but will NEVER connect to the P2P network."
        echo "  Set SEED_ADDR to a comma-separated list of seed host:port, e.g.:"
        echo "    SEED_ADDR=lilith0.dark.fi:31340,lilith1.dark.fi:31340"
        echo "  ================================================================"
    fi
fi

if [ -n "$PEER_ADDR" ]; then
    PEER_LIST=""
    IFS=',' read -ra PEERS <<< "$PEER_ADDR"
    for peer in "${PEERS[@]}"; do
        peer=$(echo "$peer" | xargs)  # trim whitespace
        if [ -z "$PEER_LIST" ]; then
            PEER_LIST="\"tcp+tls://${peer}\""
        else
            PEER_LIST="${PEER_LIST}, \"tcp+tls://${peer}\""
        fi
    done
    PEERS_LINE="peers = [${PEER_LIST}]"
    echo "  Peers: ${PEER_LIST}"
fi

if [ -n "$EXTERNAL_ADDR" ]; then
    EXTERNAL_LINE="external_addrs = [\"tcp+tls://${EXTERNAL_ADDR}\"]"
    echo "  External addr: tcp+tls://${EXTERNAL_ADDR}"
fi

# --- Generate dwowd config ---
mkdir -p "$CONFIGDIR" "$DATADIR"

cat > "$CONFIGFILE" << DWOWEOF
network = "${NETWORK}"

[network_config."${NETWORK}"]
database = "${DATADIR}"
threshold = ${THRESHOLD}
max_forks = 8
pow_target = ${TARGET_BLOCK_TIME}
skip_sync = ${SKIP_SYNC}
create_genesis = ${CREATE_GENESIS:-false}
skip_fees = ${SKIP_FEES}
txs_batch_size = 50

[network_config."${NETWORK}".pow]
target_block_time = ${TARGET_BLOCK_TIME}
initial_target = 268435455
min_target = 1
max_target = 4294967295
min_block_interval = 10
randomx_max_threads = ${RANDOMX_MAX_THREADS:-0}
DWOWEOF

if [ -n "$FIXED_DIFFICULTY" ]; then
    echo "pow_fixed_difficulty = ${FIXED_DIFFICULTY}" >> "$CONFIGFILE"
fi

cat >> "$CONFIGFILE" << DWOWEOF

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
DWOWEOF

if [ -n "$SEEDS_LINE" ]; then
    echo "$SEEDS_LINE" >> "$CONFIGFILE"
fi
if [ -n "$PEERS_LINE" ]; then
    echo "$PEERS_LINE" >> "$CONFIGFILE"
fi
if [ -n "$EXTERNAL_LINE" ]; then
    echo "$EXTERNAL_LINE" >> "$CONFIGFILE"
fi

cat >> "$CONFIGFILE" << DWOWEOF

[network_config."${NETWORK}".net.profiles."tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
DWOWEOF

# --- Merge mining RPC config (optional) ---
if [ "$MERGE_MINING" = "true" ]; then
    cat >> "$CONFIGFILE" << DWOWEOF

[network_config."${NETWORK}".mm_rpc]
rpc_listen = "http+tcp://0.0.0.0:${MM_RPC_PORT}"
DWOWEOF
    echo "  Merge mining RPC: http+tcp://0.0.0.0:${MM_RPC_PORT} (HTTP JSON-RPC for p2pool)"
fi

# In merge mining mode, default Monero finality to true since merge
# mining is the only source of Monero anchors.
if [ "$MERGE_MINING" = "true" ]; then
    FINALITY_ENABLE_MONERO="${FINALITY_ENABLE_MONERO:-true}"
    MONEROD_RPC_URL="${MONEROD_RPC_URL:-http://monerod:28081/json_rpc}"
fi

# --- Finality config ---
FINALITY_CARIBINA_ENABLED="true"
if [ "$FINALITY_DISABLE_CARIBINA" = "true" ]; then
    FINALITY_CARIBINA_ENABLED="false"
fi
cat >> "$CONFIGFILE" << DWOWEOF

[network_config."${NETWORK}".finality]
mode = "${FINALITY_MODE}"
caribina_enabled = ${FINALITY_CARIBINA_ENABLED}
monero_enabled = ${FINALITY_ENABLE_MONERO}
monero_min_confirmations = ${MONERO_MIN_CONFIRMATIONS}
DWOWEOF
if [ -n "$MONEROD_RPC_URL" ]; then
    echo "monerod_url = \"${MONEROD_RPC_URL}\"" >> "$CONFIGFILE"
fi
echo "  Finality: mode=${FINALITY_MODE} caribina_enabled=${FINALITY_CARIBINA_ENABLED} monero_enabled=${FINALITY_ENABLE_MONERO}"

echo "  Config written to $CONFIGFILE"

# --- Pre-seed mining keypair ---
MINER_ADDRESS_FILE="${DATADIR}/mining_address"
MINER_SECRET_FILE="${DATADIR}/mining_secret"

# Resolve wallet secret: prefer file, fall back to env var
RESOLVED_SECRET=""
if [ -n "$WALLET_SECRET_FILE" ] && [ -f "$WALLET_SECRET_FILE" ]; then
    RESOLVED_SECRET=$(cat "$WALLET_SECRET_FILE")
elif [ -n "$WALLET_SECRET" ]; then
    echo "  WARNING: WALLET_SECRET from environment is visible in docker inspect."
    echo "  Use WALLET_SECRET_FILE instead for production deployments."
    RESOLVED_SECRET="$WALLET_SECRET"
fi

if [ -n "$WALLET_ADDRESS" ] && [ -n "$RESOLVED_SECRET" ]; then
    if [ ! -f "$MINER_ADDRESS_FILE" ] || [ ! -f "$MINER_SECRET_FILE" ]; then
        echo "Pre-seeding mining keypair..."
        echo "$WALLET_ADDRESS" > "$MINER_ADDRESS_FILE"
        echo "$RESOLVED_SECRET" > "$MINER_SECRET_FILE"
    fi
elif [ -z "$RESOLVED_SECRET" ] && [ ! -f "$MINER_SECRET_FILE" ]; then
    echo "No wallet secret provided — dwowd will auto-generate a random mining keypair."
    echo "Mining rewards will go to an address whose secret exists only in this container."
    echo "To use a pre-configured wallet, set WALLET_SECRET_FILE."
fi

# --- Start dwowd ---
echo "Starting dwowd..."
/app/dwowd &
DWOWD_PID=$!

# --- Mining ---
# Mining is handled internally by the dwowd node (built-in miner task).
# The node reads the mining address from the persisted file and mines
# blocks in a background loop — no external script needed.
# This matches production topology (Bitcoin Core -gen, Geth --mine).
if [ "$MINING_ENABLED" = "true" ] && [ "$MINING_THREADS" -gt 0 ]; then
    echo "Mining enabled — node will mine internally via built-in miner task"
fi

# --- Start p2pool sidecar for merge mining ---
# Each merge-mining node runs its own p2pool instance as a sidecar.
# p2pool connects to monerod and dwowd's mm_rpc, and exposes stratum
# on localhost for xmrig. Readiness checks ensure all dependencies
# are up before starting — no blind sleep-based races.
if [ "$MERGE_MINING" = "true" ] && [ "$MINING_THREADS" -gt 0 ]; then
    MONEROD_HOST="${MONEROD_HOST:-monerod}"
    MONEROD_RPC_PORT="${MONEROD_RPC_PORT:-28081}"
    MONEROD_ZMQ_PORT="${MONEROD_ZMQ_PORT:-28083}"
    P2POOL_STRATUM_PORT="${P2POOL_STRATUM_PORT:-3333}"
    MM_RPC_PORT="${MM_RPC_PORT:-31348}"
    MONERO_NETWORK="${MONERO_NETWORK:-testnet}"

    # Capture DarkWow wallet before xmrig block can modify WALLET_ADDRESS
    MERGE_MINE_WALLET="$WALLET_ADDRESS"

    # B-2: Wait for monerod RPC to be ready
    echo "Merge mining: waiting for monerod RPC at ${MONEROD_HOST}:${MONEROD_RPC_PORT}..."
    for i in $(seq 1 60); do
        if curl -s --max-time 2 "http://${MONEROD_HOST}:${MONEROD_RPC_PORT}/json_rpc" \
            -H 'Content-Type: application/json' \
            -d '{"jsonrpc":"2.0","method":"get_info","id":1}' 2>/dev/null | grep -q "result"; then
            echo "  monerod RPC ready (attempt $i)"
            break
        fi
        [ "$i" -eq 60 ] && echo "  WARNING: monerod RPC not ready after 60 attempts"
        sleep 2
    done

    # B-3: Wait for dwowd mm_rpc to be ready
    echo "Merge mining: waiting for dwowd mm_rpc at 127.0.0.1:${MM_RPC_PORT}..."
    for i in $(seq 1 60); do
        if curl -s --max-time 2 "http://127.0.0.1:${MM_RPC_PORT}" \
            -H 'Content-Type: application/json' \
            -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","params":[],"id":1}' 2>/dev/null | grep -q "result"; then
            echo "  mm_rpc ready (attempt $i)"
            break
        fi
        [ "$i" -eq 60 ] && echo "  WARNING: mm_rpc not ready after 60 attempts"
        sleep 2
    done

    # W-3: Warn if using hardcoded Monero wallet in online mode
    MONERO_WALLET="${MONERO_WALLET_ADDRESS:-9yMzH45FsTfM3Pa7Smmpc2Kk42zUgHHD5zPkAsiVpQFx7xajE2z7Rjz9E1SGfPbjRxDg5QVJ1b4MUpoxx3vVSKRQ8SPf9qD}"
    if [ -z "$MONERO_WALLET_ADDRESS" ]; then
        echo "  NOTE: Using default Monero testnet wallet address (set MONERO_WALLET_ADDRESS to override)"
    fi

    # Start p2pool sidecar (B-1: --mini flag for testnet)
    echo "Merge mining: starting p2pool sidecar..."
    P2POOL_ARGS="--host ${MONEROD_HOST} --rpc-port ${MONEROD_RPC_PORT} --zmq-port ${MONEROD_ZMQ_PORT}"
    P2POOL_ARGS="$P2POOL_ARGS --wallet ${MONERO_WALLET}"
    P2POOL_ARGS="$P2POOL_ARGS --stratum 0.0.0.0:${P2POOL_STRATUM_PORT} --no-randomx --no-igd --mini"
    P2POOL_ARGS="$P2POOL_ARGS --merge-mine 127.0.0.1:${MM_RPC_PORT} ${MERGE_MINE_WALLET}"
    P2POOL_ARGS="$P2POOL_ARGS --data-dir /tmp/p2pool"
    p2pool $P2POOL_ARGS > /tmp/p2pool.log 2>&1 &
    P2POOL_PID=$!
    echo "  p2pool sidecar started (PID=$P2POOL_PID, stratum=0.0.0.0:${P2POOL_STRATUM_PORT})"

    # B-4: Wait for p2pool stratum port to be ready before starting xmrig
    echo "Merge mining: waiting for p2pool stratum at 127.0.0.1:${P2POOL_STRATUM_PORT}..."
    for i in $(seq 1 30); do
        if timeout 1 bash -c "exec 3<>/dev/tcp/127.0.0.1/${P2POOL_STRATUM_PORT}" 2>/dev/null; then
            echo "  p2pool stratum ready (attempt $i)"
            break
        fi
        # Check if p2pool is still alive
        if ! kill -0 $P2POOL_PID 2>/dev/null; then
            echo "  ERROR: p2pool exited unexpectedly — check /tmp/p2pool.log"
            cat /tmp/p2pool.log 2>/dev/null | tail -20
            break
        fi
        [ "$i" -eq 30 ] && echo "  WARNING: p2pool stratum not ready after 30 attempts"
        sleep 2
    done
fi

# --- Start xmrig for merge mining ---
# xmrig runs as a sidecar inside the node container, connecting to the
# locally-running p2pool sidecar's stratum port on 127.0.0.1.
if [ "$MERGE_MINING" = "true" ] && [ "$MINING_THREADS" -gt 0 ]; then
    echo "Merge mining: starting xmrig sidecar (stratum=127.0.0.1:${P2POOL_STRATUM_PORT:-3333})..."
    MINER_ADDRESS_FILE="${DATADIR}/mining_address"

    if [ -z "$WALLET_ADDRESS" ] && [ -f "$MINER_ADDRESS_FILE" ]; then
        WALLET_ADDRESS=$(cat "$MINER_ADDRESS_FILE")
        echo "  Using persisted mining address: $WALLET_ADDRESS"
    fi

    if [ -n "$WALLET_ADDRESS" ]; then
        xmrig \
            -o "stratum+tcp://127.0.0.1:${P2POOL_STRATUM_PORT:-3333}" \
            -u "$WALLET_ADDRESS" \
            -a rx/0 \
            -t "$MINING_THREADS" \
            --keepalive &
        echo "  xmrig sidecar started"
    else
        echo "  WARNING: No wallet address, xmrig merge mining not started"
    fi
fi

wait $DWOWD_PID
