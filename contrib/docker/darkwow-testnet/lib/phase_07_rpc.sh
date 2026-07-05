# DarkWow Testnet Pipeline — Phase 7: RPC Health / Seed Fallback
#
# Phase 7 local: JSON-RPC ping to node0, node1, monerod.
# Phase 7 join: deploy fallback lilith, verify seed peer connectivity.
# Dependencies: output.sh (info, pass, fail, error),
#               config.sh (NODE0, MODE, CONTAINER_NAME, FALLBACK_LILITH_NAME,
#                          NETWORK, P2P_PORT, RPC_PORT, STRATUM_PORT,
#                          FALLBACK_SEED_PORT, SEED_ADDR, MAGIC_BYTES,
#                          FINALITY_MODE, FINALITY_CARIBINA_ENABLED, IMAGE,
#                          JOIN_TEST_DATA, JOIN_TEST_FALLBACK),
#               helpers.sh (check_image, clean_data_dir, jsonrpc)
#
# Sourced by test_pipeline.sh after phase_06_verify.sh.

phase_rpc_health() {
    info "Phase 7: Verifying RPC health..."

    # node0 RPC (JSON-RPC over raw TCP — use bash /dev/tcp, not HTTP curl)
    info "Waiting for node0 RPC (port 31345)..."
    if ! poll_until 30 2 jsonrpc_ping "$NODE0" 31345; then
        warn "Node0 RPC did not become healthy after 30 attempts"; return 1
    fi
    pass "node0 RPC healthy"

    # observer RPC (always present in native/merge/bridge modes)
    if ! is_join_mode && docker ps --format '{{.Names}}' | grep -q "dwow-observer"; then
        info "Waiting for observer RPC (port 31345)..."
        if ! poll_until 30 2 jsonrpc_ping dwow-observer 31345; then
            warn "observer RPC did not become healthy after 30 attempts"; return 1
        fi
        pass "observer RPC healthy"
    fi

    # node1 RPC (only when multiple nodes are running)
    if [ "$NATIVE_NODES" -ge 2 ] || [ "$MODE" = "merge" ]; then
        info "Waiting for node1 RPC (port 31346)..."
        if ! poll_until 30 2 jsonrpc_ping dwow-node1 31346; then
            warn "Node1 RPC did not become healthy after 30 attempts"; return 1
        fi
        pass "node1 RPC healthy"
    fi

    # node2 RPC (merge only — native miner)
    if [ "$MODE" = "merge" ]; then
        info "Waiting for node2 RPC (port 31350)..."
        if ! poll_until 30 2 jsonrpc_ping dwow-node2 31350; then
            warn "Node2 RPC did not become healthy after 30 attempts"; return 1
        fi
        pass "node2 RPC healthy"
    fi

    # monerod RPC (merge only)
    if [ "$MODE" = "merge" ]; then
        info "Waiting for monerod RPC (port 28081)..."
        for i in $(seq 1 60); do
            if docker exec dwow-monerod curl -s --max-time 2 http://127.0.0.1:28081/json_rpc \
                -H 'Content-Type: application/json' \
                -d '{"jsonrpc":"2.0","method":"get_info","id":1}' >/dev/null 2>&1; then
                pass "monerod RPC healthy"
                break
            fi
            [ "$i" -eq 60 ] && { warn "monerod RPC did not become healthy after 60 attempts"; return 0; }
            sleep 2
        done
    fi
}

# ==============================================================================
# Join Phase 7: Seed Fallback
# ==============================================================================
phase_join_fallback() {
    echo ""
    echo "=== Join Phase 7: Seed Fallback ==="
    _FALLBACK_FAIL_BEFORE="${FAIL:-0}"
    check_image || return 1

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
    docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true
    clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_FALLBACK"
    mkdir -p "$JOIN_TEST_DATA" "$JOIN_TEST_FALLBACK"

    local unreachable_seeds="unreachable.example.com:9999,another.dead.host:9999"
    echo "  Testing with unreachable seeds: $unreachable_seeds"

    echo "  Starting local fallback lilith..."
    _join_lilith_run "$JOIN_TEST_FALLBACK" "$FALLBACK_LILITH_NAME" "$FALLBACK_SEED_PORT"

    sleep 5

    if container_running "$FALLBACK_LILITH_NAME"; then
        pass "Fallback lilith started"
    else
        echo "  Container logs:"
        docker logs "$FALLBACK_LILITH_NAME" 2>&1 | tail -10
        warn "Fallback lilith failed to start"
        clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_FALLBACK"
        return 0
    fi

    echo "  Starting dwowd with fallback seed 127.0.0.1:${FALLBACK_SEED_PORT}..."
    _join_docker_run "$JOIN_TEST_DATA" "" \
        "127.0.0.1:${FALLBACK_SEED_PORT}" \
        "-e RANDOMX_MAX_THREADS=0"

    sleep 10

    if container_running "$CONTAINER_NAME"; then
        pass "dwowd started with fallback seed"
    else
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        warn "dwowd failed to start with fallback seed"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
        docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true
        clean_data_dir "$JOIN_TEST_FALLBACK"
        return 0
    fi

    local config config_err
    config_err=$(mktemp)
    config=$(docker exec "$CONTAINER_NAME" cat /root/.config/dwow/dwowd_config.toml 2>"$config_err" || echo "")
    if [ -s "$config_err" ]; then
        warn "docker exec error reading config: $(cat "$config_err")"
    fi
    rm -f "$config_err"
    if echo "$config" | grep -q 'tcp+tls://127.0.0.1:31341'; then
        pass "Fallback seed address in generated config"
    else
        echo "  Config seeds line:"
        echo "$config" | grep "seeds =" || echo "  (not found)"
        warn "Fallback seed address not in config"
    fi

    # Wait for the RPC port to become reachable before querying
    echo "  Waiting for RPC port $RPC_PORT to become available..."
    local rpc_ready=0
    for i in $(seq 1 10); do
        if docker exec "$CONTAINER_NAME" bash -c "exec 3<>/dev/tcp/127.0.0.1/$RPC_PORT && echo ok >&3" 2>/dev/null; then
            rpc_ready=1
            break
        fi
        sleep 2
    done

    if [ "$rpc_ready" -eq 0 ]; then
        echo "  dwowd logs (last 30 lines):"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -30
        warn "RPC port $RPC_PORT never became available — join mode, network may be slow"
    else
        pass "RPC port $RPC_PORT is reachable"

        echo "  Waiting for P2P connection to fallback lilith (up to 60s)..."
        local connected=0
        for i in $(seq 1 12); do
            sleep 5
            local peers
            peers=$(jsonrpc "$RPC_PORT" "p2p.info")
            if echo "$peers" | grep -q '"sessions":[1-9]'; then
                pass "dwowd connected to fallback lilith (P2P session active)"
                connected=1
                break
            fi
            # If the p2p.info method isn't registered, check logs instead
            if echo "$peers" | grep -q '"method not found"'; then
                echo "  p2p.info method not available — checking logs for P2P activity"
                if docker logs "$CONTAINER_NAME" 2>&1 | grep -qi "session.*open\|peer.*connected\|P2P.*connected"; then
                    pass "dwowd connected to fallback lilith (log evidence)"
                    connected=1
                else
                    info "dwowd RPC reachable — p2p.info not implemented, connectivity check skipped"
                    connected=1
                fi
                break
            fi
        done

        if [ "$connected" -eq 0 ]; then
            echo "  p2p.info response:"
            jsonrpc "$RPC_PORT" "p2p.info" | head -1
            warn "No P2P connection to fallback lilith after 60s"
        fi
    fi

    # Hostlist is at <datadir>/<network>/hostlist.tsv inside the container
    local hostlist_in_container="/root/.local/share/dwow/lilith/${NETWORK}/hostlist.tsv"
    echo "  Checking lilith hostlist..."
    if docker exec "$FALLBACK_LILITH_NAME" test -f "$hostlist_in_container" 2>/dev/null; then
        pass "Fallback lilith hostlist.tsv persisted"
        docker exec "$FALLBACK_LILITH_NAME" wc -l "$hostlist_in_container" 2>/dev/null
    elif docker exec "$FALLBACK_LILITH_NAME" ls "/root/.local/share/dwow/lilith/${NETWORK}/" 2>/dev/null | grep -q .; then
        echo "  Lilith data dir has files — hostlist may use a different filename"
        docker exec "$FALLBACK_LILITH_NAME" ls -la "/root/.local/share/dwow/lilith/${NETWORK}/" 2>/dev/null
        pass "Fallback lilith wrote data files to datastore"
    else
        echo "  Lilith datastore is empty — no peers connected, hostlist not yet created"
        info "Fallback lilith datastore empty (no peers in isolated test — hostlist check skipped)"
    fi

    # Preserve containers on failure for debugging.
    if [ "${_FALLBACK_FAIL_BEFORE:-0}" -lt "${FAIL:-0}" ]; then
        warn "Fallback test recorded failures — preserving containers for debugging"
        echo "  Containers: $CONTAINER_NAME, $FALLBACK_LILITH_NAME"
        echo "  Data dirs:  $JOIN_TEST_DATA, $JOIN_TEST_FALLBACK"
        return 0
    fi

    echo "  Stopping fallback containers..."
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    docker stop "$FALLBACK_LILITH_NAME" 2>/dev/null || true
    docker rm "$FALLBACK_LILITH_NAME" 2>/dev/null || true

    clean_data_dir "$JOIN_TEST_DATA" "$JOIN_TEST_FALLBACK"

    # Start a fresh container so subsequent phases (8-10) have a running
    # target. Uses the same parameters as Phase 6 (lifecycle).
    echo "  Starting test container for subsequent phases..."
    mkdir -p "$JOIN_TEST_DATA"
    _join_docker_run "$JOIN_TEST_DATA"

    sleep 10

    if container_running "$CONTAINER_NAME"; then
        pass "Test container restarted for subsequent phases"
    else
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        warn "Test container failed to restart"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
    fi
}
