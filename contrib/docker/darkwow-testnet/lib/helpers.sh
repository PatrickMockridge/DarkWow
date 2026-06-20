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
        else
            exec 3<>/dev/tcp/127.0.0.1/"$port" 2>/dev/null || { echo '{"error":"RPC unreachable"}'; return; }
            echo "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":[],\"id\":1}" >&3
            timeout 3 cat <&3 2>/dev/null || echo '{"error":"RPC unreachable"}'
            exec 3>&-
            return
        fi
        [ "$attempt" -lt 3 ] && sleep 2
    done
    echo '{"error":"RPC unreachable after 3 attempts"}'
}

# ==============================================================================
# LOCAL TESTING ONLY — NOT FOR PRODUCTION
# RPC is firewalled to this single function, single purpose:
# cross-check wallet P2P height against node0 RPC height.
# This function is the ONLY place RPC appears in the entire codebase.
# RPC NEVER touches bin/drk/src/. NOT for production use.
# ==============================================================================
_verify_height_via_rpc() {
    curl -s --max-time 5 -X POST http://127.0.0.1:31345 \
      -H 'Content-Type: application/json' \
      -d '{"method":"blockchain.info","params":[],"id":1}' 2>/dev/null | \
      grep -oP '"height":\s*\K\d+' | head -1 || echo 0
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
