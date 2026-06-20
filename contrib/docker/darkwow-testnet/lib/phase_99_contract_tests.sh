# DarkWow Testnet Pipeline — Contract E2E Tests
#
# Post-pipeline: run test-contracts.sh if CONTRACT_TIER > 0.
# Skip for join modes where no local devnet is running.
# Dependencies: output.sh (info, fail, check),
#               config.sh (CONTRACT_TIER, MODE, SCRIPT_DIR),
#               helpers.sh (is_join_mode)
#
# Sourced by test_pipeline.sh after phase_21_persistence.sh.

phase_contract_tests() {
    if [ "$CONTRACT_TIER" -eq 0 ]; then
        return 0
    fi

    # Contract tests require a running local devnet — skip for join modes
    # where the container is torn down during the pipeline.
    if is_join_mode; then
        info "Contract tests skipped — not supported in join mode"
        return 0
    fi

    echo ""
    echo "==========================================="
    info "Running contract E2E tests (tier $CONTRACT_TIER)..."
    echo "==========================================="

    local contract_script="$SCRIPT_DIR/test-contracts.sh"
    if [ ! -x "$contract_script" ]; then
        fail "test-contracts.sh not found at $contract_script"
        return 1
    fi

    "$contract_script" --mode "$MODE" --tier "$CONTRACT_TIER"
    check $? "contract E2E tests (tier $CONTRACT_TIER)"
}
