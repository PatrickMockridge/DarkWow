# DarkWow Testnet Pipeline — Phase 21: Data Persistence
#
# Stop container, restart, verify chain data survives.
# Only meaningful for join modes (local devnet uses compose volumes).
# Dependencies: output.sh (info, pass, fail),
#               config.sh (JOIN_TEST_PERSIST, CONTAINER_NAME, NETWORK, P2P_PORT,
#                          RPC_PORT, STRATUM_PORT, SEED_ADDR, MAGIC_BYTES,
#                          FINALITY_MODE, FINALITY_CARIBINA_ENABLED, IMAGE),
#               helpers.sh (is_join_mode, check_image, clean_data_dir)
#
# Sourced by test_pipeline.sh after phase_20_report.sh.

phase_persistence() {
    if ! is_join_mode; then
        info "persistence not applicable for local devnet"
        return 0
    fi

    echo ""
    echo "=== Join Phase 11: Persistence / Restart ==="
    check_image || return 1

    local persist_dir="$JOIN_TEST_PERSIST"
    clean_data_dir "$persist_dir"
    mkdir -p "$persist_dir"

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true

    echo "  Starting first run..."
    _join_docker_run "$persist_dir"

    sleep 15

    if ! container_running "$CONTAINER_NAME"; then
        fail "Container failed to start"
        clean_data_dir "$persist_dir"
        return 0
    fi

    if [ -f "$persist_dir/$NETWORK/hostlist.tsv" ] || ls "$persist_dir/$NETWORK/"*.sled 2>/dev/null | head -1 | grep -q sled; then
        pass "Data files created on first run"
    elif [ -f "$persist_dir/hostlist.tsv" ] || ls "$persist_dir/"*.sled 2>/dev/null | head -1 | grep -q sled; then
        pass "Data files created on first run"
    else
        echo "  Data dir contents:"
        find "$persist_dir" -type f 2>/dev/null | head -20 || echo "  (empty)"
        fail "Data files not created on first run"
    fi

    echo "  Stopping container..."
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true

    if [ -d "$persist_dir" ] && [ "$(ls -A "$persist_dir" 2>/dev/null)" ]; then
        pass "Host data survived container removal"
    else
        fail "Host data missing after container stop"
        clean_data_dir "$persist_dir"
        return 0
    fi

    echo "  Starting second run (same data dir)..."
    _join_docker_run "$persist_dir"

    sleep 10

    if container_running "$CONTAINER_NAME"; then
        pass "Container restarted successfully with persisted data"
    else
        fail "Container failed to restart"
    fi

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    clean_data_dir "$persist_dir"
}
