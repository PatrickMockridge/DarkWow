#!/bin/bash
# DarkWow Devnet Entrypoint
# Generates dwowd_config.toml from environment variables at container start.
# A single pre-built image works for any devnet topology.
#
# Usage:
#   docker run --network=host -e ROLE=dwowd -e IS_SEED=true dwow-devnet
#   docker run --network=host -e SEED_ADDR=192.168.1.10:31342 dwow-devnet
#   docker compose up                      # bridge mode (3-node local devnet)
#   docker compose --profile host up       # host mode (multi-machine LAN)

set -e

# --- Configuration from environment ---
ROLE="${ROLE:-dwowd}"
NETWORK_NAME="${NETWORK_NAME:-dwow-devnet}"
IS_SEED="${IS_SEED:-false}"
SEED_ADDR="${SEED_ADDR:-}"
EXTERNAL_ADDR="${EXTERNAL_ADDR:-}"
P2P_PORT="${P2P_PORT:-31342}"
RPC_PORT="${RPC_PORT:-31345}"
STRATUM_PORT="${STRATUM_PORT:-31347}"
MANAGEMENT_PORT="${MANAGEMENT_PORT:-31346}"
TARGET_BLOCK_TIME="${TARGET_BLOCK_TIME:-120}"
MINING_ENABLED="${MINING_ENABLED:-true}"
MINING_THREADS="${MINING_THREADS:-1}"
RANDOMX_MAX_THREADS="${RANDOMX_MAX_THREADS:-0}"
SKIP_SYNC="${SKIP_SYNC:-true}"
LOCALNET="${LOCALNET:-false}"
WALLET_ADDRESS="${WALLET_ADDRESS:-}"
WALLET_SECRET="${WALLET_SECRET:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
LILITH_RPC_PORT="${LILITH_RPC_PORT:-18927}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dwowd/${NETWORK_NAME}}"
LILITH_DATADIR="${LILITH_DATADIR:-/root/.local/share/dwow/lilith/${NETWORK_NAME}}"
CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"
CONFIGFILE="${CONFIGDIR}/dwowd_config.toml"

echo "=== DarkWow Devnet Node ==="
echo "  ROLE=$ROLE  NETWORK_NAME=$NETWORK_NAME"

# --- Derive magic bytes from NETWORK_NAME if not explicitly set ---
if [ -z "$MAGIC_BYTES" ]; then
    if command -v b3sum >/dev/null 2>&1; then
        MAGIC_BYTES=$(echo -n "$NETWORK_NAME" | b3sum --no-names | head -c 8 | \
            xxd -r -p | od -A n -t u1 -w4 | head -1 | sed 's/ /, /g')
    elif command -v openssl >/dev/null 2>&1; then
        RAW=$(echo -n "$NETWORK_NAME" | openssl dgst -blake2b512 -binary | head -c 4 | \
            od -A n -t u1 -w4 | head -1 | sed 's/ /, /g')
        MAGIC_BYTES="$RAW"
    else
        SUM=0; for ((i=0; i<${#NETWORK_NAME}; i++)); do
            SUM=$(( (SUM + $(printf '%d' "'${NETWORK_NAME:$i:1}")) % 256 ))
        done
        MAGIC_BYTES="$SUM, $(( (SUM * 7 + 13) % 256 )), $(( (SUM * 31 + 37) % 256 )), $(( (SUM * 127 + 73) % 256 ))"
        echo "  WARNING: No b3sum/openssl, magic bytes from simple hash: [$MAGIC_BYTES]"
    fi
fi

# ============================================================================
# ROLE: lilith — P2P seed node
# ============================================================================
if [ "$ROLE" = "lilith" ]; then
    echo "  Role: lilith seed node"
    echo "  P2P=$P2P_PORT  LILITH_RPC=$LILITH_RPC_PORT"
    echo "  Magic bytes: [$MAGIC_BYTES]"

    mkdir -p "$(dirname "$LILITH_DATADIR")"

    cat > /tmp/lilith.toml << LILITHEOF
[rpc]
rpc_listen = "tcp://127.0.0.1:${LILITH_RPC_PORT}"

[network."${NETWORK_NAME}"]
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
    exec /app/lilith --config /tmp/lilith.toml "$@"
fi

# ============================================================================
# ROLE: dwowd (default) — fullnode with optional mining
# ============================================================================
echo "  P2P=$P2P_PORT  RPC=$RPC_PORT  STRATUM=$STRATUM_PORT"
echo "  Mining: enabled=$MINING_ENABLED  threads=$MINING_THREADS"
echo "  Magic bytes: [$MAGIC_BYTES]"

# --- Resolve wallet secret: prefer file, fall back to env var ---
RESOLVED_SECRET=""
if [ -n "$WALLET_SECRET_FILE" ] && [ -f "$WALLET_SECRET_FILE" ]; then
    RESOLVED_SECRET=$(cat "$WALLET_SECRET_FILE")
    echo "  Wallet secret: loaded from $WALLET_SECRET_FILE"
elif [ -n "$WALLET_SECRET" ]; then
    echo "  WARNING: WALLET_SECRET from environment is visible in docker inspect."
    echo "  Use WALLET_SECRET_FILE instead for production deployments."
    RESOLVED_SECRET="$WALLET_SECRET"
fi

# --- Build seeds/external_addrs config lines ---
SEEDS_LINE=""
PEERS_LINE=""
EXTERNAL_LINE=""

if [ "$IS_SEED" = "true" ]; then
    echo "  Mode: SEED (no upstream seeds configured)"
else
    if [ -n "$SEED_ADDR" ]; then
        SEEDS_LINE="seeds = [\"tcp+tls://${SEED_ADDR}\"]"
        echo "  Seeds: tcp+tls://${SEED_ADDR}"
    fi
fi

if [ -n "$EXTERNAL_ADDR" ]; then
    EXTERNAL_LINE="external_addrs = [\"tcp+tls://${EXTERNAL_ADDR}\"]"
    echo "  External addr: tcp+tls://${EXTERNAL_ADDR}"
fi

# --- Generate config ---
mkdir -p "$CONFIGDIR" "$DATADIR"

cat > "$CONFIGFILE" << DWOWEOF
network = "${NETWORK_NAME}"

[network_config."${NETWORK_NAME}"]
database = "${DATADIR}"
skip_sync = ${SKIP_SYNC}
txs_batch_size = 50

[network_config."${NETWORK_NAME}".rpc]
rpc_listen = "tcp://0.0.0.0:${RPC_PORT}"

[network_config."${NETWORK_NAME}".stratum_rpc]
rpc_listen = "tcp://0.0.0.0:${STRATUM_PORT}"

[network_config."${NETWORK_NAME}".management_rpc]
rpc_listen = "tcp://127.0.0.1:${MANAGEMENT_PORT}"

[network_config."${NETWORK_NAME}".net]
localnet = ${LOCALNET}
active_profiles = ["tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
magic_bytes = [${MAGIC_BYTES}]
hostlist = "${DATADIR}/hostlist.tsv"

[network_config."${NETWORK_NAME}".net.pow]
target_block_time = ${TARGET_BLOCK_TIME}
initial_target = 16777215
min_target = 1
max_target = 4294967295
min_block_interval = 10
randomx_max_threads = ${RANDOMX_MAX_THREADS:-0}
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

[network_config."${NETWORK_NAME}".net.profiles."tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
DWOWEOF

echo "  Config written to $CONFIGFILE"

# --- Pre-seed mining keypair ---
MINER_ADDRESS_FILE="${DATADIR}/mining_address"
MINER_SECRET_FILE="${DATADIR}/mining_secret"

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
/app/dwowd "$@" &
DWOWD_PID=$!

# --- Start xmrig for mining ---
if [ "$MINING_ENABLED" = "true" ]; then
    MINER_ADDRESS_FILE="${DATADIR}/mining_address"

    if [ -n "$WALLET_ADDRESS" ]; then
        echo "Using provided WALLET_ADDRESS: $WALLET_ADDRESS"
    elif [ -f "$MINER_ADDRESS_FILE" ]; then
        WALLET_ADDRESS=$(cat "$MINER_ADDRESS_FILE")
        echo "Using persisted mining address: $WALLET_ADDRESS"
    else
        echo "Waiting for dwowd to generate mining address..."
        for i in $(seq 1 30); do
            if [ -f "$MINER_ADDRESS_FILE" ]; then
                WALLET_ADDRESS=$(cat "$MINER_ADDRESS_FILE")
                echo "Generated mining address: $WALLET_ADDRESS"
                break
            fi
            sleep 1
        done
    fi

    if [ -n "$WALLET_ADDRESS" ]; then
        echo "Starting xmrig (stratum+tcp://127.0.0.1:$STRATUM_PORT, $MINING_THREADS threads)..."
        xmrig \
            -o "stratum+tcp://127.0.0.1:${STRATUM_PORT}" \
            -u "$WALLET_ADDRESS" \
            -a rx/0 \
            -t "$MINING_THREADS" \
            --keepalive &
    else
        echo "WARNING: No mining address available after 30s, xmrig not started"
    fi
fi

wait $DWOWD_PID
