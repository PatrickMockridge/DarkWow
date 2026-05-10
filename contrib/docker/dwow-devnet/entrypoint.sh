#!/bin/bash
# DarkWow Devnet Entrypoint
# Generates dwowd_config.toml from environment variables at container start.
# A single pre-built image works for any devnet topology.
#
# Usage:
#   docker run --network=host -e IS_SEED=true dwow-devnet
#   docker run --network=host -e SEED_ADDR=192.168.1.10:31342 dwow-devnet

set -e

# --- Configuration from environment ---
NETWORK_NAME="${NETWORK_NAME:-dwow-devnet}"
IS_SEED="${IS_SEED:-false}"
SEED_ADDR="${SEED_ADDR:-}"
EXTERNAL_ADDR="${EXTERNAL_ADDR:-}"
P2P_PORT="${P2P_PORT:-31342}"
RPC_PORT="${RPC_PORT:-31345}"
STRATUM_PORT="${STRATUM_PORT:-31347}"
MANAGEMENT_PORT="${MANAGEMENT_PORT:-31346}"
FIXED_DIFFICULTY="${FIXED_DIFFICULTY:-1}"
TARGET_BLOCK_TIME="${TARGET_BLOCK_TIME:-120}"
MINING_ENABLED="${MINING_ENABLED:-true}"
MINING_THREADS="${MINING_THREADS:-1}"
RANDOMX_MAX_THREADS="${RANDOMX_MAX_THREADS:-0}"
THRESHOLD="${THRESHOLD:-1}"
SKIP_SYNC="${SKIP_SYNC:-true}"
SKIP_FEES="${SKIP_FEES:-true}"
LOCALNET="${LOCALNET:-false}"
WALLET_ADDRESS="${WALLET_ADDRESS:-}"
WALLET_SECRET="${WALLET_SECRET:-}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dwowd/${NETWORK_NAME}}"
CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"
CONFIGFILE="${CONFIGDIR}/dwowd_config.toml"

echo "=== DarkWow Devnet Node ==="
echo "  NETWORK_NAME=$NETWORK_NAME  IS_SEED=$IS_SEED"
echo "  P2P=$P2P_PORT  RPC=$RPC_PORT  STRATUM=$STRATUM_PORT"
echo "  Mining: enabled=$MINING_ENABLED  threads=$MINING_THREADS"

# --- Derive magic bytes from NETWORK_NAME if not explicitly set ---
if [ -z "$MAGIC_BYTES" ]; then
    # Hash the network name with blake3, take first 4 bytes as decimal tuple
    if command -v b3sum >/dev/null 2>&1; then
        MAGIC_BYTES=$(echo -n "$NETWORK_NAME" | b3sum --no-names | head -c 8 | \
            xxd -r -p | od -A n -t u1 -w4 | head -1 | sed 's/ /, /g')
    elif command -v openssl >/dev/null 2>&1; then
        RAW=$(echo -n "$NETWORK_NAME" | openssl dgst -blake2b512 -binary | head -c 4 | \
            od -A n -t u1 -w4 | head -1 | sed 's/ /, /g')
        MAGIC_BYTES="$RAW"
    else
        # Fallback: deterministic bytes from simple character sum
        SUM=0; for ((i=0; i<${#NETWORK_NAME}; i++)); do
            SUM=$(( (SUM + $(printf '%d' "'${NETWORK_NAME:$i:1}")) % 256 ))
        done
        MAGIC_BYTES="$SUM, $(( (SUM * 7 + 13) % 256 )), $(( (SUM * 31 + 37) % 256 )), $(( (SUM * 127 + 73) % 256 ))"
        echo "  WARNING: No b3sum/openssl, magic bytes derived from simple hash: [$MAGIC_BYTES]"
    fi
fi
echo "  Magic bytes: [$MAGIC_BYTES]"

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
threshold = ${THRESHOLD}
max_forks = 8
pow_target = ${TARGET_BLOCK_TIME}
skip_sync = ${SKIP_SYNC}
skip_fees = ${SKIP_FEES}
txs_batch_size = 50

[network_config."${NETWORK_NAME}".pow]
target_block_time = ${TARGET_BLOCK_TIME}
initial_difficulty = 255
min_difficulty = 1
max_difficulty = 4294967295
min_block_interval = 10
randomx_max_threads = ${RANDOMX_MAX_THREADS:-0}
DWOWEOF

# Add fixed difficulty if set
if [ -n "$FIXED_DIFFICULTY" ]; then
    echo "pow_fixed_difficulty = ${FIXED_DIFFICULTY}" >> "$CONFIGFILE"
fi

cat >> "$CONFIGFILE" << DWOWEOF

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

echo "Config written to $CONFIGFILE"

# --- Pre-seed mining keypair if both address and secret are provided ---
MINER_ADDRESS_FILE="${DATADIR}/mining_address"
MINER_SECRET_FILE="${DATADIR}/mining_secret"

if [ -n "$WALLET_ADDRESS" ] && [ -n "$WALLET_SECRET" ]; then
    if [ ! -f "$MINER_ADDRESS_FILE" ] || [ ! -f "$MINER_SECRET_FILE" ]; then
        echo "Pre-seeding mining keypair from WALLET_ADDRESS/WALLET_SECRET..."
        echo "$WALLET_ADDRESS" > "$MINER_ADDRESS_FILE"
        echo "$WALLET_SECRET" > "$MINER_SECRET_FILE"
    fi
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
