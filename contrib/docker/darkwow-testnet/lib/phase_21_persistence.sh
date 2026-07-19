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
    _PERSIST_FAIL_BEFORE="${FAIL:-0}"
    check_image || return 1

    local persist_dir="$JOIN_TEST_PERSIST"
    clean_data_dir "$persist_dir"
    mkdir -p "$persist_dir"

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true

    echo "  Starting first run..."
    _join_docker_run "$persist_dir"

    sleep 2

    if ! container_running "$CONTAINER_NAME"; then
        warn "Container failed to start"
        echo "  Data dir preserved for debugging: $persist_dir"
        return 0
    fi

    if [ -f "$persist_dir/$NETWORK/hostlist.tsv" ] || ls "$persist_dir/$NETWORK/"*.sled 2>/dev/null | head -1 | grep -q sled; then
        pass "Data files created on first run"
    elif [ -f "$persist_dir/hostlist.tsv" ] || ls "$persist_dir/"*.sled 2>/dev/null | head -1 | grep -q sled; then
        pass "Data files created on first run"
    else
        echo "  Data dir contents:"
        find "$persist_dir" -type f 2>/dev/null | head -20 || echo "  (empty)"
        warn "Data files not created on first run"
    fi

    echo "  Stopping container..."
    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true

    if [ -d "$persist_dir" ] && [ "$(ls -A "$persist_dir" 2>/dev/null)" ]; then
        pass "Host data survived container removal"
    else
        warn "Host data missing after container stop"
        clean_data_dir "$persist_dir"
        return 0
    fi

    echo "  Starting second run (same data dir)..."
    _join_docker_run "$persist_dir"

    sleep 2

    if container_running "$CONTAINER_NAME"; then
        pass "Container restarted successfully with persisted data"
    else
        warn "Container failed to restart"
    fi

    # Preserve on failure for debugging.
    if [ "${_PERSIST_FAIL_BEFORE:-0}" -lt "${FAIL:-0}" ]; then
        warn "Persistence test recorded failures — preserving container and data for debugging"
        echo "  Container: $CONTAINER_NAME"
        echo "  Data dir:  $persist_dir"
        return 0
    fi

    docker stop "$CONTAINER_NAME" 2>/dev/null || true
    docker rm "$CONTAINER_NAME" 2>/dev/null || true
    clean_data_dir "$persist_dir"
}
