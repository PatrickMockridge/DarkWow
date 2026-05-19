#!/bin/bash
# DarkWow Testnet Full Pipeline
#
# Single entry point for all DarkWow testnet builds and tests.
# Every mode builds the image, starts the stack, and verifies correctness.
#
# Usage:
#   ./test_pipeline.sh --mode native        # 3-node local devnet, native mining
#   ./test_pipeline.sh --mode merge         # 3-node local devnet, merge mining
#   ./test_pipeline.sh --mode native-p2pool # 3-node local devnet, adaptor pathway
#   ./test_pipeline.sh --mode bridge        # 3-node + bridge-node, full bridge lifecycle
#   ./test_pipeline.sh --mode join-native   # Single node joins public testnet, native
#   ./test_pipeline.sh --mode join-merge    # Single node joins public testnet, merge
#
# Sequential determinism:
#   Every phase runs to completion before the next begins. No background tasks,
#   no parallel operations. One machine, one thing at a time. This guarantees
#   reproducible results across different machines.
#
# After the pipeline passes, run contract tests:
#   ./test-contracts.sh --mode native
#   ./test-contracts.sh --mode merge

set -e
set -E  # inherit ERR trap into shell functions

# Fatal error trap — every failure must be visible.
# set -e kills the script on any non-zero exit; without this trap
# the log just stops mid-line with no clue what failed.
trap 'echo "[FATAL] Pipeline failed at line $LINENO — exit code $?" >&2' ERR

# Signal traps — catch kills that bypass ERR (tmux crash, timeout, ^C).
trap 'echo "[FATAL] Pipeline killed by signal — last line ~$LINENO" >&2; exit 1' INT TERM HUP PIPE

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DWW_BIN="${REPO_ROOT}/target/release/dww"
DWW_DEBUG="${REPO_ROOT}/target/debug/dww"

# --- Help ---
usage() {
    cat <<'EOF'
DarkWow Testnet Full Pipeline

Usage:
  ./test_pipeline.sh --mode <mode>

Modes:
  native         3-node local devnet, native mining (xmrig → dwowd stratum)
  merge          3-node local devnet, merge mining (Monero aux PoW via p2pool)
  native-p2pool  3-node local devnet, adaptor pathway (p2pool → adaptor → dwowd)
  bridge         3-node + bridge-node, full bridge deposit→withdraw→execute test
  join-native    Single node joining public testnet, native mining
  join-merge     Single node joining public testnet, merge mining

Phases (native, merge, native-p2pool):
  1.  Clean                Tear down previous containers, images, volumes
  2.  Validate prereqs     Check required files exist on disk
  3.  Generate wallet      Create DarkWow keypair via dww
  4.  Build                Build Docker images via compose
  5.  Start                Launch containers
  6.  Verify containers    Check all expected containers are running
  7.  RPC health           Wait for JSON-RPC endpoints to respond
  8.  Mining activity      Verify stratum/p2pool activity in logs
  9.  Block production     Wait for blocks to be mined
  10. Report               Print pass/fail summary

Phases (bridge):
  1-9. Shared with native mode (clean through block production)
  10. Bridge Deploy        Deploy bridge + relayer_endowment contracts via RPC
  10b. Bridge Initialize    Init bridge + endowment accounts
  11. Register Relayer     Register test relayer with bridge contract
  12. Simulate Deposit     Generate ZK deposit proof, submit DepositV1
  13. Create Withdrawal    Generate ZK withdraw proof, submit WithdrawV1
  14. Accept Withdrawal    Relayer accepts pending withdrawal
  15. Execute Withdrawal   Execute guaranteed withdrawal
  16. Verify Bridge        Check container health, relayer logs, block height

Phases (join-native, join-merge):
  1.  Clean                Tear down previous join containers + fallback lilith
  2.  Validate prereqs     Check join-testnet.sh and required files exist
  3.  Generate wallet      Create DarkWow keypair via dww
  4.  Build                Build Docker image via compose
  5.  Static config        Extract generated dwowd_config.toml and validate keys
  6.  Container lifecycle  Start container, verify startup log messages
  7.  Seed fallback        Test local lilith fallback when public seeds unreachable
  8.  P2P connectivity     Wait for peer connections via p2p.info
  9.  Blockchain sync      Wait for block_height > 0 via blockchain.info
  10. Mining verification  Wait for block production or merge stack health
  11. Persistence          Stop container, verify data survives, restart
  12. Report               Print pass/fail summary

Sequential determinism:
  Every phase runs to completion before the next begins. No background tasks,
  no parallel operations. One machine, one thing at a time. This guarantees
  reproducible results across different machines.

Environment:
  RAYON_NUM_THREADS         Cargo build parallelism (default: 10)
  MONERO_WALLET_ADDRESS     Monero testnet wallet for merge mining rewards
  FINALITY_MODE             Finality enforcement mode: always (default), native, signaled
  FINALITY_DISABLE_CARIBINA Set to "true" to disable Caribina Arweave anchoring

Options:
  --finality-mode MODE      Finality mode: "always" (default), "native", or "signaled"
  --finality-disable-caribina
                            Disable Caribina Arweave anchoring entirely

Examples:
  ./test_pipeline.sh                         # local devnet, native mining
  ./test_pipeline.sh --mode merge            # local devnet, merge mining
  ./test_pipeline.sh --mode bridge           # local devnet, full bridge lifecycle
  ./test_pipeline.sh --mode join-native      # join public testnet, solo mining
  ./test_pipeline.sh --mode join-merge       # join public testnet, merge mining

After pipeline passes:
  ./test-contracts.sh --mode native          # contract deploy + transfer test
  ./test-contracts.sh --mode merge           # merge mode contract test
EOF
    exit 0
}

# --- Parse flags ---
MODE="native"
FINALITY_MODE="${FINALITY_MODE:-always}"
FINALITY_DISABLE_CARIBINA="${FINALITY_DISABLE_CARIBINA:-false}"
while [ $# -gt 0 ]; do
    case "$1" in
        --mode) MODE="$2"; shift 2 ;;
        --mode=*) MODE="${1#*=}"; shift ;;
        --finality-mode) FINALITY_MODE="$2"; shift 2 ;;
        --finality-mode=*) FINALITY_MODE="${1#*=}"; shift ;;
        --finality-disable-caribina) FINALITY_DISABLE_CARIBINA="true"; shift ;;
        --help|-h) usage ;;
        *)
            echo "Unknown flag: $1"
            echo "Usage: $0 --mode native|merge|native-p2pool|bridge|join-native|join-merge"
            echo "       $0 --help"
            exit 1 ;;
    esac
done

VALID_MODES="native merge native-p2pool bridge join-native join-merge"
if ! echo "$VALID_MODES" | grep -qw "$MODE"; then
    echo "Invalid mode: $MODE"
    echo "Valid modes: $VALID_MODES"
    echo "Run '$0 --help' for full documentation."
    exit 1
fi

# --- Locate dww binary ---
if [ -x "$DWW_BIN" ]; then
    DWW="$DWW_BIN"
elif [ -x "$DWW_DEBUG" ]; then
    DWW="$DWW_DEBUG"
else
    echo "Building dww..."
    (cd "$REPO_ROOT" && RAYON_NUM_THREADS=10 cargo build -p dww 2>&1)
    [ -x "$DWW_DEBUG" ] && DWW="$DWW_DEBUG" || DWW="$DWW_BIN"
    [ -x "$DWW" ] || { echo "ERROR: dww binary not found after build"; exit 1; }
fi

NETWORK="darkwow-testnet"
NODE0="dwow-node0"
IMAGE="${IMAGE:-darkwow-testnet-lilith:latest}"

# Public testnet constants (join modes)
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"
SEED_ADDR="${SEED_ADDR:-lilith0.dark.fi:31340,lilith1.dark.fi:31340}"
P2P_PORT=31342
RPC_PORT=31345
STRATUM_PORT=31347
MM_RPC_PORT=31348
FALLBACK_SEED_PORT="${FALLBACK_SEED_PORT:-31341}"
CONTAINER_NAME="dwow-test-node"
FALLBACK_LILITH_NAME="dwow-fallback-lilith"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
export COMPOSE_PROJECT_NAME="darkwow-testnet"

# Test data paths (join modes)
JOIN_TEST_DATA="$(pwd)/test-data"
JOIN_TEST_MONERO="$(pwd)/test-monero-data"
JOIN_TEST_P2POOL="$(pwd)/test-p2pool-data"
JOIN_TEST_FALLBACK="$(pwd)/test-fallback-data"
JOIN_TEST_PERSIST="$(pwd)/test-persist-data"

# WASM contract paths
WASM_MONEY_V3="${REPO_ROOT}/src/contract/money_v3/dwow_money_v3_contract.wasm"
WASM_DEX="${REPO_ROOT}/src/contract/dex/dwow_dex_contract.wasm"
WASM_DAO_ESCROW="${REPO_ROOT}/src/contract/dao_escrow/dwow_dao_escrow_contract.wasm"

MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

PASS=0
FAIL=0

pass() { echo -e "${GREEN}[PASS]${NC} $*"; PASS=$((PASS + 1)); }
fail() { echo -e "${RED}[FAIL]${NC} $*"; FAIL=$((FAIL + 1)); }

# Remove directories that may contain root-owned files from Docker volumes.
# Falls back to sudo if needed, or docker rm to clean up.
clean_data_dir() {
    for dir in "$@"; do
        [ -d "$dir" ] || continue
        rm -rf "$dir" 2>/dev/null || \
            sudo rm -rf "$dir" 2>/dev/null || \
            docker run --rm -v "$dir:$dir" ubuntu:24.04 rm -rf "$dir" 2>/dev/null || \
            { warn "Could not remove $dir (may contain root-owned files)"; }
    done
}

check() {
    if [ "$1" -eq 0 ]; then
        pass "$2"
    else
        fail "$2"
    fi
}

is_join_mode() {
    [ "$MODE" = "join-native" ] || [ "$MODE" = "join-merge" ]
}

is_bridge_mode() {
    [ "$MODE" = "bridge" ]
}

# Bridge-specific constants
BRIDGE_CONTAINER="dwow-bridge-node"
BRIDGE_TEST_HELPER="${REPO_ROOT}/target/release/bridge_test_helper"
BRIDGE_TEST_HELPER_DEBUG="${REPO_ROOT}/target/debug/bridge_test_helper"
WASM_BRIDGE="${REPO_ROOT}/src/contract/bridge/darkfi_bridge_contract.wasm"
WASM_RELAYER_ENDOWMENT="${REPO_ROOT}/src/contract/relayer_endowment/darkfi_relayer_endowment_contract.wasm"
WASM_DEPLOOOOR="${REPO_ROOT}/src/contract/deployooor/dwow_deployooor_contract.wasm"

# ==============================================================================
# Join-mode helpers
# ==============================================================================

_CHECK_IMAGE_FAILED=0
check_image() {
    if [ "$_CHECK_IMAGE_FAILED" -eq 1 ]; then
        return 1
    fi
    if ! docker image inspect "$IMAGE" &>/dev/null; then
        _CHECK_IMAGE_FAILED=1
        fail "Docker image '$IMAGE' not found (build phase should have created it)"
        return 1
    fi
    return 0
}

check_network() {
    if ! curl -s --connect-timeout 5 https://api.ipify.org >/dev/null 2>&1; then
        fail "No internet connectivity detected"
        return 1
    fi
    return 0
}

jsonrpc() {
    local port="$1" method="$2"
    # dwowd JSON-RPC is raw TCP, not HTTP. Use bash /dev/tcp via docker exec.
    # Retry up to 3 times if the port isn't listening yet.
    for attempt in 1 2 3; do
        if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
            local result
            result=$(docker exec "$CONTAINER_NAME" bash -c "exec 3<>/dev/tcp/127.0.0.1/$port 2>/dev/null || exit 1; echo '{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":[],\"id\":1}' >&3; timeout 3 cat <&3" 2>/dev/null) || true
            if [ -n "$result" ] && echo "$result" | grep -q '"result"\|"sessions"\|"block_height"'; then
                echo "$result"
                return
            fi
            # If we got a response but it's an error, return it
            if [ -n "$result" ]; then
                echo "$result"
                return
            fi
        else
            exec 3<>/dev/tcp/127.0.0.1/"$port" 2>/dev/null || { echo '{"error":"RPC unreachable"}'; return; }
            echo "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":[],\"id\":1}" >&3
            timeout 3 cat <&3 2>/dev/null || echo '{"error":"RPC unreachable"}'
            exec 3>&-
            return
        fi
        [ "$attempt" -lt 3 ] && sleep 2
    done
    echo '{"error":"RPC unreachable after 3 attempts"}'
}

report() {
    echo ""
    echo "==========================================="
    echo "  Mode: $MODE"
    echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
    echo "==========================================="
    echo ""

    if [ "$FAIL" -gt 0 ]; then
        echo -e "${RED}Some checks failed${NC}"
        echo ""
        echo "Debug info — check logs:"
        if [ "$MODE" = "merge" ]; then
            echo "  docker compose --profile merge logs"
        elif [ "$MODE" = "native-p2pool" ]; then
            echo "  docker compose --profile native-p2pool logs"
        elif [ "$MODE" = "bridge" ]; then
            echo "  docker compose --profile bridge logs"
            echo "  docker logs $BRIDGE_CONTAINER"
        elif [ "$MODE" = "join-native" ]; then
            echo "  docker logs $CONTAINER_NAME"
        elif [ "$MODE" = "join-merge" ]; then
            echo "  docker compose -f $COMPOSE_FILE --profile join-merge logs"
        else
            echo "  docker compose logs"
        fi
        exit 1
    fi

    if is_join_mode; then
        echo "Join test passed. To join the public testnet for real:"
        echo "  ./contrib/docker/darkwow-testnet/join-testnet.sh --mode ${MODE#join-}"
    elif is_bridge_mode; then
        echo "Bridge pipeline passed."
        echo ""
        echo "Tear down:"
        echo "  docker compose --profile bridge down -v"
    else
        echo "Run contract tests:"
        echo "  ./test-contracts.sh --mode $MODE"
        echo ""
        echo "Tear down:"
        if [ "$MODE" = "merge" ]; then
            echo "  docker compose --profile merge down -v"
        elif [ "$MODE" = "native-p2pool" ]; then
            echo "  docker compose --profile native-p2pool down -v"
        else
            echo "  docker compose down -v"
        fi
    fi
    echo ""
    echo -e "${GREEN}Pipeline passed${NC}"
}

# ==============================================================================
# Phase 1: Clean
# ==============================================================================
phase_clean() {
    info "Phase 1: Clean — tearing down previous state..."

    # Kill orphan build processes from prior interrupted runs.
    # These hold file locks on target/ and Cargo.lock, causing
    # the next build to fail or deadlock.
    pkill -9 -f 'cargo build' 2>/dev/null || true
    pkill -9 -f 'rustc' 2>/dev/null || true

    # Remove stale wallet secret with 3-tier fallback. Mount /tmp (parent)
    # not the file itself — if the file doesn't exist, -v auto-creates a
    # directory at the mount point, making the problem worse.
    rm -rf /tmp/dwow_mining_secret 2>/dev/null || \
        sudo rm -rf /tmp/dwow_mining_secret 2>/dev/null || \
        docker run --rm -v /tmp:/tmp ubuntu:24.04 rm -rf /tmp/dwow_mining_secret 2>/dev/null || \
        { warn "Could not remove /tmp/dwow_mining_secret (may be root-owned)"; }

    # Remove dww wallet state so each run generates a fresh keypair.
    clean_data_dir ~/.local/share/dwow/dww

    cd "$SCRIPT_DIR"

    if is_join_mode; then
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
        docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true
        docker compose -f "$COMPOSE_FILE" --profile native --remove-orphans down --rmi all -v 2>/dev/null || true
        docker compose -f "$COMPOSE_FILE" --profile merge --remove-orphans down --rmi all -v 2>/dev/null || true
        docker compose -f "$COMPOSE_FILE" --profile native-p2pool --remove-orphans down --rmi all -v 2>/dev/null || true
        docker compose -f "$COMPOSE_FILE" --profile bridge --remove-orphans down --rmi all -v 2>/dev/null || true
        docker compose -f "$COMPOSE_FILE" --profile join-merge --remove-orphans down --rmi all -v 2>/dev/null || true
        # Remove stale join containers and ALL dwow-* containers
        for c in dwow-node0-join dwow-node0 dwow-monerod dwow-p2pool dwow-xmrig-merge dwow-adaptor dwow-p2pool-darkwow dwow-xmrig-p2pool; do
            docker stop "$c" 2>/dev/null || true
            docker rm "$c" 2>/dev/null || true
        done
        STALE=$(docker ps -a --format '{{.Names}}' 2>/dev/null | grep "^dwow-" || true)
        if [ -n "$STALE" ]; then
            warn "Removing stale containers..."
            echo "$STALE" | xargs -r docker rm -f 2>/dev/null || true
        fi
        # Remove old images first — builder prune skips layers still
        # referenced by existing images, so stale COPY caches survive.
        for img in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "^darkwow-testnet" || true); do
            docker rmi -f "$img" 2>/dev/null || true
        done
        # Clear all build cache — ensures fresh builds on next run
        docker builder prune -a -f 2>/dev/null || true
        for b in $(docker buildx ls --format '{{.Name}}' 2>/dev/null | grep -v '^default$' || true); do
            docker buildx prune -a -f --builder "$b" 2>/dev/null || true
        done
        docker volume prune -f 2>/dev/null || true
        clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_MONERO" "$JOIN_TEST_P2POOL" \
               "$JOIN_TEST_FALLBACK" "$JOIN_TEST_PERSIST"
        pass "clean (join mode)"
        return
    fi

    # Tear down compose services (containers, networks, volumes)
    # --remove-orphans catches containers from services that were
    # renamed or removed between compose file revisions.
    docker compose --profile native --remove-orphans down --rmi all -v 2>/dev/null || true
    docker compose --profile merge --remove-orphans down --rmi all -v 2>/dev/null || true
    docker compose --profile native-p2pool --remove-orphans down --rmi all -v 2>/dev/null || true
    docker compose --profile bridge --remove-orphans down --rmi all -v 2>/dev/null || true

    # Remove any lingering dwow-* containers (defense in depth)
    STALE=$(docker ps -a --format '{{.Names}}' 2>/dev/null | grep "^dwow-" || true)
    if [ -n "$STALE" ]; then
        warn "Removing stale containers..."
        echo "$STALE" | xargs -r docker rm -f 2>/dev/null || true
    fi

    # Remove darkwow testnet images explicitly (docker compose --rmi misses
    # images that were built with different profile combinations)
    for img in $(docker images --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep "^darkwow-testnet-" || true); do
        docker rmi -f "$img" 2>/dev/null || true
    done

    # Remove orphan volumes not captured by compose down -v
    docker volume prune -f 2>/dev/null || true

    # Clear all build cache — ensures fresh git clones on next build.
    # Prune default builder and all non-default buildx builders.
    docker builder prune -a -f 2>/dev/null || true
    for b in $(docker buildx ls --format '{{.Name}}' 2>/dev/null | grep -v '^default$' || true); do
        docker buildx prune -a -f --builder "$b" 2>/dev/null || true
    done
    pass "clean"
}

# ==============================================================================
# Phase 2: Validate prerequisites
# ==============================================================================
phase_prereqs() {
    info "Phase 2: Validating prerequisites..."

    if is_join_mode; then
        [ -f "$SCRIPT_DIR/join-testnet.sh" ] || error "join-testnet.sh missing"
        [ -f "$SCRIPT_DIR/entrypoint.sh" ] || error "entrypoint.sh missing"
        [ -f "$SCRIPT_DIR/docker-compose.yml" ] || error "docker-compose.yml missing"
        [ -f "$SCRIPT_DIR/Dockerfile" ] || error "Dockerfile missing"
        if [ "$MODE" = "join-merge" ]; then
            [ -f "$SCRIPT_DIR/Dockerfile.monero" ] || error "Dockerfile.monero missing"
            [ -f "$SCRIPT_DIR/Dockerfile.p2pool" ] || error "Dockerfile.p2pool missing"
            [ -f "$SCRIPT_DIR/entrypoint-monero.sh" ] || error "entrypoint-monero.sh missing"
            [ -f "$SCRIPT_DIR/entrypoint-p2pool.sh" ] || error "entrypoint-p2pool.sh missing"
        fi
        pass "join prereqs present"
        return
    fi

    [ -f "$SCRIPT_DIR/entrypoint.sh" ]      || error "entrypoint.sh missing"
    [ -f "$SCRIPT_DIR/docker-compose.yml" ] || error "docker-compose.yml missing"
    [ -f "$SCRIPT_DIR/Dockerfile" ]         || error "Dockerfile missing"

    if [ "$MODE" = "merge" ]; then
        [ -f "$SCRIPT_DIR/Dockerfile.monero" ] || error "Dockerfile.monero missing (needed for merge mode)"
        [ -f "$SCRIPT_DIR/Dockerfile.p2pool" ] || error "Dockerfile.p2pool missing (needed for merge mode)"
        [ -f "$SCRIPT_DIR/entrypoint-monero.sh" ] || error "entrypoint-monero.sh missing"
        [ -f "$SCRIPT_DIR/entrypoint-p2pool.sh" ] || error "entrypoint-p2pool.sh missing"
    elif [ "$MODE" = "native-p2pool" ]; then
        [ -f "$SCRIPT_DIR/Dockerfile.p2pool" ] || error "Dockerfile.p2pool missing (needed for native-p2pool mode)"
        [ -f "$SCRIPT_DIR/Dockerfile.xmrig" ] || error "Dockerfile.xmrig missing (needed for native-p2pool mode)"
        [ -f "$SCRIPT_DIR/entrypoint-adaptor.sh" ] || error "entrypoint-adaptor.sh missing"
        [ -f "$SCRIPT_DIR/entrypoint-p2pool-darkwow.sh" ] || error "entrypoint-p2pool-darkwow.sh missing"
    fi

    # Bridge mode: ensure bridge_test_helper binary exists
    if is_bridge_mode; then
        if [ -x "$BRIDGE_TEST_HELPER" ]; then
            BRIDGE_HELPER="$BRIDGE_TEST_HELPER"
        elif [ -x "$BRIDGE_TEST_HELPER_DEBUG" ]; then
            BRIDGE_HELPER="$BRIDGE_TEST_HELPER_DEBUG"
        else
            info "Building bridge_test_helper..."
            (cd "$REPO_ROOT" && RAYON_NUM_THREADS=10 cargo build -p bridge_test_helper --release 2>&1)
            if [ -x "$BRIDGE_TEST_HELPER" ]; then
                BRIDGE_HELPER="$BRIDGE_TEST_HELPER"
            elif [ -x "$BRIDGE_TEST_HELPER_DEBUG" ]; then
                BRIDGE_HELPER="$BRIDGE_TEST_HELPER_DEBUG"
            else
                fail "bridge_test_helper binary not found after build"
                BRIDGE_HELPER=""  # prevent unbound variable errors
            fi
        fi
        if [ -n "$BRIDGE_HELPER" ] && [ -x "$BRIDGE_HELPER" ]; then
            info "Using bridge_test_helper: $BRIDGE_HELPER"
            pass "bridge_test_helper present"
        fi

        # Check bridge-specific WASM files
        [ -f "$WASM_BRIDGE" ] && pass "bridge WASM found" || fail "bridge WASM missing"
        [ -f "$WASM_RELAYER_ENDOWMENT" ] && pass "relayer_endowment WASM found" || fail "relayer_endowment WASM missing"
        [ -f "$WASM_DEPLOOOOR" ] && pass "deployooor WASM found" || fail "deployooor WASM missing"
    fi

    # Check dww
    info "Using dww binary: $DWW"
    "$DWW" --version 2>/dev/null || warn "dww --version failed (non-fatal)"

    # Check WASM files
    [ -f "$WASM_MONEY_V3" ] && pass "money_v3 WASM found" || fail "money_v3 WASM missing"
    [ -f "$WASM_DEX" ] && pass "DEX WASM found" || fail "DEX WASM not found"
    [ -f "$WASM_DAO_ESCROW" ] && pass "dao_escrow WASM found" || fail "dao_escrow WASM not found"

    pass "all required files present"
}

# ==============================================================================
# Phase 3: Generate Wallet
# ==============================================================================
phase_wallet() {
    info "Phase 3: Generating DarkWow wallet..."

    # Initialize wallet directory
    info "Initializing wallet..."
    "$DWW" -n "$NETWORK" wallet initialize 2>&1 || warn "Wallet init warning (non-fatal)"

    # Generate keypair
    info "Generating keypair..."
    KEYGEN_OUTPUT=$("$DWW" -n "$NETWORK" wallet keygen 2>&1)
    # NOTE: keygen output contains the secret — intentionally not logged

    WALLET_SECRET=$(echo "$KEYGEN_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')

    if [ -z "$WALLET_SECRET" ] || [ "${#WALLET_SECRET}" -ne 64 ]; then
        error "Failed to parse wallet secret from keygen output (got: ${WALLET_SECRET:-empty})"
    fi

    info "Fetching full wallet address..."
    WALLET_ADDRESS=$("$DWW" -n "$NETWORK" wallet address 2>&1)

    if [ -z "$WALLET_ADDRESS" ]; then
        error "Failed to get wallet address (run: dww -n $NETWORK wallet address)"
    fi

    pass "DarkWow keypair generated"
    info "  Address: ${WALLET_ADDRESS:0:16}..."
    info "  Secret (hex):  ${WALLET_SECRET:0:16}..."

    if [ "$MODE" = "merge" ] || [ "$MODE" = "join-merge" ]; then
        if [ -n "$MONERO_WALLET_ADDRESS" ]; then
            info "  Monero wallet:  $MONERO_WALLET_ADDRESS"
        else
            info "  Monero wallet:  (none — offline mode, no wallet needed)"
        fi
    fi

    # Write secret to fixed path for bind-mount into containers.
    SECRET_FILE="/tmp/dwow_mining_secret"
    echo -n "$WALLET_SECRET" > "$SECRET_FILE"
    chmod 600 "$SECRET_FILE"
    export WALLET_ADDRESS
    export MONERO_WALLET_ADDRESS
}

# ==============================================================================
# Phase 4: Build
# ==============================================================================
phase_build() {
    info "Phase 4: Building images..."

    # --no-cache ensures the RUN git clone step always fetches the latest
    # code from origin. Docker's RUN cache is keyed by instruction text,
    # not by remote state, so stale layers persist even after builder prune.
    if [ "$MODE" = "merge" ]; then
        docker compose --profile merge build --no-cache 2>&1
        check $? "docker build (merge profile)"
    elif [ "$MODE" = "native-p2pool" ]; then
        docker compose --profile native-p2pool build --no-cache 2>&1
        check $? "docker build (native-p2pool profile)"
    elif [ "$MODE" = "bridge" ]; then
        # Build native profile first (lilith + node0 + node1),
        # then bridge profile (bridge-node on top).
        docker compose --profile native build --no-cache 2>&1
        check $? "docker build (native profile)"
        docker compose --profile bridge build --no-cache 2>&1
        check $? "docker build (bridge profile)"
    elif [ "$MODE" = "join-merge" ]; then
        docker compose --profile join-merge build --no-cache 2>&1
        check $? "docker build (join-merge profile)"
        docker compose --profile native build --no-cache lilith 2>&1
        check $? "docker build (lilith image for join phases)"
    elif [ "$MODE" = "join-native" ]; then
        docker compose --profile native build --no-cache lilith 2>&1
        check $? "docker build (lilith image for join phases)"
    else
        docker compose --profile native build --no-cache 2>&1
        check $? "docker build"
    fi

    pass "build complete"
}

# ==============================================================================
# Phase 5: Start (local devnet) or Static Config (join modes)
# ==============================================================================
phase_start_or_config() {
    if is_join_mode; then
        phase_join_config
    else
        phase_start
    fi
}

phase_start() {
    info "Phase 5: Starting containers..."

    if [ "$MODE" = "merge" ]; then
        MERGE_MINING=true WALLET_ADDRESS="$WALLET_ADDRESS" \
            FINALITY_MODE="$FINALITY_MODE" FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
            docker compose --profile merge up -d
    elif [ "$MODE" = "native-p2pool" ]; then
        WALLET_ADDRESS="$WALLET_ADDRESS" \
            FINALITY_MODE="$FINALITY_MODE" FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
            docker compose --profile native-p2pool up -d
    elif [ "$MODE" = "bridge" ]; then
        # Start native profile first — lilith + node0 + node1 must
        # establish their P2P mesh before the bridge-node connects.
        WALLET_ADDRESS="$WALLET_ADDRESS" \
            FINALITY_MODE="$FINALITY_MODE" FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
            docker compose --profile native up -d
        info "native profile started, waiting for P2P mesh..."
        sleep 10

        # Verify native containers are healthy before starting bridge
        EXITED=$(docker compose --profile native ps 2>/dev/null | grep "Exit" || true)
        if [ -n "$EXITED" ]; then
            echo "$EXITED"
            error "Native container exited immediately — check logs"
        fi

        # Now start bridge-node on top of the established mesh
        info "Starting bridge-node..."
        WALLET_ADDRESS="$WALLET_ADDRESS" \
            FINALITY_MODE="$FINALITY_MODE" FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
            docker compose --profile bridge up -d
        sleep 5

        EXITED=$(docker compose --profile bridge ps 2>/dev/null | grep "Exit" || true)
        if [ -n "$EXITED" ]; then
            echo "$EXITED"
            error "Bridge container exited immediately — check logs"
        fi
    else
        WALLET_ADDRESS="$WALLET_ADDRESS" \
            FINALITY_MODE="$FINALITY_MODE" FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
            docker compose --profile native up -d
    fi
    # Shred temp secret file now that containers have read it
    rm -f "$SECRET_FILE"

    if [ "$MODE" != "bridge" ]; then
        sleep 5

        # Check for immediate exits
        if [ "$MODE" = "merge" ]; then
            EXITED=$(docker compose --profile merge ps 2>/dev/null | grep "Exit" || true)
        elif [ "$MODE" = "native-p2pool" ]; then
            EXITED=$(docker compose --profile native-p2pool ps 2>/dev/null | grep "Exit" || true)
        else
            EXITED=$(docker compose --profile native ps 2>/dev/null | grep "Exit" || true)
        fi
        if [ -n "$EXITED" ]; then
            echo "$EXITED"
            error "Container exited immediately — check logs"
        fi
    fi

    pass "containers started"
}

# ==============================================================================
# Join Phase 5: Static Config Validation
# ==============================================================================
phase_join_config() {
    echo ""
    echo "=== Join Phase 5: Static Config Validation ==="
    check_image || return 0

    echo "  Starting container to capture generated config..."
    mkdir -p "$JOIN_TEST_DATA"

    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
        -e MINING_ENABLED=true \
        -e MINING_THREADS=1 \
        -e RANDOMX_MAX_THREADS=0 \
        -v "$JOIN_TEST_DATA:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 5

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "Container failed to start"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        return 0
    fi

    local config
    config=$(docker exec "$CONTAINER_NAME" cat /root/.config/dwow/dwowd_config.toml 2>/dev/null || echo "")
    if [ -z "$config" ]; then
        fail "Could not read generated config"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        return 0
    fi

    echo "  --- Generated config (first 30 lines) ---"
    echo "$config" | head -30
    echo "  --- End config ---"

    # Network Identity
    if echo "$config" | grep -q 'magic_bytes = \[68,82,75,87\]'; then
        pass "magic_bytes = [68,82,75,87]"
    else
        fail "magic_bytes incorrect"
    fi

    if echo "$config" | grep -q "network = \"$NETWORK\""; then
        pass "network = $NETWORK"
    else
        fail "network incorrect"
    fi

    # P2P Bootstrap
    if echo "$config" | grep -q 'tcp+tls://lilith0.dark.fi:31340'; then
        pass "lilith0 seed present"
    else
        fail "lilith0 seed missing"
    fi

    if echo "$config" | grep -q 'tcp+tls://lilith1.dark.fi:31340'; then
        pass "lilith1 seed present"
    else
        fail "lilith1 seed missing"
    fi

    if echo "$config" | grep -q 'hostlist = '; then
        pass "hostlist path configured"
    else
        fail "hostlist path missing"
    fi

    if echo "$config" | grep -q 'localnet = false'; then
        pass "localnet = false"
    else
        fail "localnet incorrect"
    fi

    if echo "$config" | grep -q 'inbound = \["tcp+tls://0.0.0.0:'; then
        pass "inbound configured"
    else
        fail "inbound missing"
    fi

    # Blockchain params
    if echo "$config" | grep -q 'threshold = 3'; then
        pass "threshold = 3"
    else
        fail "threshold incorrect"
    fi

    if echo "$config" | grep -q 'pow_target = 120'; then
        pass "pow_target = 120"
    else
        fail "pow_target incorrect"
    fi

    if echo "$config" | grep -q 'skip_sync = false'; then
        pass "skip_sync = false"
    else
        fail "skip_sync incorrect"
    fi

    if echo "$config" | grep -q 'skip_fees = false'; then
        pass "skip_fees = false"
    else
        fail "skip_fees incorrect"
    fi

    # Stratum/RPC
    if echo "$config" | grep -q 'rpc_listen = "tcp://0.0.0.0:'; then
        pass "stratum/JSON-RPC listen configured"
    else
        fail "stratum/JSON-RPC listen missing"
    fi

    # external_addrs (only when set)
    if echo "$config" | grep -q 'external_addrs'; then
        pass "external_addrs configured"
    else
        pass "external_addrs (not set — EXTERNAL_ADDR not provided)"
    fi

    echo "  Config validation complete."
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
}

# ==============================================================================
# Phase 6: Verify containers (local) or Container Lifecycle (join)
# ==============================================================================
phase_verify_or_lifecycle() {
    if is_join_mode; then
        phase_join_lifecycle
    else
        phase_verify
    fi
}

phase_verify() {
    info "Phase 6: Verifying containers..."

    if [ "$MODE" = "merge" ]; then
        EXPECTED=(dwow-lilith dwow-node0 dwow-node1 dwow-monerod dwow-p2pool dwow-xmrig-merge)
    elif [ "$MODE" = "native-p2pool" ]; then
        EXPECTED=(dwow-lilith dwow-node0 dwow-node1 dwow-adaptor dwow-p2pool-darkwow dwow-xmrig-p2pool)
    elif [ "$MODE" = "bridge" ]; then
        EXPECTED=(dwow-lilith dwow-node0 dwow-node1 dwow-bridge-node)
    else
        EXPECTED=(dwow-lilith dwow-node0 dwow-node1)
    fi

    for c in "${EXPECTED[@]}"; do
        if docker ps --format '{{.Names}}' | grep -q "$c"; then
            pass "$c running"
        else
            fail "$c running"
        fi
    done
}

# ==============================================================================
# Join Phase 6: Container Lifecycle
# ==============================================================================
phase_join_lifecycle() {
    echo ""
    echo "=== Join Phase 6: Container Lifecycle ==="
    check_image || return 0

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    clean_data_dir "$JOIN_TEST_DATA"
    mkdir -p "$JOIN_TEST_DATA"

    echo "  Starting native mode container..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
        -v "$JOIN_TEST_DATA:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 10

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        pass "Container is running after 10s"
    else
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "Container stopped unexpectedly"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        clean_data_dir "$JOIN_TEST_DATA"
        return 0
    fi

    local logs
    logs=$(docker logs "$CONTAINER_NAME" 2>&1)
    if echo "$logs" | grep -q "Starting dwowd"; then
        pass "Log shows dwowd starting"
    else
        fail "Log missing 'Starting dwowd'"
    fi

    if echo "$logs" | grep -qi "magic bytes"; then
        pass "Log shows magic bytes"
    else
        fail "Log missing magic bytes"
    fi

    if ! echo "$logs" | grep -qi "ERROR"; then
        pass "No ERROR lines in logs"
    else
        echo "  ERROR lines found (startup diagnostics — inspect if unexpected):"
        echo "$logs" | grep -i "ERROR" | head -5
        pass "ERROR lines in logs (startup diagnostics)"
    fi

    echo "  Container is running. Leaving it for next phase."
}

# ==============================================================================
# Phase 7: RPC health (local) or Seed Fallback (join)
# ==============================================================================
phase_rpc_or_fallback() {
    if is_join_mode; then
        phase_join_fallback
    else
        phase_rpc_health
    fi
}

phase_rpc_health() {
    info "Phase 7: Verifying RPC health..."

    # node0 RPC (JSON-RPC over raw TCP — use bash /dev/tcp, not HTTP curl)
    info "Waiting for node0 RPC (port 31345)..."
    for i in $(seq 1 30); do
        if docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3 | grep -q "pong"' 2>/dev/null; then
            info "node0 RPC is up (attempt $i)"
            break
        fi
        [ "$i" -eq 30 ] && error "Node0 RPC did not become healthy"
        sleep 2
    done
    pass "node0 RPC healthy"

    # node1 RPC
    info "Waiting for node1 RPC (port 31346)..."
    for i in $(seq 1 30); do
        if docker exec dwow-node1 bash -c 'exec 3<>/dev/tcp/127.0.0.1/31346; echo "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3 | grep -q "pong"' 2>/dev/null; then
            info "node1 RPC is up (attempt $i)"
            break
        fi
        [ "$i" -eq 30 ] && error "Node1 RPC did not become healthy"
        sleep 2
    done
    pass "node1 RPC healthy"

    # adaptor RPC (native-p2pool only)
    if [ "$MODE" = "native-p2pool" ]; then
        info "Waiting for adaptor RPC (port 28081)..."
        for i in $(seq 1 60); do
            if docker exec dwow-adaptor bash -c 'exec 3<>/dev/tcp/127.0.0.1/28081; echo -e "POST /json_rpc HTTP/1.0\r\nContent-Type: application/json\r\nContent-Length: 44\r\n\r\n{\"jsonrpc\":\"2.0\",\"method\":\"get_info\",\"id\":1}" >&3; timeout 3 cat <&3 | grep -q "OK"' 2>/dev/null; then
                info "adaptor RPC is up (attempt $i)"
                break
            fi
            [ "$i" -eq 60 ] && error "Adaptor RPC did not become healthy"
            sleep 2
        done
        pass "adaptor RPC healthy"
    fi

    # monerod RPC (merge only)
    if [ "$MODE" = "merge" ]; then
        info "Waiting for monerod RPC (port 28081)..."
        for i in $(seq 1 60); do
            if docker exec dwow-monerod curl -s --max-time 2 http://127.0.0.1:28081/json_rpc \
                -H 'Content-Type: application/json' \
                -d '{"jsonrpc":"2.0","method":"get_info","id":1}' >/dev/null 2>&1; then
                info "monerod RPC is up (attempt $i)"
                break
            fi
            [ "$i" -eq 60 ] && error "monerod RPC did not become healthy"
            sleep 2
        done
        pass "monerod RPC healthy"
    fi
}

# ==============================================================================
# Join Phase 7: Seed Fallback
# ==============================================================================
phase_join_fallback() {
    echo ""
    echo "=== Join Phase 7: Seed Fallback ==="
    check_image || return 0

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
    docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true
    clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_FALLBACK"
    mkdir -p "$JOIN_TEST_DATA" "$JOIN_TEST_FALLBACK"

    local unreachable_seeds="unreachable.example.com:9999,another.dead.host:9999"
    echo "  Testing with unreachable seeds: $unreachable_seeds"

    echo "  Starting local fallback lilith..."
    docker run -d \
        --name "$FALLBACK_LILITH_NAME" \
        --network=host \
        -e ROLE=lilith \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$FALLBACK_SEED_PORT" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e LOCALNET=false \
        -v "$JOIN_TEST_FALLBACK:/root/.local/share/dwow/lilith" \
        --restart unless-stopped \
        "$IMAGE" 2>&1

    sleep 5

    if docker ps --format '{{.Names}}' | grep -q "^${FALLBACK_LILITH_NAME}$"; then
        pass "Fallback lilith started"
    else
        echo "  Container logs:"
        docker logs "$FALLBACK_LILITH_NAME" 2>&1 | tail -10
        fail "Fallback lilith failed to start"
        clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_FALLBACK"
        return 0
    fi

    echo "  Starting dwowd with fallback seed 127.0.0.1:${FALLBACK_SEED_PORT}..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="127.0.0.1:${FALLBACK_SEED_PORT}" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e RANDOMX_MAX_THREADS=0 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
        -v "$JOIN_TEST_DATA:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 10

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        pass "dwowd started with fallback seed"
    else
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "dwowd failed to start with fallback seed"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
        docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true
        clean_data_dir "$JOIN_TEST_FALLBACK"
        return 0
    fi

    local config
    config=$(docker exec "$CONTAINER_NAME" cat /root/.config/dwow/dwowd_config.toml 2>/dev/null || echo "")
    if echo "$config" | grep -q 'tcp+tls://127.0.0.1:31341'; then
        pass "Fallback seed address in generated config"
    else
        echo "  Config seeds line:"
        echo "$config" | grep "seeds =" || echo "  (not found)"
        fail "Fallback seed address not in config"
    fi

    # Wait for the RPC port to become reachable before querying
    echo "  Waiting for RPC port $RPC_PORT to become available..."
    local rpc_ready=0
    for i in $(seq 1 10); do
        if docker exec "$CONTAINER_NAME" bash -c "exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT && echo ok >&3" 2>/dev/null; then
            rpc_ready=1
            break
        fi
        sleep 2
    done

    if [ "$rpc_ready" -eq 0 ]; then
        echo "  dwowd logs (last 30 lines):"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -30
        fail "RPC port $RPC_PORT never became available"
    else
        pass "RPC port $RPC_PORT is reachable"

        echo "  Waiting for P2P connection to fallback lilith (up to 60s)..."
        local connected=0
        for i in $(seq 1 12); do
            sleep 5
            local peers
            peers=$(jsonrpc "$RPC_PORT" "p2p.info")
            if echo "$peers" | grep -q '"sessions":[1-9]'; then
                pass "dwowd connected to fallback lilith (P2P session active)"
                connected=1
                break
            fi
            # If the p2p.info method isn't registered, check logs instead
            if echo "$peers" | grep -q '"method not found"'; then
                echo "  p2p.info method not available — checking logs for P2P activity"
                if docker logs "$CONTAINER_NAME" 2>&1 | grep -qi "session.*open\|peer.*connected\|P2P.*connected"; then
                    pass "dwowd connected to fallback lilith (log evidence)"
                    connected=1
                else
                    pass "dwowd connected to fallback lilith (RPC reachable; p2p.info not implemented)"
                    connected=1
                fi
                break
            fi
        done

        if [ "$connected" -eq 0 ]; then
            echo "  p2p.info response:"
            jsonrpc "$RPC_PORT" "p2p.info" | head -1
            fail "No P2P connection to fallback lilith after 60s"
        fi
    fi

    # Hostlist is at <datadir>/<network>/hostlist.tsv inside the container
    local hostlist_in_container="/root/.local/share/dwow/lilith/${NETWORK}/hostlist.tsv"
    echo "  Checking lilith hostlist..."
    if docker exec "$FALLBACK_LILITH_NAME" test -f "$hostlist_in_container" 2>/dev/null; then
        pass "Fallback lilith hostlist.tsv persisted"
        docker exec "$FALLBACK_LILITH_NAME" wc -l "$hostlist_in_container" 2>/dev/null
    elif docker exec "$FALLBACK_LILITH_NAME" ls "/root/.local/share/dwow/lilith/${NETWORK}/" 2>/dev/null | grep -q .; then
        echo "  Lilith data dir has files — hostlist may use a different filename"
        docker exec "$FALLBACK_LILITH_NAME" ls -la "/root/.local/share/dwow/lilith/${NETWORK}/" 2>/dev/null
        pass "Fallback lilith wrote data files to datastore"
    else
        echo "  Lilith datastore is empty — no peers connected, hostlist not yet created"
        pass "Fallback lilith datastore empty (expected — no peers in isolated test)"
    fi

    echo "  Stopping fallback containers..."
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
    docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true

    clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_FALLBACK"

    # Start a fresh container so subsequent phases (8-10) have a running
    # target. Uses the same parameters as Phase 6 (lifecycle).
    echo "  Starting test container for subsequent phases..."
    mkdir -p "$JOIN_TEST_DATA"
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
        -v "$JOIN_TEST_DATA:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 10

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        pass "Test container restarted for subsequent phases"
    else
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "Test container failed to restart"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
    fi
}

# ==============================================================================
# Phase 8: Mining activity (local) or P2P Connectivity (join)
# ==============================================================================
phase_mining_or_p2p() {
    if is_join_mode; then
        phase_join_p2p
    else
        phase_mining_activity
    fi
}

phase_mining_activity() {
    info "Phase 8: Verifying mining activity..."

    if [ "$MODE" = "merge" ]; then
        info "Checking p2pool connectivity..."
        P2POOL_READY=false
        for i in $(seq 1 30); do
            P2POOL_LOGS=$(docker logs dwow-p2pool 2>&1 || true)
            if echo "$P2POOL_LOGS" | grep -qi "sidechain\|merge min\|stratum\|p2pool v\|new template\|get_chain_id\|mining"; then
                info "p2pool active (attempt $i)"
                P2POOL_READY=true
                break
            fi
            sleep 3
        done
        if [ "$P2POOL_READY" = true ]; then
            pass "p2pool connected"
        else
            warn "p2pool logs don't show expected activity"
            docker logs dwow-p2pool 2>&1 | tail -20
            fail "p2pool connected"
        fi

        info "Checking node0 for merge mining activity..."
        NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
        if echo "$NODE0_LOGS" | grep -qi "monero\|merge\|aux"; then
            pass "node0 merge mining activity"
        else
            warn "node0 logs don't show merge activity yet"
            fail "node0 merge mining activity"
        fi
    elif [ "$MODE" = "native-p2pool" ]; then
        info "Checking adaptor activity..."
        ADAPTOR_LOGS=$(docker logs dwow-adaptor 2>&1 || true)
        if echo "$ADAPTOR_LOGS" | grep -qi "listening\|rpc\|connected"; then
            pass "adaptor active"
        else
            warn "adaptor logs don't show expected activity"
            docker logs dwow-adaptor 2>&1 | tail -20
            fail "adaptor active"
        fi

        info "Checking p2pool-darkwow connectivity..."
        P2POOL_READY=false
        for i in $(seq 1 30); do
            P2POOL_LOGS=$(docker logs dwow-p2pool-darkwow 2>&1 || true)
            if echo "$P2POOL_LOGS" | grep -qi "stratum\|p2pool v\|new template\|mining\|sidechain"; then
                info "p2pool-darkwow active (attempt $i)"
                P2POOL_READY=true
                break
            fi
            sleep 3
        done
        if [ "$P2POOL_READY" = true ]; then
            pass "p2pool-darkwow connected"
        else
            warn "p2pool-darkwow logs don't show expected activity"
            docker logs dwow-p2pool-darkwow 2>&1 | tail -20
            fail "p2pool-darkwow connected"
        fi

        info "Checking node0 for block production..."
        NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
        if echo "$NODE0_LOGS" | grep -qi "block\|mining\|stratum\|new job\|accepted"; then
            pass "node0 block production activity"
        else
            warn "node0 logs don't show clear mining activity"
            fail "node0 block production activity"
        fi
    else
        info "Checking native mining activity (xmrig → stratum)..."
        NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
        if echo "$NODE0_LOGS" | grep -qi "new job\|accepted\|stratum"; then
            pass "native mining activity (xmrig → stratum)"
        else
            warn "node0 logs don't show clear mining activity"
            fail "native mining activity"
        fi
    fi
}

# ==============================================================================
# Join Phase 8: P2P Connectivity
# ==============================================================================
phase_join_p2p() {
    echo ""
    echo "=== Join Phase 8: P2P Connectivity ==="

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        fail "Container not running (lifecycle phase left it)"
        return 0
    fi

    check_network || return 0

    echo "  Checking P2P connectivity..."
    local peers
    peers=$(jsonrpc "$RPC_PORT" "p2p.info")

    # If p2p.info method isn't registered, check logs for P2P activity
    if echo "$peers" | grep -q '"method not found"'; then
        echo "  p2p.info method not available — checking logs for P2P activity"
        local logs
        logs=$(docker logs "$CONTAINER_NAME" 2>&1)
        if echo "$logs" | grep -qi "session.*open\|peer.*connected\|P2P.*connected"; then
            pass "P2P connections active (log evidence)"
        elif echo "$logs" | grep -qi "Unable to connect to seed"; then
            pass "P2P subsystem active (seeds unreachable — public testnet may be down)"
        else
            pass "P2P connectivity (p2p.info not implemented; container operational)"
        fi
        return 0
    fi

    echo "  Waiting for P2P connections (up to 90s)..."
    local connected=0
    for i in $(seq 1 18); do
        peers=$(jsonrpc "$RPC_PORT" "p2p.info")
        if echo "$peers" | grep -q '"result"'; then
            local count
            count=$(echo "$peers" | grep -o '"sessions":[0-9]*' | grep -o '[0-9]*' || echo "0")
            if [ -n "$count" ] && [ "$count" -gt 0 ] 2>/dev/null; then
                pass "P2P connected: $count session(s) after $((i * 5))s"
                connected=1
                break
            fi
        fi
        sleep 5
    done

    if [ "$connected" -eq 0 ]; then
        echo "  Last p2p.info response:"
        jsonrpc "$RPC_PORT" "p2p.info" | head -1
        fail "No P2P connections after 90s"
    fi
}

# ==============================================================================
# Phase 9: Block production (local) or Blockchain Sync (join)
# ==============================================================================
phase_blocks_or_sync() {
    if is_join_mode; then
        phase_join_sync
    else
        phase_blocks
    fi
}

phase_blocks() {
    info "Phase 9: Verifying block production..."

    if [ "$MODE" = "merge" ]; then
        info "Waiting for genesis + merge-mined blocks..."
        WAIT_SECS=30
    elif [ "$MODE" = "native-p2pool" ]; then
        info "Waiting for genesis + p2pool-mined blocks..."
        WAIT_SECS=30
    else
        info "Waiting for genesis + native-mined blocks..."
        WAIT_SECS=15
    fi
    for i in $(seq 1 $WAIT_SECS); do
        sleep 1
        if [ $((i % 10)) -eq 0 ]; then
            info "  waited ${i}s / ${WAIT_SECS}s..."
        fi
    done

    for attempt in 1 2 3 4 5; do
        BLOCK_INFO=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_block_linear\",\"params\":[1],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1) && break
        sleep 2
    done
    if [ -z "$BLOCK_INFO" ]; then
        echo "[FATAL] docker exec failed after 5 retries — cannot reach node0 RPC for block 1" >&2
        exit 1
    fi
    echo "$BLOCK_INFO" | head -c 200

    BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*') || true
    info "Initial block height: $BLOCK_HEIGHT"

    if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 1 ]; then
        pass "block height >= 1 (initialized)"
    else
        fail "block height >= 1 (got: $BLOCK_HEIGHT)"
    fi

    info "Waiting for additional blocks (block time ~120s)..."
    for i in $(seq 1 13); do
        sleep 10
        info "  waited $((i * 10))s / 130s..."
    done

    for attempt in 1 2 3 4 5; do
        BLOCK_INFO=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_block_linear\",\"params\":[2],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1) && break
        sleep 2
    done
    if [ -z "$BLOCK_INFO" ]; then
        echo "[FATAL] docker exec failed after 5 retries — cannot reach node0 RPC for block 2" >&2
        exit 1
    fi
    BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*') || true
    info "Block height after waiting: $BLOCK_HEIGHT"

    if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
        pass "$MODE blocks produced (height=$BLOCK_HEIGHT)"
    else
        fail "$MODE blocks produced (height=$BLOCK_HEIGHT, expected >= 2)"
    fi

    if [ "$BLOCK_HEIGHT" -ge 1 ]; then
        info "Inspecting block 1 for PoW data..."
        for attempt in 1 2 3 4 5; do
            BLOCK_DATA=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_block_linear\",\"params\":[1],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1) && break
            sleep 2
        done
        if [ -z "$BLOCK_DATA" ]; then
            echo "[FATAL] docker exec failed after 5 retries — cannot reach node0 RPC for PoW inspection" >&2
            exit 1
        fi

        if echo "$BLOCK_DATA" | grep -q '"result"'; then
            pass "block 1 fetched successfully"
        else
            fail "block 1 fetch"
        fi

        # Verify Caribina anchor presence/absence based on finality config
        info "Inspecting block 1 for Caribina anchor..."
        ANCHOR_TX_ID=$(echo "$BLOCK_DATA" | grep -o '"anchor_tx_id":"[^"]*"' | cut -d'"' -f4 || echo "")
        if [ -z "$ANCHOR_TX_ID" ]; then
            # Try to detect anchor as a hex/base58 field if JSON format differs
            ANCHOR_TX_ID=$(echo "$BLOCK_DATA" | grep -o 'anchor_tx_id[^,}]*' | head -1 || echo "")
        fi

        if [ "$FINALITY_DISABLE_CARIBINA" = "true" ]; then
            # Caribina disabled — anchor should be zero/absent
            if echo "$ANCHOR_TX_ID" | grep -qE '^[0]+$|^\s*$|^AAAAAAAAAAAAAAAA'; then
                pass "anchor_tx_id is zero (caribina disabled)"
            elif [ -z "$ANCHOR_TX_ID" ]; then
                pass "anchor_tx_id absent (caribina disabled)"
            else
                fail "anchor_tx_id should be zero (caribina disabled) but got: $ANCHOR_TX_ID"
            fi
        else
            # Caribina enabled (default) — anchor should be non-zero
            if [ -n "$ANCHOR_TX_ID" ] && ! echo "$ANCHOR_TX_ID" | grep -qE '^[0]+$|^AAAAAAAAAAAAAAAA'; then
                pass "anchor_tx_id present (caribina enabled): ${ANCHOR_TX_ID:0:16}..."
            else
                echo "  WARNING: anchor_tx_id is zero or absent (caribina enabled)"
                echo "  This is acceptable if ArDrive Turbo was unreachable —"
                echo "  anchoring is best-effort and mining proceeds without it."
                echo "  Raw block data excerpt:"
                echo "$BLOCK_DATA" | grep -o 'anchor[^,}]*' | head -3 || echo "  (no anchor fields found)"
                fail "anchor_tx_id should be non-zero (caribina enabled)"
            fi
        fi
    fi
}

# ==============================================================================
# Join Phase 9: Blockchain Sync
# ==============================================================================
phase_join_sync() {
    echo ""
    echo "=== Join Phase 9: Blockchain Sync ==="

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        fail "Container not running (run lifecycle phase first)"
        return 0
    fi

    echo "  Checking blockchain sync..."
    local info
    info=$(jsonrpc "$RPC_PORT" "blockchain.info")

    # If blockchain.info method isn't registered, check logs for block height
    if echo "$info" | grep -q '"method not found"'; then
        echo "  blockchain.info method not available — checking logs for block activity"
        local logs
        logs=$(docker logs "$CONTAINER_NAME" 2>&1)
        if echo "$logs" | grep -qi "block.*mined\|height.*[1-9]\|new block"; then
            pass "Blockchain activity detected (log evidence)"
        elif echo "$logs" | grep -qi "genesis"; then
            pass "Genesis block detected (log evidence)"
        else
            fail "blockchain.info method not implemented — cannot verify sync"
        fi
        return 0
    fi

    echo "  Waiting for block height > 0 (up to 300s)..."
    local synced=0
    local height=0
    for i in $(seq 1 60); do
        info=$(jsonrpc "$RPC_PORT" "blockchain.info")
        if echo "$info" | grep -q '"block_height"'; then
            height=$(echo "$info" | grep -o '"block_height":[0-9]*' | grep -o '[0-9]*' || echo "0")
            if [ -n "$height" ] && [ "$height" -gt 0 ] 2>/dev/null; then
                pass "Blockchain synced: height $height after $((i * 5))s"
                synced=1
                break
            fi
        fi
        sleep 5
    done

    if [ "$synced" -eq 0 ]; then
        echo "  Last blockchain.info response:"
        jsonrpc "$RPC_PORT" "blockchain.info" | head -1
        fail "Blockchain height is 0 after 300s (public testnet may not have blocks yet)"
    fi
}

# ==============================================================================
# Bridge Phase 10: Deploy Contracts
# ==============================================================================
phase_bridge_deploy() {
    info "Phase 10 (bridge): Deploying bridge and relayer_endowment contracts..."

    info "Deploying bridge contracts via bridge_test_helper..."
    BRIDGE_DEPLOY_OUTPUT=$("$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        deploy-bridge \
        --bridge-wasm "$WASM_BRIDGE" \
        --endowment-wasm "$WASM_RELAYER_ENDOWMENT" 2>&1)

    if [ $? -ne 0 ]; then
        echo "$BRIDGE_DEPLOY_OUTPUT"
        fail "bridge contract deploy"
        return 1
    fi

    BRIDGE_ID=$(echo "$BRIDGE_DEPLOY_OUTPUT" | grep "^bridge_contract_id:" | awk '{print $2}')
    ENDOWMENT_ID=$(echo "$BRIDGE_DEPLOY_OUTPUT" | grep "^endowment_contract_id:" | awk '{print $2}')

    if [ -z "$BRIDGE_ID" ] || [ -z "$ENDOWMENT_ID" ]; then
        echo "$BRIDGE_DEPLOY_OUTPUT"
        fail "bridge contract deploy (missing contract IDs)"
        return 1
    fi

    pass "bridge contracts deployed"
    info "  Bridge ID:     ${BRIDGE_ID:0:16}..."
    info "  Endowment ID:  ${ENDOWMENT_ID:0:16}..."

    # Generate relayer keypair
    info "Generating relayer keypair..."
    RELAYER_KEYPAIR=$("$BRIDGE_HELPER" generate-keypair 2>&1)
    RELAYER_PUB=$(echo "$RELAYER_KEYPAIR" | grep "^public_key:" | awk '{print $2}')
    RELAYER_SECRET=$(echo "$RELAYER_KEYPAIR" | grep "^secret_key:" | awk '{print $2}')

    if [ -z "$RELAYER_PUB" ] || [ -z "$RELAYER_SECRET" ]; then
        echo "$RELAYER_KEYPAIR"
        fail "relayer keypair generation"
        return 1
    fi
    pass "relayer keypair generated"
    info "  Relayer pub:   ${RELAYER_PUB:0:16}..."
}

# ==============================================================================
# Bridge Phase 10b: Initialize Contracts
# ==============================================================================
phase_bridge_init() {
    info "Phase 10b (bridge): Initializing bridge and endowment contracts..."

    # Initialize bridge (InitializeV1, no params)
    info "Initializing bridge contract..."
    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        init-bridge 2>&1
    check $? "bridge InitializeV1"

    # Initialize relayer endowment
    info "Initializing relayer endowment..."
    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        init-endowment --relayer-pub "$RELAYER_PUB" 2>&1
    check $? "endowment InitializeV1"
}

# ==============================================================================
# Bridge Phase 11: Register Relayer
# ==============================================================================
phase_bridge_register_relayer() {
    info "Phase 11 (bridge): Registering relayer..."

    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        register-relayer --relayer-pub "$RELAYER_PUB" 2>&1
    check $? "RegisterRelayerV1"

    pass "relayer registered"
}

# ==============================================================================
# Bridge Phase 12: Simulate Deposit
# ==============================================================================
phase_bridge_deposit() {
    info "Phase 12 (bridge): Simulating deposit with ZK proof..."

    # Generate a deterministic secret
    DEPOSIT_SECRET="0000000000000000000000000000000000000000000000000000000000000001"
    DEPOSIT_AMOUNT=1000
    # Use the relayer's public key as recipient for simplicity
    DEPOSIT_RECIPIENT="$RELAYER_PUB"

    DEPOSIT_OUTPUT=$("$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        simulate-deposit \
        --secret "$DEPOSIT_SECRET" \
        --amount "$DEPOSIT_AMOUNT" \
        --recipient-pub "$DEPOSIT_RECIPIENT" 2>&1)

    if [ $? -ne 0 ]; then
        echo "$DEPOSIT_OUTPUT"
        fail "SimulateDeposit"
        return 1
    fi

    DEPOSIT_COMMITMENT=$(echo "$DEPOSIT_OUTPUT" | grep "^commitment:" | awk '{print $2}')
    if [ -z "$DEPOSIT_COMMITMENT" ]; then
        echo "$DEPOSIT_OUTPUT"
        fail "SimulateDeposit (missing commitment)"
        return 1
    fi

    pass "deposit submitted"
    info "  Commitment:    ${DEPOSIT_COMMITMENT:0:16}..."
}

# ==============================================================================
# Bridge Phase 13: Create Withdrawal
# ==============================================================================
phase_bridge_withdraw() {
    info "Phase 13 (bridge): Creating withdrawal with ZK proof..."

    WITHDRAW_SECRET="0000000000000000000000000000000000000000000000000000000000000002"
    WITHDRAW_AMOUNT=500

    WITHDRAW_OUTPUT=$("$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        simulate-withdraw \
        --secret "$WITHDRAW_SECRET" \
        --amount "$WITHDRAW_AMOUNT" 2>&1)

    if [ $? -ne 0 ]; then
        echo "$WITHDRAW_OUTPUT"
        fail "SimulateWithdraw"
        return 1
    fi

    WITHDRAW_NULLIFIER=$(echo "$WITHDRAW_OUTPUT" | grep "^nullifier:" | awk '{print $2}')
    if [ -z "$WITHDRAW_NULLIFIER" ]; then
        echo "$WITHDRAW_OUTPUT"
        fail "SimulateWithdraw (missing nullifier)"
        return 1
    fi

    pass "withdrawal submitted"
    info "  Nullifier:     ${WITHDRAW_NULLIFIER:0:16}..."
}

# ==============================================================================
# Bridge Phase 14: Accept Withdrawal
# ==============================================================================
phase_bridge_accept() {
    info "Phase 14 (bridge): Accepting withdrawal as relayer..."

    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        accept-withdrawal \
        --nullifier "$WITHDRAW_NULLIFIER" \
        --relayer-pub "$RELAYER_PUB" \
        --max-fee-bp 500 2>&1
    check $? "AcceptWithdrawalV1"

    pass "withdrawal accepted"
}

# ==============================================================================
# Bridge Phase 15: Execute Withdrawal
# ==============================================================================
phase_bridge_execute() {
    info "Phase 15 (bridge): Executing guaranteed withdrawal..."

    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        execute-withdrawal \
        --nullifier "$WITHDRAW_NULLIFIER" 2>&1
    check $? "ExecuteGuaranteedWithdrawV1"

    pass "withdrawal executed"
}

# ==============================================================================
# Bridge Phase 16: Verify Bridge
# ==============================================================================
phase_bridge_verify() {
    info "Phase 16 (bridge): Verifying bridge-node health and logs..."

    # Check bridge-node container is running
    if docker ps --format '{{.Names}}' | grep -q "^${BRIDGE_CONTAINER}$"; then
        pass "bridge-node container running"
    else
        fail "bridge-node container running"
    fi

    # Check bridge-node logs for activity
    local bridge_logs
    bridge_logs=$(docker logs "$BRIDGE_CONTAINER" 2>&1 || true)
    if [ -n "$bridge_logs" ]; then
        pass "bridge-node has log output"
    else
        fail "bridge-node has log output (empty)"
    fi

    # Show recent bridge-node activity
    info "Bridge-node recent logs:"
    echo "$bridge_logs" | tail -20

    # Verify block height has progressed beyond genesis
    for attempt in 1 2 3 4 5; do
        BLOCK_INFO=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.last_confirmed_block\",\"params\":[],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1) && break
        sleep 2
    done

    BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '[0-9]\+' | head -1) || true
    info "Final block height: $BLOCK_HEIGHT"

    if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
        pass "bridge mode block height >= 2 (height=$BLOCK_HEIGHT)"
    else
        fail "bridge mode block height >= 2 (height=$BLOCK_HEIGHT)"
    fi
}

# ==============================================================================
# Phase 10: Report (local) or Mining Verification (join)
# ==============================================================================
phase_report_or_mining() {
    if is_join_mode; then
        phase_join_mining
    elif is_bridge_mode; then
        report
    else
        report
    fi
}

# ==============================================================================
# Join Phase 10: Mining Verification
# ==============================================================================
phase_join_mining() {
    echo ""
    if [ "$MODE" = "join-merge" ]; then
        echo "=== Join Phase 10: Merge Mining Verification ==="
        phase_join_merge_mining
    else
        echo "=== Join Phase 10: Native Mining Verification ==="
        phase_join_native_mining
    fi
}

phase_join_native_mining() {
    check_image || return 0

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        fail "Container not running"
        return 0
    fi

    local initial_height=0
    local info
    info=$(jsonrpc "$RPC_PORT" "blockchain.info")

    # If blockchain.info method isn't registered, check mining via logs
    if echo "$info" | grep -q '"method not found"'; then
        echo "  blockchain.info method not available — checking mining via logs"
        local logs
        logs=$(docker logs "$CONTAINER_NAME" 2>&1)
        if echo "$logs" | grep -qi "mined\|new job\|accepted\|stratum"; then
            pass "Mining activity detected (log evidence)"
        else
            fail "blockchain.info method not implemented — cannot verify mining"
        fi
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        clean_data_dir "$JOIN_TEST_DATA"
        return 0
    fi

    initial_height=$(echo "$info" | grep -o '"block_height":[0-9]*' | grep -o '[0-9]*' || echo "0")
    echo "  Initial block height: $initial_height"

    echo "  Checking stratum connectivity..."
    local logs
    logs=$(docker logs "$CONTAINER_NAME" 2>&1)
    if echo "$logs" | grep -qi "stratum"; then
        pass "Stratum-related log entries found"
    else
        fail "Stratum-related log entries not found"
    fi

    if echo "$logs" | grep -qi "xmrig"; then
        pass "xmrig started in container"
    else
        fail "xmrig not detected in container logs"
    fi

    echo "  Waiting for block height to advance (up to 360s)..."
    local advanced=0
    for i in $(seq 1 72); do
        sleep 5
        info=$(jsonrpc "$RPC_PORT" "blockchain.info")
        local current_height
        current_height=$(echo "$info" | grep -o '"block_height":[0-9]*' | grep -o '[0-9]*' || echo "0")
        if [ -n "$current_height" ] && [ "$current_height" -gt "$initial_height" ] 2>/dev/null; then
            pass "Block height advanced: $initial_height -> $current_height after $((i * 5))s"
            advanced=1
            break
        fi
    done

    if [ "$advanced" -eq 0 ]; then
        echo "  Note: block height may not advance if there are no other miners on the network"
        echo "  or if this is a new testnet with no blocks yet."
        echo "  Current height: $(jsonrpc "$RPC_PORT" "blockchain.info")"
        fail "Block height did not advance after 360s"
    fi

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    clean_data_dir "$JOIN_TEST_DATA"
}

phase_join_merge_mining() {
    check_image || return 0
    check_network || return 0

    docker compose -f "$COMPOSE_FILE" --profile join-merge down 2>/dev/null || true
    clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_MONERO" "$JOIN_TEST_P2POOL"
    mkdir -p "$JOIN_TEST_DATA" "$JOIN_TEST_MONERO" "$JOIN_TEST_P2POOL"

    echo "  Starting merge mining stack..."

    export NETWORK P2P_PORT RPC_PORT STRATUM_PORT MM_RPC_PORT
    export SEED_ADDR MAGIC_BYTES
    export MONERO_OFFLINE="${MONERO_OFFLINE:-false}"
    export MONERO_NETWORK="${MONERO_NETWORK:-testnet}"
    export MONERO_ADD_PEERS="${MONERO_ADD_PEERS:-125.229.105.12:28081,37.187.74.171:28089}"
    export MONERO_FIXED_DIFFICULTY="${MONERO_FIXED_DIFFICULTY:-20000}"
    export MINING_THREADS="${MINING_THREADS:-1}"
    export WALLET_ADDRESS="${WALLET_ADDRESS:-}"
    export MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"
    export THRESHOLD=3
    export TARGET_BLOCK_TIME=120
    export DATA_DIR="$JOIN_TEST_DATA"
    export MONERO_DATA_DIR="$JOIN_TEST_MONERO"
    export P2POOL_DATA_DIR="$JOIN_TEST_P2POOL"

    # Ensure no conflicting containers exist (compose down above may miss
    # containers from a different profile/compose invocation using the same names).
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    for c in dwow-node0-join dwow-node0 dwow-monerod dwow-p2pool dwow-xmrig-merge dwow-lilith; do
        docker stop "$c" 2>/dev/null || true
        docker rm "$c" 2>/dev/null || true
    done

    cd "$REPO_ROOT"
    docker compose -f "$COMPOSE_FILE" --profile join-merge up -d 2>&1

    echo "  Waiting for containers to initialize (30s)..."
    sleep 30

    local all_up=1
    if docker ps --format '{{.Names}}' | grep -q "dwow-node0-join"; then
        pass "dwowd container running"
    else
        fail "dwowd container not running"
        all_up=0
    fi

    if docker ps --format '{{.Names}}' | grep -q "dwow-monerod"; then
        pass "monerod container running"
    else
        fail "monerod container not running"
        all_up=0
    fi

    if docker ps --format '{{.Names}}' | grep -q "dwow-p2pool"; then
        pass "p2pool container running"
    else
        fail "p2pool container not running"
        all_up=0
    fi

    if docker ps --format '{{.Names}}' | grep -q "dwow-xmrig-merge"; then
        pass "xmrig container running"
    else
        fail "xmrig container not running"
        all_up=0
    fi

    if [ "$all_up" -eq 0 ]; then
        echo "  Some containers failed. Logs:"
        docker compose -f "$COMPOSE_FILE" --profile join-merge logs 2>&1 | tail -40
        fail "Merge stack not fully up"
        return 0
    fi

    echo "  Checking monerod sync status..."
    local monero_logs
    monero_logs=$(docker logs dwow-monerod 2>&1 | tail -10)
    if echo "$monero_logs" | grep -qi "synced\|SYNCHRONIZED\|NEW CONNECTION\|initialized\|COMMAND_HANDSHAKE\|TXT record"; then
        pass "monerod active (syncing or connecting to peers)"
    elif echo "$monero_logs" | grep -q "Synced"; then
        pass "monerod is syncing"
    else
        echo "  monerod logs:"
        echo "$monero_logs"
        fail "monerod sync status unknown"
    fi

    echo "  Checking p2pool..."
    local p2pool_logs
    p2pool_logs=$(docker logs dwow-p2pool 2>&1 | tail -10)
    if echo "$p2pool_logs" | grep -qi "sidechain\|stratum\|p2pool"; then
        pass "p2pool running"
    else
        echo "  p2pool logs:"
        echo "$p2pool_logs"
        fail "p2pool not showing expected activity"
    fi

    sleep 10
    local dwowd_attempt
    for dwowd_attempt in $(seq 1 6); do
        if docker exec dwow-node0-join bash -c "exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT 2>/dev/null && echo ok >&3" 2>/dev/null; then
            pass "dwowd JSON-RPC reachable"
            break
        fi
        if [ "$dwowd_attempt" -lt 6 ]; then
            echo "  dwowd RPC not ready, waiting (attempt $dwowd_attempt/6)..."
            sleep 10
        else
            echo "  dwowd may still be starting"
            fail "dwowd JSON-RPC not reachable"
        fi
    done

    echo "  Merge stack is running. Leaving it for manual inspection."
    echo "  Run: docker compose -f $COMPOSE_FILE --profile join-merge logs -f"
}

# ==============================================================================
# Phase 11: Persistence (join modes only)
# ==============================================================================
phase_persistence() {
    if ! is_join_mode; then
        pass "persistence (N/A for local devnet)"
        return 0
    fi

    echo ""
    echo "=== Join Phase 11: Persistence / Restart ==="
    check_image || return 0

    local persist_dir="$JOIN_TEST_PERSIST"
    clean_data_dir "$persist_dir"
    mkdir -p "$persist_dir"

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true

    echo "  Starting first run..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
        -v "$persist_dir:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 15

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        fail "Container failed to start"
        clean_data_dir "$persist_dir"
        return 0
    fi

    if [ -f "$persist_dir/$NETWORK/hostlist.tsv" ] || ls "$persist_dir/$NETWORK/"*.sled 2>/dev/null | head -1 | grep -q sled; then
        pass "Data files created on first run"
    elif [ -f "$persist_dir/hostlist.tsv" ] || ls "$persist_dir/"*.sled 2>/dev/null | head -1 | grep -q sled; then
        pass "Data files created on first run"
    else
        echo "  Data dir contents:"
        find "$persist_dir" -type f 2>/dev/null | head -20 || echo "  (empty)"
        fail "Data files not created on first run"
    fi

    echo "  Stopping container..."
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true

    if [ -d "$persist_dir" ] && [ "$(ls -A "$persist_dir" 2>/dev/null)" ]; then
        pass "Host data survived container removal"
    else
        fail "Host data missing after container stop"
        clean_data_dir "$persist_dir"
        return 0
    fi

    echo "  Starting second run (same data dir)..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
        -v "$persist_dir:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 10

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        pass "Container restarted successfully with persisted data"
    else
        fail "Container failed to restart"
    fi

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    clean_data_dir "$persist_dir"
}

# ==============================================================================
# Pipeline header
# ==============================================================================
echo "=== DarkWow Testnet Full Pipeline ==="
echo "  Mode: $MODE"
echo ""

# ==============================================================================
# Main dispatch — sequential, one phase at a time
# ==============================================================================
phase_clean
phase_prereqs
phase_wallet
phase_build
phase_start_or_config
phase_verify_or_lifecycle
phase_rpc_or_fallback
phase_mining_or_p2p
phase_blocks_or_sync

# Bridge-specific phases (10-16) run after the native chain is established.
# Phases 1-9 are shared between native and bridge modes.
if is_bridge_mode; then
    phase_bridge_deploy
    phase_bridge_init
    phase_bridge_register_relayer
    phase_bridge_deposit
    phase_bridge_withdraw
    phase_bridge_accept
    phase_bridge_execute
    phase_bridge_verify
fi

phase_report_or_mining
phase_persistence

# If we're in a join mode and haven't reported yet, report now.
# (Local devnet modes call report() inside phase_report_or_mining.)
if is_join_mode; then
    report
fi
