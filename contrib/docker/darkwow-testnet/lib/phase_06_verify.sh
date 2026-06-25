# DarkWow Testnet Pipeline — Phase 6: Verify Containers / Join Lifecycle
#
# Phase 6 local: docker ps check all expected containers.
# Phase 6 join: docker run container, verify startup log messages.
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NATIVE_NODES, WITH_WALLET, IMAGE,
#                          JOIN_TEST_DATA, CONTAINER_NAME, NETWORK, P2P_PORT,
#                          RPC_PORT, STRATUM_PORT, SEED_ADDR, MAGIC_BYTES,
#                          FINALITY_MODE, FINALITY_CARIBINA_ENABLED),
#               helpers.sh (check_image, clean_data_dir)
#
# Sourced by test_pipeline.sh after phase_05_start.sh.

phase_verify() {
    info "Phase 6: Verifying containers..."

    # Pre-flight: Docker daemon must be reachable for container checks
    docker info >/dev/null 2>&1 || { fail "Docker daemon unavailable — cannot verify containers"; return 1; }

    if [ "$MODE" = "merge" ]; then
        EXPECTED=(dwow-lilith dwow-node0 dwow-node1 dwow-node2 dwow-monerod)
    elif [ "$MODE" = "bridge" ]; then
        EXPECTED=(dwow-lilith dwow-node0 dwow-node1 dwow-bridge-node)
    else
        # Native mode: expected containers based on --nodes
        if [ "$NATIVE_NODES" = "1" ]; then
            EXPECTED=(dwow-node0)
        elif [ "$NATIVE_NODES" = "5" ]; then
            EXPECTED=(dwow-lilith dwow-node0 dwow-node1 dwow-node2 dwow-node3 dwow-node4)
        else
            EXPECTED=(dwow-lilith dwow-node0 dwow-node1)
        fi
    fi

    if [ "$WITH_WALLET" -gt 0 ]; then
        for i in $(seq 1 "$WITH_WALLET"); do
            EXPECTED+=(dwow-wallet-$i)
        done
    fi

    for c in "${EXPECTED[@]}"; do
        if docker ps --format '{{.Names}}' | grep -q "$c"; then
            pass "$c running"
        else
            fail "$c running"
        fi
    done
}

# ==============================================================================
# Join Phase 6: Container Lifecycle
# ==============================================================================
phase_join_lifecycle() {
    echo ""
    echo "=== Join Phase 6: Container Lifecycle ==="
    check_image || return 1

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    clean_data_dir "$JOIN_TEST_DATA"
    mkdir -p "$JOIN_TEST_DATA"

    echo "  Starting native mode container..."
    _join_docker_run "$JOIN_TEST_DATA"

    # Poll for RPC port instead of fixed sleep
    echo "  Waiting for RPC port $RPC_PORT to become available..."
    local rpc_ready=0
    for i in $(seq 1 10); do
        if docker exec "$CONTAINER_NAME" bash -c "exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT && echo ok >&3" 2>/dev/null; then
            rpc_ready=1
            break
        fi
        sleep 2
    done

    if container_running "$CONTAINER_NAME"; then
        pass "Container is running"
    else
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "Container stopped unexpectedly"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        clean_data_dir "$JOIN_TEST_DATA"
        return 0
    fi

    if [ "$rpc_ready" -eq 0 ]; then
        echo "  Container logs (last 20 lines):"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "RPC port $RPC_PORT never became available"
    else
        pass "RPC port $RPC_PORT reachable"
    fi

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

    if ! echo "$logs" | grep -qi "ERROR"; then
        pass "No ERROR lines in logs"
    else
        echo "  ERROR lines found (startup diagnostics — inspect if unexpected):"
        echo "$logs" | grep -i "ERROR" | head -5
        warn "ERROR lines in logs (diagnostic — container is running)"
    fi

    echo "  Container is running. Leaving it for next phase."
}
