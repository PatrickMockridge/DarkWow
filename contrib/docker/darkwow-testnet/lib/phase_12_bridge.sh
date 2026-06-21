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
    BRIDGE_DEPLOY_OUTPUT=$("$BRIDGE_HELPER" --url "tcp://127.0.0.1:31345" \
        --block-time 120 --timeout 300 \
        deploy-bridge \
        --bridge-wasm "$WASM_BRIDGE" \
        --endowment-wasm "$WASM_RELAYER_ENDOWMENT" 2>&1)

    if [ $? -ne 0 ]; then
        echo "$BRIDGE_DEPLOY_OUTPUT"
        fail "bridge contract deploy"
        return 1
    fi

    BRIDGE_ID=$(echo "$BRIDGE_DEPLOY_OUTPUT" | grep "^bridge_contract_id:" | awk '{print $2}')
    ENDOWMENT_ID=$(echo "$BRIDGE_DEPLOY_OUTPUT" | grep "^endowment_contract_id:" | awk '{print $2}')

    if [ -z "$BRIDGE_ID" ] || [ -z "$ENDOWMENT_ID" ]; then
        echo "$BRIDGE_DEPLOY_OUTPUT"
        fail "bridge contract deploy (missing contract IDs)"
        return 1
    fi

    pass "bridge contracts deployed"
    info "  Bridge ID:     ${BRIDGE_ID:0:16}..."
    info "  Endowment ID:  ${ENDOWMENT_ID:0:16}..."

    # Generate relayer keypair
    info "Generating relayer keypair..."
    RELAYER_KEYPAIR=$("$BRIDGE_HELPER" generate-keypair 2>&1)
    RELAYER_PUB=$(echo "$RELAYER_KEYPAIR" | grep "^public_key:" | awk '{print $2}')
    RELAYER_SECRET=$(echo "$RELAYER_KEYPAIR" | grep "^secret_key:" | awk '{print $2}')

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
    DEPOSIT_SECRET="0000000000000000000000000000000000000000000000000000000000000001"
    DEPOSIT_AMOUNT=1000
    # Use the relayer's public key as recipient for simplicity
    DEPOSIT_RECIPIENT="$RELAYER_PUB"

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

    DEPOSIT_COMMITMENT=$(echo "$DEPOSIT_OUTPUT" | grep "^commitment:" | awk '{print $2}')
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

    WITHDRAW_SECRET="0000000000000000000000000000000000000000000000000000000000000002"
    WITHDRAW_AMOUNT=500

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

    WITHDRAW_NULLIFIER=$(echo "$WITHDRAW_OUTPUT" | grep "^nullifier:" | awk '{print $2}')
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

    # Check bridge-node container is running
    if docker ps --format '{{.Names}}' | grep -q "^${BRIDGE_CONTAINER}$"; then
        pass "bridge-node container running"
    else
        fail "bridge-node container running"
    fi

    # Check bridge-node logs for activity
    local bridge_logs
    bridge_logs=$(docker logs "$BRIDGE_CONTAINER" 2>&1 || true)
    if [ -n "$bridge_logs" ]; then
        pass "bridge-node has log output"
    else
        fail "bridge-node has log output (empty)"
    fi

    # Show recent bridge-node activity
    info "Bridge-node recent logs:"
    echo "$bridge_logs" | tail -20

    # Verify block height has progressed beyond genesis
    for attempt in 1 2 3 4 5; do
        BLOCK_INFO=$(docker exec "$NODE0" bash -c 'exec 3<>/dev/tcp/127.0.0.1/31345; echo "{\"jsonrpc\":\"2.0\",\"method\":\"blockchain.last_confirmed_block\",\"params\":[],\"id\":1}" >&3; timeout 5 cat <&3' 2>&1) && break
        sleep 2
    done

    BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '[0-9]\+' | head -1) || true
    info "Final block height: $BLOCK_HEIGHT"

    if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
        pass "bridge mode block height >= 2 (height=$BLOCK_HEIGHT)"
    else
        fail "bridge mode block height >= 2 (height=$BLOCK_HEIGHT)"
    fi
}
