# DarkWow Testnet Pipeline — Phase 4: Prepare Wallet Secrets
#
# Reads deterministic keys from test-keys.txt and writes them to files
# for bind-mount into wallet containers. The containers self-initialize
# (entrypoint-wallet.sh runs wallet initialize + wallet import-secrets).
# No host-side Docker calls on the critical path.
#
# Dependencies: output.sh (info, pass, fail, error),
#               config.sh (WITH_WALLET, FORWARD_DESTINATION)
#
# Writes: WALLET_SECRET_1, WALLET_SECRET_2, FORWARD_DESTINATION
#         /tmp/dwow_mining_secret_1, /tmp/dwow_mining_secret_2
#
# Sourced by test_pipeline.sh after phase_03_prereqs.sh.

phase_wallet() {
    # Only run when wallets are explicitly enabled.
    if [ "${WITH_WALLET:-0}" -le 0 ]; then
        info "Phase 4: No wallets enabled (WITH_WALLET=$WITH_WALLET) — skipping"
        return 0
    fi

    local wallet_count="$WITH_WALLET"
    info "Phase 4: Preparing wallet secrets ($wallet_count wallet(s))..."

    # Load pre-configured keys from test-keys.txt (deterministic, one per line).
    local keys_file="${SCRIPT_DIR}/test-keys.txt"
    if [ ! -f "$keys_file" ]; then
        error "test-keys.txt not found at $keys_file — required for deterministic wallet keys"
    fi
    mapfile -t __KEYS < "$keys_file"

    local secret_dir="${SCRIPT_DIR}/.secrets"
    mkdir -p "$secret_dir"

    # Write each key to a file for container bind-mount.
    # Container entrypoint reads these and self-initializes.
    for i in $(seq 1 "$wallet_count"); do
        local secret_val="${__KEYS[$((i - 1))]}"

        if [ -z "$secret_val" ] || [ "${#secret_val}" -ne 64 ]; then
            error "Failed to read wallet-$i key from test-keys.txt line $i (got: ${secret_val:-empty})"
        fi

        eval "WALLET_SECRET_$i=\$secret_val"

        # Write hex secret to pipeline-owned directory.
        # (was /tmp — Docker bind-mount creates root-owned dirs there)
        echo -n "$secret_val" > "${secret_dir}/dwow_mining_secret_$i" || \
            error "Failed to write secret file ${secret_dir}/dwow_mining_secret_$i"
        chmod 600 "${secret_dir}/dwow_mining_secret_$i" || true

        pass "  wallet-$i secret prepared"
        info "    Secret:  ${secret_val:0:16}..."
    done

    # Export wallet-1 address as canonical (FORWARD_DESTINATION).
    # Actual address retrieval happens in phase 5 after container init.
    WALLET_ADDRESS="${WALLET_ADDRESS_1:-}"
    export WALLET_ADDRESS

    # Pass through coinbase forwarding destination if set.
    export FORWARD_DESTINATION="${FORWARD_DESTINATION:-}"

    if [ "${WITH_WALLET:-0}" -gt 0 ] && [ -z "$FORWARD_DESTINATION" ]; then
        # FORWARD_DESTINATION will be set after container init in phase 5
        echo "[WALLET] FORWARD_DESTINATION will be collected from container after init"
    fi
}