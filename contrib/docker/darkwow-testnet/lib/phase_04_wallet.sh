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

    # Determine keys.toml path — single source of truth for all keys.
    local key_config="${KEYS_FILE:-${SCRIPT_DIR}/keys.toml}"
    if [ ! -f "$key_config" ]; then
        error "Key config not found at $key_config — required for wallet and mining keys"
    fi
    info "  Loading keys from $key_config..."

    # Single Python invocation: parse ALL keys (nodes + wallets) and write
    # a shell-parseable output. Errors are NOT swallowed — malformed TOML
    # or missing keys cause immediate failure.
    local parsed
    parsed=$(python3 -c "
import sys
try:
    import tomllib
except ImportError:
    import tomli as tomllib

try:
    with open('${key_config}', 'rb') as f:
        cfg = tomllib.load(f)
except Exception as e:
    print(f'KEY_ERROR=TOML parse failed: {e}', file=sys.stderr)
    sys.exit(1)

# Extract node keys (node0, node1, ...)
node_secrets = {}
for section in cfg:
    if section.startswith('node'):
        secret = cfg[section].get('wallet_secret', '')
        if secret and len(secret) == 64:
            node_secrets[section] = secret

if not node_secrets:
    print('KEY_ERROR=No valid node sections found in keys.toml', file=sys.stderr)
    sys.exit(1)

# Extract wallet keys (wallet-1, wallet-2, ...)
wallet_secrets = {}
for section in cfg:
    if section.startswith('wallet-'):
        secret = cfg[section].get('wallet_secret', '')
        if secret and len(secret) == 64:
            wallet_secrets[section] = secret

# Output shell-parseable lines
for name, secret in sorted(node_secrets.items()):
    print(f'NODE_SECRET_{name}=\"{secret}\"')
for name, secret in sorted(wallet_secrets.items()):
    # wallet-1 -> WALLET_SECRET_1
    idx = name.split('-')[1]
    print(f'WALLET_SECRET_{idx}=\"{secret}\"')
print(f'WALLET_COUNT={len(wallet_secrets)}')
" 2>&1)
    local parse_rc=$?
    if [ $parse_rc -ne 0 ]; then
        error "Failed to parse keys.toml: ${parsed}"
    fi

    # Check for KEY_ERROR in output
    if echo "$parsed" | grep -q "^KEY_ERROR="; then
        error "$(echo "$parsed" | grep '^KEY_ERROR=' | cut -d= -f2-)"
    fi

    # Source the parsed output into shell variables
    eval "$parsed"

    # Export per-node WALLET_SECRET env vars for docker-compose.
    # These replace the old single WALLET_SECRET — each node gets its own.
    if [ -n "${NODE_SECRET_node0:-}" ]; then
        export WALLET_SECRET_0="${NODE_SECRET_node0}"
        info "  WALLET_SECRET_0 exported for node0"
    fi
    if [ -n "${NODE_SECRET_node1:-}" ]; then
        export WALLET_SECRET_1="${NODE_SECRET_node1}"
        info "  WALLET_SECRET_1 exported for node1"
    fi

    # Write wallet secret files for bind-mount into wallet containers.
    # Uses printf (POSIX) instead of echo -n for portability.
    local secret_dir="${SCRIPT_DIR}/.secrets"
    mkdir -p "$secret_dir"

    for i in $(seq 1 "$wallet_count"); do
        local secret_var="WALLET_SECRET_$i"
        local secret_val="${!secret_var:-}"

        if [ -z "$secret_val" ] || [ "${#secret_val}" -ne 64 ]; then
            error "Failed to read wallet-$i key from $key_config (got: ${secret_val:-empty})"
        fi

        printf '%s' "$secret_val" > "${secret_dir}/dwow_mining_secret_$i" || \
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