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

    # If --keys is set, parse the keys TOML to get per-node/per-wallet secrets.
    if [ -n "${KEYS_FILE:-}" ]; then
        if [ ! -f "$KEYS_FILE" ]; then
            error "Keys file not found: $KEYS_FILE"
        fi
        info "  Loading keys from $KEYS_FILE..."
        # Export node0 WALLET_SECRET (base58) and WALLET_ADDRESS for docker-compose.
        # WALLET_SECRET lets the miner use a pre-configured key via entrypoint.
        # WALLET_ADDRESS pre-seeds the mining address file that dwowd reads on startup.
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
        if [ -n "$NODE0_SECRET" ]; then
            WALLET_SECRET=$(echo -n "$NODE0_SECRET" | xxd -r -p | bs58 2>/dev/null || echo "")
            export WALLET_SECRET
            info "  WALLET_SECRET exported for node0 (from keys file)"
        fi
        # Export WALLET_ADDRESS from test-wallets.json (deterministic, same key as wallet-1)
        local wallets_json="${SCRIPT_DIR}/test-wallets.json"
        if [ -f "$wallets_json" ]; then
            WALLET_ADDRESS=$(python3 -c "
import json
with open('${wallets_json}') as f:
    print(json.load(f)[0]['address'])
" 2>/dev/null)
            if [ -n "$WALLET_ADDRESS" ]; then
                export WALLET_ADDRESS
                info "  WALLET_ADDRESS=$WALLET_ADDRESS (for node0 mining address)"
            fi
        fi
    fi

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
        echo -n "$secret_val" > "${secret_dir}/dwow_mining_secret_$i" || \
            error "Failed to write secret file ${secret_dir}/dwow_mining_secret_$i"
        chmod 600 "${secret_dir}/dwow_mining_secret_$i" || true

        pass "  wallet-$i secret prepared"
        info "    Secret:  ${secret_val:0:16}..."
    done

    # --forward: pre-computed address from test-wallets.json (deterministic keys).
    # Mining nodes pick up FORWARD_DESTINATION and WALLET_SECRET from the env.
    if [ "${FORWARD_ENABLED:-false}" = "true" ]; then
        local wallets_json="${SCRIPT_DIR}/test-wallets.json"
        if [ -f "$wallets_json" ]; then
            FORWARD_DESTINATION=$(python3 -c "
import json
with open('${wallets_json}') as f:
    print(json.load(f)[0]['address'])
" 2>/dev/null)
            if [ -n "$FORWARD_DESTINATION" ]; then
                export FORWARD_DESTINATION
                info "FORWARD_DESTINATION=$FORWARD_DESTINATION (from test-wallets.json)"
            else
                fail "failed to read wallet-1 address from test-wallets.json"
            fi
        else
            fail "test-wallets.json not found at $wallets_json"
        fi

        # Export deterministic test secret so mining nodes use the same key as wallet.
        # Test key hex 0000...0001 → bs58 11111111111111111111111111111112.
        # The mining node entrypoint reads WALLET_SECRET and uses it as mining keypair.
        local test_secret="11111111111111111111111111111112"
        export WALLET_SECRET="$test_secret"
        info "WALLET_SECRET exported for mining nodes (shared with wallet-1)"
    fi

    # Export wallet-1 address as canonical.
    WALLET_ADDRESS="${WALLET_ADDRESS_1:-}"
    export WALLET_ADDRESS
}