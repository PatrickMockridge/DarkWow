# DarkWow Testnet Pipeline — Phase 5: Start / Join Config
#
# Phase 5 local: docker compose up containers, wallet containers.
# Phase 5 join: generate dwowd config, validate keys.
# Dependencies: output.sh (info, pass, fail, warn, error, check),
#               config.sh (MODE, WALLET_ADDRESS, FINALITY_MODE,
#                          FINALITY_CARIBINA_ENABLED, FINALITY_ENABLE_MONERO,
#                          MONERO_MIN_CONFIRMATIONS, MONEROD_RPC_URL,
#                          NATIVE_NODES, WITH_WALLET, COMPOSE_PROJECT_NAME,
#                          COMPOSE_FILE, IMAGE, JOIN_TEST_DATA, CONTAINER_NAME,
#                          NETWORK, P2P_PORT, RPC_PORT, STRATUM_PORT,
#                          SEED_ADDR, MAGIC_BYTES),
#               helpers.sh (is_join_mode, check_image)
#
# Sourced by test_pipeline.sh after phase_04_wallet.sh.

phase_start() {
    info "Phase 5: Starting containers..."

    if [ "$MODE" = "merge" ]; then
        # Merge mode: 3 mining nodes + monerod. Stagger startup to serialize
        # RandomX dataset init (2GB/node) and prevent memory thrashing.
        export MONERO_DATA_DIR="${MONERO_DATA_DIR:-$HOME/.cache/dwow_merge_testnet_monero}"
        export P2POOL_DATA_DIR="${P2POOL_DATA_DIR:-$HOME/.cache/dwow_merge_testnet_p2pool}"
        export MONERO_OFFLINE="${MONERO_OFFLINE:-true}"
        export MONERO_FIXED_DIFFICULTY="${MONERO_FIXED_DIFFICULTY:-1000}"
        export MERGE_MINING=true
        export WALLET_ADDRESS FINALITY_MODE FINALITY_CARIBINA_ENABLED
        export FINALITY_ENABLE_MONERO MONERO_MIN_CONFIRMATIONS MONEROD_RPC_URL
        docker compose --profile merge up -d lilith; check $? "compose up merge lilith"
        sleep 5
        docker compose --profile merge up -d node0; check $? "compose up merge node0"
        sleep 5
        docker compose --profile merge up -d node1 node2 monerod; check $? "compose up merge node1 node2 monerod"
    elif [ "$MODE" = "bridge" ]; then
        WALLET_ADDRESS="$WALLET_ADDRESS" \
            FINALITY_MODE="$FINALITY_MODE" FINALITY_CARIBINA_ENABLED="$FINALITY_CARIBINA_ENABLED" \
            FINALITY_ENABLE_MONERO="$FINALITY_ENABLE_MONERO" \
            MONERO_MIN_CONFIRMATIONS="$MONERO_MIN_CONFIRMATIONS" \
            MONEROD_RPC_URL="$MONEROD_RPC_URL" \
            docker compose --profile native up -d lilith node0 node1; check $? "compose up native lilith node0 node1"
        info "native profile started, waiting for P2P mesh..."
        sleep 10

        # Verify native containers are healthy before starting bridge
        EXITED=$(docker compose --profile native ps 2>/dev/null | grep "Exit" || true)
        if [ -n "$EXITED" ]; then
            echo "$EXITED"
            error "Native container exited immediately — check logs"
        fi

        # Now start bridge-node on top of the established mesh
        info "Starting bridge-node..."
        WALLET_ADDRESS="$WALLET_ADDRESS" \
            FINALITY_MODE="$FINALITY_MODE" FINALITY_CARIBINA_ENABLED="$FINALITY_CARIBINA_ENABLED" \
            FINALITY_ENABLE_MONERO="$FINALITY_ENABLE_MONERO" \
            MONERO_MIN_CONFIRMATIONS="$MONERO_MIN_CONFIRMATIONS" \
            MONEROD_RPC_URL="$MONEROD_RPC_URL" \
            docker compose --profile bridge up -d; check $? "compose up bridge"
        sleep 5

        EXITED=$(docker compose --profile bridge ps 2>/dev/null | grep "Exit" || true)
        if [ -n "$EXITED" ]; then
            echo "$EXITED"
            error "Bridge container exited immediately — check logs"
        fi
    else
        # Native mode: start only the requested number of nodes.
        # Stagger startup to serialize RandomX dataset initialization (2GB/node).
        # Simultaneous RandomX init on multiple nodes causes memory pressure
        # spikes that can freeze the host.
        if [ "$NATIVE_NODES" = "1" ]; then
            WALLET_ADDRESS="$WALLET_ADDRESS" \
                FINALITY_MODE="$FINALITY_MODE" FINALITY_CARIBINA_ENABLED="$FINALITY_CARIBINA_ENABLED" \
                FINALITY_ENABLE_MONERO="$FINALITY_ENABLE_MONERO" \
                MONERO_MIN_CONFIRMATIONS="$MONERO_MIN_CONFIRMATIONS" \
                MONEROD_RPC_URL="$MONEROD_RPC_URL" \
                docker compose --profile native up -d node0; check $? "compose up native node0"
        elif [ "$NATIVE_NODES" = "5" ]; then
            WALLET_ADDRESS="$WALLET_ADDRESS" \
                FINALITY_MODE="$FINALITY_MODE" FINALITY_CARIBINA_ENABLED="$FINALITY_CARIBINA_ENABLED" \
                FINALITY_ENABLE_MONERO="$FINALITY_ENABLE_MONERO" \
                MONERO_MIN_CONFIRMATIONS="$MONERO_MIN_CONFIRMATIONS" \
                MONEROD_RPC_URL="$MONEROD_RPC_URL" \
                docker compose --profile native up -d; check $? "compose up native all"
        else
            # Default: 2 nodes. Start sequentially to stagger RandomX init.
            WALLET_ADDRESS="$WALLET_ADDRESS" \
                FINALITY_MODE="$FINALITY_MODE" FINALITY_CARIBINA_ENABLED="$FINALITY_CARIBINA_ENABLED" \
                FINALITY_ENABLE_MONERO="$FINALITY_ENABLE_MONERO" \
                MONERO_MIN_CONFIRMATIONS="$MONERO_MIN_CONFIRMATIONS" \
                MONEROD_RPC_URL="$MONEROD_RPC_URL" \
                docker compose --profile native up -d lilith; check $? "compose up native lilith"
            sleep 5
            WALLET_ADDRESS="$WALLET_ADDRESS" \
                FINALITY_MODE="$FINALITY_MODE" FINALITY_CARIBINA_ENABLED="$FINALITY_CARIBINA_ENABLED" \
                FINALITY_ENABLE_MONERO="$FINALITY_ENABLE_MONERO" \
                MONERO_MIN_CONFIRMATIONS="$MONERO_MIN_CONFIRMATIONS" \
                MONEROD_RPC_URL="$MONEROD_RPC_URL" \
                docker compose --profile native up -d node0; check $? "compose up native node0"
            sleep 5
            WALLET_ADDRESS="$WALLET_ADDRESS" \
                FINALITY_MODE="$FINALITY_MODE" FINALITY_CARIBINA_ENABLED="$FINALITY_CARIBINA_ENABLED" \
                FINALITY_ENABLE_MONERO="$FINALITY_ENABLE_MONERO" \
                MONERO_MIN_CONFIRMATIONS="$MONERO_MIN_CONFIRMATIONS" \
                MONEROD_RPC_URL="$MONEROD_RPC_URL" \
                docker compose --profile native up -d node1; check $? "compose up native node1"
        fi
    fi

    if [ "$WITH_WALLET" -gt 0 ] && ! is_join_mode; then
        info "Starting $WITH_WALLET wallet container(s)..."
        for i in $(seq 1 "$WITH_WALLET"); do
            info "  Starting wallet-$i..."
            VOLUME_ARGS=(-v "wallet_data_$i:/root/.local/share/dwow/dww")
            # Mount per-wallet secret file (generated in phase_wallet)
            if [ -e "/tmp/dwow_mining_secret_$i" ] && [ ! -d "/tmp/dwow_mining_secret_$i" ]; then
                VOLUME_ARGS+=(-v "/tmp/dwow_mining_secret_$i:/run/secrets/mining_secret:ro")
            fi
            docker run -d \
                --name "dwow-wallet-$i" \
                --hostname "wallet-$i" \
                --restart no \
                --network ${COMPOSE_PROJECT_NAME}_dwow-local \
                -e WALLET_MODE=interactive \
                -e WALLET_INDEX="$i" \
                -e NETWORK=darkwow-testnet \
                -e RPC_URL="tcp://node0:31345" \
                -e WALLET_PASS=walletpass \
                -e SEED_ADDR="tcp+tls://lilith:31340" \
                -e P2P_PORT=31360 \
                -e MAGIC_BYTES="68,82,75,87" \
                "${VOLUME_ARGS[@]}" \
                darkwow-wallet:latest 2>&1
            check $? "docker run dwow-wallet-$i"
        done
        # Readiness probe: loop until wallet responds or container exits.
        # wallet initialize compiles genesis contracts (~2-3 min first run).
        for i in $(seq 1 "$WITH_WALLET"); do
            info "  Waiting for wallet-$i readiness (wallet initialize may take several minutes)..."
            local elapsed=0
            while true; do
                # Container must still be running
                if ! container_running "dwow-wallet-$i"; then
                    echo "  Container logs for dwow-wallet-$i:"
                    docker logs "dwow-wallet-$i" 2>&1 | tail -40
                    fail "  wallet-$i exited before becoming ready"
                    break
                fi
                if docker exec "dwow-wallet-$i" /app/dwow_wallet wallet address 2>/dev/null | grep -q .; then
                    pass "  wallet-$i ready (${elapsed}s)"
                    break
                fi
                # Status every 60s so we know it's not stuck
                if [ $((elapsed % 60)) -eq 0 ] && [ "$elapsed" -gt 0 ]; then
                    info "    wallet-$i still initializing (${elapsed}s elapsed)..."
                fi
                sleep 5
                elapsed=$((elapsed + 5))
            done
        done
    fi

    # Shred temp secret files now that containers have read them.
    # Docker -v bind-mount may create a directory if the file doesn't exist;
    # use 3-tier fallback to handle permission issues.
    for sf in /tmp/dwow_mining_secret_*; do
        [ -e "$sf" ] || continue
        rm -rf "$sf" 2>/dev/null || \
            sudo rm -rf "$sf" 2>/dev/null || \
            warn "Could not remove $sf (may be root-owned)"
    done

    if [ "$MODE" != "bridge" ]; then
        sleep 5

        # Check for immediate exits
        if [ "$MODE" = "merge" ]; then
            EXITED=$(docker compose --profile merge ps 2>/dev/null | grep "Exit" || true)
        else
            EXITED=$(docker compose --profile native ps 2>/dev/null | grep "Exit" || true)
        fi
        if [ -n "$EXITED" ]; then
            echo "$EXITED"
            error "Container exited immediately — check logs"
        fi
    fi

    pass "containers started"
}

# ==============================================================================
# Join Phase 5: Static Config Validation
# ==============================================================================
phase_join_config() {
    echo ""
    echo "=== Join Phase 5: Static Config Validation ==="
    check_image || return 1

    echo "  Starting container to capture generated config..."
    mkdir -p "$JOIN_TEST_DATA"

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

    if ! container_running "$CONTAINER_NAME"; then
        echo "  Container logs:"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "Container failed to start"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        return 0
    fi

    if [ "$rpc_ready" -eq 0 ]; then
        echo "  Container logs (last 20 lines):"
        docker logs "$CONTAINER_NAME" 2>&1 | tail -20
        fail "RPC port $RPC_PORT never became available"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        return 0
    else
        pass "RPC port $RPC_PORT reachable"
    fi

    local config
    config=$(docker exec "$CONTAINER_NAME" cat /root/.config/dwow/dwowd_config.toml 2>/dev/null || echo "")
    if [ -z "$config" ]; then
        fail "Could not read generated config"
        docker stop "$CONTAINER_NAME" 2>/dev/null || true
        docker rm "$CONTAINER_NAME" 2>/dev/null || true
        return 0
    fi

    echo "  --- Generated config (first 30 lines) ---"
    echo "$config" | head -30
    echo "  --- End config ---"

    # Network Identity
    if echo "$config" | grep -q 'magic_bytes = \[68,82,75,87\]'; then
        pass "magic_bytes = [68,82,75,87]"
    else
        fail "magic_bytes incorrect"
    fi

    if echo "$config" | grep -q "network = \"$NETWORK\""; then
        pass "network = $NETWORK"
    else
        fail "network incorrect"
    fi

    # P2P Bootstrap
    if echo "$config" | grep -q 'tcp+tls://lilith0.dark.fi:31340'; then
        pass "lilith0 seed present"
    else
        fail "lilith0 seed missing"
    fi

    if echo "$config" | grep -q 'tcp+tls://lilith1.dark.fi:31340'; then
        pass "lilith1 seed present"
    else
        fail "lilith1 seed missing"
    fi

    if echo "$config" | grep -q 'hostlist = '; then
        pass "hostlist path configured"
    else
        fail "hostlist path missing"
    fi

    if echo "$config" | grep -q 'localnet = false'; then
        pass "localnet = false"
    else
        fail "localnet incorrect"
    fi

    if echo "$config" | grep -q 'inbound = \["tcp+tls://0.0.0.0:'; then
        pass "inbound configured"
    else
        fail "inbound missing"
    fi

    # Blockchain params
    if echo "$config" | grep -q 'threshold = 3'; then
        pass "threshold = 3"
    else
        fail "threshold incorrect"
    fi

    if echo "$config" | grep -q 'pow_target = 120'; then
        pass "pow_target = 120"
    else
        fail "pow_target incorrect"
    fi

    if echo "$config" | grep -q 'skip_sync = false'; then
        pass "skip_sync = false"
    else
        fail "skip_sync incorrect"
    fi

    if echo "$config" | grep -q 'skip_fees = false'; then
        pass "skip_fees = false"
    else
        fail "skip_fees incorrect"
    fi

    # Stratum/RPC
    if echo "$config" | grep -q 'rpc_listen = "tcp://0.0.0.0:'; then
        pass "stratum/JSON-RPC listen configured"
    else
        fail "stratum/JSON-RPC listen missing"
    fi

    # external_addrs (only when set)
    if echo "$config" | grep -q 'external_addrs'; then
        pass "external_addrs configured"
    else
        info "external_addrs not set (EXTERNAL_ADDR not provided)"
    fi

    echo "  Config validation complete."
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
}
