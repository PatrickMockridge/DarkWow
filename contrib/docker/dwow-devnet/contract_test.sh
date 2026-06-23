#!/bin/bash
# Contract Deployment & Transfer Test on Dwow-Devnet
#
# Tests the full economic cycle:
#   1. Wallet funded from mining rewards (coinbase)
#   2. Contract deployment via deployooor
#   3. Token transfer with fee payment
#
# Prerequisites: test_pipeline.sh must have completed successfully (Docker
# stack running, wallet initialized with mining keys).
#
# Usage:
#   ./contract_test.sh

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

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

# ==============================================================================
# Prerequisites
# ==============================================================================
echo "=== Contract Test on Dwow-Devnet ==="
echo ""

info "Checking prerequisites..."

docker ps >/dev/null 2>&1 || error "Docker not running"

if ! docker ps --format '{{.Names}}' | grep -q "$NODE0"; then
    error "Docker devnet not running. Run test_pipeline.sh first."
fi

# Verify node0 RPC is reachable
info "Checking node0 RPC health..."
for i in $(seq 1 30); do
    if docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":[],\"id\":1}" >&3; timeout 3 cat <&3 | grep -q "pong"' 2>/dev/null; then
        info "Node0 RPC is up (attempt $i)"
        break
    fi
    [ "$i" -eq 30 ] && error "Node0 RPC did not become healthy"
    sleep 2
done

[ -f "$WASM_PROMISSORY_NOTE" ] || error "promissory_note WASM not found at $WASM_PROMISSORY_NOTE"

info "Using dwow_wallet binary: $DWW"
"$DWW" --version 2>/dev/null || warn "dww --version failed (non-fatal)"

# ==============================================================================
# Phase 1: Fund wallet from mining
# ==============================================================================
echo ""
info "=== Phase 1: Funding wallet from mining ==="

# Wallet should already be initialized by test_pipeline.sh with the mining key.
# The wallet holds the secret key locally — scan decrypts coinbase notes
# client-side via AEAD. No secret extraction from the node needed.
info "Checking wallet status..."
if ! "$DWW" -n "$NETWORK" wallet address >/dev/null 2>&1; then
    error "Wallet has no keys. Run test_pipeline.sh first to generate a keypair."
fi

info "Scanning blockchain for coins..."
SCAN_OUTPUT=$("$DWW" -n "$NETWORK" scan 2>&1) || error "Scan failed"
echo "$SCAN_OUTPUT" | tail -20

info "Checking wallet balance..."
BALANCE_OUTPUT=$("$DWW" -n "$NETWORK" wallet balance 2>&1) || error "Balance check failed"
echo "$BALANCE_OUTPUT"

if echo "$BALANCE_OUTPUT" | grep -q "DRKW\|drkw"; then
    info "Wallet has DRKW coins — ready for testing"
else
    warn "No DRKW coins found yet. Mining may need more time."
    warn "Waiting 10 seconds then scanning again..."
    sleep 10
    "$DWW" -n "$NETWORK" scan 2>&1 | tail -5
    BALANCE_OUTPUT=$("$DWW" -n "$NETWORK" wallet balance 2>&1)
    echo "$BALANCE_OUTPUT"
fi

# ==============================================================================
# Phase 2: Deploy promissory_note contract
# ==============================================================================
echo ""
info "=== Phase 2: Deploy promissory_note contract ==="

info "Generating deploy authority..."
DEPLOY_OUTPUT=$("$DWW" -n "$NETWORK" contract generate-deploy 2>&1) || error "Failed to generate deploy authority"
echo "$DEPLOY_OUTPUT"

DEPLOY_SECRET=$(echo "$DEPLOY_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')
CONTRACT_ID=$(echo "$DEPLOY_OUTPUT" | grep "Contract ID:" | awk '{print $3}')

[ -n "$DEPLOY_SECRET" ] || error "Failed to parse deploy secret"
[ -n "$CONTRACT_ID" ] || error "Failed to parse contract ID"

info "Expected contract ID: $CONTRACT_ID"

info "Deploying promissory_note contract..."
DEPLOY_TX=$("$DWW" -n "$NETWORK" contract deploy "$DEPLOY_SECRET" "$WASM_PROMISSORY_NOTE" 2>&1) || error "Contract deploy failed"
info "Deploy transaction: ${DEPLOY_TX:0:64}..."

info "Broadcasting deploy transaction..."
echo "$DEPLOY_TX" | "$DWW" -n "$NETWORK" broadcast 2>&1 || error "Broadcast failed"

info "Waiting for block inclusion..."
sleep 5

info "Registering contract ID: $CONTRACT_ID"
"$DWW" -n "$NETWORK" contract register promissory_note "$CONTRACT_ID" 2>&1 || error "Contract registration failed"

# ==============================================================================
# Phase 3: Transfer with fee payment
# ==============================================================================
echo ""
info "=== Phase 3: Transfer with fee payment ==="

info "Generating recipient keypair..."
"$DWW" -n "$NETWORK" wallet keygen 2>&1 >/dev/null || error "Keygen failed"

RECIPIENT_ADDR=$("$DWW" -n "$NETWORK" wallet address 2>&1) || error "Failed to get wallet address"
info "Recipient address: ${RECIPIENT_ADDR:0:16}..."

TRANSFER_AMOUNT="100000000"
info "Creating transfer of $TRANSFER_AMOUNT base units..."
TRANSFER_TX=$("$DWW" -n "$NETWORK" transfer "$TRANSFER_AMOUNT" DRKW "$RECIPIENT_ADDR" 2>&1) || error "Transfer creation failed"
info "Transfer transaction: ${TRANSFER_TX:0:64}..."

info "Broadcasting transfer transaction..."
echo "$TRANSFER_TX" | "$DWW" -n "$NETWORK" broadcast 2>&1 || error "Transfer broadcast failed"

# ==============================================================================
# Phase 4: Verify
# ==============================================================================
echo ""
info "=== Phase 4: Verification ==="

info "Waiting for block inclusion..."
sleep 5

info "Scanning for updates..."
"$DWW" -n "$NETWORK" scan 2>&1 | tail -10

info "Final wallet balance:"
"$DWW" -n "$NETWORK" wallet balance 2>&1

info "Wallet coins:"
"$DWW" -n "$NETWORK" wallet capabilities 2>&1 | head -10

echo ""
echo -e "${GREEN}=== Contract Test Complete ===${NC}"
echo ""
echo "Summary:"
echo "  - Wallet funded from mining rewards (no secret extraction)"
echo "  - promissory_note contract deployed (ID: $CONTRACT_ID)"
echo "  - DRKW transfer with fee payment broadcast"
echo "  - Full economic cycle tested: mine → fund → deploy → transfer → fee"
echo ""
echo "Manual verification:"
echo "  $DWW -n $NETWORK wallet balance"
echo "  $DWW -n $NETWORK wallet capabilities"
echo "  $DWW -n $NETWORK scan"
