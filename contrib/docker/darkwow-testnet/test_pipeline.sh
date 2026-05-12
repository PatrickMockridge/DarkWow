#!/bin/bash
# DarkWow Testnet Full Pipeline
#
# Builds, starts, and verifies the darkwow-testnet Docker stack.
# Supports both native mining (xmrig → dwowd stratum) and merge mining
# (xmrig → p2pool → monerod + dwowd mm_rpc).
#
# Usage:
#   ./test_pipeline.sh               # native mining (default)
#   ./test_pipeline.sh --mode native  # native mining
#   ./test_pipeline.sh --mode merge   # merge mining (Monero aux PoW)
#   ./test_pipeline.sh --mode native-p2pool  # DarkWow-primary pooled mining (adaptor)
#
# After this succeeds, run contract tests:
#   ./test-contracts.sh --mode native
#   ./test-contracts.sh --mode merge

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DWW_BIN="${REPO_ROOT}/target/release/dww"
DWW_DEBUG="${REPO_ROOT}/target/debug/dww"

# --- Parse flags ---
MODE="native"
while [ $# -gt 0 ]; do
    case "$1" in
        --mode) MODE="$2"; shift 2 ;;
        --mode=*) MODE="${1#*=}"; shift ;;
        *) echo "Unknown flag: $1"; echo "Usage: $0 [--mode native|merge|native-p2pool]"; exit 1 ;;
    esac
done

if [ "$MODE" != "native" ] && [ "$MODE" != "merge" ] && [ "$MODE" != "native-p2pool" ]; then
    echo "Invalid mode: $MODE (must be 'native', 'merge', or 'native-p2pool')"
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
RPC_URL="http://127.0.0.1:31345"

# WASM contract paths
WASM_MONEY_V3="${REPO_ROOT}/src/contract/money_v3/dwow_money_v3_contract.wasm"
WASM_DEX="${REPO_ROOT}/src/contract/dex/dwow_dex_contract.wasm"
WASM_DAO_ESCROW="${REPO_ROOT}/src/contract/dao_escrow/dwow_dao_escrow_contract.wasm"

# Monero wallet for p2pool parent chain rewards (merge mode only)
# In offline mode, p2pool doesn't need a wallet. Set this to a valid
# testnet address for live Monero testnet merge mining.
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

check() {
    if [ "$1" -eq 0 ]; then
        pass "$2"
    else
        fail "$2"
    fi
}

echo "=== DarkWow Testnet Full Pipeline ==="
echo "  Mode: $MODE mining"
echo ""

# ==============================================================================
# Phase 1: Clean
# ==============================================================================
info "[1/10] Cleaning previous deployment..."

cd "$SCRIPT_DIR"

# Tear down compose services (containers, networks, volumes)
docker compose down --rmi all -v 2>/dev/null || true
docker compose --profile merge down --rmi all -v 2>/dev/null || true
docker compose --profile native-p2pool down --rmi all -v 2>/dev/null || true

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

# Clear all build cache — ensures fresh git clones on next build
docker builder prune -a -f 2>/dev/null || true
pass "clean"

# ==============================================================================
# Phase 2: Validate prerequisites
# ==============================================================================
info "[2/10] Validating prerequisites..."

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

# Check dww
info "Using dww binary: $DWW"
"$DWW" --version 2>/dev/null || warn "dww --version failed (non-fatal)"

# Check WASM files
[ -f "$WASM_MONEY_V3" ] && pass "money_v3 WASM found" || fail "money_v3 WASM missing"
[ -f "$WASM_DEX" ] && pass "DEX WASM found" || warn "DEX WASM not found"
[ -f "$WASM_DAO_ESCROW" ] && pass "dao_escrow WASM found" || warn "dao_escrow WASM not found"

pass "all required files present"

# ==============================================================================
# Phase 3: Generate Wallet
# ==============================================================================
info "[3/10] Generating DarkWow wallet..."

# Initialize wallet directory
info "Initializing wallet..."
"$DWW" -n "$NETWORK" wallet initialize 2>&1 || warn "Wallet init warning (non-fatal)"

# Generate keypair
info "Generating keypair..."
KEYGEN_OUTPUT=$("$DWW" -n "$NETWORK" wallet keygen 2>&1)
echo "$KEYGEN_OUTPUT"

WALLET_SECRET=$(echo "$KEYGEN_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')

if [ -z "$WALLET_SECRET" ] || [ "${#WALLET_SECRET}" -ne 64 ]; then
    error "Failed to parse wallet secret from keygen output (got: ${WALLET_SECRET:-empty})"
fi

# Get the full Address (with network prefix + checksum) from the wallet.
# The raw public key from `wallet keygen` is structurally incompatible with
# Address::from_str (which expects base58check). `wallet address` produces
# the correct format: base58check([0xaf prefix][32B pubkey][4B blake3 checksum])
info "Fetching full wallet address..."
WALLET_ADDRESS=$("$DWW" -n "$NETWORK" wallet address 2>&1)

if [ -z "$WALLET_ADDRESS" ]; then
    error "Failed to get wallet address (run: dww -n $NETWORK wallet address)"
fi

pass "DarkWow keypair generated"
info "  Address: ${WALLET_ADDRESS:0:16}..."
info "  Secret (hex):  ${WALLET_SECRET:0:16}..."

if [ "$MODE" = "merge" ]; then
    if [ -n "$MONERO_WALLET_ADDRESS" ]; then
        info "  Monero wallet:  $MONERO_WALLET_ADDRESS"
    else
        info "  Monero wallet:  (none — offline mode, no wallet needed)"
    fi
fi

# Export for docker compose
export WALLET_ADDRESS
export WALLET_SECRET
export MONERO_WALLET_ADDRESS

# ==============================================================================
# Phase 4: Build
# ==============================================================================
info "[4/10] Building images..."

if [ "$MODE" = "merge" ]; then
    docker compose --profile merge build 2>&1 | tail -20
    check $? "docker build (merge profile)"
elif [ "$MODE" = "native-p2pool" ]; then
    docker compose --profile native-p2pool build 2>&1 | tail -20
    check $? "docker build (native-p2pool profile)"
else
    docker compose build 2>&1 | tail -20
    check $? "docker build"
fi

pass "build complete"

# ==============================================================================
# Phase 5: Start
# ==============================================================================
info "[5/10] Starting containers..."

if [ "$MODE" = "merge" ]; then
    MERGE_MINING=true WALLET_ADDRESS="$WALLET_ADDRESS" WALLET_SECRET="$WALLET_SECRET" \
        docker compose --profile merge up -d
elif [ "$MODE" = "native-p2pool" ]; then
    WALLET_ADDRESS="$WALLET_ADDRESS" WALLET_SECRET="$WALLET_SECRET" \
        docker compose --profile native-p2pool up -d
else
    WALLET_ADDRESS="$WALLET_ADDRESS" WALLET_SECRET="$WALLET_SECRET" \
        docker compose up -d
fi

sleep 5

# Check for immediate exits
if [ "$MODE" = "merge" ]; then
    EXITED=$(docker compose --profile merge ps 2>/dev/null | grep "Exit" || true)
elif [ "$MODE" = "native-p2pool" ]; then
    EXITED=$(docker compose --profile native-p2pool ps 2>/dev/null | grep "Exit" || true)
else
    EXITED=$(docker compose ps 2>/dev/null | grep "Exit" || true)
fi
if [ -n "$EXITED" ]; then
    echo "$EXITED"
    error "Container exited immediately — check logs"
fi

pass "containers started"

# ==============================================================================
# Phase 6: Verify containers
# ==============================================================================
info "[6/10] Verifying containers..."

if [ "$MODE" = "merge" ]; then
    EXPECTED=(dwow-lilith dwow-node0 dwow-node1 dwow-monerod dwow-p2pool dwow-xmrig-merge)
elif [ "$MODE" = "native-p2pool" ]; then
    EXPECTED=(dwow-lilith dwow-node0 dwow-node1 dwow-adaptor dwow-p2pool-darkwow dwow-xmrig-p2pool)
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

# ==============================================================================
# Phase 7: Verify RPC health
# ==============================================================================
info "[7/10] Verifying RPC health..."

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
        if docker exec dwow-adaptor bash -c 'exec 3<>/dev/tcp/127.0.0.1/28081; echo -e "POST /json_rpc HTTP/1.0\r\nContent-Type: application/json\r\nContent-Length: 64\r\n\r\n{\"jsonrpc\":\"2.0\",\"method\":\"get_info\",\"id\":1}" >&3; timeout 3 cat <&3 | grep -q "OK"' 2>/dev/null; then
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

# ==============================================================================
# Phase 8: Verify mining activity
# ==============================================================================
info "[8/10] Verifying mining activity..."

if [ "$MODE" = "merge" ]; then
    # p2pool connectivity — check it's running (not crash-looping) and
    # communicating with monerod and dwowd mm_rpc.
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

    # node0 merge mining logs
    info "Checking node0 for merge mining activity..."
    NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
    if echo "$NODE0_LOGS" | grep -qi "monero\|merge\|aux"; then
        pass "node0 merge mining activity"
    else
        warn "node0 logs don't show merge activity yet"
        fail "node0 merge mining activity"
    fi
elif [ "$MODE" = "native-p2pool" ]; then
    # Adaptor activity
    info "Checking adaptor activity..."
    ADAPTOR_LOGS=$(docker logs dwow-adaptor 2>&1 || true)
    if echo "$ADAPTOR_LOGS" | grep -qi "listening\|rpc\|connected"; then
        pass "adaptor active"
    else
        warn "adaptor logs don't show expected activity"
        docker logs dwow-adaptor 2>&1 | tail -20
        fail "adaptor active"
    fi

    # p2pool-darkwow connectivity
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

    # node0 should show block production
    info "Checking node0 for block production..."
    NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
    if echo "$NODE0_LOGS" | grep -qi "block\|mining\|stratum\|new job\|accepted"; then
        pass "node0 block production activity"
    else
        warn "node0 logs don't show clear mining activity"
        fail "node0 block production activity"
    fi
else
    # Native — check xmrig stratum
    info "Checking native mining activity (xmrig → stratum)..."
    NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
    if echo "$NODE0_LOGS" | grep -qi "new job\|accepted\|stratum"; then
        pass "native mining activity (xmrig → stratum)"
    else
        warn "node0 logs don't show clear mining activity"
        fail "native mining activity"
    fi
fi

# ==============================================================================
# Phase 9: Verify block production
# ==============================================================================
info "[9/10] Verifying block production..."

# Wait for genesis + first blocks
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
    [ $((i % 10)) -eq 0 ] && info "  waited ${i}s / ${WAIT_SECS}s..."
done

# Check initial block height (JSON-RPC over raw TCP)
BLOCK_INFO=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.last_confirmed_block\",\"params\":[],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1)
echo "$BLOCK_INFO"

BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | sed -n 's/.*"result":\[\([0-9]*\).*/\1/p')
info "Initial block height: $BLOCK_HEIGHT"

if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 1 ]; then
    pass "block height >= 1 (initialized)"
else
    fail "block height >= 1 (got: $BLOCK_HEIGHT)"
fi

# Wait for more blocks
info "Waiting for additional blocks (block time ~120s)..."
for i in $(seq 1 13); do
    sleep 10
    info "  waited $((i * 10))s / 130s..."
done

BLOCK_INFO=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.last_confirmed_block\",\"params\":[],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1)
BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | sed -n 's/.*"result":\[\([0-9]*\).*/\1/p')
info "Block height after waiting: $BLOCK_HEIGHT"

if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
    pass "$MODE blocks produced (height=$BLOCK_HEIGHT)"
else
    fail "$MODE blocks produced (height=$BLOCK_HEIGHT, expected >= 2)"
fi

# Mode-specific PoW verification in block data
if [ "$BLOCK_HEIGHT" -ge 1 ]; then
    info "Inspecting block 1 for PoW data..."
    BLOCK_DATA=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_block\",\"params\":[1],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1)

    if echo "$BLOCK_DATA" | grep -q '"result"'; then
        pass "block 1 fetched successfully"
    else
        fail "block 1 fetch"
    fi
fi

# ==============================================================================
# Phase 10: Report
# ==============================================================================
echo ""
info "[10/10] Pipeline Complete"
echo ""
echo "==========================================="
echo "  Mode: $MODE mining"
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
    else
        echo "  docker compose logs"
    fi
    exit 1
fi

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
echo ""
echo -e "${GREEN}Pipeline passed — ready for contract testing${NC}"
