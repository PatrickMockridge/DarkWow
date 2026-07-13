# DarkWow Testnet Pipeline — shared helpers
#
# Shared utilities used by multiple phase modules.
# Dependencies: output.sh (PASS, FAIL, pass, fail, info, warn, error),
#               config.sh (MODE, IMAGE, CONTAINER_NAME, RPC_PORT,
#                          BRIDGE_CONTAINER, COMPOSE_FILE)
#
# Sourced by test_pipeline.sh after config.sh.

clean_data_dir() {
    local _clean_failed=0
    for dir in "$@"; do
        [ -d "$dir" ] || continue
        rm -rf "$dir" 2>/dev/null || \
            sudo rm -rf "$dir" 2>/dev/null || \
            { warn "Could not remove $dir (may contain root-owned files)"; _clean_failed=1; }
    done
    return $_clean_failed
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

# Reset the stale-failure cache after a successful build.
# Called by phase_02_build.sh so that --resume-from doesn't
# inherit a poisoned _CHECK_IMAGE_FAILED flag from a prior run.
reset_check_image() {
    _CHECK_IMAGE_FAILED=0
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
    # Ensure temp file cleanup on ALL exit paths (early return, ERR trap, etc).
    local _jsonrpc_trap_set=
    trap 'rm -f "${exec_err:-}"' RETURN

    # dwowd JSON-RPC is raw TCP, not HTTP. Use bash /dev/tcp via docker exec.
    # Retry up to 3 times if the port isn't listening yet.
    for attempt in 1 2 3; do
        if container_running "$CONTAINER_NAME"; then
            local result exec_err
            exec_err=$(mktemp)
            result=$(docker exec "$CONTAINER_NAME" bash -c "exec 3<>/dev/tcp/127.0.0.1/$port 2>/dev/null || exit 1; echo '{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":[],\"id\":1}' >&3; timeout 3 cat <&3" 2>"$exec_err") || true
            if [ -s "$exec_err" ]; then
                warn "jsonrpc docker exec error: $(cat "$exec_err")"
            fi
            rm -f "$exec_err"
            if [ -n "$result" ] && echo "$result" | grep -q '"result"'; then
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
    echo -e "${GREEN}All infrastructure checks passed — monitoring...${NC}"
}

# ==============================================================================
# Precondition validation — used by --resume-from and --phase
# ==============================================================================
# Each function validates that the state required for a phase exists.
# Returns 0 if preconditions are met, 1 if not (after calling fail()).

_check_preconditions_phase_5() {
    # Phase 5 (start): Docker images must exist
    docker image inspect darkwow-testnet:latest >/dev/null 2>&1 || \
        { fail "Precondition: darkwow-testnet:latest not found. Run phase 2 (build) first."; return 1; }
    return 0
}

_check_preconditions_phase_6() {
    # Phase 6 (verify): containers must be running
    docker ps --format '{{.Names}}' | grep -q "dwow-node0" || \
        { fail "Precondition: dwow-node0 not running. Run phases 1-5 first."; return 1; }
    return 0
}

_check_preconditions_phase_8() {
    # Phase 8 (blocks): node0 RPC healthy + mining active
    _check_preconditions_phase_6 || return 1
    NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
    echo "$NODE0_LOGS" | grep -qi "miner.mine_linear\|Mined and applied block\|Block.*mined" || \
        { fail "Precondition: no mining activity detected. Run phases 1-8 first."; return 1; }
    return 0
}

_check_preconditions_phase_9() {
    # Phase 9 (wallet verify): wallet containers running + chain has blocks
    container_running "dwow-wallet-1" || \
        { fail "Precondition: dwow-wallet-1 not running. Run phases 1-5 first."; return 1; }
    return 0
}

_check_preconditions_phase_11() {
    # Phase 11 is mode-dependent (wallet transfer, bridge deploy, join persistence).
    # Validate the right preconditions for the current MODE.
    if is_bridge_mode; then
        _check_preconditions_phase_6 || return 1
        [ -n "$BRIDGE_HELPER" ] && [ -x "$BRIDGE_HELPER" ] || \
            { fail "Precondition: bridge_test_helper not found. Run phase 3 (prereqs) first."; return 1; }
    elif [ "${WITH_WALLET:-0}" -ge 2 ]; then
        container_running "dwow-wallet-1" || \
            { fail "Precondition: dwow-wallet-1 not running. Run phases 1-9 first."; return 1; }
        container_running "dwow-wallet-2" || \
            { fail "Precondition: dwow-wallet-2 not running. Run phases 1-9 first."; return 1; }
    fi
    return 0
}

_check_preconditions() {
    local phase="$1"
    local fn="_check_preconditions_phase_$phase"
    if type "$fn" >/dev/null 2>&1; then
        "$fn" || return 1
    fi
    return 0
}

# ==============================================================================
# Shared helpers — Docker
# ==============================================================================

container_running() {
    docker ps --format '{{.Names}}' | grep -q "^${1}$"
}

# Run a dwowd container with standard join-mode parameters.
# Usage:
#   _join_docker_run <datadir> [container_name] [seed_addr] [extra_env_vars]
# Arguments:
#   datadir          -- Host path mounted as dwowd data dir (required)
#   container_name   -- Docker container name (default: $CONTAINER_NAME)
#   seed_addr        -- SEED_ADDR env var value  (default: $SEED_ADDR)
#   extra_env_vars   -- Additional -e VAR=VAL string(s) appended verbatim
_join_docker_run() {
    local datadir="$1" container_name="${2:-$CONTAINER_NAME}"
    local seed_addr="${3:-$SEED_ADDR}" extra_env="$4"
    docker run -d \
        --pull=never \
        --name "$container_name" \
        --network=host \
        -e ROLE=dwowd \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$P2P_PORT" \
        -e RPC_PORT="$RPC_PORT" \
        -e STRATUM_PORT="$STRATUM_PORT" \
        -e SEED_ADDR="$seed_addr" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e MINING_THREADS=1 \
        -e THRESHOLD=3 \
        -e TARGET_BLOCK_TIME=120 \
        -e SKIP_SYNC=false \
        -e SKIP_FEES=false \
        -e LOCALNET=false \
        -e FINALITY_MODE="$FINALITY_MODE" \
        -e FINALITY_CARIBINA_ENABLED="$FINALITY_CARIBINA_ENABLED" \
        -v "$datadir:/root/.local/share/dwow/dwowd" \
        ${extra_env:+"$extra_env"} \
        "$IMAGE" 2>&1
}

# Run a lilith seed-node container for fallback-seed testing.
# Usage: _join_lilith_run <datadir> <container_name> <p2p_port>
_join_lilith_run() {
    local datadir="$1" container_name="$2" p2p_port="$3"
    docker run -d \
        --pull=never \
        --name "$container_name" \
        --network=host \
        --restart unless-stopped \
        -e ROLE=lilith \
        -e NETWORK="$NETWORK" \
        -e P2P_PORT="$p2p_port" \
        -e MAGIC_BYTES="$MAGIC_BYTES" \
        -e LOCALNET=false \
        -v "$datadir:/root/.local/share/dwow/lilith" \
        "$IMAGE" 2>&1
}

# ==============================================================================
# Shared helpers — RPC
# ==============================================================================

# Retry an RPC call up to N times. No timeout — the cat reads whatever the
# server sends when it sends it. A busy mining node is a healthy node; the
# retry handles transient unavailability.
rpc_retry() {
    local container="$1" port="$2" method="$3" params="$4" max_attempts="${5:-5}"
    local attempt=0
    while [ "$attempt" -lt "$max_attempts" ]; do
        attempt=$((attempt + 1))
        local result
        result=$(docker exec "$container" bash -c \
            "exec 3<>/dev/tcp/127.0.0.1/$port 2>/dev/null || exit 1; echo '{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}' >&3; timeout 30 cat <&3" 2>/dev/null)
        if [ -n "$result" ] && echo "$result" | grep -q '"result"'; then
            echo "$result"
            return 0
        fi
        [ "$attempt" -lt "$max_attempts" ] && sleep 2
    done
    return 1
}

jsonrpc_get_block() {
    local container="$1" port="$2" block_num="$3"
    rpc_retry "$container" "$port" "blockchain.get_block_linear" "[$block_num]" 5 2>/dev/null
}

# Get the current block height from a node via JSON-RPC.
# Returns the height as a bare integer, or 0 on failure.
jsonrpc_get_height() {
    local container="$1" port="$2"
    local raw
    raw=$(rpc_retry "$container" "$port" "blockchain.get_height" "[]" 5 2>/dev/null || echo "")
    if [ -n "$raw" ]; then
        echo "$raw" | jq -r '.result.height // 0' 2>/dev/null | head -1 | tr -d '[:space:]' || echo 0
    else
        echo 0
    fi
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
