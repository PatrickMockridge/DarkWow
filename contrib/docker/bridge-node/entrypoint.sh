#!/bin/bash
# DarkWow Bridge Node — Entrypoint
#
# Single entrypoint dispatching on MODE environment variable.
#   MODE=full         — dwowd + deploy contracts + universal_relayer
#   MODE=relayer-only — universal_relayer only (external dwowd)
#   MODE=lilith       — standalone P2P seed node
#
# Usage:
#   docker run --network=host -e MODE=full darkwow-node/bridge
#   docker run --network=host -e MODE=relayer-only -e DARKFID_URL=... darkwow-node/bridge
#   docker run --network=host -e MODE=lilith darkwow-node/bridge

set -e

# --- Mode selection ---
MODE="${MODE:-full}"

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
DATADIR="${DATADIR:-/root/.local/share/dwow/dwowd/${NETWORK}}"

# Bridge contract defaults
BRIDGE_RELAYER_FEE_BP="${BRIDGE_RELAYER_FEE_BP:-100}"
BRIDGE_TIMEOUT_BLOCKS="${BRIDGE_TIMEOUT_BLOCKS:-100}"

# Chain enables
ETH_ENABLED="${ETH_ENABLED:-false}"
XMR_ENABLED="${XMR_ENABLED:-false}"
ZEC_ENABLED="${ZEC_ENABLED:-false}"
AZT_ENABLED="${AZT_ENABLED:-false}"
LTC_ENABLED="${LTC_ENABLED:-false}"

# Universal relayer defaults
DARKFID_URL="${DARKFID_URL:-tcp://127.0.0.1:${RPC_PORT}}"
POLL_INTERVAL_SECS="${POLL_INTERVAL_SECS:-10}"
MAX_CONCURRENT_WITHDRAWALS="${MAX_CONCURRENT_WITHDRAWALS:-10}"
RELAYER_TIMEOUT_BLOCKS="${RELAYER_TIMEOUT_BLOCKS:-100}"
RELAYER_FEE_PERCENTAGE="${RELAYER_FEE_PERCENTAGE:-1}"

echo "=== DarkWow Bridge Node ==="
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
fi
echo "  Magic bytes: [$MAGIC_BYTES]"

# ============================================================================
# Helper: generate dwowd config
# ============================================================================
generate_dwowd_config() {
    local configfile="${1:-/root/.config/dwow/dwowd_config.toml}"

    CONFIGDIR=$(dirname "$configfile")
    mkdir -p "$CONFIGDIR" "$DATADIR"

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
initial_target = 268435455
min_target = 1
max_target = 4294967295
min_block_interval = 10

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

    echo "  dwowd config written to $configfile"
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
        resolved_secret="$WALLET_SECRET"
    fi

    if [ -n "$WALLET_ADDRESS" ] && [ -n "$resolved_secret" ]; then
        if [ ! -f "$miner_addr_file" ] || [ ! -f "$miner_secret_file" ]; then
            echo "  Pre-seeding mining keypair..."
            echo "$WALLET_ADDRESS" > "$miner_addr_file"
            echo "$resolved_secret" > "$miner_secret_file"
        fi
    fi
}

# ============================================================================
# Helper: wait for dwowd RPC to be ready
# ============================================================================
wait_for_rpc() {
    local port="${1:-${RPC_PORT}}"
    local max_wait="${2:-60}"
    echo "  Waiting for dwowd RPC (port $port)..."
    for i in $(seq 1 "$max_wait"); do
        if timeout 2 bash -c "exec 3<>/dev/tcp/127.0.0.1/$port" 2>/dev/null; then
            echo "  dwowd RPC ready"
            return 0
        fi
        sleep 2
    done
    echo "  ERROR: dwowd RPC did not become ready after ${max_wait}s"
    return 1
}

# ============================================================================
# Helper: send JSON-RPC call to dwowd (raw TCP)
# ============================================================================
rpc_call() {
    local method="$1"
    local params="${2:-[]}"
    local port="${3:-${RPC_PORT}}"
    exec 3<>/dev/tcp/127.0.0.1/"$port"
    echo "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}" >&3
    timeout 5 cat <&3
    exec 3>&-
}

# ============================================================================
# Helper: generate universal_relayer config
# ============================================================================
generate_relayer_config() {
    local configfile="${1:-/root/.config/dwow/universal_relayer.toml}"
    mkdir -p "$(dirname "$configfile")"

    cat > "$configfile" << RELEOF
[darkfi]
darkfid_url = "${DARKFID_URL}"
poll_interval_secs = ${POLL_INTERVAL_SECS}
max_concurrent_withdrawals = ${MAX_CONCURRENT_WITHDRAWALS}

[ethereum]
enabled = ${ETH_ENABLED}
node_url = "${ETH_NODE_URL:-https://mainnet.infura.io/v3/YOUR_KEY}"
relayer_private_key = "${ETH_RELAYER_PRIVATE_KEY:-0x0000000000000000000000000000000000000000000000000000000000000001}"
max_gas_gwei = ${ETH_MAX_GAS_GWEI:-50}
max_gas = ${ETH_MAX_GAS:-21000}

[monero]
enabled = ${XMR_ENABLED}
wallet_rpc_url = "${XMR_WALLET_RPC_URL:-http://127.0.0.1:18083}"
node_rpc_url = "${XMR_NODE_RPC_URL:-http://127.0.0.1:18081}"
view_key = "${XMR_VIEW_KEY:-}"
fee_address = "${XMR_FEE_ADDRESS:-}"
min_confirmations = ${XMR_MIN_CONFIRMATIONS:-10}

[zcash]
enabled = ${ZEC_ENABLED}
node_rpc_url = "${ZEC_NODE_RPC_URL:-http://127.0.0.1:8232}"
shielded_pool = ${ZEC_SHIELDED_POOL:-true}
min_confirmations = ${ZEC_MIN_CONFIRMATIONS:-10}

[litecoin]
enabled = ${LTC_ENABLED}
node_rpc_url = "${LTC_NODE_RPC_URL:-http://127.0.0.1:9332}"
rpc_user = "${LTC_RPC_USER:-user}"
rpc_pass = "${LTC_RPC_PASS:-pass}"
min_confirmations = ${LTC_MIN_CONFIRMATIONS:-6}

[aztec]
enabled = ${AZT_ENABLED}
rollup_address = "${AZT_ROLLUP_ADDRESS:-0x0000000000000000000000000000000000000000}"
sequencer_url = "${AZT_SEQUENCER_URL:-https://aztec.network}"
min_confirmations = ${AZT_MIN_CONFIRMATIONS:-5}

[relayer]
timeout_blocks = ${RELAYER_TIMEOUT_BLOCKS}
fee_percentage = ${RELAYER_FEE_PERCENTAGE}
RELEOF

    echo "  Relayer config written to $configfile"
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

    mkdir -p "$LILITH_DATADIR"

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
    exec /app/lilith --config /tmp/lilith.toml
fi

# ============================================================================
# MODE: relayer-only — universal_relayer connected to external dwowd
# ============================================================================
if [ "$MODE" = "relayer-only" ]; then
    echo "=== Mode: relayer-only ==="
    echo "  DarkFi URL: $DARKFID_URL"

    CONFIGFILE="/root/.config/dwow/universal_relayer.toml"
    generate_relayer_config "$CONFIGFILE"

    echo "  Starting universal_relayer..."
    exec /app/universal_relayer --config "$CONFIGFILE"
fi

# ============================================================================
# MODE: full — dwowd + deploy contracts + universal_relayer
# ============================================================================
if [ "$MODE" = "full" ]; then
    echo "=== Mode: full (dwowd + bridge contracts + relayer) ==="
    echo "  P2P=$P2P_PORT  RPC=$RPC_PORT  Seeds: $SEED_ADDR"

    # --- 1. Start dwowd ---
    CONFIGFILE="/root/.config/dwow/dwowd_config.toml"
    generate_dwowd_config "$CONFIGFILE"
    preseed_wallet

    echo "  Starting dwowd..."
    /app/dwowd &
    DWOWD_PID=$!

    wait_for_rpc "$RPC_PORT" 60 || exit 1

    # --- 2. Wait for sync ---
    echo "  Waiting for chain sync..."
    SYNCED=false
    for i in $(seq 1 120); do
        RESPONSE=$(rpc_call "blockchain.info" "[]" "$RPC_PORT" 2>/dev/null || echo "")
        HEIGHT=$(echo "$RESPONSE" | grep -o '"height":[0-9]*' | cut -d: -f2)
        if [ -n "$HEIGHT" ] && [ "$HEIGHT" -gt 0 ]; then
            echo "  Chain synced at height $HEIGHT"
            SYNCED=true
            break
        fi
        sleep 5
    done
    if [ "$SYNCED" = "false" ]; then
        echo "  WARNING: Chain did not sync within 10 minutes, proceeding anyway..."
    fi

    # --- 3. Set up wallet ---
    echo "  Setting up wallet..."
    WALLET_DIR="/root/.local/share/dwow/drk/${NETWORK}"
    mkdir -p "$WALLET_DIR"

    # Wallet initialization — must succeed. Without a wallet, the bridge
    # cannot deploy contracts or sign transactions.
    echo "  Initializing wallet..."
    if ! /app/dwow_wallet -n "$NETWORK" wallet initialize 2>&1; then
        echo "  ERROR: wallet initialize failed — cannot continue"
        exit 1
    fi
    if ! /app/dwow_wallet -n "$NETWORK" wallet keygen 2>&1; then
        echo "  ERROR: wallet keygen failed — cannot continue"
        exit 1
    fi
    WALLET_ADDR=$(/app/dwow_wallet -n "$NETWORK" wallet address 2>&1 | tail -1)
    if [ -z "$WALLET_ADDR" ]; then
        echo "  ERROR: failed to get wallet address"
        exit 1
    fi

    # Import mining secret if available
    if [ -f "${DATADIR}/mining_secret" ]; then
        MINING_SECRET=$(cat "${DATADIR}/mining_secret")
        /app/dwow_wallet -n "$NETWORK" wallet import-secrets "$MINING_SECRET" 2>/dev/null || true
    fi

    echo "  Wallet address: ${WALLET_ADDR:-unknown}"

    # --- 4. Deploy contracts ---
    echo "  Deploying contracts..."

    # Generate deploy authority — must succeed before any contract deploy
    DEPLOY_AUTH=$(/app/dwow_wallet -n "$NETWORK" contract generate-deploy 2>&1 | tail -1)
    if [ -z "$DEPLOY_AUTH" ]; then
        echo "  ERROR: contract generate-deploy failed — no deploy authority"
        exit 1
    fi
    echo "  Deploy authority generated"

    deploy_contract() {
        local name="$1"
        local wasm="$2"
        echo "  Deploying $name..."
        local result
        result=$(/app/dwow_wallet -n "$NETWORK" contract deploy "$DEPLOY_AUTH" "$wasm" 2>&1 | \
            /app/dwow_wallet -n "$NETWORK" broadcast 2>&1)
        if [ -z "$result" ]; then
            echo "  ERROR: $name deploy+broadcast returned empty result"
            return 1
        fi
        echo "  $name deployed: $result"
    }

    # Deploy deployooor
    DEPLOOOR_OUT=$(deploy_contract "deployooor" "/app/wasm/deployooor.wasm")
    echo "  deployooor: $DEPLOOOR_OUT"

    # Deploy promissory_note
    MONEY_OUT=$(deploy_contract "promissory_note" "/app/wasm/promissory_note.wasm")
    echo "  promissory_note: $MONEY_OUT"

    # Deploy bridge
    BRIDGE_OUT=$(deploy_contract "bridge" "/app/wasm/bridge.wasm")
    echo "  bridge: $BRIDGE_OUT"

    # Initialize bridge (if deploy succeeded)
    BRIDGE_ID=$(echo "$BRIDGE_OUT" | grep -o '"ContractId":"[^"]*"' | cut -d'"' -f4 || echo "")
    if [ -n "$BRIDGE_ID" ]; then
        /app/dwow_wallet -n "$NETWORK" contract register bridge "$BRIDGE_ID" 2>/dev/null || true
        echo "  Initializing bridge (fee=${BRIDGE_RELAYER_FEE_BP}bp, timeout=${BRIDGE_TIMEOUT_BLOCKS} blocks)..."
        /app/dwow_wallet -n "$NETWORK" contract invoke "$BRIDGE_ID" initialize \
            --params "{\"relayer_fee_bp\":${BRIDGE_RELAYER_FEE_BP},\"timeout_blocks\":${BRIDGE_TIMEOUT_BLOCKS}}" 2>/dev/null | \
            /app/dwow_wallet -n "$NETWORK" broadcast 2>/dev/null || echo "  WARNING: bridge init skipped"
    fi

    # Deploy relayer_endowment
    ENDOWMENT_OUT=$(deploy_contract "relayer_endowment" "/app/wasm/relayer_endowment.wasm")
    echo "  relayer_endowment: $ENDOWMENT_OUT"

    # Initialize relayer_endowment (if deploy succeeded)
    ENDOWMENT_ID=$(echo "$ENDOWMENT_OUT" | grep -o '"ContractId":"[^"]*"' | cut -d'"' -f4 || echo "")
    if [ -n "$ENDOWMENT_ID" ]; then
        /app/dwow_wallet -n "$NETWORK" contract register relayer_endowment "$ENDOWMENT_ID" 2>/dev/null || true
        echo "  Initializing relayer_endowment..."
        /app/dwow_wallet -n "$NETWORK" contract invoke "$ENDOWMENT_ID" initialize \
            --params "{\"default_backer_cut_bp\":500}" 2>/dev/null | \
            /app/dwow_wallet -n "$NETWORK" broadcast 2>/dev/null || echo "  WARNING: endowment init skipped"
    fi

    # --- 5. Start universal_relayer ---
    echo "  Starting universal_relayer..."
    RELAYER_CONFIG="/root/.config/dwow/universal_relayer.toml"

    # Update DARKFID_URL to point to local dwowd
    DARKFID_URL="tcp://127.0.0.1:${RPC_PORT}"
    generate_relayer_config "$RELAYER_CONFIG"

    /app/universal_relayer --config "$RELAYER_CONFIG" &
    RELAYER_PID=$!

    echo
    echo "=== Bridge node fully started ==="
    echo "  dwowd PID:             $DWOWD_PID"
    echo "  universal_relayer PID: $RELAYER_PID"
    echo "  RPC port:              $RPC_PORT"
    echo "  Wallet address:        ${WALLET_ADDR:-unknown}"
    echo
    echo "  Check status:"
    echo "    docker exec <container> /app/universal_relayer --config $RELAYER_CONFIG status"
    echo "  Logs: docker logs -f <container>"

    # Monitor and restart
    while true; do
        if ! kill -0 $DWOWD_PID 2>/dev/null; then
            echo "  ERROR: dwowd died — exiting"
            exit 1
        fi
        if ! kill -0 $RELAYER_PID 2>/dev/null; then
            echo "  WARNING: universal_relayer died — restarting..."
            /app/universal_relayer --config "$RELAYER_CONFIG" &
            RELAYER_PID=$!
        fi
        sleep 10
    done
fi

# --- Unknown MODE ---
echo "ERROR: Unknown MODE='$MODE'. Valid modes: full, relayer-only, lilith"
exit 1
