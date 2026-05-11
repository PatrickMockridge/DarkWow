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
        *) echo "Unknown flag: $1"; echo "Usage: $0 [--mode native|merge]"; exit 1 ;;
    esac
done

if [ "$MODE" != "native" ] && [ "$MODE" != "merge" ]; then
    echo "Invalid mode: $MODE (must be 'native' or 'merge')"
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
MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-9y52SGYaGQAPFh4gFg2KBiq6Q2kHhvCD8A8VqnBVBSoed3i6jJe57L3osLpFtQxkXcRaPqWCMk3sxUMwvXmPLgRSLXCwYTM}"

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
docker compose down -v 2>/dev/null || true
docker compose --profile merge down -v 2>/dev/null || true

STALE=$(docker ps -a --format '{{.Names}}' 2>/dev/null | grep "^dwow-" || true)
if [ -n "$STALE" ]; then
    warn "Removing stale containers..."
    echo "$STALE" | xargs docker rm -f 2>/dev/null || true
fi

# Clear build cache to reclaim disk and ensure fresh git clones
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

WALLET_ADDRESS=$(echo "$KEYGEN_OUTPUT" | grep "Address (bs58):" | awk '{print $3}')
WALLET_SECRET=$(echo "$KEYGEN_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')

if [ -z "$WALLET_ADDRESS" ]; then
    error "Failed to parse wallet address from keygen output"
fi
if [ -z "$WALLET_SECRET" ] || [ "${#WALLET_SECRET}" -ne 64 ]; then
    error "Failed to parse wallet secret from keygen output (got: ${WALLET_SECRET:-empty})"
fi

pass "DarkWow keypair generated"
info "  Address (bs58): ${WALLET_ADDRESS:0:16}..."
info "  Secret (hex):   ${WALLET_SECRET:0:16}..."

if [ "$MODE" = "merge" ]; then
    info "  Monero wallet:  $MONERO_WALLET_ADDRESS"
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
else
    WALLET_ADDRESS="$WALLET_ADDRESS" WALLET_SECRET="$WALLET_SECRET" \
        docker compose up -d
fi

sleep 5

# Check for immediate exits
if [ "$MODE" = "merge" ]; then
    EXITED=$(docker compose --profile merge ps 2>/dev/null | grep "Exit" || true)
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

# node0 RPC
info "Waiting for node0 RPC (port 31345)..."
for i in $(seq 1 30); do
    if docker exec "$NODE0" curl -s --max-time 2 "$RPC_URL" >/dev/null 2>&1; then
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
    if docker exec dwow-node1 curl -s --max-time 2 http://127.0.0.1:31346 >/dev/null 2>&1; then
        info "node1 RPC is up (attempt $i)"
        break
    fi
    [ "$i" -eq 30 ] && error "Node1 RPC did not become healthy"
    sleep 2
done
pass "node1 RPC healthy"

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
    # p2pool connectivity
    info "Checking p2pool connectivity..."
    P2POOL_READY=false
    for i in $(seq 1 30); do
        P2POOL_LOGS=$(docker logs dwow-p2pool 2>&1 || true)
        if echo "$P2POOL_LOGS" | grep -qi "sidechain id\|new template\|merge mine\|merge_mine"; then
            info "p2pool connected to dwowd mm_rpc (attempt $i)"
            P2POOL_READY=true
            break
        fi
        sleep 3
    done
    if [ "$P2POOL_READY" = true ]; then
        pass "p2pool connected"
    else
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
    sleep 30
else
    info "Waiting for genesis + native-mined blocks..."
    sleep 15
fi

# Check initial block height
BLOCK_INFO=$(docker exec "$NODE0" curl -s --max-time 5 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"blockchain.info","params":[],"id":1}' \
    "$RPC_URL" 2>&1)
echo "$BLOCK_INFO"

BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '"block_height":[0-9]*' | head -1 | cut -d':' -f2)
info "Initial block height: $BLOCK_HEIGHT"

if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 1 ]; then
    pass "block height >= 1 (initialized)"
else
    fail "block height >= 1 (got: $BLOCK_HEIGHT)"
fi

# Wait for more blocks
info "Waiting for additional blocks (block time ~120s)..."
sleep 130

BLOCK_INFO=$(docker exec "$NODE0" curl -s --max-time 5 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"blockchain.info","params":[],"id":1}' \
    "$RPC_URL" 2>&1)
BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '"block_height":[0-9]*' | head -1 | cut -d':' -f2)
info "Block height after waiting: $BLOCK_HEIGHT"

if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
    pass "$MODE blocks produced (height=$BLOCK_HEIGHT)"
else
    fail "$MODE blocks produced (height=$BLOCK_HEIGHT, expected >= 2)"
fi

# Mode-specific PoW verification in block data
if [ "$BLOCK_HEIGHT" -ge 1 ]; then
    info "Inspecting block 1 for PoW data..."
    BLOCK_DATA=$(docker exec "$NODE0" curl -s --max-time 5 -X POST \
        -H 'Content-Type: application/json' \
        -d "{\"method\":\"blockchain.get_block\",\"params\":[1],\"id\":1}" \
        "$RPC_URL" 2>&1)

    if [ "$MODE" = "merge" ]; then
        if echo "$BLOCK_DATA" | grep -qi "monero\|aux_blob\|aux_hash\|mm_tag"; then
            pass "block contains Monero merge mining data"
        else
            warn "block 1 — no clear Monero fields (genesis may use native PoW)"
            fail "block contains Monero merge mining data"
        fi
    else
        if echo "$BLOCK_DATA" | grep -qi "darkfi\|pow_data\|powdata"; then
            pass "block contains native PoW data"
        else
            fail "block contains native PoW data"
        fi
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
else
    echo "  docker compose down -v"
fi
echo ""
echo -e "${GREEN}Pipeline passed — ready for contract testing${NC}"
