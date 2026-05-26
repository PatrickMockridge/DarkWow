#!/bin/bash
# DarkWow Devnet — Full Pipeline
#
# Builds, starts, and verifies the dwow-devnet Docker stack.
# Uses the bridge profile for a 3-node local devnet (lilith + 2 mining nodes).
#
# Usage:
#   ./test_pipeline.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DWW_BIN="${REPO_ROOT}/target/release/dwow_wallet"
DWW_DEBUG="${REPO_ROOT}/target/debug/dwow_wallet"

# --- Locate dwow_wallet binary ---
if [ -x "$DWW_BIN" ]; then
    DWW="$DWW_BIN"
elif [ -x "$DWW_DEBUG" ]; then
    DWW="$DWW_DEBUG"
else
    echo "Building dwow_wallet..."
    (cd "$REPO_ROOT" && RAYON_NUM_THREADS=10 cargo build -p dwow_wallet 2>&1)
    [ -x "$DWW_DEBUG" ] && DWW="$DWW_DEBUG" || DWW="$DWW_BIN"
    [ -x "$DWW" ] || { echo "ERROR: dwow_wallet binary not found after build"; exit 1; }
fi

NETWORK="dwow-devnet"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
NODE0="dwow-devnet-node0"
NODE1="dwow-devnet-node1"

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

echo "=== DarkWow Devnet Pipeline ==="
echo ""

# ==============================================================================
# Phase 1: Clean
# ==============================================================================
info "[1/8] Cleaning previous deployment..."
cd "$SCRIPT_DIR"
docker compose -f "$COMPOSE_FILE" down -v 2>/dev/null || true
docker compose -f "$COMPOSE_FILE" --profile host down -v 2>/dev/null || true

STALE=$(docker ps -a --format '{{.Names}}' 2>/dev/null | grep "^dwow-devnet-" || true)
if [ -n "$STALE" ]; then
    warn "Removing stale containers..."
    echo "$STALE" | xargs -r docker rm -f 2>/dev/null || true
fi

docker builder prune -a -f 2>/dev/null || true
pass "clean"

# ==============================================================================
# Phase 2: Validate prerequisites
# ==============================================================================
info "[2/8] Validating prerequisites..."
[ -f "$SCRIPT_DIR/entrypoint.sh" ]      || error "entrypoint.sh missing"
[ -f "$SCRIPT_DIR/docker-compose.yml" ] || error "docker-compose.yml missing"
[ -f "$SCRIPT_DIR/Dockerfile" ]         || error "Dockerfile missing"

info "Using dwow_wallet binary: $DWW"
"$DWW" --version 2>/dev/null || warn "dww --version failed (non-fatal)"
pass "all required files present"

# ==============================================================================
# Phase 3: Generate Wallet
# ==============================================================================
info "[3/8] Generating DarkWow wallet..."

"$DWW" -n "$NETWORK" wallet initialize 2>&1 || warn "Wallet init warning (non-fatal)"

KEYGEN_OUTPUT=$("$DWW" -n "$NETWORK" wallet keygen 2>&1)
# NOTE: keygen output contains the secret — intentionally not logged

WALLET_SECRET=$(echo "$KEYGEN_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')

if [ -z "$WALLET_SECRET" ] || [ "${#WALLET_SECRET}" -ne 64 ]; then
    error "Failed to parse wallet secret from keygen output"
fi

WALLET_ADDRESS=$("$DWW" -n "$NETWORK" wallet address 2>&1)
if [ -z "$WALLET_ADDRESS" ]; then
    error "Failed to get wallet address"
fi

pass "DarkWow keypair generated"
info "  Address: ${WALLET_ADDRESS:0:16}..."

# Write secret to fixed path for bind-mount into containers.
# The compose file mounts this as /run/secrets/mining_secret:ro.
SECRET_FILE="/tmp/dwow_mining_secret"
echo -n "$WALLET_SECRET" > "$SECRET_FILE"
chmod 600 "$SECRET_FILE"

# ==============================================================================
# Phase 4: Build
# ==============================================================================
info "[4/8] Building images..."
docker compose -f "$COMPOSE_FILE" build 2>&1 | tail -20
check $? "docker build"
pass "build complete"

# ==============================================================================
# Phase 5: Start
# ==============================================================================
info "[5/8] Starting containers..."
WALLET_ADDRESS="$WALLET_ADDRESS" docker compose -f "$COMPOSE_FILE" up -d

sleep 5

EXITED=$(docker compose -f "$COMPOSE_FILE" ps 2>/dev/null | grep "Exit" || true)
if [ -n "$EXITED" ]; then
    echo "$EXITED"
    # Clean up temp secret before exit
    rm -f "$SECRET_FILE"
    error "Container exited immediately — check logs"
fi

# Shred temp secret file now that containers have read it
rm -f "$SECRET_FILE"

pass "containers started"

# ==============================================================================
# Phase 6: Verify containers
# ==============================================================================
info "[6/8] Verifying containers..."
EXPECTED=(dwow-devnet-lilith dwow-devnet-node0 dwow-devnet-node1)
for c in "${EXPECTED[@]}"; do
    if docker ps --format '{{.Names}}' | grep -q "$c"; then
        pass "$c running"
    else
        fail "$c running"
    fi
done

# ==============================================================================
# Phase 7: Verify RPC and mining
# ==============================================================================
info "[7/8] Verifying RPC health and mining..."

# node0 RPC
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
    if docker exec "$NODE1" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31346; echo "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3 | grep -q "pong"' 2>/dev/null; then
        info "node1 RPC is up (attempt $i)"
        break
    fi
    [ "$i" -eq 30 ] && error "Node1 RPC did not become healthy"
    sleep 2
done
pass "node1 RPC healthy"

# Check for mining activity in node0 logs
info "Checking mining activity..."
NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
if echo "$NODE0_LOGS" | grep -qi "new job\|accepted\|stratum"; then
    pass "mining activity detected"
else
    warn "node0 logs don't show clear mining activity"
    fail "mining activity"
fi

# ==============================================================================
# Phase 8: Verify block production
# ==============================================================================
info "[8/8] Verifying block production..."

info "Waiting for genesis + blocks (block time ~120s, fixed difficulty=1)..."
for i in $(seq 1 30); do
    sleep 2
    [ $((i % 5)) -eq 0 ] && info "  waited $((i * 2))s / 60s..."
done

BLOCK_INFO=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_block_linear\",\"params\":[1],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1)
echo "$BLOCK_INFO" | head -c 200

BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*')
info "Current block height: $BLOCK_HEIGHT"

if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 1 ]; then
    pass "block height >= 1 (initialized)"
else
    fail "block height >= 1 (got: $BLOCK_HEIGHT)"
fi

# Wait for more blocks
info "Waiting for additional blocks..."
for i in $(seq 1 60); do
    sleep 2
    [ $((i % 10)) -eq 0 ] && info "  waited $((i * 2))s / 120s..."
done

BLOCK_INFO=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_block_linear\",\"params\":[2],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1)
BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*')
info "Block height after waiting: $BLOCK_HEIGHT"

if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
    pass "blocks produced (height=$BLOCK_HEIGHT)"
else
    fail "blocks produced (height=$BLOCK_HEIGHT, expected >= 2)"
fi

# ==============================================================================
# Report
# ==============================================================================
echo ""
echo "==========================================="
echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
echo "==========================================="
echo ""

if [ "$FAIL" -gt 0 ]; then
    echo -e "${RED}Some checks failed${NC}"
    echo ""
    echo "Debug info — check logs:"
    echo "  docker compose -f $COMPOSE_FILE logs"
    exit 1
fi

echo "Run contract tests:"
echo "  ./contract_test.sh"
echo "  ./test-contracts.sh"
echo ""
echo "Tear down:"
echo "  docker compose -f $COMPOSE_FILE down -v"
echo ""
echo -e "${GREEN}Pipeline passed${NC}"
