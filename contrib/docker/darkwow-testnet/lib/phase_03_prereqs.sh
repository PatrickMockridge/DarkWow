# DarkWow Testnet Pipeline — Phase 3: Validate Prerequisites
#
# Validate binaries, WASM files, bridge helper exist.
# Dependencies: output.sh (info, pass, fail, warn, error),
#               config.sh (SCRIPT_DIR, MODE, REPO_ROOT, BRIDGE_TEST_HELPER,
#                          BRIDGE_TEST_HELPER_DEBUG, WASM_BRIDGE,
#                          WASM_RELAYER_ENDOWMENT, WASM_DEPLOOOOR),
#               helpers.sh (is_join_mode, is_bridge_mode)
#
# Writes: BRIDGE_HELPER
#
# Sourced by test_pipeline.sh after phase_02_build.sh.

phase_prereqs() {
    info "Phase 3: Validating prerequisites..."

    # Pre-flight: Docker daemon must be running
    if ! docker info >/dev/null 2>&1; then
        warn "Docker daemon is not running or not accessible — container phases will fail"
    fi

    if is_join_mode; then
        [ -f "$SCRIPT_DIR/join-testnet.sh" ] || error "join-testnet.sh missing"
        [ -f "$SCRIPT_DIR/entrypoint.sh" ] || error "entrypoint.sh missing"
        [ -f "$SCRIPT_DIR/docker-compose.yml" ] || error "docker-compose.yml missing"
        [ -f "$SCRIPT_DIR/Dockerfile" ] || error "Dockerfile missing"
        if [ "$MODE" = "join-merge" ]; then
            [ -f "$SCRIPT_DIR/Dockerfile.monero" ] || error "Dockerfile.monero missing"
            [ -f "$SCRIPT_DIR/Dockerfile.p2pool" ] || error "Dockerfile.p2pool missing"
            [ -f "$SCRIPT_DIR/entrypoint-monero.sh" ] || error "entrypoint-monero.sh missing"
            [ -f "$SCRIPT_DIR/entrypoint-p2pool.sh" ] || error "entrypoint-p2pool.sh missing"
        fi
        pass "join prereqs present"
        return
    fi

    [ -f "$SCRIPT_DIR/entrypoint.sh" ]      || error "entrypoint.sh missing"
    [ -f "$SCRIPT_DIR/docker-compose.yml" ] || error "docker-compose.yml missing"
    [ -f "$SCRIPT_DIR/Dockerfile" ]         || error "Dockerfile missing"

    if [ "$MODE" = "merge" ]; then
        [ -f "$SCRIPT_DIR/Dockerfile.monero" ] || error "Dockerfile.monero missing (needed for merge mode)"
        [ -f "$SCRIPT_DIR/entrypoint-monero.sh" ] || error "entrypoint-monero.sh missing"
    fi

    # Bridge mode: ensure bridge_test_helper binary exists
    if is_bridge_mode; then
        if [ -x "$BRIDGE_TEST_HELPER" ]; then
            BRIDGE_HELPER="$BRIDGE_TEST_HELPER"
        elif [ -x "$BRIDGE_TEST_HELPER_DEBUG" ]; then
            BRIDGE_HELPER="$BRIDGE_TEST_HELPER_DEBUG"
        else
            info "Building bridge_test_helper..."
            (cd "$REPO_ROOT" && RAYON_NUM_THREADS=10 cargo build -p bridge_test_helper --release 2>&1)
            if [ -x "$BRIDGE_TEST_HELPER" ]; then
                BRIDGE_HELPER="$BRIDGE_TEST_HELPER"
            elif [ -x "$BRIDGE_TEST_HELPER_DEBUG" ]; then
                BRIDGE_HELPER="$BRIDGE_TEST_HELPER_DEBUG"
            else
                fail "bridge_test_helper binary not found after build"
                BRIDGE_HELPER=""  # prevent unbound variable errors
            fi
        fi
        if [ -n "$BRIDGE_HELPER" ] && [ -x "$BRIDGE_HELPER" ]; then
            info "Using bridge_test_helper: $BRIDGE_HELPER"
            pass "bridge_test_helper present"
        fi

        # Check bridge-specific WASM files
        [ -f "$WASM_BRIDGE" ] && pass "bridge WASM found" || fail "bridge WASM missing"
        [ -f "$WASM_RELAYER_ENDOWMENT" ] && pass "relayer_endowment WASM found" || fail "relayer_endowment WASM missing"
        [ -f "$WASM_DEPLOOOOR" ] && pass "deployooor WASM found" || fail "deployooor WASM missing"
    fi

    # Check dwow_wallet (only when wallets are enabled)
    if [ "${WITH_WALLET:-0}" -gt 0 ]; then
        info "Using dwow_wallet via Docker (self-contained, builds from origin)"
        DWW --version 2>/dev/null || warn "dww --version failed (non-fatal)"

        # Smoke test: verify wallet binary accepts its own subcommands.
        # Catches stale images where the binary's CLI doesn't match expectations.
        # Note: clap 2 --help exits 1, so check output rather than exit code.
        info "Verifying wallet subcommands..."
        if DWW wallet initialize --help 2>&1 | grep -q "Initialize wallet database"; then
            pass "wallet subcommand smoke test"
        else
            fail "wallet subcommand smoke test — binary may be stale, rebuild with --no-cache"
        fi
    fi

    pass "all required files present"
}
