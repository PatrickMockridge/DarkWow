#!/bin/bash
# Multi-Contract Deployment & Transaction Test on Dwow-Devnet
#
# Tests the full economic cycle with multiple contracts:
#   1. Wallet funded from mining rewards (coinbase via stratum/xmrig)
#   2. Deploy promissory_note, DEX, dao_escrow contracts
#   3. DRKW transfer with fee payment
#   4. attach-fee command
#
# Prerequisites:
#   - test_pipeline.sh must have completed successfully (Docker stack
#     running, wallet initialized with mining keys)
#   - Docker, dwow_wallet + dwowd binaries built
#
# Usage:
#   ./test-contracts.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DWW_BIN="${REPO_ROOT}/target/release/dwow_wallet"
DWW_DEBUG="${REPO_ROOT}/target/debug/dwow_wallet"

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
NODE0="dwow-devnet-node0"
WASM_PROMISSORY_NOTE="${REPO_ROOT}/src/contract/promissory_note/dwow_promissory_note_contract.wasm"
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
echo "=== Multi-Contract Test on Dwow-Devnet ==="
echo ""

info "Checking prerequisites..."

docker ps >/dev/null 2>&1 || { error "Docker not running"; exit 1; }

if ! docker ps --format '{{.Names}}' | grep -q "$NODE0"; then
    error "Docker devnet not running. Run test_pipeline.sh first."
fi

# Verify node0 RPC
info "Checking node0 RPC health..."
for i in $(seq 1 30); do
    if docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3 | grep -q "pong"' 2>/dev/null; then
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

# Wallet should already be initialized by test_pipeline.sh with the mining key.
if ! "$DWW" -n "$NETWORK" wallet address >/dev/null 2>&1; then
    error "Wallet has no keys. Run test_pipeline.sh first to generate a keypair."
fi

info "Scanning blockchain for coins..."
"$DWW" -n "$NETWORK" scan 2>&1 | tail -5
check $? "scan blockchain"

info "Wallet balance:"
BALANCE=$("$DWW" -n "$NETWORK" wallet balance 2>&1)
echo "$BALANCE"
echo "$BALANCE" | grep -q "DRKW\|drkw" && pass "wallet has DRKW coins" || fail "wallet has DRKW coins"

# ==============================================================================
# Phase 2: Deploy promissory_note contract
# ==============================================================================
echo ""
info "=== Phase 2: Deploy promissory_note ==="

[ -f "$WASM_PROMISSORY_NOTE" ] || { error "promissory_note WASM not found at $WASM_PROMISSORY_NOTE"; exit 1; }

DEPLOY_OUTPUT=$("$DWW" -n "$NETWORK" contract generate-deploy 2>&1)
echo "$DEPLOY_OUTPUT"
PROMISSORY_NOTE_SECRET=$(echo "$DEPLOY_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')
PROMISSORY_NOTE_CID=$(echo "$DEPLOY_OUTPUT" | grep "Contract ID:" | awk '{print $3}')
[ -n "$PROMISSORY_NOTE_SECRET" ] && [ -n "$PROMISSORY_NOTE_CID" ]
check $? "generate deploy authority for promissory_note"

DEPLOY_TX=$("$DWW" -n "$NETWORK" contract deploy "$PROMISSORY_NOTE_SECRET" "$WASM_PROMISSORY_NOTE" 2>&1)
echo "$DEPLOY_TX" | "$DWW" -n "$NETWORK" broadcast 2>&1
check $? "deploy promissory_note"

sleep 3

"$DWW" -n "$NETWORK" contract register promissory_note "$PROMISSORY_NOTE_CID" 2>&1
check $? "register promissory_note contract ID"

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
info "Recipient: ${RECIPIENT:0:16}..."

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

# ==============================================================================
# Results
# ==============================================================================
echo ""
echo "==========================================="
echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
echo "==========================================="

if [ "$FAIL" -gt 0 ]; then
    echo -e "${RED}Some tests failed${NC}"
    exit 1
else
    echo -e "${GREEN}All contract tests passed${NC}"
fi
