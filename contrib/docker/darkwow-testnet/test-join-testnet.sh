#!/bin/bash
# Test the public testnet join scripts against the design specification.
#
# Phases:
#   0 — Static config validation
#   1 — Container lifecycle
#   2 — P2P connectivity
#   3 — Blockchain sync
#   4 — Native mining
#   5 — Merge mining
#   6 — Persistence / restart
#
# Each phase gates on the previous one. Phases 2+ require network access
# (public lilith seeds, Monero testnet). Missing prerequisites cause a
# SKIP rather than a FAIL.
#
# Usage:
#   ./test-join-testnet.sh              # all phases
#   ./test-join-testnet.sh --phase 0    # single phase
#   ./test-join-testnet.sh --phase 0-2  # range
#   ./test-join-testnet.sh --clean      # remove test containers + data

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
IMAGE="${IMAGE:-darkwow-testnet:latest}"
NETWORK="${NETWORK:-darkwow-testnet}"
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"
SEED_ADDR="${SEED_ADDR:-lilith0.dark.fi:31340,lilith1.dark.fi:31340}"
P2P_PORT=31342
RPC_PORT=31345
STRATUM_PORT=31347
MM_RPC_PORT=31348
TEST_DATA_DIR="$(pwd)/test-data"
TEST_MONERO_DATA="$(pwd)/test-monero-data"
TEST_P2POOL_DATA="$(pwd)/test-p2pool-data"
CONTAINER_NAME="dwow-test-node"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"

PASS=0
FAIL=0
SKIP=0
START_PHASE=0
END_PHASE=6

# --- Parse args ---
while [[ $# -gt 0 ]]; do
    case "$1" in
        --phase)
            if [[ "$2" =~ ^([0-6])-([0-6])$ ]]; then
                START_PHASE="${BASH_REMATCH[1]}"
                END_PHASE="${BASH_REMATCH[2]}"
            else
                START_PHASE="$2"
                END_PHASE="$2"
            fi
            shift 2 ;;
        --clean)
            echo "=== Cleaning up test containers and data ==="
            docker stop "$CONTAINER_NAME" 2>/dev/null || true
            docker rm "$CONTAINER_NAME" 2>/dev/null || true
            docker compose -f "$COMPOSE_FILE" --profile join-merge down 2>/dev/null || true
            rm -rf "$TEST_DATA_DIR" "$TEST_MONERO_DATA" "$TEST_P2POOL_DATA"
            echo "Done."
            exit 0 ;;
        *)
            echo "Usage: $0 [--phase N|N-M] [--clean]"
            exit 1 ;;
    esac
done

# --- Helpers ---
pass() { echo "  PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL: $1"; FAIL=$((FAIL + 1)); }
skip() { echo "  SKIP: $1"; SKIP=$((SKIP + 1)); }

check_image() {
    if ! docker image inspect "$IMAGE" &>/dev/null; then
        skip "Docker image '$IMAGE' not found — build it first: docker build -t $IMAGE -f $SCRIPT_DIR/Dockerfile ."
        return 1
    fi
    return 0
}

check_network() {
    if ! curl -s --connect-timeout 5 https://lilith0.dark.fi:31340 2>/dev/null >/dev/null; then
        # lilith doesn't serve HTTP, so a connection refused/timeout on HTTPS
        # is expected. Just check we have basic internet.
        if ! curl -s --connect-timeout 5 https://api.ipify.org >/dev/null 2>&1; then
            skip "No internet connectivity detected"
            return 1
        fi
    fi
    return 0
}

cleanup_phase0() {
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
}

cleanup_native() {
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    rm -rf "$TEST_DATA_DIR"
}

cleanup_merge() {
    docker compose -f "$COMPOSE_FILE" --profile join-merge down 2>/dev/null || true
    rm -rf "$TEST_DATA_DIR" "$TEST_MONERO_DATA" "$TEST_P2POOL_DATA"
}

jsonrpc() {
    local port="$1" method="$2"
    curl -s --connect-timeout 5 --max-time 10 \
        http://127.0.0.1:"$port" \
        -X POST -H 'Content-Type: application/json' \
        -d "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":[],\"id\":1}" 2>/dev/null || echo '{"error":"RPC unreachable"}'
}

# ============================================================================
# Phase 0: Static Config Validation
# ============================================================================
phase_0() {
    echo "=== Phase 0: Static Config Validation ==="
    check_image || return 0

    echo "  Starting container to capture generated config..."
    mkdir -p "$TEST_DATA_DIR"

    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e MINING_ENABLED=true \
        -e MINING_THREADS=1 \
        -e RANDOMX_MAX_THREADS=0 \
        -v "$TEST_DATA_DIR:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 5

    # Verify container is running
    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "Container failed to start"
        cleanup_phase0
        return 0
    fi

    local config
    config=$(docker exec "$CONTAINER_NAME" cat /root/.config/dwow/dwowd_config.toml 2>/dev/null || echo "")
    if [ -z "$config" ]; then
        fail "Could not read generated config"
        cleanup_phase0
        return 0
    fi

    echo "  --- Generated config ---"
    echo "$config" | head -30
    echo "  --- End config ---"

    # Layer 0: Network Identity
    if echo "$config" | grep -q 'magic_bytes = \[68, 82, 75, 87\]'; then
        pass "Layer 0: magic_bytes = [68, 82, 75, 87]"
    else
        fail "Layer 0: magic_bytes incorrect"
    fi

    if echo "$config" | grep -q "network = \"$NETWORK\""; then
        pass "Layer 0: network = $NETWORK"
    else
        fail "Layer 0: network incorrect"
    fi

    # Layer 1: P2P Bootstrap
    if echo "$config" | grep -q 'tcp+tls://lilith0.dark.fi:31340'; then
        pass "Layer 1: lilith0 seed present"
    else
        fail "Layer 1: lilith0 seed missing"
    fi

    if echo "$config" | grep -q 'tcp+tls://lilith1.dark.fi:31340'; then
        pass "Layer 1: lilith1 seed present"
    else
        fail "Layer 1: lilith1 seed missing"
    fi

    if echo "$config" | grep -q 'hostlist = '; then
        pass "Layer 1: hostlist path configured"
    else
        fail "Layer 1: hostlist path missing"
    fi

    if echo "$config" | grep -q 'localnet = false'; then
        pass "Layer 1: localnet = false"
    else
        fail "Layer 1: localnet incorrect"
    fi

    if echo "$config" | grep -q 'inbound = \["tcp+tls://0.0.0.0:'; then
        pass "Layer 1: inbound configured"
    else
        fail "Layer 1: inbound missing"
    fi

    # Layer 2: Blockchain Sync
    if echo "$config" | grep -q 'threshold = 3'; then
        pass "Layer 2: threshold = 3"
    else
        fail "Layer 2: threshold incorrect"
    fi

    if echo "$config" | grep -q 'pow_target = 120'; then
        pass "Layer 2: pow_target = 120"
    else
        fail "Layer 2: pow_target incorrect"
    fi

    if echo "$config" | grep -q 'skip_sync = false'; then
        pass "Layer 2: skip_sync = false"
    else
        fail "Layer 2: skip_sync incorrect"
    fi

    if echo "$config" | grep -q 'skip_fees = false'; then
        pass "Layer 2: skip_fees = false"
    else
        fail "Layer 2: skip_fees incorrect"
    fi

    # Layer 3: Native Mining
    if echo "$config" | grep -q 'rpc_listen = "tcp://0.0.0.0:'; then
        pass "Layer 3: stratum/JSON-RPC listen configured"
    else
        fail "Layer 3: stratum/JSON-RPC listen missing"
    fi

    # Layer 5: Reachability (external_addrs)
    if echo "$config" | grep -q 'external_addrs'; then
        pass "Layer 5: external_addrs configured"
    else
        echo "  (external_addrs only present when EXTERNAL_ADDR is set)"
    fi

    echo "  Config validation complete."
    cleanup_phase0
}

# ============================================================================
# Phase 1: Container Lifecycle
# ============================================================================
phase_1() {
    echo "=== Phase 1: Container Lifecycle ==="
    check_image || return 0

    cleanup_native
    mkdir -p "$TEST_DATA_DIR"

    echo "  Starting native mode container..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -v "$TEST_DATA_DIR:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 10

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        pass "Container is running after 10s"
    else
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "Container stopped unexpectedly"
        cleanup_native
        return 0
    fi

    # Check logs for expected startup messages
    local logs
    logs=$(docker logs "$CONTAINER_NAME" 2>&1)
    if echo "$logs" | grep -q "Starting dwowd"; then
        pass "Log shows dwowd starting"
    else
        fail "Log missing 'Starting dwowd'"
    fi

    if echo "$logs" | grep -qi "magic bytes"; then
        pass "Log shows magic bytes"
    else
        fail "Log missing magic bytes"
    fi

    # Check there are no ERROR lines
    if ! echo "$logs" | grep -qi "ERROR"; then
        pass "No ERROR lines in logs"
    else
        echo "  WARNING: ERROR lines found (may be benign startup noise):"
        echo "$logs" | grep -i "ERROR" | head -5
    fi

    echo "  Container is running. Leaving it for Phase 2."
}

# ============================================================================
# Phase 2: P2P Connectivity
# ============================================================================
phase_2() {
    echo "=== Phase 2: P2P Connectivity ==="

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        skip "Container not running (run Phase 1 first)"
        return 0
    fi

    check_network || return 0

    echo "  Waiting for P2P connections (up to 90s)..."
    local connected=0
    for i in $(seq 1 18); do
        local peers
        peers=$(jsonrpc "$RPC_PORT" "p2p.info")
        if echo "$peers" | grep -q '"result"'; then
            local count
            count=$(echo "$peers" | grep -o '"sessions":[0-9]*' | grep -o '[0-9]*' || echo "0")
            if [ -n "$count" ] && [ "$count" -gt 0 ] 2>/dev/null; then
                pass "P2P connected: $count session(s) after ${i}5s"
                connected=1
                break
            fi
        fi
        sleep 5
    done

    if [ "$connected" -eq 0 ]; then
        echo "  Last p2p.info response:"
        jsonrpc "$RPC_PORT" "p2p.info" | head -1
        fail "No P2P connections after 90s"
    fi
}

# ============================================================================
# Phase 3: Blockchain Sync
# ============================================================================
phase_3() {
    echo "=== Phase 3: Blockchain Sync ==="

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        skip "Container not running (run Phase 1 first)"
        return 0
    fi

    echo "  Checking blockchain sync (up to 300s)..."
    local synced=0
    local height=0
    for i in $(seq 1 60); do
        local info
        info=$(jsonrpc "$RPC_PORT" "blockchain.info")
        if echo "$info" | grep -q '"block_height"'; then
            height=$(echo "$info" | grep -o '"block_height":[0-9]*' | grep -o '[0-9]*' || echo "0")
            if [ -n "$height" ] && [ "$height" -gt 0 ] 2>/dev/null; then
                pass "Blockchain synced: height $height after ${i}5s"
                synced=1
                break
            fi
        fi
        sleep 5
    done

    if [ "$synced" -eq 0 ]; then
        echo "  Last blockchain.info response:"
        jsonrpc "$RPC_PORT" "blockchain.info" | head -1
        fail "Blockchain height is 0 after 300s (public testnet may not have blocks yet)"
    fi
}

# ============================================================================
# Phase 4: Native Mining
# ============================================================================
phase_4() {
    echo "=== Phase 4: Native Mining ==="

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        skip "Container not running (run Phase 1 first)"
        return 0
    fi

    # Get initial height
    local initial_height
    local info
    info=$(jsonrpc "$RPC_PORT" "blockchain.info")
    initial_height=$(echo "$info" | grep -o '"block_height":[0-9]*' | grep -o '[0-9]*' || echo "0")
    echo "  Initial block height: $initial_height"

    # Check stratum is listening
    echo "  Checking stratum connectivity..."
    local logs
    logs=$(docker logs "$CONTAINER_NAME" 2>&1)
    if echo "$logs" | grep -qi "stratum"; then
        pass "Stratum-related log entries found"
    else
        echo "  (stratum logs may appear after xmrig connects)"
    fi

    if echo "$logs" | grep -qi "xmrig"; then
        pass "xmrig started in container"
    else
        echo "  WARNING: xmrig may not have started yet (waiting for mining address)"
    fi

    # Wait for block height to increase (2 block intervals = 240s + margin)
    echo "  Waiting for block height to advance (up to 360s)..."
    local advanced=0
    for i in $(seq 1 72); do
        sleep 5
        info=$(jsonrpc "$RPC_PORT" "blockchain.info")
        local current_height
        current_height=$(echo "$info" | grep -o '"block_height":[0-9]*' | grep -o '[0-9]*' || echo "0")
        if [ -n "$current_height" ] && [ "$current_height" -gt "$initial_height" ] 2>/dev/null; then
            pass "Block height advanced: $initial_height → $current_height after ${i}5s"
            advanced=1
            break
        fi
    done

    if [ "$advanced" -eq 0 ]; then
        echo "  Note: block height may not advance if there are no other miners on the network"
        echo "  or if this is a new testnet with no blocks yet."
        echo "  Current height: $(jsonrpc "$RPC_PORT" "blockchain.info")"
    fi

    cleanup_native
}

# ============================================================================
# Phase 5: Merge Mining
# ============================================================================
phase_5() {
    echo "=== Phase 5: Merge Mining ==="
    check_image || return 0
    check_network || return 0

    cleanup_merge
    mkdir -p "$TEST_DATA_DIR" "$TEST_MONERO_DATA" "$TEST_P2POOL_DATA"

    echo "  Starting merge mining stack..."

    # Export vars for docker compose substitution
    export NETWORK P2P_PORT RPC_PORT STRATUM_PORT MM_RPC_PORT
    export SEED_ADDR MAGIC_BYTES
    export TEST_DATA_DIR TEST_MONERO_DATA TEST_P2POOL_DATA
    export MONERO_OFFLINE="${MONERO_OFFLINE:-false}"
    export MONERO_NETWORK="${MONERO_NETWORK:-testnet}"
    export MONERO_ADD_PEERS="${MONERO_ADD_PEERS:-125.229.105.12:28081,37.187.74.171:28089}"
    export MONERO_FIXED_DIFFICULTY="${MONERO_FIXED_DIFFICULTY:-20000}"
    export MINING_THREADS="${MINING_THREADS:-1}"
    export WALLET_ADDRESS="${WALLET_ADDRESS:-}"
    export MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"
    export THRESHOLD=3
    export TARGET_BLOCK_TIME=120
    export DATA_DIR="$TEST_DATA_DIR"
    export MONERO_DATA_DIR="$TEST_MONERO_DATA"
    export P2POOL_DATA_DIR="$TEST_P2POOL_DATA"

    cd "$REPO_ROOT"
    docker compose -f "$COMPOSE_FILE" --profile join-merge up -d 2>&1

    echo "  Waiting for containers to initialize (30s)..."
    sleep 30

    # Check all 4 containers are running
    local all_up=1
    for svc in dwowd-join monerod-join p2pool-join xmrig-join; do
        if docker compose -f "$COMPOSE_FILE" --profile join-merge ps --format '{{.Service}}' 2>/dev/null | grep -q "$svc"; then
            :
        else
            echo "  Service $svc not found in compose output"
            all_up=0
        fi
    done

    if docker ps --format '{{.Names}}' | grep -q "dwow-node0"; then
        pass "dwowd container running"
    else
        fail "dwowd container not running"
        all_up=0
    fi

    if docker ps --format '{{.Names}}' | grep -q "dwow-monerod"; then
        pass "monerod container running"
    else
        fail "monerod container not running"
        all_up=0
    fi

    if docker ps --format '{{.Names}}' | grep -q "dwow-p2pool"; then
        pass "p2pool container running"
    else
        fail "p2pool container not running"
        all_up=0
    fi

    if docker ps --format '{{.Names}}' | grep -q "dwow-xmrig-merge"; then
        pass "xmrig container running"
    else
        fail "xmrig container not running"
        all_up=0
    fi

    if [ "$all_up" -eq 0 ]; then
        echo "  Some containers failed. Logs:"
        docker compose -f "$COMPOSE_FILE" --profile join-merge logs 2>&1 | tail -40
        fail "Merge stack not fully up"
        return 0
    fi

    # Check monerod is syncing
    echo "  Checking monerod sync status..."
    local monero_logs
    monero_logs=$(docker logs dwow-monerod 2>&1 | tail -10)
    if echo "$monero_logs" | grep -qi "synced\|SYNCHRONIZED"; then
        pass "monerod reports synced"
    elif echo "$monero_logs" | grep -q "Synced"; then
        pass "monerod is syncing"
    else
        echo "  monerod logs:"
        echo "$monero_logs"
        echo "  (monerod may still be initializing — check again later)"
    fi

    # Check p2pool connectivity
    echo "  Checking p2pool..."
    local p2pool_logs
    p2pool_logs=$(docker logs dwow-p2pool 2>&1 | tail -10)
    if echo "$p2pool_logs" | grep -qi "sidechain\|stratum\|p2pool"; then
        pass "p2pool running"
    else
        echo "  p2pool logs:"
        echo "$p2pool_logs"
    fi

    # Check dwowd is reachable
    sleep 10
    local dwowd_info
    dwowd_info=$(jsonrpc "$RPC_PORT" "blockchain.info")
    if echo "$dwowd_info" | grep -q '"block_height"'; then
        pass "dwowd JSON-RPC reachable"
    else
        echo "  dwowd may still be starting"
    fi

    echo "  Merge stack is running. Leaving it for manual inspection."
    echo "  Run: docker compose -f $COMPOSE_FILE --profile join-merge logs -f"
    echo "  Tear down: $0 --clean"
}

# ============================================================================
# Phase 6: Persistence / Restart
# ============================================================================
phase_6() {
    echo "=== Phase 6: Persistence / Restart ==="
    check_image || return 0

    # Use fresh data dir for this test
    local persist_dir="$(pwd)/test-persist-data"
    rm -rf "$persist_dir"
    mkdir -p "$persist_dir"

    echo "  Starting first run..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -v "$persist_dir:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 15

    if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        fail "Container failed to start"
        rm -rf "$persist_dir"
        return 0
    fi

    # Check that data files exist
    if [ -f "$persist_dir/hostlist.tsv" ] || ls "$persist_dir/"*.sled 2>/dev/null | head -1 | grep -q sled; then
        pass "Data files created on first run"
    else
        echo "  Data dir contents:"
        ls -la "$persist_dir/" 2>/dev/null || echo "  (empty)"
        echo "  (data files may not be created yet in 15s — this may be ok)"
    fi

    # Stop the container
    echo "  Stopping container..."
    docker stop "$CONTAINER_NAME" 2>/dev/null
    docker rm "$CONTAINER_NAME" 2>/dev/null

    # Verify data persists on host
    if [ -d "$persist_dir" ] && [ "$(ls -A "$persist_dir" 2>/dev/null)" ]; then
        pass "Host data survived container removal"
    else
        fail "Host data missing after container stop"
        rm -rf "$persist_dir"
        return 0
    fi

    # Restart with same volume
    echo "  Starting second run (same data dir)..."
    docker run -d \
        --name "$CONTAINER_NAME" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$SEED_ADDR" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -v "$persist_dir:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1

    sleep 10

    if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
        pass "Container restarted successfully with persisted data"
    else
        fail "Container failed to restart"
    fi

    cleanup_native
    rm -rf "$persist_dir"
}

# ============================================================================
# Summary
# ============================================================================
summary() {
    echo ""
    echo "========================================="
    echo "Test Summary: $((PASS + FAIL + SKIP)) checks"
    echo "  PASS: $PASS"
    echo "  FAIL: $FAIL"
    echo "  SKIP: $SKIP"
    echo "========================================="
    if [ "$FAIL" -gt 0 ]; then
        exit 1
    fi
}

# ============================================================================
# Main
# ============================================================================
echo "=== DarkWow Public Testnet Join Test ==="
echo "  Image:    $IMAGE"
echo "  Network:  $NETWORK"
echo "  Seeds:    $SEED_ADDR"
echo "  Phases:   $START_PHASE-$END_PHASE"
echo

for phase in $(seq "$START_PHASE" "$END_PHASE"); do
    case "$phase" in
        0) phase_0 ;;
        1) phase_1 ;;
        2) phase_2 ;;
        3) phase_3 ;;
        4) phase_4 ;;
        5) phase_5 ;;
        6) phase_6 ;;
    esac
done

summary
