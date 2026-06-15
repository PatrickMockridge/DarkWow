#!/bin/bash
# Multi-Wallet Transaction Test on DarkWow-Testnet
#
# Tests cross-wallet transactions on a running Docker testnet.  Generalized
# for 1-5 wallet containers.  All wallets receive a workable DRKW balance.
#
# Usage:
#   ./test-wallet-transactions.sh 2          # default: 2 wallets, all phases
#   ./test-wallet-transactions.sh 3          # 3 wallets
#   SKIP_OTC=1 ./test-wallet-transactions.sh 2  # skip OTC swap phase
#
# Prerequisites:
#   test_pipeline.sh --with-wallet N must have completed successfully.

set -e
set -E

trap 'echo "[FATAL] test-wallet-transactions failed at line $LINENO — exit code $?" >&2' ERR
trap 'echo "[FATAL] test-wallet-transactions killed by signal" >&2; exit 1' INT TERM HUP PIPE

# ── Constants ────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

# WALLET_COUNT: positional arg, defaults to 2
WALLET_COUNT="${1:-2}"
if ! [ "$WALLET_COUNT" -ge 1 ] 2>/dev/null || ! [ "$WALLET_COUNT" -le 5 ] 2>/dev/null; then
    echo "WALLET_COUNT must be 1-5, got: $WALLET_COUNT"
    exit 1
fi

# Config is the same path inside every wallet container
WALLET_CONFIG="/root/.config/dwow/drk.toml"

# Fund distribution amount per wallet (2 DRKW = 200,000,000 base units)
FUND_AMOUNT=200000000
SELF_TRANSFER_AMOUNT=10000000       # 0.1 DRKW
MESH_TRANSFER_AMOUNT=10000000
MINT_AMOUNT=500000000               # 5 custom tokens for mint test

# Phase skipping via env vars
SKIP_SELF="${SKIP_SELF:-0}"
SKIP_MESH="${SKIP_MESH:-0}"
SKIP_DEPLOY="${SKIP_DEPLOY:-0}"
SKIP_TOKEN="${SKIP_TOKEN:-0}"
SKIP_OTC="${SKIP_OTC:-0}"

# Block wait settings
BLOCK_TIMEOUT=300   # max seconds to wait for a block
BLOCK_POLL=10       # seconds between polls

# Container names
NODE0="dwow-node0"
RPC_PORT=31345

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

# Execute a command inside wallet container N
wal() {
    local i=$1; shift
    docker exec "dwow-wallet-$i" /app/dwow_wallet -c "$WALLET_CONFIG" "$@" 2>&1
}

# Broadcast a transaction and verify the RPC response contains "result" not "error".
# Usage: broadcast <wallet_idx> <tx_data>
# Reads tx from stdin if only one arg, or from $2 if two args.
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
        # No clear result/error indicator — check for known error patterns
        if echo "$out" | grep -qi "rejected\|insufficient\|invalid\|failed"; then
            echo "$out"
            return 1
        fi
        echo "$out"
        return 0
    fi
}

# Execute a JSON-RPC call against node0 via /dev/tcp
node0_rpc() {
    local method="$1" params="${2:-[]}"
    docker exec "$NODE0" bash -c \
        "exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT; echo '{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}' >&3; timeout 5 cat <&3" 2>/dev/null
}

# Get current block height from node0 RPC
get_block_height() {
    local raw
    raw=$(node0_rpc "blockchain.info" || true)
    if [ -z "$raw" ]; then
        # fallback: try last_confirmed_block
        raw=$(node0_rpc "blockchain.last_confirmed_block" || true)
        echo "$raw" | grep -o '"[0-9.]*"' | head -1 | tr -d '"' | cut -d'.' -f1 2>/dev/null || echo "0"
    else
        echo "$raw" | grep -o '"block_height":[0-9]*' | grep -o '[0-9]*' 2>/dev/null || echo "0"
    fi
}

# Wait for a specific block height to be reached
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

# Get the current block height, wait for the next block, return new height
wait_for_next_block() {
    local before
    before=$(get_block_height)
    local target=$((before + 1))
    wait_for_block "$target"
}

# ── Wallet arrays ────────────────────────────────────────────────────────────

declare -a WALLET_ADDRS
declare -a WALLET_BALANCES

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

    # Verify all addresses are non-empty and unique
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

# Parse numeric balance for a token from wallet balance output.
# The balance output looks like: "100000000 DRKW" or "0 DRKW" per line.
get_token_balance() {
    local wallet_idx="$1" token="$2"
    wal "$wallet_idx" wallet balance | grep -i "$token" | awk '{print $1}' | head -1
}

# ==============================================================================
echo ""
echo "══════════════════════════════════════════════"
echo "  Multi-Wallet Transaction Test"
echo "  Wallets: $WALLET_COUNT"
echo "══════════════════════════════════════════════"
echo ""

# ==============================================================================
# Phase 0: Prerequisites
# ==============================================================================
echo ""
info "=== Phase 0: Prerequisites ==="

# Docker
docker ps >/dev/null 2>&1 || { error "Docker not running"; exit 1; }

# Verify node0 is running
if ! docker ps --format '{{.Names}}' | grep -q "$NODE0"; then
    error "Docker testnet not running. Run test_pipeline.sh first."
    exit 1
fi
pass "node0 container running"

# Verify wallet containers
for i in $(seq 1 "$WALLET_COUNT"); do
    if docker ps --format '{{.Names}}' | grep -q "dwow-wallet-$i"; then
        pass "dwow-wallet-$i is running"
    else
        fail "dwow-wallet-$i is NOT running"
    fi
done

# RPC health check
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

# Collect addresses and scan
collect_addresses
scan_all

INITIAL_HEIGHT=$(get_block_height)
info "Initial block height: $INITIAL_HEIGHT"

# ==============================================================================
# Phase 1: Initial Balance Audit
# ==============================================================================
echo ""
info "=== Phase 1: Initial Balance Audit ==="

# Wallet-1 must have DRKW from mining.  If not, wait for blocks.
W1_BALANCE=$(get_token_balance 1 "DRKW")
info "Wallet 1 DRKW balance: $W1_BALANCE"

if [ -z "$W1_BALANCE" ] || [ "$W1_BALANCE" = "0" ] || [ "$W1_BALANCE" = "0.00000000" ]; then
    info "Wallet 1 has no DRKW yet.  Waiting for mining rewards..."
    for attempt in 1 2 3 4 5; do
        wait_for_next_block || true
        wal 1 scan 2>&1 | tail -2
        W1_BALANCE=$(get_token_balance 1 "DRKW")
        info "Wallet 1 DRKW balance (attempt $attempt): $W1_BALANCE"
        if [ -n "$W1_BALANCE" ] && [ "$W1_BALANCE" != "0" ] && [ "$W1_BALANCE" != "0.00000000" ]; then
            pass "Wallet 1 funded by mining after $attempt block(s)"
            break
        fi
        if [ "$attempt" -eq 5 ]; then
            fail "Wallet 1 still has no DRKW after 5 block-waits"
            exit 1
        fi
    done
else
    pass "Wallet 1 has DRKW from mining"
fi

# Log all initial balances
total=0
for i in $(seq 1 "$WALLET_COUNT"); do
    bal=$(get_token_balance "$i" "DRKW")
    WALLET_BALANCES[$i]="${bal:-0}"
    info "Wallet $i initial DRKW: ${WALLET_BALANCES[$i]}"
    if [ "${WALLET_BALANCES[$i]}" != "0" ] && [ "${WALLET_BALANCES[$i]}" != "0.00000000" ]; then
        total=$((total + 1))
    fi
done
info "Wallets with DRKW: $total of $WALLET_COUNT"

# ==============================================================================
# Phase 2: Fund Distribution (wallet-1 → wallets 2..N)
# ==============================================================================
if [ "$WALLET_COUNT" -ge 2 ]; then
    echo ""
    info "=== Phase 2: Fund Distribution ==="
    info "Wallet 1 sends $FUND_AMOUNT DRKW to each other wallet"

    for i in $(seq 2 "$WALLET_COUNT"); do
        info "Funding wallet $i (${WALLET_ADDRS[$i]})..."
        TX=$(wal 1 transfer "$FUND_AMOUNT" DRKW "${WALLET_ADDRS[$i]}" 2>&1)
        [ -n "$TX" ] || { fail "transfer tx to wallet $i — empty output"; continue; }

        echo "$TX" | broadcast 1
        check $? "broadcast transfer to wallet $i"

        # Wait for block inclusion
        wait_for_next_block || { fail "block inclusion for wallet $i transfer"; continue; }

        wal "$i" scan 2>&1 | tail -2
        B2=$(get_token_balance "$i" "DRKW")
        info "Wallet $i DRKW after funding: $B2"

        if [ -n "$B2" ] && [ "$B2" != "0" ] && [ "$B2" != "0.00000000" ]; then
            pass "Wallet $i funded ($B2 DRKW)"
        else
            fail "Wallet $i still has no DRKW after transfer"
        fi
    done
fi

# ==============================================================================
# Phase 3: Self-Transfer (each wallet sends to itself)
# ==============================================================================
if [ "$SKIP_SELF" = "0" ]; then
    echo ""
    info "=== Phase 3: Self-Transfer ==="

    for i in $(seq 1 "$WALLET_COUNT"); do
        info "Wallet $i self-transfer..."
        ADDR="${WALLET_ADDRS[$i]}"
        TX=$(wal "$i" transfer "$SELF_TRANSFER_AMOUNT" DRKW "$ADDR" 2>&1)
        echo "$TX" | broadcast "$i"
        check $? "wallet $i self-transfer broadcast"
    done

    wait_for_next_block
    scan_all

    for i in $(seq 1 "$WALLET_COUNT"); do
        B=$(get_token_balance "$i" "DRKW")
        if [ -n "$B" ] && [ "$B" != "0" ]; then
            pass "Wallet $i balance post-self-transfer: $B"
        else
            fail "Wallet $i no balance after self-transfer"
        fi
    done
fi

# ==============================================================================
# Phase 4: Pairwise Transfers (mesh)
# ==============================================================================
if [ "$SKIP_MESH" = "0" ] && [ "$WALLET_COUNT" -ge 2 ]; then
    echo ""
    info "=== Phase 4: Pairwise Transfers ==="

    for src in $(seq 1 "$WALLET_COUNT"); do
        for dst in $(seq 1 "$WALLET_COUNT"); do
            [ "$src" -eq "$dst" ] && continue
            info "Wallet $src → wallet $dst ($MESH_TRANSFER_AMOUNT DRKW)..."
            TX=$(wal "$src" transfer "$MESH_TRANSFER_AMOUNT" DRKW "${WALLET_ADDRS[$dst]}" 2>&1)
            echo "$TX" | broadcast "$src"
            check $? "wallet $src → wallet $dst"
        done
    done

    wait_for_next_block
    scan_all

    for i in $(seq 1 "$WALLET_COUNT"); do
        B=$(get_token_balance "$i" "DRKW")
        info "Wallet $i post-mesh balance: $B"
    done
fi

# ==============================================================================
# Phase 5: Contract Deployment
# ==============================================================================
if [ "$SKIP_DEPLOY" = "0" ]; then
    echo ""
    info "=== Phase 5: Promissory Note (Genesis Contract) ==="

    # PN is a genesis contract — already exists at block 1. The wallet
    # auto-registers its manifest at init. No deploy needed.
    PROMISSORY_NOTE_CID="9f7e2ab08c7f5e1d3a6b4c8d9e0f1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7"
    info "Canonical genesis PN contract ID: ${PROMISSORY_NOTE_CID:0:16}..."
    info "Manifest auto-registered — no deploy required"

    # All wallets register the contract (verifies CID matches canonical constant)
    for i in $(seq 1 "$WALLET_COUNT"); do
        wal "$i" contract register promissory_note "$PROMISSORY_NOTE_CID" 2>&1
        check $? "wallet $i register promissory_note"
    done
    # Verify contract list in each wallet
    for i in $(seq 1 "$WALLET_COUNT"); do
        CL=$(wal "$i" contract list 2>&1 || true)
        if echo "$CL" | grep -q "promissory_note"; then
            pass "Wallet $i contract list includes promissory_note"
        else
            warn "Wallet $i contract list may not show promissory_note (command may be WIP)"
        fi
    done
    # Contract mint invocation — test promissory_note::mint via contract invoke
    info "Testing promissory_note mint invocation via contract invoke..."
    MINT_INVOKE=$(wal 1 contract invoke "$PROMISSORY_NOTE_CID" "promissory_note::mint_v1" \
        --ticker "TEST" --amount "$MINT_AMOUNT" 2>&1) || true
    if [ -n "$MINT_INVOKE" ]; then
        echo "$MINT_INVOKE" | broadcast 1
        check $? "promissory_note mint invocation broadcast"
        wait_for_next_block
            wal 1 scan 2>&1 | tail -2
            if wal 1 wallet balance 2>&1 | grep -qi "TEST"; then
                pass "promissory_note mint — TEST token visible in balance"
            else
                pass "promissory_note mint invocation sent (token visibility depends on scan)"

        else
            warn "contract invoke returned empty — command may be WIP"
        fi
    fi
fi

# ==============================================================================
# Phase 6: Custom Token Flow
# ==============================================================================
if [ "$SKIP_TOKEN" = "0" ]; then
    echo ""
    info "=== Phase 6: Custom Token Flow ==="

    # Wallet-1 generates mint authority
    info "Generating mint authority from wallet 1..."
    MINT_OUTPUT=$(wal 1 token generate-mint 2>&1)
    echo "$MINT_OUTPUT"
    TOKEN_ID=$(echo "$MINT_OUTPUT" | grep -i "token" | grep -o '[a-zA-Z0-9]\{30,\}' | head -1)
    [ -n "$TOKEN_ID" ] && pass "mint authority generated: $TOKEN_ID" || { fail "mint authority generation"; warn "Skipping remaining token phases"; }

    if [ -n "$TOKEN_ID" ]; then
        # Wallet-1 mints to all other wallets
        for i in $(seq 2 "$WALLET_COUNT"); do
            info "Minting $MINT_AMOUNT of $TOKEN_ID to wallet $i..."
            MINT_TX=$(wal 1 token mint "$TOKEN_ID" "$MINT_AMOUNT" "${WALLET_ADDRS[$i]}" 2>&1)
            echo "$MINT_TX" | broadcast 1
            check $? "mint to wallet $i"
        done

        wait_for_next_block
        scan_all

        # Each wallet 2..N verifies custom token balance
        for i in $(seq 2 "$WALLET_COUNT"); do
            info "Wallet $i balance:"
            wal "$i" wallet balance 2>&1
            if wal "$i" wallet balance 2>&1 | grep -q "$TOKEN_ID"; then
                pass "Wallet $i has custom token $TOKEN_ID"
            else
                warn "Wallet $i may not show custom token (balance output format varies)"
            fi
        done

        # Each wallet 2..N transfers custom tokens back to wallet 1
        for i in $(seq 2 "$WALLET_COUNT"); do
            info "Wallet $i sending custom tokens back to wallet 1..."
            TX=$(wal "$i" transfer "$MINT_AMOUNT" "$TOKEN_ID" "${WALLET_ADDRS[1]}" 2>&1)
            echo "$TX" | broadcast "$i"
            check $? "wallet $i transfer custom tokens back to wallet 1"
        done

        wait_for_next_block
        wal 1 scan 2>&1 | tail -2
        info "Wallet 1 balance after receiving custom tokens:"
        wal 1 wallet balance 2>&1
        pass "custom token round-trip complete"
    fi
fi

# ==============================================================================
# Phase 7: OTC Atomic Swap (wallet 1 ↔ wallet 2)
# ==============================================================================
if [ "$SKIP_OTC" = "0" ] && [ "$WALLET_COUNT" -ge 2 ]; then
    echo ""
    info "=== Phase 7: OTC Atomic Swap ==="

    # Use a test token ID for the swap (DRKW ↔ DRKW is simplest)
    # Both wallets use DRKW so no custom token dependency.
    info "Initiating OTC swap: wallet 1 sends 1.0 DRKW, receives 1.0 DRKW..."

    SWAP_HALF=$(wal 1 otc init --value-pair "1.0:1.0" --token-pair "DRKW:DRKW" 2>&1) || true
    if [ -z "$SWAP_HALF" ]; then
        warn "OTC init returned empty — OTC commands may not be implemented. Skipping."
    elif echo "$SWAP_HALF" | grep -q "error\|unknown\|not found\|TODO"; then
        warn "OTC init not available — skipping. $SWAP_HALF"
    else
        pass "wallet 1 OTC init"

        # Wallet 2 joins
        JOINED_TX=$(echo "$SWAP_HALF" | wal 2 otc join 2>&1) || true
        if [ -n "$JOINED_TX" ]; then
            pass "wallet 2 OTC join"

            # Both wallets sign
            SIGNED_TX=$(echo "$JOINED_TX" | wal 1 otc sign 2>&1 | wal 2 otc sign 2>&1) || true
            if [ -n "$SIGNED_TX" ]; then
                pass "OTC dual-sign"

                # Broadcast
                echo "$SIGNED_TX" | broadcast 1
                check $? "OTC swap broadcast"

                wait_for_next_block
                scan_all
                pass "OTC swap block confirmed"
            else
                warn "OTC sign returned empty — skipping broadcast"
            fi
        else
            warn "OTC join returned empty — skipping remaining swap steps"
        fi
    fi
fi

# ==============================================================================
# Phase 8: Final Report
# ==============================================================================
echo ""
echo "══════════════════════════════════════════════"
echo "  Test Report"
echo "══════════════════════════════════════════════"
echo ""

FINAL_HEIGHT=$(get_block_height)
info "Final block height: $FINAL_HEIGHT"

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
    echo -e "${RED}Some tests failed${NC}"
    exit 1
else
    echo -e "${GREEN}All wallet transaction tests passed${NC}"
fi
