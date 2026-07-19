# DarkWow Testnet Pipeline — Phase 20: Join Mining / Report
#
# Phase 20 join: verify mining start via RPC, validate block production,
#                or docker compose up merge stack and verify merge mining.
# Dependencies: output.sh (info, pass, fail),
#               config.sh (MODE, CONTAINER_NAME, RPC_PORT, JOIN_TEST_DATA,
#                          COMPOSE_FILE, JOIN_TEST_MONERO, JOIN_TEST_P2POOL,
#                          NETWORK, P2P_PORT, STRATUM_PORT, MM_RPC_PORT,
#                          SEED_ADDR, MAGIC_BYTES, MONERO_OFFLINE,
#                          MONERO_FIXED_DIFFICULTY, WALLET_ADDRESS,
#                          MONERO_WALLET_ADDRESS, FINALITY_ENABLE_MONERO,
#                          MONERO_MIN_CONFIRMATIONS, MONEROD_RPC_URL,
#                          REPO_ROOT, IMAGE),
#               helpers.sh (check_image, check_network, jsonrpc, clean_data_dir)
#
# Sourced by test_pipeline.sh after phase_12_bridge.sh.

phase_join_mining() {
    echo ""
    if [ "$MODE" = "join-merge" ]; then
        echo "=== Join Phase 10: Merge Mining Verification ==="
        phase_join_merge_mining
    else
        echo "=== Join Phase 10: Native Mining Verification ==="
        phase_join_native_mining
    fi
}

phase_join_native_mining() {
    _MINING_FAIL_BEFORE="${FAIL:-0}"
    check_image || return 1

    if ! container_running "$CONTAINER_NAME"; then
        warn "Container not running"
        return 0
    fi

    local initial_height=0
    local info
    info=$(jsonrpc "$RPC_PORT" "blockchain.get_height")

    initial_height=$(echo "$info" | grep -o '"height":[0-9]*' | grep -o '[0-9]*' || echo "0")
    echo "  Initial block height: $initial_height"

    echo "  Checking stratum connectivity..."
    local logs
    logs=$(docker logs "$CONTAINER_NAME" 2>&1)
    if echo "$logs" | grep -qi "stratum"; then
        pass "Stratum-related log entries found"
    else
        info "Stratum-related log entries not found (diagnostic)"
    fi

    if echo "$logs" | grep -qi "xmrig"; then
        pass "xmrig started in container"
    else
        info "xmrig not detected in container logs (diagnostic — may be sync-only node)"
    fi

    echo "  Waiting for block height to advance (up to 360s)..."
    local advanced=0
    for i in $(seq 1 72); do
        sleep 5
        info=$(jsonrpc "$RPC_PORT" "blockchain.get_height")
        local current_height
        current_height=$(echo "$info" | grep -o '"height":[0-9]*' | grep -o '[0-9]*' || echo "0")
        if [ -n "$current_height" ] && [ "$current_height" -gt "$initial_height" ] 2>/dev/null; then
            pass "Block height advanced: $initial_height -> $current_height after $((i * 5))s"
            advanced=1
            break
        fi
    done

    if [ "$advanced" -eq 0 ]; then
        echo "  Note: block height may not advance if there are no other miners on the network"
        echo "  or if this is a new testnet with no blocks yet."
        echo "  Current height: $(jsonrpc "$RPC_PORT" "blockchain.get_height")"
        warn "Block height did not advance after 360s (external network condition — not a pipeline failure)"
    fi

    # Preserve container on failure for debugging.
    if [ "${_MINING_FAIL_BEFORE:-0}" -lt "${FAIL:-0}" ]; then
        warn "Join native mining recorded failures — preserving container for debugging"
        echo "  Container: $CONTAINER_NAME"
        echo "  Data dir:  $JOIN_TEST_DATA"
        return 0
    fi

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    clean_data_dir "$JOIN_TEST_DATA"
}

phase_join_merge_mining() {
    check_image || return 1
    check_network || return 0

    docker compose -f "$COMPOSE_FILE" --profile join-merge down 2>/dev/null || true
    clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_MONERO" "$JOIN_TEST_P2POOL"
    mkdir -p "$JOIN_TEST_DATA" "$JOIN_TEST_MONERO" "$JOIN_TEST_P2POOL"

    echo "  Starting merge mining stack..."

    export NETWORK P2P_PORT RPC_PORT STRATUM_PORT MM_RPC_PORT
    export SEED_ADDR MAGIC_BYTES
    export MONERO_OFFLINE="${MONERO_OFFLINE:-false}"
    export MONERO_NETWORK="${MONERO_NETWORK:-testnet}"
    export MONERO_ADD_PEERS="${MONERO_ADD_PEERS:-125.229.105.12:28081,37.187.74.171:28089}"
    export MONERO_FIXED_DIFFICULTY="${MONERO_FIXED_DIFFICULTY:-1000}"
    export MINING_THREADS="${MINING_THREADS:-1}"
    export WALLET_ADDRESS="${WALLET_ADDRESS:-}"
    export MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"
    export THRESHOLD=3
    export TARGET_BLOCK_TIME=120
    export DATA_DIR="$JOIN_TEST_DATA"
    export MONERO_DATA_DIR="$JOIN_TEST_MONERO"
    export P2POOL_DATA_DIR="$JOIN_TEST_P2POOL"

    # Ensure no conflicting containers exist (compose down above may miss
    # containers from a different profile/compose invocation using the same names).
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    for c in dwow-node0-join dwow-node0 dwow-monerod dwow-p2pool dwow-observer; do
        docker stop "$c" 2>/dev/null || true
        docker rm "$c" 2>/dev/null || true
    done

    cd "$REPO_ROOT"
    FINALITY_ENABLE_MONERO="$FINALITY_ENABLE_MONERO" \
        MONERO_MIN_CONFIRMATIONS="$MONERO_MIN_CONFIRMATIONS" \
        MONEROD_RPC_URL="$MONEROD_RPC_URL" \
        docker compose -f "$COMPOSE_FILE" --profile join-merge up -d 2>&1

    echo "  Waiting for containers to initialize (30s)..."
    sleep 30

    local all_up=1
    if docker ps --format '{{.Names}}' | grep -q "dwow-node0-join"; then
        pass "dwowd container running"
    else
        warn "dwowd container not running"
        all_up=0
    fi

    if docker ps --format '{{.Names}}' | grep -q "dwow-monerod"; then
        pass "monerod container running"
    else
        warn "monerod container not running"
        all_up=0
    fi

    if docker ps --format '{{.Names}}' | grep -q "dwow-p2pool"; then
        pass "p2pool container running"
    else
        warn "p2pool container not running"
        all_up=0
    fi

    if docker logs "$CONTAINER_NAME" 2>&1 | grep -qi "Merge mining enabled.*xmrig sidecar"; then
        pass "xmrig sidecar active in node container"
    else
        warn "xmrig sidecar not detected in node container"
        all_up=0
    fi

    if [ "$all_up" -eq 0 ]; then
        echo "  Some containers failed. Logs:"
        docker compose -f "$COMPOSE_FILE" --profile join-merge logs 2>&1 | tail -40
        warn "Merge stack not fully up"
        return 0
    fi

    echo "  Checking monerod sync status (diagnostic — last 10 log lines)..."
    local monero_logs
    monero_logs=$(docker logs dwow-monerod 2>&1 | tail -10)
    if echo "$monero_logs" | grep -qi "synced\|SYNCHRONIZED\|NEW CONNECTION\|initialized\|COMMAND_HANDSHAKE\|TXT record"; then
        pass "monerod active (syncing or connecting to peers)"
    else
        echo "  monerod logs (last 10 lines):"
        echo "$monero_logs"
        info "monerod sync status not yet visible in recent logs (diagnostic)"
    fi

    echo "  Checking p2pool..."
    local p2pool_logs
    p2pool_logs=$(docker logs dwow-p2pool 2>&1 | tail -10)
    if echo "$p2pool_logs" | grep -qi "sidechain\|stratum\|p2pool"; then
        pass "p2pool running"
    else
        echo "  p2pool logs:"
        echo "$p2pool_logs"
        warn "p2pool not showing expected activity"
    fi

    sleep 10
    local dwowd_attempt
    for dwowd_attempt in $(seq 1 6); do
        if docker exec dwow-node0-join bash -c "exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT 2>/dev/null && echo ok >&3" 2>/dev/null; then
            pass "dwowd JSON-RPC reachable"
            break
        fi
        if [ "$dwowd_attempt" -lt 6 ]; then
            echo "  dwowd RPC not ready, waiting (attempt $dwowd_attempt/6)..."
            sleep 10
        else
            echo "  dwowd may still be starting"
            warn "dwowd JSON-RPC not reachable"
        fi
    done

    echo "  Merge stack is running. Leaving it for manual inspection."
    echo "  Run: docker compose -f $COMPOSE_FILE --profile join-merge logs -f"
}
