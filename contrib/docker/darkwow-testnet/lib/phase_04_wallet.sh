# DarkWow Testnet Pipeline — Phase 4: Generate Wallet
#
# Generate wallet keypairs, set FORWARD_DESTINATION.
# Dependencies: output.sh (info, pass, fail, error),
#               config.sh (WITH_WALLET, FORWARD_DESTINATION, DWW)
#
# Writes: WALLET_SECRET_1, WALLET_SECRET_2, WALLET_ADDRESS_1,
#         WALLET_ADDRESS_2, WALLET_ADDRESS, FORWARD_DESTINATION
#
# Sourced by test_pipeline.sh after phase_03_prereqs.sh.

phase_wallet() {
    local wallet_count="${WITH_WALLET:-1}"
    # Default to 1 wallet for keygen even if --with-wallet=0 (needed for address display)
    [ "$wallet_count" -lt 1 ] && wallet_count=1

    info "Phase 4: Generating DarkWow wallet(s) ($wallet_count wallet(s))..."

    # Initialize wallet directory
    info "Initializing wallet..."
    DWW wallet initialize 2>&1 || fail "Wallet init failed — container will also fail"

    # Generate N keypairs, one per wallet.
    for i in $(seq 1 "$wallet_count"); do
        info "  Generating keypair for wallet-$i..."
        KEYGEN_OUTPUT=$(DWW wallet keygen 2>&1) || { fail "wallet-$i keygen failed"; continue; }
        # NOTE: keygen output contains the secret — intentionally not logged

        local secret_val
        secret_val=$(echo "$KEYGEN_OUTPUT" | grep -oP 'Secret \(hex\):\s+\K\S+')
        eval "WALLET_SECRET_$i=\$secret_val"

        if [ -z "$secret_val" ] || [ "${#secret_val}" -ne 64 ]; then
            error "Failed to parse wallet-$i secret from keygen output (got: ${secret_val:-empty})"
        fi

        local addr_val
        addr_val=$(DWW wallet address 2>&1 | tail -1) || { fail "wallet-$i address retrieval failed"; continue; }
        eval "WALLET_ADDRESS_$i=\$addr_val"

        if [ -z "$addr_val" ]; then
            error "Failed to get wallet-$i address"
        fi

        pass "  wallet-$i keypair generated"
        info "    Address: ${addr_val:0:16}..."
        info "    Secret:  ${secret_val:0:16}..."

        # Write secret to indexed file for bind-mount into container.
        echo -n "$secret_val" > "/tmp/dwow_mining_secret_$i"
        chmod 600 "/tmp/dwow_mining_secret_$i"
    done

    # Export wallet-1 address as the canonical WALLET_ADDRESS for backward compat.
    WALLET_ADDRESS="${WALLET_ADDRESS_1:-}"
    export WALLET_ADDRESS
    export MONERO_WALLET_ADDRESS

    # Pass through coinbase forwarding destination if set.
    export FORWARD_DESTINATION="${FORWARD_DESTINATION:-}"

    # G1: When wallet containers are active and no external FORWARD_DESTINATION
    # is set, auto-set it to wallet-1's address. Coinbase rewards go to wallet-1;
    # wallet-2 is funded via transfer in phase_wallet_transfer.
    if [ "${WITH_WALLET:-0}" -gt 0 ] && [ -z "$FORWARD_DESTINATION" ]; then
        export FORWARD_DESTINATION="${WALLET_ADDRESS_1:-}"
        echo "[WALLET] Auto-setting FORWARD_DESTINATION=$WALLET_ADDRESS_1"
    fi
}
