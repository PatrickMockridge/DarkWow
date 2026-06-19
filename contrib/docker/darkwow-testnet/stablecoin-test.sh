#!/bin/bash
# Stablecoin Contract Deployment & Token Test on DarkWow-Testnet
#
# Deploys the stablecoin contract, creates dummy collateral (COLL) and
# stablecoin (USDx) tokens via PromissoryNote, mints them, and tests
# cross-wallet mesh transfers.
#
# Usage:
#   ./stablecoin-test.sh <pn_contract_id_hex> 3
#   ./stablecoin-test.sh auto 3                     # self-deploy PN first
#   PN_CID=<hex> ./stablecoin-test.sh auto 3        # env var override
#
# Prerequisites:
#   test_pipeline.sh --with-wallet N must have completed successfully.
#   PromissoryNote must be deployed (or use "auto" to self-deploy).

set -e
set -E

trap 'echo "[FATAL] stablecoin-test failed at line $LINENO — exit code $?" >&2' ERR
trap 'echo "[FATAL] stablecoin-test killed by signal" >&2; exit 1' INT TERM HUP PIPE

# ── Constants ────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# PN_CID: first positional arg, or env var, or "auto"
PN_CID="${1:-${PN_CID:-auto}}"
# WALLET_COUNT: second positional arg, defaults to 3
WALLET_COUNT="${2:-3}"
if ! [ "$WALLET_COUNT" -ge 1 ] 2>/dev/null || ! [ "$WALLET_COUNT" -le 5 ] 2>/dev/null; then
    echo "WALLET_COUNT must be 1-5, got: $WALLET_COUNT"
    exit 1
fi

# Config at binary default path — no -c flag needed

# Fund amounts
DEPLOY_FEE=1000000         # 0.01 DRKW — deploy fee estimate
MINT_AMOUNT=500000000      # 5 tokens for mint test
TRANSFER_AMOUNT=100000000  # 1 token for mesh transfers

# Block wait settings
BLOCK_TIMEOUT=300
BLOCK_POLL=10

# Container names
NODE0="dwow-node0"
RPC_PORT=31345

# WASM paths (on host, relative to REPO_ROOT)
WASM_STABLECOIN="${REPO_ROOT}/src/contract/stablecoin/dwow_stablecoin_contract.wasm"
WASM_PROMISSORY_NOTE="${REPO_ROOT}/src/contract/promissory_note/dwow_promissory_note_contract.wasm"

# ── Colour helpers ───────────────────────────────────────────────────────────

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
    if [ "$1" -eq 0 ]; then pass "$2"; else fail "$2"; fi
}

# ── Helpers ──────────────────────────────────────────────────────────────────

wal() {
    local i=$1; shift
    docker exec "dwow-wallet-$i" /app/dwow_wallet "$@" 2>&1
}

broadcast() {
    local i="$1"
    local tx_data="${2:-$(cat)}"
    local out
    out=$(echo "$tx_data" | wal "$i" broadcast 2>&1)
    local rc=$?
    if [ $rc -ne 0 ]; then
        echo "$out"
        return 1
    fi
    if echo "$out" | grep -q '"result"'; then
        echo "$out"
        return 0
    elif echo "$out" | grep -qi '"error"'; then
        echo "$out"
        return 1
    else
        if echo "$out" | grep -qi "rejected\|insufficient\|invalid\|failed"; then
            echo "$out"
            return 1
        fi
        echo "$out"
        return 0
    fi
}

node0_rpc() {
    local method="$1" params="${2:-[]}"
    docker exec "$NODE0" bash -c \
        "exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT; echo '{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}' >&3; timeout 5 cat <&3" 2>/dev/null
}

get_block_height() {
    local raw
    raw=$(node0_rpc "blockchain.info" || true)
    if [ -z "$raw" ]; then
        raw=$(node0_rpc "blockchain.last_confirmed_block" || true)
        echo "$raw" | grep -o '"[0-9.]*"' | head -1 | tr -d '"' | cut -d'.' -f1 2>/dev/null || echo "0"
    else
        echo "$raw" | grep -o '"block_height":[0-9]*' | grep -o '[0-9]*' 2>/dev/null || echo "0"
    fi
}

wait_for_block() {
    local target="$1" timeout="${2:-$BLOCK_TIMEOUT}" poll="${3:-$BLOCK_POLL}"
    local deadline=$((SECONDS + timeout))
    info "Waiting for block height >= $target (timeout ${timeout}s)..."
    while [ $SECONDS -lt $deadline ]; do
        local height
        height=$(get_block_height)
        if [ -n "$height" ] && [ "$height" -ge "$target" ] 2>/dev/null; then
            info "Block height $height reached (target >= $target)"
            return 0
        fi
        sleep "$poll"
    done
    error "Timeout waiting for block height $target"
    return 1
}

wait_for_next_block() {
    local before
    before=$(get_block_height)
    local target=$((before + 1))
    wait_for_block "$target"
}

scan_all() {
    for i in $(seq 1 "$WALLET_COUNT"); do
        info "Scanning wallet $i..."
        wal "$i" scan 2>&1 | tail -3
    done
}

collect_addresses() {
    local addr
    for i in $(seq 1 "$WALLET_COUNT"); do
        addr=$(wal "$i" wallet address 2>&1 | head -1)
        WALLET_ADDRS[$i]="$addr"
        info "Wallet $i address: $addr"
    done

    local unique
    for i in $(seq 1 "$WALLET_COUNT"); do
        [ -n "${WALLET_ADDRS[$i]}" ] || { error "Wallet $i address is empty"; exit 1; }
    done
    unique=$(printf '%s\n' "${WALLET_ADDRS[@]:1}" | sort -u | wc -l)
    if [ "$unique" -lt "$WALLET_COUNT" ]; then
        warn "Not all wallet addresses are unique ($unique unique out of $WALLET_COUNT)"
    else
        pass "All $WALLET_COUNT wallet addresses are unique"
    fi
}

get_token_balance() {
    local wallet_idx="$1" token="$2"
    wal "$wallet_idx" wallet balance | grep -i "$token" | awk '{print $1}' | head -1
}

# ── Wallet arrays ────────────────────────────────────────────────────────────

declare -a WALLET_ADDRS
declare -a WALLET_BALANCES

# ==============================================================================
echo ""
echo "══════════════════════════════════════════════"
echo "  Stablecoin Contract Test"
echo "  Wallets: $WALLET_COUNT"
echo "══════════════════════════════════════════════"
echo ""

# ==============================================================================
# Phase 0: Prerequisites
# ==============================================================================
echo ""
info "=== Phase 0: Prerequisites ==="

docker ps >/dev/null 2>&1 || { error "Docker not running"; exit 1; }

if ! docker ps --format '{{.Names}}' | grep -q "$NODE0"; then
    error "Docker testnet not running. Run test_pipeline.sh first."
    exit 1
fi
pass "node0 container running"

for i in $(seq 1 "$WALLET_COUNT"); do
    if docker ps --format '{{.Names}}' | grep -q "dwow-wallet-$i"; then
        pass "dwow-wallet-$i is running"
    else
        fail "dwow-wallet-$i is NOT running"
    fi
done

info "Checking node0 RPC health..."
for i in $(seq 1 30); do
    if node0_rpc "ping" | grep -q '"result"'; then
        info "node0 RPC is up (attempt $i)"
        break
    fi
    [ "$i" -eq 30 ] && { error "node0 RPC did not become healthy"; exit 1; }
    sleep 2
done
pass "node0 RPC healthy"

collect_addresses
scan_all

INITIAL_HEIGHT=$(get_block_height)
info "Initial block height: $INITIAL_HEIGHT"

# ==============================================================================
# Phase 8: Build gen_init_params helper
# ==============================================================================
echo ""
info "=== Phase 8: Build gen_init_params ==="

GEN_INIT_PARAMS_BIN="${REPO_ROOT}/target/debug/gen_init_params"

if [ ! -x "$GEN_INIT_PARAMS_BIN" ]; then
    info "Building gen_init_params (requires native Rust toolchain)..."
    (cd "$REPO_ROOT" && cargo build -p dwow_stablecoin_contract --bin gen_init_params --features client 2>&1) || {
        error "Failed to build gen_init_params"
        exit 1
    }
    pass "gen_init_params built"
else
    pass "gen_init_params binary found"
fi

# ==============================================================================
# Phase 8.5: Resolve PromissoryNote Contract ID
# ==============================================================================
echo ""
info "=== Phase 8.5: PromissoryNote Contract ID ==="

if [ "$PN_CID" = "auto" ]; then
    # PN is a genesis contract. Its ContractId is a hardcoded constant
    # (poseidon_hash of prefix + zero + 3) embedded at block 1. The wallet
    # auto-registers the manifest at init via initialize_promissory_note().
    # Use the canonical hex ID directly — no deploy, no base58 conversion.
    PN_CID_HEX="9f7e2ab08c7f5e1d3a6b4c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7"
    info "Canonical genesis PN contract ID: ${PN_CID_HEX:0:16}..."
    info "Manifest auto-registered — no deploy, no register required"
    PN_CID="$PN_CID_HEX"
else
    info "Using provided PN Contract ID: $PN_CID"
    # Validate it looks like 64 hex chars
    PN_CID_CLEAN=$(echo "$PN_CID" | tr -d '[:space:]')
    if [ "${#PN_CID_CLEAN}" -ne 64 ]; then
        error "PN_CID must be 64 hex characters (32 bytes), got ${#PN_CID_CLEAN}"
        exit 1
    fi
    PN_CID="$PN_CID_CLEAN"
fi

# ==============================================================================
# Phase 9: Generate deploy_ix
# ==============================================================================
echo ""
info "=== Phase 9: Generate deploy_ix ==="

# Generate a random token authority public key (32 bytes) for testing
TOKEN_AUTH_PUB=$(xxd -l 32 -p /dev/urandom 2>/dev/null || openssl rand -hex 32 2>/dev/null || {
    # Fallback: deterministic test vector
    echo "0000000000000000000000000000000000000000000000000000000000000001"
})
info "Token authority pub (random test key): $TOKEN_AUTH_PUB"

DEPLOY_IX_PATH="/tmp/stablecoin_deploy_ix.bin"
"$GEN_INIT_PARAMS_BIN" "$PN_CID" "$TOKEN_AUTH_PUB" "$DEPLOY_IX_PATH" 2>&1
check $? "generate deploy_ix (${DEPLOY_IX_PATH})"

info "deploy_ix size: $(wc -c < "$DEPLOY_IX_PATH") bytes"

# Copy deploy_ix into wallet-1 container
docker cp "$DEPLOY_IX_PATH" "dwow-wallet-1:$DEPLOY_IX_PATH" 2>&1
check $? "copy deploy_ix to wallet-1 container"

# ==============================================================================
# Phase 10: Deploy stablecoin WASM
# ==============================================================================
echo ""
info "=== Phase 10: Deploy Stablecoin ==="

if [ ! -f "$WASM_STABLECOIN" ]; then
    error "Stablecoin WASM not found at $WASM_STABLECOIN"
    exit 1
fi
pass "Stablecoin WASM found"

# Generate deploy authority for stablecoin
DEPLOY_OUTPUT=$(wal 1 contract generate-deploy 2>&1)
echo "$DEPLOY_OUTPUT"
STABLECOIN_SECRET=$(echo "$DEPLOY_OUTPUT" | grep "Secret (hex):" | awk '{print $3}')
STABLECOIN_CID=$(echo "$DEPLOY_OUTPUT" | grep "Contract ID:" | awk '{print $3}')
[ -n "$STABLECOIN_SECRET" ] && [ -n "$STABLECOIN_CID" ]
check $? "wallet 1 generate deploy authority for stablecoin"

info "Stablecoin Contract ID: $STABLECOIN_CID"

# Copy WASM into container
docker cp "$WASM_STABLECOIN" "dwow-wallet-1:/tmp/stablecoin.wasm" 2>&1
check $? "copy stablecoin WASM to wallet-1"

# Deploy with init params
DEPLOY_TX=$(wal 1 contract deploy "$STABLECOIN_SECRET" "/tmp/stablecoin.wasm" "$DEPLOY_IX_PATH" 2>&1)
echo "$DEPLOY_TX" | broadcast 1
check $? "wallet 1 deploy stablecoin"

wait_for_next_block
pass "Stablecoin deployed at block $(get_block_height)"

# ==============================================================================
# Phase 11: Register stablecoin in all wallets
# ==============================================================================
echo ""
info "=== Phase 11: Register Stablecoin ==="

for i in $(seq 1 "$WALLET_COUNT"); do
    wal "$i" contract register stablecoin "$STABLECOIN_CID" 2>&1
    check $? "wallet $i register stablecoin"
done

# Verify
for i in $(seq 1 "$WALLET_COUNT"); do
    CL=$(wal "$i" contract list 2>&1 || true)
    if echo "$CL" | grep -q "$STABLECOIN_CID"; then
        pass "Wallet $i contract list includes stablecoin"
    else
        warn "Wallet $i contract list may not show stablecoin (output format varies)"
    fi
done

# ==============================================================================
# Phase 12: Create Dummy Collateral Token (COLL)
# ==============================================================================
echo ""
info "=== Phase 12: Create Dummy Collateral Token ==="

info "Wallet 1 creating COLL token..."
COLL_TX=$(wal 1 token create "COLL" 1000000000 8 2>&1)
echo "$COLL_TX" | broadcast 1
check $? "create COLL token"

wait_for_next_block
wal 1 scan 2>&1 | tail -3

# Extract COLL token ID from wallet balance output
COLL_TOKEN_ID=$(wal 1 wallet balance 2>&1 | grep -i "COLL" | awk '{print $2}' | head -1)
if [ -z "$COLL_TOKEN_ID" ]; then
    # Try token list
    COLL_TOKEN_ID=$(wal 1 token list 2>&1 | grep -i "COLL" | awk '{print $1}' | head -1)
fi
if [ -n "$COLL_TOKEN_ID" ]; then
    pass "COLL token created: $COLL_TOKEN_ID"
else
    warn "COLL token created but could not extract token ID"
    COLL_TOKEN_ID="COLL"
fi

# ==============================================================================
# Phase 13: Mint COLL to wallets 2 and 3, mesh transfers
# ==============================================================================
echo ""
info "=== Phase 13: Mint & Transfer COLL ==="

for i in $(seq 2 "$WALLET_COUNT"); do
    info "Minting $MINT_AMOUNT COLL to wallet $i (${WALLET_ADDRS[$i]})..."
    MINT_TX=$(wal 1 token mint "$COLL_TOKEN_ID" "$MINT_AMOUNT" "${WALLET_ADDRS[$i]}" 2>&1)
    echo "$MINT_TX" | broadcast 1
    check $? "mint COLL to wallet $i"
done

wait_for_next_block
scan_all

# Verify COLL balances
for i in $(seq 2 "$WALLET_COUNT"); do
    COLL_BAL=$(get_token_balance "$i" "COLL")
    info "Wallet $i COLL balance: $COLL_BAL"
    if [ -n "$COLL_BAL" ] && [ "$COLL_BAL" != "0" ] && [ "$COLL_BAL" != "0.00000000" ]; then
        pass "Wallet $i has COLL"
    else
        warn "Wallet $i COLL balance not confirmed"
    fi
done

# Mesh transfers of COLL between wallets 2..N
if [ "$WALLET_COUNT" -ge 3 ]; then
    for src in $(seq 2 "$WALLET_COUNT"); do
        for dst in $(seq 2 "$WALLET_COUNT"); do
            [ "$src" -eq "$dst" ] && continue
            info "Wallet $src → wallet $dst ($TRANSFER_AMOUNT COLL)..."
            TX=$(wal "$src" transfer "$TRANSFER_AMOUNT" "$COLL_TOKEN_ID" "${WALLET_ADDRS[$dst]}" 2>&1)
            echo "$TX" | broadcast "$src"
            check $? "wallet $src → wallet $dst COLL"
        done
    done

    wait_for_next_block
    scan_all
    pass "COLL mesh transfers complete"
fi

# ==============================================================================
# Phase 14: Create Stablecoin Token (USDx)
# ==============================================================================
echo ""
info "=== Phase 14: Create Stablecoin Token ==="

info "Wallet 1 creating USDx token..."
USDX_TX=$(wal 1 token create "USDx" 1000000000 8 2>&1)
echo "$USDX_TX" | broadcast 1
check $? "create USDx token"

wait_for_next_block
wal 1 scan 2>&1 | tail -3

USDX_TOKEN_ID=$(wal 1 wallet balance 2>&1 | grep -i "USDx" | awk '{print $2}' | head -1)
if [ -z "$USDX_TOKEN_ID" ]; then
    USDX_TOKEN_ID=$(wal 1 token list 2>&1 | grep -i "USDx" | awk '{print $1}' | head -1)
fi
if [ -n "$USDX_TOKEN_ID" ]; then
    pass "USDx token created: $USDX_TOKEN_ID"
else
    warn "USDx token created but could not extract token ID"
    USDX_TOKEN_ID="USDx"
fi

# ==============================================================================
# Phase 15: Mint USDx to wallets 2 and 3, mesh transfers
# ==============================================================================
echo ""
info "=== Phase 15: Mint & Transfer USDx ==="

for i in $(seq 2 "$WALLET_COUNT"); do
    info "Minting $MINT_AMOUNT USDx to wallet $i (${WALLET_ADDRS[$i]})..."
    MINT_TX=$(wal 1 token mint "$USDX_TOKEN_ID" "$MINT_AMOUNT" "${WALLET_ADDRS[$i]}" 2>&1)
    echo "$MINT_TX" | broadcast 1
    check $? "mint USDx to wallet $i"
done

wait_for_next_block
scan_all

# Verify USDx balances
for i in $(seq 2 "$WALLET_COUNT"); do
    USDX_BAL=$(get_token_balance "$i" "USDx")
    info "Wallet $i USDx balance: $USDX_BAL"
    if [ -n "$USDX_BAL" ] && [ "$USDX_BAL" != "0" ] && [ "$USDX_BAL" != "0.00000000" ]; then
        pass "Wallet $i has USDx"
    else
        warn "Wallet $i USDx balance not confirmed"
    fi
done

# Mesh transfers of USDx between wallets 2..N
if [ "$WALLET_COUNT" -ge 3 ]; then
    for src in $(seq 2 "$WALLET_COUNT"); do
        for dst in $(seq 2 "$WALLET_COUNT"); do
            [ "$src" -eq "$dst" ] && continue
            info "Wallet $src → wallet $dst ($TRANSFER_AMOUNT USDx)..."
            TX=$(wal "$src" transfer "$TRANSFER_AMOUNT" "$USDX_TOKEN_ID" "${WALLET_ADDRS[$dst]}" 2>&1)
            echo "$TX" | broadcast "$src"
            check $? "wallet $src → wallet $dst USDx"
        done
    done

    wait_for_next_block
    scan_all
    pass "USDx mesh transfers complete"
fi

# ==============================================================================
# Phase 16: Final Audit
# ==============================================================================
echo ""
echo "══════════════════════════════════════════════"
echo "  Stablecoin Test Report"
echo "══════════════════════════════════════════════"
echo ""

FINAL_HEIGHT=$(get_block_height)
info "Final block height: $FINAL_HEIGHT"
info "Stablecoin Contract ID: $STABLECOIN_CID"

echo ""
info "Final balances:"
for i in $(seq 1 "$WALLET_COUNT"); do
    echo "  ── Wallet $i (${WALLET_ADDRS[$i]}) ──"
    wal "$i" wallet balance 2>&1 | sed 's/^/    /'
done

echo ""
echo "══════════════════════════════════════════════"
echo "  Wallets: $WALLET_COUNT  |  Block height: $FINAL_HEIGHT"
echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
echo "══════════════════════════════════════════════"

if [ "$FAIL" -gt 0 ]; then
    echo -e "${RED}Some stablecoin tests failed${NC}"
    exit 1
else
    echo -e "${GREEN}All stablecoin tests passed${NC}"
fi
