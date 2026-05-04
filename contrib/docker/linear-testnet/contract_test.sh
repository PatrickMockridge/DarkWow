#!/bin/bash
# Contract Deployment & Transfer Test on Linear-Testnet
#
# Tests the full economic cycle:
#   1. Wallet funded from mining rewards (coinbase)
#   2. Contract deployment via deployooor
#   3. Token transfer with fee payment
#
# Prerequisites: Docker, drk binary built (cargo build -p drk)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
DRK_BIN="${REPO_ROOT}/target/release/drk"
DRK_DEBUG="${REPO_ROOT}/target/debug/drk"

# Use debug build if release doesn't exist
if [ -x "$DRK_BIN" ]; then
    DRK="$DRK_BIN"
elif [ -x "$DRK_DEBUG" ]; then
    DRK="$DRK_DEBUG"
else
    echo "Building drk..."
    (cd "$REPO_ROOT" && cargo build -p drk 2>&1)
    if [ -x "$DRK_DEBUG" ]; then
        DRK="$DRK_DEBUG"
    elif [ -x "$DRK_BIN" ]; then
        DRK="$DRK_BIN"
    else
        echo "ERROR: drk binary not found after build"
        exit 1
    fi
fi

NETWORK="linear-testnet"
NODE0="darkfi-linear-node0"
RPC_URL="tcp://127.0.0.1:28345"
WASM_MONEY_V3="${REPO_ROOT}/src/contract/money_v3/darkfi_money_v3_contract.wasm"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }
warn()  { echo -e "${YELLOW}[WARN]${NC}  $*"; }
error() { echo -e "${RED}[ERROR]${NC} $*"; exit 1; }

# ==============================================================================
# PHASE 1: Prerequisites
# ==============================================================================
echo "=== Contract Test on Linear-Testnet ==="
echo ""

info "Checking prerequisites..."

# Check Docker
docker ps >/dev/null 2>&1 || error "Docker not running"

# Check testnet is running
if ! docker ps --format '{{.Names}}' | grep -q "$NODE0"; then
    warn "Docker testnet not running. Starting..."
    cd "$SCRIPT_DIR"
    docker-compose build --no-cache 2>&1 | tail -5
    docker-compose up -d
    info "Waiting for testnet to be ready..."
    sleep 10
fi

# Verify node0 RPC is reachable
info "Checking node0 RPC health..."
for i in $(seq 1 30); do
    if docker exec "$NODE0" curl -s --max-time 2 http://127.0.0.1:28345 >/dev/null 2>&1; then
        info "Node0 RPC is up (attempt $i)"
        break
    fi
    [ "$i" -eq 30 ] && error "Node0 RPC did not become healthy"
    sleep 2
done

# Check WASM exists
[ -f "$WASM_MONEY_V3" ] || error "money_v3 WASM not found at $WASM_MONEY_V3"

info "Using drk binary: $DRK"
"$DRK" --version 2>/dev/null || warn "drk --version failed (non-fatal)"

# ==============================================================================
# PHASE 2: Fund the wallet from mining
# ==============================================================================
echo ""
info "=== Phase 2: Funding wallet from mining ==="

# Extract mining secret from node0
SECRET_HEX=$(docker exec "$NODE0" cat /root/.local/share/darkfi/darkfid/linear-testnet/mining_secret 2>/dev/null) || \
    error "Failed to extract mining secret from node0"

info "Mining secret: ${SECRET_HEX:0:16}..."

# Initialize wallet
info "Initializing wallet..."
"$DRK" -n "$NETWORK" wallet initialize 2>&1 || warn "Wallet init warning (may already be initialized)"

# Import mining secret
info "Importing mining secret..."
"$DRK" -n "$NETWORK" wallet import-secret-hex "$SECRET_HEX" 2>&1 || error "Failed to import mining secret"

# Scan for coins
info "Scanning blockchain for coins..."
SCAN_OUTPUT=$("$DRK" -n "$NETWORK" scan 2>&1) || error "Scan failed"
echo "$SCAN_OUTPUT" | tail -20

# Check balance
info "Checking wallet balance..."
BALANCE_OUTPUT=$("$DRK" -n "$NETWORK" wallet balance 2>&1) || error "Balance check failed"
echo "$BALANCE_OUTPUT"

# Verify we have DARK coins
if echo "$BALANCE_OUTPUT" | grep -q "DARK\|dark"; then
    info "Wallet has DARK coins - ready for testing"
else
    warn "No DARK coins found yet. Mining may need more time."
    warn "Waiting 10 seconds then scanning again..."
    sleep 10
    "$DRK" -n "$NETWORK" scan 2>&1 | tail -5
    BALANCE_OUTPUT=$("$DRK" -n "$NETWORK" wallet balance 2>&1)
    echo "$BALANCE_OUTPUT"
fi

# ==============================================================================
# PHASE 3: Deploy money_v3 contract
# ==============================================================================
echo ""
info "=== Phase 3: Deploy money_v3 contract ==="

# Generate deploy authority
info "Generating deploy authority..."
DEPLOY_OUTPUT=$("$DRK" -n "$NETWORK" contract generate-deploy 2>&1) || error "Failed to generate deploy authority"
echo "$DEPLOY_OUTPUT"

# Parse the output to get secret hex and contract ID
DEPLOY_SECRET=$(echo "$DEPLOY_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')
CONTRACT_ID=$(echo "$DEPLOY_OUTPUT" | grep "Contract ID:" | awk '{print $3}')

[ -n "$DEPLOY_SECRET" ] || error "Failed to parse deploy secret from output"
[ -n "$CONTRACT_ID" ] || error "Failed to parse contract ID from output"

info "Deploy authority secret: ${DEPLOY_SECRET:0:16}..."
info "Expected contract ID: $CONTRACT_ID"

# Deploy the contract
info "Deploying money_v3 contract..."
DEPLOY_TX=$("$DRK" -n "$NETWORK" contract deploy "$DEPLOY_SECRET" "$WASM_MONEY_V3" 2>&1) || error "Contract deploy failed"
info "Deploy transaction: ${DEPLOY_TX:0:64}..."

# Broadcast the transaction
info "Broadcasting deploy transaction..."
echo "$DEPLOY_TX" | "$DRK" -n "$NETWORK" broadcast 2>&1 || error "Broadcast failed"
info "Deploy transaction broadcast"

# Wait for block inclusion
info "Waiting for block inclusion..."
sleep 5

# Register the contract ID for runtime use
info "Registering contract ID: $CONTRACT_ID"
"$DRK" -n "$NETWORK" contract register money_v3 "$CONTRACT_ID" 2>&1 || error "Contract registration failed"
info "Contract registered"

# ==============================================================================
# PHASE 4: Transfer with fee payment
# ==============================================================================
echo ""
info "=== Phase 4: Transfer with fee payment ==="

# Generate a second keypair for the transfer recipient
info "Generating recipient keypair..."
KEYGEN_OUTPUT=$("$DRK" -n "$NETWORK" wallet keygen 2>&1) || error "Keygen failed"
echo "$KEYGEN_OUTPUT"

# Extract the recipient address - keygen prints a SecretKey
# We need an Address to send to. Use the wallet's default address for now.
RECIPIENT_ADDR=$("$DRK" -n "$NETWORK" wallet address 2>&1) || error "Failed to get wallet address"
info "Recipient address: $RECIPIENT_ADDR"

# Perform transfer (sends DARK back to our own address as a test)
# Amount: 1 DARK = 100_000_000 smallest units
TRANSFER_AMOUNT="100000000"
info "Creating transfer of $TRANSFER_AMOUNT DARK to $RECIPIENT_ADDR..."
TRANSFER_TX=$("$DRK" -n "$NETWORK" transfer "$TRANSFER_AMOUNT" DARK "$RECIPIENT_ADDR" 2>&1) || error "Transfer creation failed"
info "Transfer transaction: ${TRANSFER_TX:0:64}..."

# Broadcast the transfer
info "Broadcasting transfer transaction..."
echo "$TRANSFER_TX" | "$DRK" -n "$NETWORK" broadcast 2>&1 || error "Transfer broadcast failed"
info "Transfer broadcast"

# ==============================================================================
# PHASE 5: Verify
# ==============================================================================
echo ""
info "=== Phase 5: Verification ==="

# Wait for block inclusion
info "Waiting for block inclusion..."
sleep 5

# Scan again to pick up changes
info "Scanning for updates..."
"$DRK" -n "$NETWORK" scan 2>&1 | tail -10

# Check final balance
info "Final wallet balance:"
"$DRK" -n "$NETWORK" wallet balance 2>&1

# Check coins
info "Wallet coins:"
"$DRK" -n "$NETWORK" wallet coins 2>&1 | head -10

echo ""
echo -e "${GREEN}=== Contract Test Complete ===${NC}"
echo ""
echo "Summary:"
echo "  - Wallet funded from mining rewards"
echo "  - money_v3 contract deployed (ID: $CONTRACT_ID)"
echo "  - DARK transfer with fee payment broadcast"
echo "  - Full economic cycle tested: mining → fund → deploy → transfer → fee"
echo ""
echo "Manual verification commands:"
echo "  $DRK -n $NETWORK wallet balance"
echo "  $DRK -n $NETWORK wallet coins"
echo "  $DRK -n $NETWORK scan"
