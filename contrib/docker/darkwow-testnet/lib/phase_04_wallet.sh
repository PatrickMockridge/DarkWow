# DarkWow Testnet Pipeline — Phase 4: Validate Wallet Declarations
#
# Under the keys.toml declaration model, wallets derive their identity on boot
# from the mounted keys.toml [wallet-N] section (WALLET_NAME). There is NO
# host-side key parsing and NO secret files — this phase only validates that the
# keys.toml declares every wallet section the run needs. keys.toml is the single
# source of truth; nothing here extracts or writes key material.
#
# Dependencies: output.sh (info, pass, fail, error), config.sh (WITH_WALLET)
#
# Sourced by test_pipeline.sh after phase_03_prereqs.sh.

phase_wallet() {
    # Only run when wallets are explicitly enabled.
    if [ "${WITH_WALLET:-0}" -le 0 ]; then
        info "Phase 4: No wallets enabled (WITH_WALLET=$WITH_WALLET) — skipping"
        return 0
    fi

    local wallet_count="$WITH_WALLET"
    info "Phase 4: Validating wallet declarations ($wallet_count wallet(s))..."

    # keys.toml is the single source of truth for all identities (nodes + wallets).
    local key_config="${KEYS_FILE:-${SCRIPT_DIR}/keys.toml}"
    if [ ! -f "$key_config" ]; then
        error "Key config not found at $key_config — required (declares node + wallet identities)"
    fi
    info "  keys.toml: $key_config"

    # Assert each required [wallet-N] section is declared. Wallets derive their
    # own key from this section on boot — no export/import, no .secrets files.
    for i in $(seq 1 "$wallet_count"); do
        if grep -qE "^\[wallet-$i\]" "$key_config"; then
            pass "  wallet-$i declared in keys.toml"
        else
            error "keys.toml missing [wallet-$i] section — required for wallet-$i identity"
        fi
    done
}