#!/bin/bash
# Multi-Contract Deployment & Transaction Test on DarkWow-Testnet
#
# Tests the full economic cycle with multiple contracts:
#   1. Wallet funded from mining rewards (coinbase via stratum/xmrig)
#   2. Deploy money_v3, DEX, dao_escrow contracts
#   3. DRKW transfer with fee payment
#   4. attach-fee command
#
# Prerequisites:
#   - test_pipeline.sh must have completed successfully (Docker stack running)
#   - Docker, dww + dwowd binaries built
#
# Usage:
#   ./test-contracts.sh               # defaults to native mode
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
WASM_MONEY_V3="${REPO_ROOT}/src/contract/money_v3/dwow_money_v3_contract.wasm"
WASM_DEX="${REPO_ROOT}/src/contract/dex/dwow_dex_contract.wasm"
WASM_DAO_ESCROW="${REPO_ROOT}/src/contract/dao_escrow/dwow_dao_escrow_contract.wasm"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; }

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

# ==============================================================================
# Setup
# ==============================================================================
echo "=== Multi-Contract Test on DarkWow-Testnet ==="
echo "  Mode: $MODE mining"
echo ""

info "Checking prerequisites..."

# Check Docker
docker ps >/dev/null 2>&1 || { error "Docker not running"; exit 1; }

# Verify testnet is running (pipeline should have started it)
if ! docker ps --format '{{.Names}}' | grep -q "$NODE0"; then
    error "Docker testnet not running. Run test_pipeline.sh first:"
    echo "  ./test_pipeline.sh --mode $MODE"
    exit 1
fi

# Verify node0 RPC
info "Checking node0 RPC health..."
for i in $(seq 1 30); do
    if docker exec "$NODE0" curl -s --max-time 2 "$RPC_URL" >/dev/null 2>&1; then
        info "Node0 RPC is up (attempt $i)"
        break
    fi
    [ "$i" -eq 30 ] && { error "Node0 RPC did not become healthy"; exit 1; }
    sleep 2
done

# ==============================================================================
# Phase 1: Fund wallet from mining
# ==============================================================================
echo ""
info "=== Phase 1: Funding wallet from mining ==="

SECRET_HEX=$(docker exec "$NODE0" cat /root/.local/share/dwow/dwowd/${NETWORK}/mining_secret 2>/dev/null)
[ -n "$SECRET_HEX" ] || { error "Failed to extract mining secret from node0"; exit 1; }
info "Mining secret: ${SECRET_HEX:0:16}..."

info "Initializing wallet..."
"$DWW" -n "$NETWORK" wallet initialize 2>&1 || warn "Wallet already initialized"

info "Importing mining secret..."
"$DWW" -n "$NETWORK" wallet import-secret-hex "$SECRET_HEX" 2>&1
check $? "import mining secret"

info "Scanning blockchain for coins..."
"$DWW" -n "$NETWORK" scan 2>&1 | tail -5
check $? "scan blockchain"

info "Wallet balance:"
BALANCE=$("$DWW" -n "$NETWORK" wallet balance 2>&1)
echo "$BALANCE"
echo "$BALANCE" | grep -q "DRKW\|drkw" && pass "wallet has DRKW coins" || fail "wallet has DRKW coins"

# ==============================================================================
# Phase 2: Deploy money_v3 contract
# ==============================================================================
echo ""
info "=== Phase 2: Deploy money_v3 ==="

[ -f "$WASM_MONEY_V3" ] || { error "money_v3 WASM not found at $WASM_MONEY_V3"; exit 1; }

DEPLOY_OUTPUT=$("$DWW" -n "$NETWORK" contract generate-deploy 2>&1)
echo "$DEPLOY_OUTPUT"
MONEY_V3_SECRET=$(echo "$DEPLOY_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')
MONEY_V3_CID=$(echo "$DEPLOY_OUTPUT" | grep "Contract ID:" | awk '{print $3}')
[ -n "$MONEY_V3_SECRET" ] && [ -n "$MONEY_V3_CID" ]
check $? "generate deploy authority for money_v3"

DEPLOY_TX=$("$DWW" -n "$NETWORK" contract deploy "$MONEY_V3_SECRET" "$WASM_MONEY_V3" 2>&1)
echo "$DEPLOY_TX" | "$DWW" -n "$NETWORK" broadcast 2>&1
check $? "deploy money_v3"

sleep 3

"$DWW" -n "$NETWORK" contract register money_v3 "$MONEY_V3_CID" 2>&1
check $? "register money_v3 contract ID"

# ==============================================================================
# Phase 3: Deploy DEX contract
# ==============================================================================
echo ""
info "=== Phase 3: Deploy DEX ==="

if [ -f "$WASM_DEX" ]; then
    DEPLOY_OUTPUT=$("$DWW" -n "$NETWORK" contract generate-deploy 2>&1)
    DEX_SECRET=$(echo "$DEPLOY_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')
    DEX_CID=$(echo "$DEPLOY_OUTPUT" | grep "Contract ID:" | awk '{print $3}')
    [ -n "$DEX_SECRET" ] && [ -n "$DEX_CID" ]
    check $? "generate deploy authority for DEX"

    DEPLOY_TX=$("$DWW" -n "$NETWORK" contract deploy "$DEX_SECRET" "$WASM_DEX" 2>&1)
    echo "$DEPLOY_TX" | "$DWW" -n "$NETWORK" broadcast 2>&1
    check $? "deploy DEX"

    sleep 3

    "$DWW" -n "$NETWORK" contract register dex "$DEX_CID" 2>&1
    check $? "register DEX contract ID"
else
    warn "DEX WASM not found at $WASM_DEX — skipping"
fi

# ==============================================================================
# Phase 4: Deploy dao_escrow contract
# ==============================================================================
echo ""
info "=== Phase 4: Deploy dao_escrow ==="

if [ -f "$WASM_DAO_ESCROW" ]; then
    DEPLOY_OUTPUT=$("$DWW" -n "$NETWORK" contract generate-deploy 2>&1)
    DAO_SECRET=$(echo "$DEPLOY_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')
    DAO_CID=$(echo "$DEPLOY_OUTPUT" | grep "Contract ID:" | awk '{print $3}')
    [ -n "$DAO_SECRET" ] && [ -n "$DAO_CID" ]
    check $? "generate deploy authority for dao_escrow"

    DEPLOY_TX=$("$DWW" -n "$NETWORK" contract deploy "$DAO_SECRET" "$WASM_DAO_ESCROW" 2>&1)
    echo "$DEPLOY_TX" | "$DWW" -n "$NETWORK" broadcast 2>&1
    check $? "deploy dao_escrow"

    sleep 3

    "$DWW" -n "$NETWORK" contract register dao_escrow "$DAO_CID" 2>&1
    check $? "register dao_escrow contract ID"
else
    warn "dao_escrow WASM not found at $WASM_DAO_ESCROW — skipping"
fi

# ==============================================================================
# Phase 5: Transfer with fee
# ==============================================================================
echo ""
info "=== Phase 5: DRKW transfer with fee ==="

RECIPIENT=$("$DWW" -n "$NETWORK" wallet address 2>&1)
info "Recipient: $RECIPIENT"

TRANSFER_TX=$("$DWW" -n "$NETWORK" transfer "100000000" DRKW "$RECIPIENT" 2>&1)
echo "$TRANSFER_TX" | "$DWW" -n "$NETWORK" broadcast 2>&1
check $? "DRKW transfer broadcast"

sleep 3

# ==============================================================================
# Phase 6: attach-fee command
# ==============================================================================
echo ""
info "=== Phase 6: attach-fee command ==="

info "Building transaction with attach-fee..."
TRANSFER_TX=$("$DWW" -n "$NETWORK" transfer "50000000" DRKW "$RECIPIENT" 2>&1)
ATTACHED_TX=$(echo "$TRANSFER_TX" | "$DWW" -n "$NETWORK" attach-fee 2>&1)
check $? "attach-fee command"

[ -n "$ATTACHED_TX" ] && pass "attach-fee produced output" || fail "attach-fee produced output"

# ==============================================================================
# Phase 7: Verify
# ==============================================================================
echo ""
info "=== Phase 7: Verification ==="

sleep 5
"$DWW" -n "$NETWORK" scan 2>&1 | tail -5

info "Final wallet balance:"
"$DWW" -n "$NETWORK" wallet balance 2>&1

info "Wallet coins:"
"$DWW" -n "$NETWORK" wallet coins 2>&1 | head -10

info "Registered contracts:"
"$DWW" -n "$NETWORK" contract list 2>&1 || warn "contract list may not be implemented"

# Check final block height
BLOCK_INFO=$(docker exec "$NODE0" curl -s --max-time 5 -X POST \
    -H 'Content-Type: application/json' \
    -d '{"method":"blockchain.info","params":[],"id":1}' \
    "$RPC_URL" 2>&1)
BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '"block_height":[0-9]*' | head -1 | cut -d':' -f2)
info "Final block height: $BLOCK_HEIGHT"

# ==============================================================================
# Results
# ==============================================================================
echo ""
echo "==========================================="
echo "  Mode: $MODE mining"
echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
echo "==========================================="

if [ "$FAIL" -gt 0 ]; then
    echo -e "${RED}Some tests failed${NC}"
    exit 1
else
    echo -e "${GREEN}All contract tests passed${NC}"
fi
