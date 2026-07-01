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

    # If --keys is set, parse the keys TOML to export WALLET_SECRET for mining nodes.
    # WALLET_SECRET is hex — dwowd's resolve_mining_keypair() reads mining_secret file.
    # The entrypoint writes WALLET_SECRET to mining_secret before dwowd starts.
    if [ -n "${KEYS_FILE:-}" ]; then
        if [ ! -f "$KEYS_FILE" ]; then
            error "Keys file not found: $KEYS_FILE"
        fi
        info "  Loading keys from $KEYS_FILE..."
        NODE0_SECRET=$(python3 -c "
import sys
try:
    import tomllib
except ImportError:
    import tomli as tomllib
with open('${KEYS_FILE}', 'rb') as f:
    cfg = tomllib.load(f)
print(cfg.get('node0', {}).get('wallet_secret', ''))
" 2>/dev/null)
        if [ -n "$NODE0_SECRET" ] && [ "$NODE0_SECRET" != "None" ]; then
            export WALLET_SECRET="$NODE0_SECRET"
            info "  WALLET_SECRET exported for node0 (hex, from keys file)"
        fi
    fi

    # Load wallet secrets from keys.toml (single source of truth).
    # If --keys is not set, use default keys.toml in the pipeline directory.
    local key_config="${KEYS_FILE:-${SCRIPT_DIR}/keys.toml}"
    if [ ! -f "$key_config" ]; then
        error "Key config not found at $key_config — required for wallet keys"
    fi
    # Parse wallet secrets from keys.toml and write to files.
    # Uses Python to read TOML and write each secret directly to .secrets/.
    local secret_dir="${SCRIPT_DIR}/.secrets"
    mkdir -p "$secret_dir"

    for i in $(seq 1 "$wallet_count"); do
        local secret_val
        secret_val=$(python3 -c "
import sys
try:
    import tomllib
except ImportError:
    import tomli as tomllib
with open('${key_config}', 'rb') as f:
    cfg = tomllib.load(f)
key = cfg.get(f'wallet-${i}', {}).get('wallet_secret', '')
if key and len(key) == 64:
    print(key, end='')
" 2>/dev/null)

        if [ -z "$secret_val" ] || [ "${#secret_val}" -ne 64 ]; then
            error "Failed to read wallet-$i key from $key_config (got: ${secret_val:-empty})"
        fi

        eval "WALLET_SECRET_$i=\$secret_val"

        echo -n "$secret_val" > "${secret_dir}/dwow_mining_secret_$i" || \
            error "Failed to write secret file ${secret_dir}/dwow_mining_secret_$i"
        chmod 600 "${secret_dir}/dwow_mining_secret_$i" || true

        pass "  wallet-$i secret prepared"
        info "    Secret:  ${secret_val:0:16}..."
    done

    # --forward: pre-computed address from test-wallets.json (deterministic keys).
    # Export wallet-1 address as canonical.
    WALLET_ADDRESS="${WALLET_ADDRESS_1:-}"
    export WALLET_ADDRESS
}