#!/bin/bash
# Join the public DarkWow testnet as a mining node.
#
# Usage:
#   ./join-testnet.sh --mode native    # Solo mining (xmrig -> dwowd stratum)
#   ./join-testnet.sh --mode merge     # Merge mining (Monero testnet + DarkWow)
#
# Prerequisites:
#   - Docker image: darkwow-testnet:latest (built locally or pullable from registry)
#   - For merge mode: the image must be built or pulled on this machine
#   - A DarkWow wallet address for mining rewards (see --wallet-address)
#   - For merge mode: a Monero testnet wallet address (see --monero-wallet)
#
# Environment variables (override defaults):
#   IMAGE                  - Docker image to use (default: darkwow-testnet:latest)
#   P2P_PORT               - P2P inbound port (default: 31342)
#   RPC_PORT               - JSON-RPC port (default: 31345)
#   STRATUM_PORT           - Stratum mining port (default: 31347)
#   EXTERNAL_ADDR          - Public host:port for P2P (default: auto-detect)
#   MINING_THREADS         - xmrig thread count (default: 4)
#   WALLET_ADDRESS         - DarkWow wallet address for mining rewards
#   WALLET_SECRET_FILE     - Path to file containing hex secret key
#   MONERO_WALLET_ADDRESS  - Monero testnet wallet for p2pool mining rewards
#   DATA_DIR               - Host path for blockchain data (default: ./data/dwowd)
#   MONERO_DATA_DIR        - Host path for Monero data (default: ./data/monerod)

set -euo pipefail

MODE=""
IMAGE="${IMAGE:-darkwow-testnet:latest}"
NETWORK="${NETWORK:-darkwow-testnet}"
P2P_PORT="${P2P_PORT:-31342}"
RPC_PORT="${RPC_PORT:-31345}"
STRATUM_PORT="${STRATUM_PORT:-31347}"
MM_RPC_PORT="${MM_RPC_PORT:-31348}"
MINING_THREADS="${MINING_THREADS:-4}"
THRESHOLD="${THRESHOLD:-3}"
TARGET_BLOCK_TIME="${TARGET_BLOCK_TIME:-120}"
SEED_ADDR="${SEED_ADDR:-lilith0.dark.fi:31340,lilith1.dark.fi:31340}"
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"
WALLET_ADDRESS="${WALLET_ADDRESS:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"
DATA_DIR="${DATA_DIR:-$(pwd)/data/dwowd}"
MONERO_DATA_DIR="${MONERO_DATA_DIR:-$(pwd)/data/monerod}"
P2POOL_DATA_DIR="${P2POOL_DATA_DIR:-$(pwd)/data/p2pool}"
EXTERNAL_ADDR="${EXTERNAL_ADDR:-}"
MONERO_OFFLINE="${MONERO_OFFLINE:-false}"
MONERO_NETWORK="${MONERO_NETWORK:-testnet}"
MONERO_ADD_PEERS="${MONERO_ADD_PEERS:-125.229.105.12:28081,37.187.74.171:28089}"
MONERO_FIXED_DIFFICULTY="${MONERO_FIXED_DIFFICULTY:-20000}"
FINALITY_MODE="${FINALITY_MODE:-always}"
FINALITY_DISABLE_CARIBINA="${FINALITY_DISABLE_CARIBINA:-false}"

# --- Usage ---
usage() {
    cat <<EOF
Usage: $0 --mode native|merge [OPTIONS]

Join the public DarkWow testnet as a mining node.

Modes:
  native   Solo mining — xmrig connects directly to dwowd's built-in stratum.
           A single container runs dwowd + xmrig. No external Monero needed.

  merge    Merge mining — xmrig mines via p2pool, which submits blocks to both
           Monero testnet and DarkWow (via dwowd mm_rpc). Requires monerod and
           p2pool containers alongside dwowd.

Options:
  --mode MODE             Mining mode: 'native' or 'merge' (required)
  --image IMAGE           Docker image (default: darkwow-testnet:latest)
  --wallet-address ADDR   DarkWow wallet for mining rewards (bs58)
  --wallet-secret-file F  Path to file with hex secret key (bind-mounted read-only)
  --monero-wallet ADDR    Monero testnet wallet for p2pool rewards (merge mode)
  --mining-threads N      xmrig CPU threads (default: 4)
  --p2p-port PORT         P2P inbound port (default: 31342)
  --rpc-port PORT         JSON-RPC port (default: 31345)
  --stratum-port PORT     Stratum port (default: 31347)
  --external-addr ADDR    Public host:port for P2P (auto-detected if unset)
  --data-dir DIR          Host path for dwowd blockchain data (default: ./data/dwowd)
  --monero-data-dir DIR   Host path for monerod data (default: ./data/monerod)
  --no-host-net           Use bridge networking instead of host (not recommended)
  --finality-mode MODE    Finality mode: "always" (default), "native", or "signaled"
  --finality-disable-caribina
                          Disable Caribina Arweave anchoring entirely
  --help                  Show this help

Environment variables: IMAGE, P2P_PORT, RPC_PORT, STRATUM_PORT, EXTERNAL_ADDR,
  MINING_THREADS, WALLET_ADDRESS, WALLET_SECRET_FILE, MONERO_WALLET_ADDRESS,
  DATA_DIR, MONERO_DATA_DIR, SEED_ADDR, MAGIC_BYTES,
  FINALITY_MODE, FINALITY_DISABLE_CARIBINA

Examples:
  # Native mining with host networking
  $0 --mode native --wallet-address bs58... --wallet-secret-file /tmp/mining_secret

  # Merge mining with Monero testnet
  $0 --mode merge --wallet-address bs58... --monero-wallet 9zMU...

  # Custom ports and threads
  MINING_THREADS=8 P2P_PORT=41342 $0 --mode native
EOF
    exit 0
}

# --- Parse arguments ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --mode)
            MODE="$2"; shift 2 ;;
        --image)
            IMAGE="$2"; shift 2 ;;
        --wallet-address)
            WALLET_ADDRESS="$2"; shift 2 ;;
        --wallet-secret-file)
            WALLET_SECRET_FILE="$2"; shift 2 ;;
        --monero-wallet)
            MONERO_WALLET_ADDRESS="$2"; shift 2 ;;
        --mining-threads)
            MINING_THREADS="$2"; shift 2 ;;
        --p2p-port)
            P2P_PORT="$2"; shift 2 ;;
        --rpc-port)
            RPC_PORT="$2"; shift 2 ;;
        --stratum-port)
            STRATUM_PORT="$2"; shift 2 ;;
        --external-addr)
            EXTERNAL_ADDR="$2"; shift 2 ;;
        --data-dir)
            DATA_DIR="$2"; shift 2 ;;
        --monero-data-dir)
            MONERO_DATA_DIR="$2"; shift 2 ;;
        --no-host-net)
            USE_HOST_NET="false"; shift ;;
        --finality-mode)
            FINALITY_MODE="$2"; shift 2 ;;
        --finality-disable-caribina)
            FINALITY_DISABLE_CARIBINA="true"; shift ;;
        --help)
            usage ;;
        *)
            echo "ERROR: Unknown option: $1"
            usage ;;
    esac
done

# --- Validate ---
if [ "$MODE" != "native" ] && [ "$MODE" != "merge" ]; then
    echo "ERROR: --mode must be 'native' or 'merge'"
    usage
fi

USE_HOST_NET="${USE_HOST_NET:-true}"

if ! command -v docker &>/dev/null; then
    echo "ERROR: docker not found in PATH"
    exit 1
fi

# Check if image exists locally, offer to pull
if ! docker image inspect "$IMAGE" &>/dev/null; then
    echo "Image '$IMAGE' not found locally."
    echo "Options:"
    echo "  1. Build from source: docker build -t $IMAGE -f contrib/docker/darkwow-testnet/Dockerfile ."
    echo "  2. Pull from registry (if published)"
    echo "  3. Set IMAGE env var to an existing image"
    exit 1
fi

# --- Auto-detect external address ---
if [ -z "$EXTERNAL_ADDR" ]; then
    if command -v curl &>/dev/null; then
        PUBLIC_IP=$(curl -s --connect-timeout 5 https://api.ipify.org 2>/dev/null || \
                     curl -s --connect-timeout 5 https://icanhazip.com 2>/dev/null || \
                     echo "")
    else
        PUBLIC_IP=""
    fi
    if [ -n "$PUBLIC_IP" ]; then
        EXTERNAL_ADDR="${PUBLIC_IP}:${P2P_PORT}"
    else
        echo "WARNING: Could not detect public IP. Set --external-addr or EXTERNAL_ADDR."
        echo "  Without EXTERNAL_ADDR, other peers may not be able to connect to you."
    fi
fi

# --- Wallet check ---
if [ -z "$WALLET_ADDRESS" ]; then
    echo "WARNING: No WALLET_ADDRESS provided. dwowd will auto-generate a mining keypair."
    echo "  Mining rewards will go to an address whose secret exists only in the container."
    echo "  Set --wallet-address to use a pre-configured wallet."
fi

if [ "$MODE" = "merge" ] && [ -z "$MONERO_WALLET_ADDRESS" ]; then
    echo "WARNING: No MONERO_WALLET_ADDRESS provided. p2pool will use a dummy address."
    echo "  Monero mining rewards from the parent chain will be unspendable."
    echo "  Set --monero-wallet to receive XMR testnet rewards."
fi

# --- Create data directories ---
mkdir -p "$DATA_DIR" "$MONERO_DATA_DIR" "$P2POOL_DATA_DIR"

# --- Resolve compose file path ---
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/docker-compose.yml"

# --- Seed reachability check ---
FALLBACK_SEED_PORT="${FALLBACK_SEED_PORT:-31341}"
FALLBACK_SEED_DATA="${FALLBACK_SEED_DATA:-$(pwd)/data/lilith}"
FALLBACK_LILITH_NAME="dwow-fallback-lilith"
AUTO_FALLBACK="${AUTO_FALLBACK:-true}"

check_seeds() {
    local seeds="$1"
    local reachable=0
    IFS=',' read -ra SEED_ARRAY <<< "$seeds"
    for seed in "${SEED_ARRAY[@]}"; do
        seed=$(echo "$seed" | xargs)
        local host="${seed%:*}"
        local port="${seed##*:}"
        if timeout 5 bash -c "echo >/dev/tcp/$host/$port" 2>/dev/null; then
            echo "  Seed reachable: $seed"
            reachable=1
            break
        else
            echo "  Seed unreachable: $seed"
        fi
    done
    return $(( 1 - reachable ))
}

start_fallback_lilith() {
    echo "  No public seeds reachable — starting local fallback lilith..."
    echo "  Fallback P2P port: $FALLBACK_SEED_PORT"
    echo "  Fallback data dir: $FALLBACK_SEED_DATA"

    mkdir -p "$FALLBACK_SEED_DATA"

    # Remove any previous fallback lilith
    docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
    docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true

    docker run -d \
        --name "$FALLBACK_LILITH_NAME" \
        --network=host \
        -e ROLE=lilith \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$FALLBACK_SEED_PORT" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e LOCALNET=false \
        -v "$FALLBACK_SEED_DATA:/root/.local/share/dwow/lilith" \
        --restart unless-stopped \
        "$IMAGE"

    echo "  Fallback lilith started — nodes will use 127.0.0.1:$FALLBACK_SEED_PORT as seed"

    # Return the local seed address
    FALLBACK_SEED_ADDR="127.0.0.1:${FALLBACK_SEED_PORT}"
}

# --- Build docker run args for native mode ---
run_native() {
    echo "=== DarkWow Public Testnet — Native Mining ==="

    # Check seed reachability and start fallback if needed
    local effective_seed="$SEED_ADDR"
    if [ "$AUTO_FALLBACK" = "true" ]; then
        echo "  Checking seed reachability..."
        if ! check_seeds "$SEED_ADDR"; then
            start_fallback_lilith
            effective_seed="$FALLBACK_SEED_ADDR"
        fi
        echo
    fi

    echo "  Mode:      solo (xmrig -> dwowd stratum)"
    echo "  Image:     $IMAGE"
    echo "  Network:   $NETWORK"
    echo "  Seed:      $effective_seed"
    echo "  External:  $EXTERNAL_ADDR"
    echo "  P2P:       $P2P_PORT"
    echo "  RPC:       $RPC_PORT"
    echo "  Stratum:   $STRATUM_PORT"
    echo "  Threads:   $MINING_THREADS"
    echo "  Data dir:  $DATA_DIR"
    if [ -n "$WALLET_ADDRESS" ]; then
        echo "  Wallet:    $WALLET_ADDRESS"
    fi
    echo

    local net_arg="--network=host"
    if [ "$USE_HOST_NET" = "false" ]; then
        net_arg="-p ${P2P_PORT}:${P2P_PORT} -p ${RPC_PORT}:${RPC_PORT} -p ${STRATUM_PORT}:${STRATUM_PORT}"
    fi

    local wallet_mount=""
    if [ -n "$WALLET_SECRET_FILE" ] && [ -f "$WALLET_SECRET_FILE" ]; then
        wallet_mount="-v $(realpath "$WALLET_SECRET_FILE"):/run/secrets/mining_secret:ro"
    fi

    # shellcheck disable=SC2086
    docker run -d \
        --name dwow-node \
        $net_arg \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$effective_seed" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS="$MINING_THREADS" \
        -e THRESHOLD="$THRESHOLD" \
        -e TARGET_BLOCK_TIME="$TARGET_BLOCK_TIME" \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e MINING_ENABLED=true \
        -e RANDOMX_MAX_THREADS=0 \
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
        -e EXTERNAL_ADDR="$EXTERNAL_ADDR" \
        -e WALLET_ADDRESS="$WALLET_ADDRESS" \
        -e WALLET_SECRET_FILE=/run/secrets/mining_secret \
        $wallet_mount \
        -v "${DATA_DIR}:/root/.local/share/dwow/dwowd" \
        --restart unless-stopped \
        "$IMAGE"

    echo
    echo "=== Node started ==="
    echo
    echo "Check status:"
    echo "  docker logs -f dwow-node"
    echo
    echo "Health check:"
    echo "  curl -s http://127.0.0.1:${RPC_PORT} -X POST \\"
    echo "      -H 'Content-Type: application/json' \\"
    echo "      -d '{\"method\":\"blockchain.info\",\"params\":[],\"id\":1}'"
    echo
    echo "Check P2P connections:"
    echo "  curl -s http://127.0.0.1:${RPC_PORT} -X POST \\"
    echo "      -H 'Content-Type: application/json' \\"
    echo "      -d '{\"method\":\"p2p.info\",\"params\":[],\"id\":1}'"
    echo
    echo "Stop and remove:"
    echo "  docker stop dwow-node && docker rm dwow-node"
}

# --- Docker Compose for merge mode ---
run_merge() {
    # Check seed reachability and start fallback if needed
    local effective_seed="$SEED_ADDR"
    if [ "$AUTO_FALLBACK" = "true" ]; then
        echo "  Checking seed reachability..."
        if ! check_seeds "$SEED_ADDR"; then
            start_fallback_lilith
            effective_seed="$FALLBACK_SEED_ADDR"
        fi
        echo
    fi

    echo "=== DarkWow Public Testnet — Merge Mining ==="
    echo "  Mode:      merge (xmrig -> p2pool -> monerod + dwowd mm_rpc)"
    echo "  Image:     $IMAGE"
    echo "  Network:   $NETWORK"
    echo "  Seed:      $effective_seed"
    echo "  External:  $EXTERNAL_ADDR"
    echo "  P2P:       $P2P_PORT"
    echo "  RPC:       $RPC_PORT"
    echo "  Stratum:   $STRATUM_PORT"
    echo "  MM RPC:    $MM_RPC_PORT"
    echo "  Threads:   $MINING_THREADS"
    echo "  Monero:    $MONERO_NETWORK (offline=$MONERO_OFFLINE)"
    echo "  Data dir:  $DATA_DIR"
    if [ -n "$WALLET_ADDRESS" ]; then
        echo "  DRKW wallet: $WALLET_ADDRESS"
    fi
    if [ -n "$MONERO_WALLET_ADDRESS" ]; then
        echo "  XMR wallet:  $MONERO_WALLET_ADDRESS"
    fi
    echo

    # Export all vars so docker compose can substitute them
    export IMAGE NETWORK P2P_PORT RPC_PORT STRATUM_PORT MM_RPC_PORT
    export MINING_THREADS THRESHOLD TARGET_BLOCK_TIME
    export SEED_ADDR="$effective_seed"
    export MAGIC_BYTES EXTERNAL_ADDR
    export WALLET_ADDRESS WALLET_SECRET_FILE MONERO_WALLET_ADDRESS
    export DATA_DIR MONERO_DATA_DIR P2POOL_DATA_DIR
    export MONERO_OFFLINE MONERO_NETWORK MONERO_ADD_PEERS MONERO_FIXED_DIFFICULTY
    export FINALITY_MODE FINALITY_DISABLE_CARIBINA
    export USE_HOST_NET
    export COMPOSE_FILE

    # Use the join-merge profile to start a single-node merge mining stack
    docker compose -f "$COMPOSE_FILE" --profile join-merge up -d

    echo
    echo "=== Merge mining stack started ==="
    echo
    echo "Check status:"
    echo "  docker compose -f $COMPOSE_FILE --profile join-merge ps"
    echo
    echo "View logs:"
    echo "  docker logs -f dwow-node0        # dwowd"
    echo "  docker logs -f dwow-monerod      # monerod sync"
    echo "  docker logs -f dwow-p2pool       # p2pool"
    echo "  docker logs -f dwow-xmrig-merge  # xmrig mining"
    echo
    echo "Health check (dwowd):"
    echo "  curl -s http://127.0.0.1:${RPC_PORT} -X POST \\"
    echo "      -H 'Content-Type: application/json' \\"
    echo "      -d '{\"method\":\"blockchain.info\",\"params\":[],\"id\":1}'"
    echo
    echo "Health check (p2pool):"
    echo "  curl -s http://127.0.0.1:3333/stats"
    echo
    echo "Stop and remove:"
    echo "  docker compose -f $COMPOSE_FILE --profile join-merge down"
}

# --- Dispatch ---
case "$MODE" in
    native)
        run_native ;;
    merge)
        run_merge ;;
esac
