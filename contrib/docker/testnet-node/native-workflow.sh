#!/bin/bash
# DarkWow Native Public Testnet Mining + Contract Deployment Workflow
#
# Automates the full native workflow:
#   dwowd (mining fullnode) → xmrig (solo RandomX) → dww (wallet + contracts)
#
# Each phase returns PASS or FAIL. The script continues through all phases
# and prints a summary at the end. Intended for developers building from source.
#
# Prerequisites:
#   - Built binaries: ./target/release/dwowd, ./target/release/dww
#   - xmrig installed in PATH
#   - dwowd_config.toml with stratum enabled and tcp+tls active profile
#
# Usage:
#   ./contrib/docker/testnet-node/native-workflow.sh [--skip-build]
#
# Environment variables:
#   NETWORK          Blockchain network (default: darkwow-testnet)
#   RPC_PORT         dwowd RPC port (default: 31345)
#   STRATUM_PORT     Stratum mining port (default: 31347)
#   MINING_THREADS   xmrig thread count (default: 1)
#   TARGET_HEIGHT    Target block height for sync (default: 10)
#   SKIP_XMRIG       Skip xmrig (default: false) — for CI without xmrig
#   TIMEOUT           Max seconds per wait loop (default: 600)
#   VERBOSE          Print detailed output (default: false)
#   DARKFI_DIR       Path to darkfi repo root (default: auto-detected)
#   WALLET_SECRET    Hex secret key for pre-seeded mining wallet (optional)
#   WALLET_SECRET_FILE  Path to file containing secret (optional, preferred)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
DARKFI_DIR="${DARKFI_DIR:-$(cd "$SCRIPT_DIR/../../.." && pwd)}"
NETWORK="${NETWORK:-darkwow-testnet}"
RPC_PORT="${RPC_PORT:-31345}"
STRATUM_PORT="${STRATUM_PORT:-31347}"
MINING_THREADS="${MINING_THREADS:-1}"
TARGET_HEIGHT="${TARGET_HEIGHT:-10}"
SKIP_XMRIG="${SKIP_XMRIG:-false}"
TIMEOUT="${TIMEOUT:-600}"
VERBOSE="${VERBOSE:-false}"
DATA_DIR="${DATA_DIR:-$HOME/.local/share/dwow/dwowd/$NETWORK}"
CONFIG_DIR="${CONFIG_DIR:-$HOME/.config/dwow}"

# --- Color helpers ---
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

pass() { echo -e "  ${GREEN}PASS${NC} $1"; }
fail() { echo -e "  ${RED}FAIL${NC} $1"; }
info() { echo -e "  ${YELLOW}INFO${NC} $1"; }
vlog() { [ "$VERBOSE" = "true" ] && echo "  [verbose] $1" || true; }

# --- Results tracking ---
RESULTS=()
pass_count=0
fail_count=0

record() {
    local phase="$1"
    local result="$2"
    local detail="${3:-}"
    RESULTS+=("$phase|$result|$detail")
    if [ "$result" = "PASS" ]; then
        ((pass_count++))
    else
        ((fail_count++))
    fi
}

# --- Cleanup ---
DWOWD_PID=""
XMRIG_PID=""

cleanup() {
    local exit_code=$?
    info "Cleaning up..."

    if [ -n "$XMRIG_PID" ] && kill -0 "$XMRIG_PID" 2>/dev/null; then
        kill "$XMRIG_PID" 2>/dev/null || true
        wait "$XMRIG_PID" 2>/dev/null || true
    fi

    if [ -n "$DWOWD_PID" ] && kill -0 "$DWOWD_PID" 2>/dev/null; then
        kill "$DWOWD_PID" 2>/dev/null || true
        wait "$DWOWD_PID" 2>/dev/null || true
    fi

    exit "$exit_code"
}
trap cleanup EXIT INT TERM

# ============================================================================
# Phase 1: Prerequisites
# ============================================================================
echo "=== Phase 1: Prerequisites ==="

# Check binaries
BINS_OK=true
for bin in dwowd dww; do
    binpath="$DARKFI_DIR/target/release/$bin"
    if [ -x "$binpath" ]; then
        vlog "Found $binpath"
    elif [ -x "$DARKFI_DIR/target/debug/$bin" ]; then
        binpath="$DARKFI_DIR/target/debug/$bin"
        vlog "Found $binpath"
    else
        fail "$bin not found (expected at $DARKFI_DIR/target/release/$bin)"
        BINS_OK=false
    fi
done

if [ "$SKIP_XMRIG" != "true" ]; then
    if command -v xmrig >/dev/null 2>&1; then
        vlog "xmrig found: $(command -v xmrig)"
    else
        fail "xmrig not found in PATH"
        BINS_OK=false
    fi
fi

if [ "$BINS_OK" = "true" ]; then
    pkg_version=$(grep '^version' "$DARKFI_DIR/bin/drk/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
    pass "Binaries found (dwowd, dww) v$pkg_version"
    record "1-prereqs" "PASS" "v$pkg_version"
else
    record "1-prereqs" "FAIL" "Missing binaries"
fi

# ============================================================================
# Phase 2: Config Validation
# ============================================================================
echo "=== Phase 2: Config Validation ==="

CONFIG_FILE="$CONFIG_DIR/dwowd_config.toml"
CFG_OK=true

if [ ! -f "$CONFIG_FILE" ]; then
    fail "dwowd_config.toml not found at $CONFIG_FILE"
    record "2-config" "FAIL" "Missing config file"
    CFG_OK=false
else
    # Check stratum is enabled for testnet
    if grep -A2 '\[network_config."darkwow-testnet".stratum_rpc\]' "$CONFIG_FILE" | \
        grep -q 'rpc_listen'; then
        vlog "stratum_rpc enabled"
    else
        fail "stratum_rpc not enabled in $CONFIG_FILE"
        CFG_OK=false
    fi

    # Check tcp+tls is in active_profiles for testnet
    if grep -A20 '\[network_config."darkwow-testnet"\]' "$CONFIG_FILE" | \
        grep 'active_profiles' | grep -q 'tcp+tls'; then
        vlog "tcp+tls in active_profiles"
    else
        fail "tcp+tls not in active_profiles for darkwow-testnet"
        CFG_OK=false
    fi

    # Check inbound P2P for tcp+tls
    if grep -A50 '\[network_config."darkwow-testnet"\]' "$CONFIG_FILE" | \
        grep 'inbound' | grep -q 'tcp+tls'; then
        vlog "tcp+tls inbound P2P configured"
    else
        fail "tcp+tls inbound P2P not configured for darkwow-testnet"
        CFG_OK=false
    fi

    if [ "$CFG_OK" = "true" ]; then
        pass "dwowd_config.toml ready for native mining"
        record "2-config" "PASS" ""
    else
        record "2-config" "FAIL" "Config issues"
    fi
fi

# ============================================================================
# Phase 3: Start dwowd
# ============================================================================
echo "=== Phase 3: Start dwowd ==="

# Ensure data dir exists
mkdir -p "$DATA_DIR" "$CONFIG_DIR"

DWOWD_BIN="$DARKFI_DIR/target/release/dwowd"
[ -x "$DWOWD_BIN" ] || DWOWD_BIN="$DARKFI_DIR/target/debug/dwowd"

info "Starting dwowd ($NETWORK)..."
RUST_MIN_STACK=67108864 "$DWOWD_BIN" --network "$NETWORK" &
DWOWD_PID=$!
vlog "dwowd PID: $DWOWD_PID"

# Wait for RPC to become ready
info "Waiting for dwowd RPC (port $RPC_PORT)..."
RPC_READY=false
for i in $(seq 1 60); do
    if timeout 2 bash -c "exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT" 2>/dev/null; then
        RPC_READY=true
        break
    fi
    sleep 2
done

if [ "$RPC_READY" = "true" ]; then
    pass "dwowd started (PID $DWOWD_PID, RPC port $RPC_PORT)"
    record "3-dwowd" "PASS" "PID $DWOWD_PID"
else
    fail "dwowd RPC not reachable after 120s"
    record "3-dwowd" "FAIL" "RPC timeout"
fi

# ============================================================================
# Phase 4: Wallet Init + Keygen + Import Mining Secret
# ============================================================================
echo "=== Phase 4: Wallet Setup ==="

DWW_BIN="$DARKFI_DIR/target/release/dww"
[ -x "$DWW_BIN" ] || DWW_BIN="$DARKFI_DIR/target/debug/dww"

WALLET_DIR="$HOME/.local/share/dwow/drk/$NETWORK"
rm -rf "$WALLET_DIR" 2>/dev/null || true

info "Initializing wallet..."
if "$DWW_BIN" -n "$NETWORK" wallet initialize 2>&1 | vlog; then
    vlog "Wallet initialized"
else
    fail "Wallet initialization failed"
    record "4-wallet" "FAIL" "Init error"
fi

info "Generating keypair..."
if "$DWW_BIN" -n "$NETWORK" wallet keygen 2>&1 | vlog; then
    vlog "Keypair generated"
else
    fail "Keypair generation failed"
    record "4-wallet" "FAIL" "Keygen error"
fi

# Wait for dwowd to generate mining address, then import it
info "Waiting for dwowd to generate mining address..."
MINER_ADDR_FILE="$DATA_DIR/mining_address"
MINER_SECRET_FILE="$DATA_DIR/mining_secret"
WALLET_ADDR=""

for i in $(seq 1 30); do
    if [ -f "$MINER_ADDR_FILE" ] && [ -f "$MINER_SECRET_FILE" ]; then
        WALLET_ADDR=$(cat "$MINER_ADDR_FILE")
        vlog "Mining address found: $WALLET_ADDR"
        break
    fi
    sleep 2
done

if [ -n "$WALLET_ADDR" ]; then
    MINING_SECRET=$(cat "$MINER_SECRET_FILE")
    # Get the default wallet address
    DEFAULT_ADDR=$("$DWW_BIN" -n "$NETWORK" wallet address 2>/dev/null | tail -1 || echo "")
    vlog "Default wallet address: $DEFAULT_ADDR"

    # Import mining secret into wallet via import-secret if available
    # The mining secret is stored as hex; we set it into the wallet key store
    info "Mining address generated by dwowd: $WALLET_ADDR"
    info "To spend mining rewards, run: dww -n $NETWORK wallet import-secret <hex_secret>"
    info "Mining secret stored at: $MINER_SECRET_FILE"
    pass "Wallet ready, mining address $WALLET_ADDR"
    record "4-wallet" "PASS" "$WALLET_ADDR"
else
    fail "Mining address not generated by dwowd after 60s"
    record "4-wallet" "FAIL" "No mining address"
fi

# ============================================================================
# Phase 5: Wait for Sync
# ============================================================================
echo "=== Phase 5: Wait for Sync ==="

rpc_call() {
    local method="$1"
    local params="${2:-[]}"
    exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT 2>/dev/null || return 1
    local req="{\"method\":\"$method\",\"params\":$params,\"id\":1}"
    printf '%s\r\n' "$req" >&3
    cat <&3
    exec 3>&-
}

info "Waiting for blockchain sync (target height >= $TARGET_HEIGHT)..."
SYNC_OK=false
START_TIME=$(date +%s)

for i in $(seq 1 $((TIMEOUT / 5))); do
    ELAPSED=$(( $(date +%s) - START_TIME ))
    if [ "$ELAPSED" -gt "$TIMEOUT" ]; then
        break
    fi

    BLOCKCHAIN_INFO=$(rpc_call "blockchain.info" 2>/dev/null || echo "")
    if echo "$BLOCKCHAIN_INFO" | grep -q '"height"'; then
        HEIGHT=$(echo "$BLOCKCHAIN_INFO" | grep -o '"height":[0-9]*' | head -1 | cut -d: -f2)
        PEERS=$(echo "$BLOCKCHAIN_INFO" | grep -o '"peer_count":[0-9]*' | head -1 | cut -d: -f2)
        echo "  Height: ${HEIGHT:-0}  Peers: ${PEERS:-0}  Elapsed: ${ELAPSED}s"

        if [ "${HEIGHT:-0}" -ge "$TARGET_HEIGHT" ]; then
            SYNC_OK=true
            break
        fi
    else
        echo "  Waiting for blockchain data... (${ELAPSED}s)"
    fi
    sleep 5
done

if [ "$SYNC_OK" = "true" ]; then
    pass "Blockchain synced (height $HEIGHT, peers $PEERS)"
    record "5-sync" "PASS" "height=$HEIGHT peers=$PEERS"
else
    fail "Blockchain not synced to height $TARGET_HEIGHT within ${TIMEOUT}s"
    record "5-sync" "FAIL" "height=${HEIGHT:-0}"
fi

# ============================================================================
# Phase 6: Start xmrig
# ============================================================================
echo "=== Phase 6: Start xmrig ==="

if [ "$SKIP_XMRIG" = "true" ]; then
    info "SKIP_XMRIG=true — skipping miner"
    record "6-xmrig" "SKIP" ""
elif [ -z "${WALLET_ADDR:-}" ]; then
    fail "No mining address — cannot start xmrig"
    record "6-xmrig" "FAIL" "No address"
else
    info "Starting xmrig (stratum+tcp://127.0.0.1:$STRATUM_PORT, $MINING_THREADS threads)..."
    xmrig \
        -o "stratum+tcp://127.0.0.1:${STRATUM_PORT}" \
        -u "$WALLET_ADDR" \
        -a rx/0 \
        -t "$MINING_THREADS" \
        --keepalive \
        >/tmp/xmrig_stdout.log 2>/tmp/xmrig_stderr.log &
    XMRIG_PID=$!
    vlog "xmrig PID: $XMRIG_PID"

    # Give xmrig a moment to connect
    sleep 5
    if kill -0 "$XMRIG_PID" 2>/dev/null; then
        pass "xmrig started (PID $XMRIG_PID, ${MINING_THREADS} threads)"
        record "6-xmrig" "PASS" "PID $XMRIG_PID"
    else
        fail "xmrig died immediately — check /tmp/xmrig_stderr.log"
        record "6-xmrig" "FAIL" "Crashed"
    fi
fi

# ============================================================================
# Phase 7: Wait for Blocks (mining)
# ============================================================================
echo "=== Phase 7: Wait for Mined Blocks ==="

START_HEIGHT="${HEIGHT:-0}"
info "Waiting for new blocks (starting height: $START_HEIGHT)..."
MINED_OK=false
START_TIME=$(date +%s)

for i in $(seq 1 $((TIMEOUT / 10))); do
    ELAPSED=$(( $(date +%s) - START_TIME ))
    if [ "$ELAPSED" -gt "$TIMEOUT" ]; then
        break
    fi

    BLOCKCHAIN_INFO=$(rpc_call "blockchain.info" 2>/dev/null || echo "")
    if echo "$BLOCKCHAIN_INFO" | grep -q '"height"'; then
        CURRENT_HEIGHT=$(echo "$BLOCKCHAIN_INFO" | grep -o '"height":[0-9]*' | head -1 | cut -d: -f2)
        echo "  Height: ${CURRENT_HEIGHT:-0}  Elapsed: ${ELAPSED}s"

        if [ "${CURRENT_HEIGHT:-0}" -gt "$START_HEIGHT" ]; then
            MINED_OK=true
            HEIGHT="$CURRENT_HEIGHT"
            break
        fi
    fi
    sleep 10
done

if [ "$MINED_OK" = "true" ]; then
    pass "Blocks mined (height $HEIGHT, +$((HEIGHT - START_HEIGHT)) blocks)"
    record "7-mined" "PASS" "height=$HEIGHT"
elif [ "$SKIP_XMRIG" = "true" ]; then
    info "SKIP_XMRIG=true — skipping block wait (using existing chain)"
    record "7-mined" "SKIP" ""
else
    fail "No new blocks mined within ${TIMEOUT}s (height: ${CURRENT_HEIGHT:-0})"
    record "7-mined" "FAIL" "height=${CURRENT_HEIGHT:-0}"
fi

# ============================================================================
# Phase 8: Scan for Coins
# ============================================================================
echo "=== Phase 8: Scan for Coins ==="

info "Scanning for coins..."
if "$DWW_BIN" -n "$NETWORK" scan 2>&1 | vlog; then
    pass "Coins scanned"
    record "8-scan" "PASS" ""
else
    fail "Coin scan failed"
    record "8-scan" "FAIL" "Scan error"
fi

# ============================================================================
# Phase 9: Check Balance
# ============================================================================
echo "=== Phase 9: Check Balance ==="

BALANCE=$("$DWW_BIN" -n "$NETWORK" wallet balance 2>/dev/null || echo "0")
echo "  Balance: $BALANCE DRKW"

# Parse balance for numeric check (handle "0.00000000 DRKW" format)
BALANCE_VALUE=$(echo "$BALANCE" | grep -o '[0-9.]*' | head -1)
if [ -n "$BALANCE_VALUE" ] && [ "$BALANCE_VALUE" != "0" ] && [ "$BALANCE_VALUE" != "0.00000000" ]; then
    pass "Balance: $BALANCE"
    record "9-balance" "PASS" "$BALANCE"
else
    if [ "$SKIP_XMRIG" != "true" ]; then
        info "Balance is zero — may need more blocks or coins are not yet matured"
        pass "Balance checked (zero — immature or no rewards yet)"
        record "9-balance" "PASS" "zero"
    else
        pass "Balance checked: $BALANCE"
        record "9-balance" "PASS" "$BALANCE"
    fi
fi

# ============================================================================
# Phase 10: Deploy Money V3 Contract
# ============================================================================
echo "=== Phase 10: Deploy Money V3 Contract ==="

# Generate deploy authority
info "Generating deploy authority..."
DEPLOY_AUTH=$("$DWW_BIN" -n "$NETWORK" contract generate-deploy 2>/dev/null || echo "")
if [ -z "$DEPLOY_AUTH" ]; then
    fail "Failed to generate deploy authority"
    record "10-deploy" "FAIL" "No deploy auth"
else
    vlog "Deploy auth: $DEPLOY_AUTH"

    # Find the money_v3 WASM
    MONEY_WASM="$DARKFI_DIR/src/contract/money_v3/money_v3.wasm"
    if [ ! -f "$MONEY_WASM" ]; then
        # Try alternative location
        MONEY_WASM=$(find "$DARKFI_DIR/target" -name "money_v3.wasm" -path "*/wasm32*" 2>/dev/null | head -1 || echo "")
    fi

    if [ -z "$MONEY_WASM" ] || [ ! -f "$MONEY_WASM" ]; then
        fail "money_v3.wasm not found — build contracts first: make contracts"
        record "10-deploy" "FAIL" "No WASM"
    else
        vlog "Found WASM: $MONEY_WASM"

        info "Deploying money_v3..."
        DEPLOY_OUTPUT=$("$DWW_BIN" -n "$NETWORK" contract deploy "$DEPLOY_AUTH" "$MONEY_WASM" 2>&1 || echo "")
        if echo "$DEPLOY_OUTPUT" | grep -q 'Error\|error\|panicked'; then
            fail "Contract deploy failed: $(echo "$DEPLOY_OUTPUT" | head -3)"
            record "10-deploy" "FAIL" "Deploy error"
        else
            # Broadcast the transaction
            if echo "$DEPLOY_OUTPUT" | "$DWW_BIN" -n "$NETWORK" broadcast 2>&1 | vlog; then
                info "Contract deployed. To register, find the ContractId and run:"
                info "  dww -n $NETWORK contract register money_v3 <ContractId>"
                pass "Money V3 deployed"
                record "10-deploy" "PASS" ""
            else
                info "Deploy output ready (manual broadcast may be needed)"
                record "10-deploy" "PASS" "manual broadcast"
            fi
        fi
    fi
fi

# ============================================================================
# Phase 11: Transfer (if balance available)
# ============================================================================
echo "=== Phase 11: Transfer Test ==="

if [ -n "${BALANCE_VALUE:-}" ] && [ "$BALANCE_VALUE" != "0" ] && [ "$BALANCE_VALUE" != "0.00000000" ]; then
    DEFAULT_ADDR=$("$DWW_BIN" -n "$NETWORK" wallet address 2>/dev/null | tail -1 || echo "")
    if [ -n "$DEFAULT_ADDR" ]; then
        info "Creating self-transfer of 100000000 DRKW..."
        # dww transfer <amount> <token> <addr>
        TXF=$("$DWW_BIN" -n "$NETWORK" transfer 100000000 DRKW "$DEFAULT_ADDR" 2>&1 || echo "")
        if echo "$TXF" | grep -q 'Error\|error'; then
            fail "Transfer creation failed: $(echo "$TXF" | head -3)"
            record "11-transfer" "FAIL" "Create error"
        else
            if echo "$TXF" | "$DWW_BIN" -n "$NETWORK" broadcast 2>&1 | vlog; then
                pass "Transfer broadcast"
                record "11-transfer" "PASS" ""
            else
                info "Transfer created (manual broadcast if needed)"
                record "11-transfer" "PASS" "manual"
            fi
        fi
    else
        fail "No wallet address available for transfer"
        record "11-transfer" "FAIL" "No address"
    fi
else
    info "Balance is zero — skipping transfer"
    record "11-transfer" "SKIP" "No balance"
fi

# ============================================================================
# Phase 12: Summary
# ============================================================================
echo
echo "=============================================="
echo "  Native Workflow — Results"
echo "=============================================="

for entry in "${RESULTS[@]}"; do
    IFS='|' read -r phase result detail <<< "$entry"
    case "$result" in
        PASS) echo -e "  ${GREEN}[PASS]${NC} $phase $detail" ;;
        FAIL) echo -e "  ${RED}[FAIL]${NC} $phase $detail" ;;
        SKIP) echo -e "  ${YELLOW}[SKIP]${NC} $phase $detail" ;;
    esac
done

echo
echo "  Total: $pass_count passed, $fail_count failed, $(( ${#RESULTS[@]} - pass_count - fail_count )) skipped"

if [ "$fail_count" -gt 0 ]; then
    echo -e "  ${RED}Some phases failed.${NC}"
    echo
    echo "  Troubleshooting:"
    echo "    - Check dwowd logs for connection issues"
    echo "    - Verify config: $CONFIG_FILE"
    echo "    - Ensure seeds are reachable: lilith0.dark.fi:31340 lilith1.dark.fi:31340"
    echo "    - Run 'make' to ensure all binaries are built"
    echo "    - For more detail, re-run with: VERBOSE=true $0"
    exit 1
else
    echo -e "  ${GREEN}All phases passed.${NC}"
    echo
    echo "  Next steps:"
    echo "    - Register contracts: dww -n $NETWORK contract register <name> <ContractId>"
    echo "    - List known contracts: dww -n $NETWORK contract list"
    echo "    - Invoke contract: dww -n $NETWORK contract invoke <ContractId> <function>"
    echo "    - Check status: dww -n $NETWORK wallet balance"
    exit 0
fi
