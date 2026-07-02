#!/bin/bash
# DarkWow Testnet Entrypoint
# Generates config from environment variables at container start.
# Supports three modes:
#   - IS_SEED=true (observer): full node, no mining, no upstream seeds
#   - SEED_ADDR set: full node that bootstraps via seeds
#   - PEER_ADDR set: full node that bootstraps via static peers
# Also supports ROLE=lilith for non-blockchain P2P services (darkirc, tau).
#
# Usage:
#   docker compose up                              # 3-node local testnet
#   docker run --network=host -e ROLE=lilith ...   # standalone P2P seed (non-blockchain)
#   docker run --network=host -e SEED_ADDR=...     # join existing devnet

set -e -o pipefail

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
MINING_THREADS="${MINING_THREADS:-1}"
DWOW_RAYON_THREADS="${DWOW_RAYON_THREADS:-2}"
export RAYON_NUM_THREADS="${DWOW_RAYON_THREADS}"
RANDOMX_MAX_THREADS="${RANDOMX_MAX_THREADS:-0}"
THRESHOLD="${THRESHOLD:-3}"
SKIP_SYNC="${SKIP_SYNC:-false}"
SKIP_FEES="${SKIP_FEES:-false}"
LOCALNET="${LOCALNET:-false}"
P2P_LOCAL="${P2P_LOCAL:-false}"
MINING_EASY="${MINING_EASY:-false}"
WALLET_ADDRESS="${WALLET_ADDRESS:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
FORWARD_DESTINATION="${FORWARD_DESTINATION:-}"
MERGE_MINING="${MERGE_MINING:-false}"
MM_RPC_PORT="${MM_RPC_PORT:-31348}"
FINALITY_MODE="${FINALITY_MODE:-always}"
FINALITY_CARIBINA_ENABLED="${FINALITY_CARIBINA_ENABLED:-true}"
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
    echo "FATAL: MAGIC_BYTES is required — set in docker-compose.yml or environment"
    exit 1
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
    echo "  Mode: observer (IS_SEED, no upstream seeds configured)"
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
    elif [ -n "$PEER_ADDR" ]; then
        echo "  Bootstrapping via peers (no seeds configured)"
    else
        echo "  ================================================================"
        echo "  ERROR: No SEED_ADDR or PEER_ADDR configured and IS_SEED is not true."
        echo "  This node has no way to discover peers and cannot participate"
        echo "  in the P2P network. Set SEED_ADDR, PEER_ADDR, or IS_SEED=true."
        echo "  ================================================================"
        exit 1
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
# Only node0 sets CREATE_GENESIS=true. All other nodes default to false.
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
${SEEDS_LINE}
localnet = ${LOCALNET}
p2p_local = ${P2P_LOCAL}
mining_easy = ${MINING_EASY}
active_profiles = ["tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
magic_bytes = [${MAGIC_BYTES}]
hostlist = "${DATADIR}/hostlist.tsv"
DWOWEOF
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
# Disable Caribina in merge mode only. Native mode honors the env var.
if [ "$MERGE_MINING" = "true" ]; then
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

# --- Mining keypair ---
# Always pass --keys. AccountManager (the single key authority) handles:
#   - keys.toml exists → reads declared key
#   - keys.toml missing + localnet → auto-generates
#   - keys.toml missing + non-localnet → hard error
# No shell-level key logic — the shell doesn't make key decisions.
echo "Mining keypair: delegating to AccountManager via --keys flag"

# Export the secret key BEFORE starting the daemon, while sled is unlocked.
# The pipeline reads this file to share the miner's key with wallets.
# Must happen before daemon start — sled is locked once dwowd runs.
mkdir -p /run/secrets
if /app/dwowd --keys /run/config/keys.toml --export-secret > /run/secrets/miner_secret_b58 2>/tmp/export_secret_err; then
    echo "Mining key exported to /run/secrets/miner_secret_b58"
else
    echo "WARNING: --export-secret failed: $(cat /tmp/export_secret_err)"
    # Create empty file so the pipeline doesn't hang on cat
    touch /run/secrets/miner_secret_b58
fi

echo "Starting dwowd..."
/app/dwowd --keys /run/config/keys.toml &
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
        [ "$i" -eq 60 ] && { echo "  FATAL: monerod RPC not ready after 60 attempts — merge mining cannot proceed"; exit 1; }
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
        [ "$i" -eq 60 ] && { echo "  FATAL: mm_rpc not ready after 60 attempts — merge mining cannot proceed"; exit 1; }
        sleep 2
    done

    # Container-native Monero wallet generation.
    # Each node generates its own random Monero testnet wallet at startup.
    # No hardcoded addresses.
    if [ -z "$MONERO_WALLET_ADDRESS" ]; then
        echo "  Generating Monero testnet wallet..."
        WALLET_DIR="/tmp/monero-wallet-$$"
        mkdir -p "$WALLET_DIR"
        # monero-wallet-cli --generate-new-wallet IS INTERACTIVE.
        # No --non-interactive flag exists in v0.18.5.0.
        # Pipe '\nY\n' to stdin: accept default English, confirm seed.
        # --create-address-file writes <wallet>.address.txt (note .txt).
        printf '\nY\n' | monero-wallet-cli --testnet \
            --generate-new-wallet "$WALLET_DIR/wallet" \
            --password "" --mnemonic-language English \
            --create-address-file \
            2>/tmp/monero-wallet-gen-errors.log
        # Capture exit code BEFORE 'if' consumes $?
        MONERO_CLI_EXIT=$?
        if [ $MONERO_CLI_EXIT -ne 0 ]; then
            echo "  ERROR: monero-wallet-cli failed (exit code $MONERO_CLI_EXIT)"
            cat /tmp/monero-wallet-gen-errors.log 2>/dev/null
            rm -rf "$WALLET_DIR" /tmp/monero-wallet-gen-errors.log
            exit 1
        fi
        if [ -f "$WALLET_DIR/wallet.address.txt" ]; then
            MONERO_WALLET_ADDRESS=$(cat "$WALLET_DIR/wallet.address.txt" | tr -d ' \t\n')
        else
            echo "  ERROR: wallet.address.txt not found after generation"
            ls -la "$WALLET_DIR/"
            exit 1
        fi
        rm -rf "$WALLET_DIR" /tmp/monero-wallet-gen-errors.log
        echo "  Generated Monero wallet: ${MONERO_WALLET_ADDRESS:0:16}..."
    fi
    MONERO_WALLET="$MONERO_WALLET_ADDRESS"

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
        [ "$i" -eq 30 ] && { echo "  FATAL: p2pool stratum not ready after 30 attempts — merge mining cannot proceed"; exit 1; }
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

# Sidecar supervision: if p2pool dies, exit container so Docker restarts it.
# Docker's restart policy (unless-stopped) handles recovery.
if [ -n "$P2POOL_PID" ]; then
    (while kill -0 $DWOWD_PID 2>/dev/null; do
        if ! kill -0 $P2POOL_PID 2>/dev/null; then
            echo "FATAL: p2pool died — exiting container for restart" >&2
            kill $DWOWD_PID 2>/dev/null
            exit 1
        fi
        sleep 5
    done) &
fi

wait $DWOWD_PID
