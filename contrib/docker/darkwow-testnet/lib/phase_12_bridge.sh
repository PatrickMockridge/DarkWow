# DarkWow Testnet Pipeline — Phases 12-19: Bridge Lifecycle
#
# Full bridge lifecycle: deploy, init, register, deposit, withdraw,
# accept, execute, verify.
# Dependencies: output.sh (info, pass, fail, check),
#               config.sh (BRIDGE_HELPER, WASM_BRIDGE, WASM_RELAYER_ENDOWMENT,
#                          BRIDGE_CONTAINER, NODE0),
#               helpers.sh (none directly — uses pass/fail from output.sh)
#
# Writes: BRIDGE_ID, ENDOWMENT_ID, RELAYER_PUB, RELAYER_SECRET,
#         DEPOSIT_COMMITMENT, WITHDRAW_NULLIFIER
#
# Sourced by test_pipeline.sh after phase_10_wallet_tests.sh.

phase_bridge_deploy() {
    info "Phase 10 (bridge): Deploying bridge and relayer_endowment contracts..."

    [ -n "$BRIDGE_HELPER" ] || { fail "bridge_test_helper not found (prereqs phase may have failed silently)"; return 1; }

    info "Deploying bridge contracts via bridge_test_helper..."
    local bridge_deploy_output
    bridge_deploy_output=$("$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        deploy-bridge \
        --bridge-wasm "$WASM_BRIDGE" \
        --endowment-wasm "$WASM_RELAYER_ENDOWMENT" 2>&1)

    if [ $? -ne 0 ]; then
        echo "$bridge_deploy_output"
        fail "bridge contract deploy"
        return 1
    fi

    BRIDGE_ID=$(echo "$bridge_deploy_output" | grep -oP '^bridge_contract_id:\s+\K\S+')
    ENDOWMENT_ID=$(echo "$bridge_deploy_output" | grep -oP '^endowment_contract_id:\s+\K\S+')

    if [ -z "$BRIDGE_ID" ] || [ -z "$ENDOWMENT_ID" ]; then
        echo "$bridge_deploy_output"
        fail "bridge contract deploy (missing contract IDs)"
        return 1
    fi

    pass "bridge contracts deployed"
    info "  Bridge ID:     ${BRIDGE_ID:0:16}..."
    info "  Endowment ID:  ${ENDOWMENT_ID:0:16}..."

    # Generate relayer keypair
    info "Generating relayer keypair..."
    local RELAYER_KEYPAIR
    RELAYER_KEYPAIR=$("$BRIDGE_HELPER" generate-keypair 2>&1)
    RELAYER_PUB=$(echo "$RELAYER_KEYPAIR" | grep -oP '^public_key:\s+\K\S+')
    RELAYER_SECRET=$(echo "$RELAYER_KEYPAIR" | grep -oP '^secret_key:\s+\K\S+')

    if [ -z "$RELAYER_PUB" ] || [ -z "$RELAYER_SECRET" ]; then
        echo "$RELAYER_KEYPAIR"
        fail "relayer keypair generation"
        return 1
    fi
    pass "relayer keypair generated"
    info "  Relayer pub:   ${RELAYER_PUB:0:16}..."
}

# ==============================================================================
# Bridge Phase 10b: Initialize Contracts (resume-from 13)
# ==============================================================================
phase_bridge_init() {
    info "Phase 10b (bridge): Initializing bridge and endowment contracts..."

    # Initialize bridge (InitializeV1, no params)
    info "Initializing bridge contract..."
    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        init-bridge 2>&1
    check $? "bridge InitializeV1"

    # Initialize relayer endowment
    info "Initializing relayer endowment..."
    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        init-endowment --relayer-pub "$RELAYER_PUB" 2>&1
    check $? "endowment InitializeV1"
}

# ==============================================================================
# Bridge Phase 11: Register Relayer (resume-from 14)
# ==============================================================================
phase_bridge_register_relayer() {
    info "Phase 11 (bridge): Registering relayer..."

    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        register-relayer --relayer-pub "$RELAYER_PUB" 2>&1
    check $? "RegisterRelayerV1"

    pass "relayer registered"
}

# ==============================================================================
# Bridge Phase 12: Simulate Deposit (resume-from 15)
# ==============================================================================
phase_bridge_deposit() {
    info "Phase 12 (bridge): Simulating deposit with ZK proof..."

    # Generate a deterministic secret
    local DEPOSIT_SECRET="0000000000000000000000000000000000000000000000000000000000000001"
    local DEPOSIT_AMOUNT=1000
    # Use the relayer's public key as recipient for simplicity
    local DEPOSIT_RECIPIENT
    DEPOSIT_RECIPIENT="$RELAYER_PUB"

    local DEPOSIT_OUTPUT
    DEPOSIT_OUTPUT=$("$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        simulate-deposit \
        --secret "$DEPOSIT_SECRET" \
        --amount "$DEPOSIT_AMOUNT" \
        --recipient-pub "$DEPOSIT_RECIPIENT" 2>&1)

    if [ $? -ne 0 ]; then
        echo "$DEPOSIT_OUTPUT"
        fail "SimulateDeposit"
        return 1
    fi

    local DEPOSIT_COMMITMENT
    DEPOSIT_COMMITMENT=$(echo "$DEPOSIT_OUTPUT" | grep -oP '^commitment:\s+\K\S+')
    if [ -z "$DEPOSIT_COMMITMENT" ]; then
        echo "$DEPOSIT_OUTPUT"
        fail "SimulateDeposit (missing commitment)"
        return 1
    fi

    pass "deposit submitted"
    info "  Commitment:    ${DEPOSIT_COMMITMENT:0:16}..."
}

# ==============================================================================
# Bridge Phase 13: Create Withdrawal (resume-from 16)
# ==============================================================================
phase_bridge_withdraw() {
    info "Phase 13 (bridge): Creating withdrawal with ZK proof..."

    local WITHDRAW_SECRET="0000000000000000000000000000000000000000000000000000000000000002"
    local WITHDRAW_AMOUNT=500

    local WITHDRAW_OUTPUT
    WITHDRAW_OUTPUT=$("$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        simulate-withdraw \
        --secret "$WITHDRAW_SECRET" \
        --amount "$WITHDRAW_AMOUNT" 2>&1)

    if [ $? -ne 0 ]; then
        echo "$WITHDRAW_OUTPUT"
        fail "SimulateWithdraw"
        return 1
    fi

    local WITHDRAW_NULLIFIER
    WITHDRAW_NULLIFIER=$(echo "$WITHDRAW_OUTPUT" | grep -oP '^nullifier:\s+\K\S+')
    if [ -z "$WITHDRAW_NULLIFIER" ]; then
        echo "$WITHDRAW_OUTPUT"
        fail "SimulateWithdraw (missing nullifier)"
        return 1
    fi

    pass "withdrawal submitted"
    info "  Nullifier:     ${WITHDRAW_NULLIFIER:0:16}..."
}

# ==============================================================================
# Bridge Phase 14: Accept Withdrawal (resume-from 17)
# ==============================================================================
phase_bridge_accept() {
    info "Phase 14 (bridge): Accepting withdrawal as relayer..."

    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        accept-withdrawal \
        --nullifier "$WITHDRAW_NULLIFIER" \
        --relayer-pub "$RELAYER_PUB" \
        --max-fee-bp 500 2>&1
    check $? "AcceptWithdrawalV1"

    pass "withdrawal accepted"
}

# ==============================================================================
# Bridge Phase 15: Execute Withdrawal (resume-from 18)
# ==============================================================================
phase_bridge_execute() {
    info "Phase 15 (bridge): Executing guaranteed withdrawal..."

    "$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        execute-withdrawal \
        --nullifier "$WITHDRAW_NULLIFIER" 2>&1
    check $? "ExecuteGuaranteedWithdrawV1"

    pass "withdrawal executed"
}

# ==============================================================================
# Bridge Phase 16: Verify Bridge (resume-from 19)
# ==============================================================================
phase_bridge_verify() {
    info "Phase 16 (bridge): Verifying bridge-node health and logs..."

    # Check bridge-node logs for activity
    if ! container_running "$BRIDGE_CONTAINER"; then
        fail "bridge-node container NOT running"
    else
        local bridge_logs
        bridge_logs=$(docker logs "$BRIDGE_CONTAINER" 2>&1 || true)
        if [ -n "$bridge_logs" ]; then
            pass "bridge-node has log output"
        else
            warn "bridge-node log output is empty — binary may not have started logging yet"
        fi
    fi

    # Show recent bridge-node activity
    info "Bridge-node recent logs:"
    echo "$bridge_logs" | tail -20

    # Verify block height has progressed beyond genesis
    for attempt in 1 2 3 4 5; do
        BLOCK_INFO=$(jsonrpc_get_block "$NODE0" 31345 1 2>&1) && break
        sleep 2
    done

    BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -oP '"height":\s*\K\d+' | head -1) || true
    info "Final block height: $BLOCK_HEIGHT"

    if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
        pass "bridge mode block height >= 2 (height=$BLOCK_HEIGHT)"
    else
        fail "bridge mode block height >= 2 (height=$BLOCK_HEIGHT)"
    fi
}
