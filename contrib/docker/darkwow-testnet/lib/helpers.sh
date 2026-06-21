# DarkWow Testnet Pipeline — shared helpers
#
# Shared utilities used by multiple phase modules.
# Dependencies: output.sh (PASS, FAIL, pass, fail, info, warn, error),
#               config.sh (MODE, IMAGE, CONTAINER_NAME, RPC_PORT,
#                          BRIDGE_CONTAINER, COMPOSE_FILE)
#
# Sourced by test_pipeline.sh after config.sh.

clean_data_dir() {
    for dir in "$@"; do
        [ -d "$dir" ] || continue
        rm -rf "$dir" 2>/dev/null || \
            sudo rm -rf "$dir" 2>/dev/null || \
            { warn "Could not remove $dir (may contain root-owned files)"; }
    done
}

is_join_mode() {
    [ "$MODE" = "join-native" ] || [ "$MODE" = "join-merge" ]
}

is_bridge_mode() {
    [ "$MODE" = "bridge" ]
}

# ==============================================================================
# Join-mode helpers
# ==============================================================================

_CHECK_IMAGE_FAILED=0
check_image() {
    if [ "$_CHECK_IMAGE_FAILED" -eq 1 ]; then
        return 1
    fi
    if ! docker image inspect "$IMAGE" &>/dev/null; then
        _CHECK_IMAGE_FAILED=1
        fail "Docker image '$IMAGE' not found (build phase should have created it)"
        return 1
    fi
    return 0
}

check_network() {
    if ! curl -s --connect-timeout 5 https://api.ipify.org >/dev/null 2>&1; then
        fail "No internet connectivity detected"
        return 1
    fi
    return 0
}

jsonrpc() {
    local port="$1" method="$2"
    # dwowd JSON-RPC is raw TCP, not HTTP. Use bash /dev/tcp via docker exec.
    # Retry up to 3 times if the port isn't listening yet.
    for attempt in 1 2 3; do
        if docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
            local result
            result=$(docker exec "$CONTAINER_NAME" bash -c "exec 3<>/dev/tcp/127.0.0.1/$port 2>/dev/null || exit 1; echo '{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":[],\"id\":1}' >&3; timeout 3 cat <&3" 2>/dev/null) || true
            if [ -n "$result" ] && echo "$result" | grep -q '"result"\|"sessions"\|"block_height"'; then
                echo "$result"
                return
            fi
            # If we got a response but it's an error, return it
            if [ -n "$result" ]; then
                echo "$result"
                return
            fi
        fi
        [ "$attempt" -lt 3 ] && sleep 2
    done
    echo '{"error":"RPC unreachable after 3 attempts"}'
}

# === END RPC FIREWALL ===

report() {
    echo ""
    echo "==========================================="
    echo "  Mode: $MODE"
    echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
    echo "==========================================="
    echo ""

    if [ "$FAIL" -gt 0 ]; then
        echo -e "${RED}Some checks failed${NC}"
        echo ""
        echo "Debug info — check logs:"
        if [ "$MODE" = "merge" ]; then
            echo "  docker compose --profile merge logs"
        elif [ "$MODE" = "bridge" ]; then
            echo "  docker compose --profile bridge logs"
            echo "  docker logs $BRIDGE_CONTAINER"
        elif [ "$MODE" = "join-native" ]; then
            echo "  docker logs $CONTAINER_NAME"
        elif [ "$MODE" = "join-merge" ]; then
            echo "  docker compose -f $COMPOSE_FILE --profile join-merge logs"
        else
            echo "  docker compose logs"
        fi
        exit 1
    fi

    if is_join_mode; then
        echo "Join test passed. To join the public testnet for real:"
        echo "  ./contrib/docker/darkwow-testnet/join-testnet.sh --mode ${MODE#join-}"
    elif is_bridge_mode; then
        echo "Bridge pipeline passed."
        echo ""
        echo "Tear down:"
        echo "  docker compose --profile bridge down -v"
    else
        echo "Run contract tests:"
        echo "  ./test-contracts.sh --mode $MODE"
        echo ""
        echo "Tear down:"
        if [ "$MODE" = "merge" ]; then
            echo "  docker compose --profile merge down -v"
        else
            echo "  docker compose down -v"
        fi
    fi
    echo ""
    echo -e "${GREEN}Pipeline passed${NC}"
}

# ==============================================================================
# Shared helpers — Docker
# ==============================================================================

container_running() {
    docker ps --format '{{.Names}}' | grep -q "^${1}$"
}

_join_docker_run() {
    local datadir="$1" container_name="${2:-$CONTAINER_NAME}"
    docker run -d \
        --name "$container_name" \
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
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_DISABLE_CARIBINA="$FINALITY_DISABLE_CARIBINA" \
        -v "$datadir:/root/.local/share/dwow/dwowd" \
        "$IMAGE" 2>&1
}

handle_container_failure() {
    local container="$1" message="${2:-Container failed}"
    echo "  Container logs:"
    docker logs "$container" 2>&1 | tail -20
    fail "$message"
    docker stop "$container" 2>/dev/null || true
    docker rm "$container" 2>/dev/null || true
}

# ==============================================================================
# Shared helpers — RPC
# ==============================================================================

jsonrpc_ping() {
    local container="$1" port="$2"
    docker exec "$container" bash -c \
        "exec 3<>/dev/tcp/127.0.0.1/$port; echo '{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"params\":[],\"id\":1}' >&3; timeout 3 cat <&3 | grep -q 'pong'" 2>/dev/null
}

jsonrpc_get_block() {
    local container="$1" port="$2" block_num="$3"
    docker exec "$container" bash -c \
        "exec 3<>/dev/tcp/127.0.0.1/$port; echo '{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.get_block_linear\",\"params\":[$block_num],\"id\":1}' >&3; timeout 5 cat <&3" 2>&1
}

jsonrpc_get_height() {
    local response="$1"
    echo "$response" | grep -oP '"height":\s*\K\d+' | head -1 || echo "0"
}

poll_until() {
    local max_attempts="$1" sleep_secs="$2"
    shift 2
    local attempt=0
    while [ "$attempt" -lt "$max_attempts" ]; do
        if "$@" 2>/dev/null; then
            return 0
        fi
        attempt=$((attempt + 1))
        [ "$attempt" -lt "$max_attempts" ] && sleep "$sleep_secs"
    done
    return 1
}
