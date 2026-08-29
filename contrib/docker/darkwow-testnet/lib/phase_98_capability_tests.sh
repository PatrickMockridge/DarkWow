# DarkWow Testnet Pipeline — L1 Capability Write-Path Tests
#
# Opt-in: run the wallet-driven L1 capability write-path tests (box put/take)
# in-docker using the prebuilt dwowd lib test binary (/app/dwowd_lib_tests).
# The tests spin up a self-contained in-process chain (temp sled DBs) — they do
# NOT need the running devnet; the contract WASM/zkas are embedded at compile
# time. Gated behind CAPABILITY_TESTS (default 0 = off).
#
# Dependencies: output.sh (info, fail, check),
#               config.sh (CAPABILITY_TESTS, MODE),
#               helpers.sh (is_join_mode)

phase_capability_tests() {
    if [ "${CAPABILITY_TESTS:-0}" -ne 1 ]; then
        return 0
    fi

    # Join modes tear down the single container during the pipeline; the
    # capability tests are self-contained but skip here for consistency.
    if is_join_mode; then
        info "Capability tests skipped — not supported in join mode"
        return 0
    fi

    echo ""
    echo "==========================================="
    info "Running L1 capability write-path tests (box put/take)..."
    echo "==========================================="

    if ! docker image inspect darkwow-testnet:latest >/dev/null 2>&1; then
        fail "darkwow-testnet:latest not found — phase_build must run first"
        return 1
    fi

    # Single-threaded: each test runs a RandomX coinbase + accept_block (~12 min
    # each, two submits per test); parallel RandomX VMs spike memory.
    docker run --rm \
        --entrypoint /app/dwowd_lib_tests \
        -e RAYON_NUM_THREADS=10 \
        -e RUST_MIN_STACK=67108864 \
        darkwow-testnet:latest \
        test_box_put_wallet_driven_generic_prover \
        test_box_take_wallet_driven_generic_prover \
        --test-threads 1 --nocapture
    check $? "L1 capability write-path tests (box put/take)"
}
