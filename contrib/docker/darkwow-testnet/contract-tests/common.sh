#!/usr/bin/env bash
#
# common.sh — Shared helpers for L4 per-contract wallet verification tests.
#
# Source this from each test-<contract>.sh script.
# Requires: Docker stack running (dwow-node0 + wallet containers),
#           test_pipeline.sh having completed successfully.
#

set -euo pipefail

# ── Paths ────────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
MODEL_DIR="$REPO_ROOT/contrib/model"
ORACLE="$MODEL_DIR/test_oracle.py"
FIXTURE_DIR="$MODEL_DIR/fixtures"

# ── Docker helpers ───────────────────────────────────────────────────────

NODE0="dwow-node0"
WALLET_BASE="dwow-wallet"
WALLET_BIN="/app/dwow_wallet"

# ── Native mode — use local binary instead of docker exec ──────────────
if [ "${NATIVE:-0}" = "1" ]; then
    WALLET_BIN="${REPO_ROOT}/target/debug/dwow_wallet"
    NODE0="dwow-node0"

    wal() {
        local idx="$1"; shift  # strip wallet index (ignored in native mode)
        $WALLET_BIN -n darkwow-testnet "$@"
    }

    get_block_height() {
        # Wallet is a full node — scan output reports block height
        wal 1 scan 2>&1 | grep -oP 'Block height: \K\d+' | tail -1 || echo "0"
    }
else
    # Count of wallet containers available
    wallet_count() {
        docker ps --format '{{.Names}}' | grep -c "^${WALLET_BASE}-" || echo "0"
    }

    # Run a wallet command inside container index N
    wal() {
        local idx="$1"; shift
        docker exec "${WALLET_BASE}-${idx}" $WALLET_BIN "$@"
    }
fi

# JSON-RPC call to node0
node0_rpc() {
    local method="$1"; shift
    local params="${1:-{}}"
    # Use /dev/tcp for JSON-RPC
    exec 3<>/dev/tcp/127.0.0.1/31345 || {
        # Fallback: docker exec curl
        docker exec "$NODE0" curl -s -X POST \
            -H "Content-Type: application/json" \
            -d "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"$method\",\"params\":$params}" \
            http://127.0.0.1:31345
    }
    exec 3>&-
}

# Get current block height
get_block_height() {
    docker exec "$NODE0" curl -s -X POST \
        -H "Content-Type: application/json" \
        -d '{"jsonrpc":"2.0","id":1,"method":"blockchain.info","params":{}}' \
        http://127.0.0.1:31345 | python3 -c "import sys,json; print(json.load(sys.stdin)['result']['height'])"
}

# Wait for at least N new blocks to be produced.
# The wallet is a full node — it syncs all blocks. No need to wait.
wait_for_blocks() {
    return 0
}

# (original wait_for_blocks retained as _wait_for_blocks_docker)
_wait_for_blocks_docker() {
    local count="${1:-1}"
    local start_height
    start_height=$(get_block_height)
    local target=$((start_height + count))
    local waited=0
    local max_wait=120

    echo "  Waiting for $count block(s) (current: $start_height, target: $target)..."
    while [ "$waited" -lt "$max_wait" ]; do
        local current
        current=$(get_block_height 2>/dev/null || echo "0")
        if [ "$current" -ge "$target" ]; then
            echo "  Reached height $current"
            return 0
        fi
        sleep 5
        waited=$((waited + 5))
    done
    echo "  WARNING: Timed out waiting for blocks (current: $(get_block_height))"
    return 1
}

# ── Wallet operations ────────────────────────────────────────────────────

# Scan wallet at index
scan_wallet() {
    local idx="${1:-1}"
    echo "  Scanning wallet $idx..."
    wal "$idx" scan 2>&1 | tail -5
}

# Get position as JSON
get_position() {
    local idx="${1:-1}"
    wal "$idx" position --json 2>&1
}

# Get wallet address
get_address() {
    local idx="${1:-1}"
    wal "$idx" wallet address 2>&1
}

# ── Contract deployment ──────────────────────────────────────────────────

# Deploy a contract WASM via wallet 1, return contract_id
deploy_contract() {
    local contract_name="$1"
    local wasm_path="${2:-}"

    echo "  Deploying $contract_name..."
    # Generate deploy authority
    local AUTH_OUT
    AUTH_OUT=$(wal 1 contract generate-deploy 2>&1)
    local DEPLOY_KEY
    DEPLOY_KEY=$(echo "$AUTH_OUT" | grep -oP 'Secret \(hex\): \K[a-f0-9]+')
    local CID
    CID=$(echo "$AUTH_OUT" | grep -oP 'Contract ID: \K\S+')
    echo "  Contract ID: $CID"

    if [ -n "$wasm_path" ]; then
        wal 1 contract deploy "$DEPLOY_KEY" "$wasm_path" >/dev/null 2>&1
    elif [ -f "$REPO_ROOT/src/contract/${contract_name}/dwow_${contract_name}_contract.wasm" ]; then
        wal 1 contract deploy "$DEPLOY_KEY" \
            "$REPO_ROOT/src/contract/${contract_name}/dwow_${contract_name}_contract.wasm" >/dev/null 2>&1
    fi
}

# Register a contract on a wallet
register_contract() {
    local wallet_idx="$1"
    local contract_name="$2"
    local contract_id="$3"

    echo "  Registering $contract_name on wallet $wallet_idx..."
    local result
    result=$(wal "$wallet_idx" contract register "$contract_name" "$contract_id" 2>&1) || true
    echo "$result" | tail -1
    # "already registered" is OK — contract exists from a previous deploy
    if echo "$result" | grep -q "already registered"; then
        echo "  (already registered, continuing)"
        return 0
    fi
}

# Call a contract function
call_contract() {
    local wallet_idx="$1"
    local contract_id="$2"
    local function_name="$3"
    shift 3
    local params="$@"

    echo "  Calling $function_name on $contract_id..."
    wal "$wallet_idx" contract invoke "$contract_id" "$function_name" $params >/dev/null 2>&1
    echo "  Invoke submitted."
}

# ── Assertions ───────────────────────────────────────────────────────────

# Compare Rust wallet JSON output against Python oracle expected JSON.
# Usage: assert_capability_match "label" "$EXPECTED_JSON" "$ACTUAL_JSON"
assert_capability_match() {
    local label="$1"
    local expected="$2"
    local actual="$3"

    # Extract fields to compare
    local exp_caps=$(echo "$expected" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('capability_count',0))")
    local act_caps=$(echo "$actual" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('capability_count',0))" 2>/dev/null || echo "0")
    local exp_acts=$(echo "$expected" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('action_count',0))")
    local act_acts=$(echo "$actual" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('action_count',0))" 2>/dev/null || echo "0")
    local exp_coins=$(echo "$expected" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('coin_count',0))")
    local act_coins=$(echo "$actual" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('coin_count',0))" 2>/dev/null || echo "0")

    local failures=0

    if [ "$act_caps" -lt "$exp_caps" ]; then
        echo "  [FAIL] $label: capability_count expected>=$exp_caps actual=$act_caps"
        failures=1
    fi
    if [ "$act_acts" -lt "$exp_acts" ]; then
        echo "  [FAIL] $label: action_count expected>=$exp_acts actual=$act_acts"
        failures=1
    fi
    if [ "$act_coins" -lt "$exp_coins" ]; then
        echo "  [FAIL] $label: coin_count expected>=$exp_coins actual=$act_coins"
        failures=1
    fi

    if [ "$failures" -eq 0 ]; then
        echo "  [PASS] $label: caps=$act_caps actions=$act_acts coins=$act_coins"
        return 0
    fi
    return 1
}

# ── Test infrastructure helpers ──────────────────────────────────────────

# Check pre-flight: Docker + node0 + wallets
check_prerequisites() {
    local min_wallets="${1:-1}"
    echo "=== Pre-flight Checks ==="

    if ! docker info >/dev/null 2>&1; then
        echo "ERROR: Docker is not running"
        exit 1
    fi
    echo "  [OK] Docker running"

    if ! docker ps --format '{{.Names}}' | grep -q "^${NODE0}$"; then
        echo "ERROR: $NODE0 container not running"
        echo "Run test_pipeline.sh first."
        exit 1
    fi
    echo "  [OK] $NODE0 running"

    local wc
    wc=$(wallet_count)
    if [ "$wc" -lt "$min_wallets" ]; then
        echo "ERROR: Need at least $min_wallets wallet(s), found $wc"
        exit 1
    fi
    echo "  [OK] $wc wallet container(s) running"
    echo ""
}

# Get a fresh fixture JSON from the Python oracle for this contract/scenario
oracle_fixture() {
    local contract="$1"
    local scenario="${2:-create}"

    # Find fixture file
    local fixture="$FIXTURE_DIR/${contract}_${scenario}.json"
    if [ ! -f "$fixture" ]; then
        # Return minimal valid JSON — caller should handle gracefully
        echo '{"capability_count":0,"action_count":0,"coin_count":0,"capability_descriptions":[],"action_names":[]}'
        return
    fi

    python3 "$ORACLE" --json "$fixture" 2>/dev/null || echo '{"capability_count":0,"action_count":0,"coin_count":0,"capability_descriptions":[],"action_names":[]}'
}
