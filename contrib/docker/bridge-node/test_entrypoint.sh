#!/bin/bash
# Unit tests for bridge-node entrypoint.sh functions
#
# Usage:
#   bash contrib/docker/bridge-node/test_entrypoint.sh
#
# No Docker or binaries required. Uses temp dirs for file I/O.
# Exit 0 on success, non-zero on any failure.

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0; FAIL=0
pass() { echo -e "${GREEN}[PASS]${NC} $*"; PASS=$((PASS + 1)); }
fail() { echo -e "${RED}[FAIL]${NC} $*"; FAIL=$((FAIL + 1)); }
check() {
    if [ "$1" -eq 0 ]; then pass "$2"; else fail "$2"; fi
}
info()  { echo -e "${YELLOW}[INFO]${NC}  $*"; }

report() {
    echo ""
    echo "==========================================="
    echo -e "  ${GREEN}PASS: $PASS${NC}  ${RED}FAIL: $FAIL${NC}"
    echo "==========================================="
    if [ "$FAIL" -gt 0 ]; then
        echo -e "${RED}Some tests failed${NC}"
        exit 1
    fi
    echo -e "${GREEN}All tests passed${NC}"
}

# Source only the function definitions (not the main flow)
ENTRYPOINT_SOURCE_ONLY=1
# shellcheck source=./entrypoint.sh
source "$SCRIPT_DIR/entrypoint.sh"

# ============================================================================
# Test: env var defaults
# ============================================================================
test_defaults() {
    info "--- Env var defaults ---"

    [ "$MODE" = "full" ]
    check $? "MODE defaults to full"

    [ "$NETWORK" = "darkwow-testnet" ]
    check $? "NETWORK defaults to darkwow-testnet"

    [ "$P2P_PORT" = "31342" ]
    check $? "P2P_PORT defaults to 31342"

    [ "$RPC_PORT" = "31345" ]
    check $? "RPC_PORT defaults to 31345"

    [ "$STRATUM_PORT" = "31347" ]
    check $? "STRATUM_PORT defaults to 31347"

    [ "$MANAGEMENT_PORT" = "31346" ]
    check $? "MANAGEMENT_PORT defaults to 31346"

    [ "$SEED_ADDR" = "lilith0.dark.fi:31340,lilith1.dark.fi:31340" ]
    check $? "SEED_ADDR defaults to public testnet seeds"

    [ "$THRESHOLD" = "3" ]
    check $? "THRESHOLD defaults to 3"

    [ "$TARGET_BLOCK_TIME" = "120" ]
    check $? "TARGET_BLOCK_TIME defaults to 120"

    [ "$SKIP_SYNC" = "false" ]
    check $? "SKIP_SYNC defaults to false"

    [ "$SKIP_FEES" = "false" ]
    check $? "SKIP_FEES defaults to false"

    [ "$LOCALNET" = "false" ]
    check $? "LOCALNET defaults to false"
}

# ============================================================================
# Test: bridge-specific defaults
# ============================================================================
test_bridge_defaults() {
    info "--- Bridge-specific defaults ---"

    [ "$BRIDGE_RELAYER_FEE_BP" = "100" ]
    check $? "BRIDGE_RELAYER_FEE_BP defaults to 100 (1%)"

    [ "$BRIDGE_TIMEOUT_BLOCKS" = "100" ]
    check $? "BRIDGE_TIMEOUT_BLOCKS defaults to 100"

    [ "$ETH_ENABLED" = "false" ]
    check $? "ETH_ENABLED defaults to false"

    [ "$XMR_ENABLED" = "false" ]
    check $? "XMR_ENABLED defaults to false"

    [ "$ZEC_ENABLED" = "false" ]
    check $? "ZEC_ENABLED defaults to false"

    [ "$AZT_ENABLED" = "false" ]
    check $? "AZT_ENABLED defaults to false"

    [ "$LTC_ENABLED" = "false" ]
    check $? "LTC_ENABLED defaults to false"

    [ "$POLL_INTERVAL_SECS" = "10" ]
    check $? "POLL_INTERVAL_SECS defaults to 10"

    [ "$MAX_CONCURRENT_WITHDRAWALS" = "10" ]
    check $? "MAX_CONCURRENT_WITHDRAWALS defaults to 10"

    [ "$RELAYER_TIMEOUT_BLOCKS" = "100" ]
    check $? "RELAYER_TIMEOUT_BLOCKS defaults to 100"

    [ "$RELAYER_FEE_PERCENTAGE" = "1" ]
    check $? "RELAYER_FEE_PERCENTAGE defaults to 1"

    [ "$DARKFID_URL" = "tcp://127.0.0.1:${RPC_PORT}" ]
    check $? "DARKFID_URL defaults to localhost with RPC_PORT"
}

# ============================================================================
# Test: custom env vars are respected
# ============================================================================
test_custom_vars() {
    info "--- Custom env vars ---"

    local saved_network="$NETWORK"
    local saved_mode="$MODE"
    local saved_rpc="$RPC_PORT"

    NETWORK="custom-bridge-net"
    MODE="relayer-only"
    RPC_PORT="55555"

    [ "$NETWORK" = "custom-bridge-net" ]
    check $? "Custom NETWORK respected"

    [ "$MODE" = "relayer-only" ]
    check $? "Custom MODE respected"

    [ "$RPC_PORT" = "55555" ]
    check $? "Custom RPC_PORT respected"

    NETWORK="$saved_network"
    MODE="$saved_mode"
    RPC_PORT="$saved_rpc"
}

# ============================================================================
# Test: magic bytes derivation
# ============================================================================
test_magic_bytes() {
    info "--- Magic bytes derivation ---"

    type derive_magic_bytes >/dev/null 2>&1
    check $? "derive_magic_bytes function is defined"

    local result
    result=$(derive_magic_bytes "darkwow-testnet")
    [ -n "$result" ]
    check $? "derive_magic_bytes produces non-empty output"

    echo "$result" | grep -qE '^[0-9]+, [0-9]+, [0-9]+, [0-9]+$'
    check $? "Output format: four comma-separated unsigned bytes"

    # Determinism
    local result2
    result2=$(derive_magic_bytes "darkwow-testnet")
    [ "$result" = "$result2" ]
    check $? "Same input produces same output (deterministic)"

    # Different network produces valid output
    local result3
    result3=$(derive_magic_bytes "bridge-network")
    echo "$result3" | grep -qE '^[0-9]+, [0-9]+, [0-9]+, [0-9]+$'
    check $? "Different network name produces valid magic bytes format"
}

# ============================================================================
# Test: dwowd config generation — default
# ============================================================================
test_dwowd_config_default() {
    info "--- dwowd config generation (default) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local configfile="$tmpdir/dwowd_config.toml"
    generate_dwowd_config "$configfile"

    [ -f "$configfile" ]
    check $? "Config file created"

    grep -q "network = \"$NETWORK\"" "$configfile"
    check $? "Network name in config"

    grep -q "threshold = $THRESHOLD" "$configfile"
    check $? "Threshold in config"

    grep -q "rpc_listen = \"tcp://0.0.0.0:$RPC_PORT\"" "$configfile"
    check $? "RPC listen with correct port"

    grep -q "stratum_rpc" "$configfile"
    check $? "Stratum section present"

    grep -q "management_rpc" "$configfile"
    check $? "Management RPC section present"

    grep -q "inbound = .*tcp+tls://0.0.0.0:$P2P_PORT" "$configfile"
    check $? "P2P inbound with correct port"

    grep -q "magic_bytes = " "$configfile"
    check $? "Magic bytes present"

    grep -q "hostlist = " "$configfile"
    check $? "Hostlist path present"

    grep -q "active_profiles = .*tcp+tls" "$configfile"
    check $? "tcp+tls in active_profiles"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: dwowd config generation — seeds
# ============================================================================
test_dwowd_config_seeds() {
    info "--- dwowd config generation (seeds) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local configfile="$tmpdir/dwowd_config.toml"
    generate_dwowd_config "$configfile"

    grep -q 'seeds = \["tcp+tls://' "$configfile"
    check $? "Seeds rendered as tcp+tls TOML array"

    grep -q "lilith0.dark.fi" "$configfile"
    check $? "Default seed lilith0 present in config"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: dwowd config generation — EXTERNAL_ADDR
# ============================================================================
test_dwowd_config_external_addr() {
    info "--- dwowd config generation (EXTERNAL_ADDR) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local configfile="$tmpdir/dwowd_config.toml"

    EXTERNAL_ADDR="bridge.example.com:31342"
    generate_dwowd_config "$configfile"
    grep -q 'external_addrs = \["tcp+tls://bridge.example.com:31342"\]' "$configfile"
    check $? "EXTERNAL_ADDR rendered when set"

    EXTERNAL_ADDR=""
    generate_dwowd_config "$configfile"
    ! grep -q "external_addrs" "$configfile"
    check $? "external_addrs absent when EXTERNAL_ADDR empty"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: dwowd config generation — directory creation
# ============================================================================
test_dwowd_config_directory() {
    info "--- dwowd config generation (directory creation) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local deepdir="$tmpdir/some/nested/config/dir"
    local configfile="$deepdir/dwowd_config.toml"
    generate_dwowd_config "$configfile"

    [ -d "$deepdir" ]
    check $? "Config parent directory created"

    [ -f "$configfile" ]
    check $? "Config file exists in created directory"

    grep -q "network = " "$configfile"
    check $? "Custom-path config contains expected content"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: relayer config generation — default
# ============================================================================
test_relayer_config_default() {
    info "--- Relayer config generation (default) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local configfile="$tmpdir/universal_relayer.toml"
    generate_relayer_config "$configfile"

    [ -f "$configfile" ]
    check $? "Relayer config file created"

    grep -q "darkfid_url = " "$configfile"
    check $? "darkfid_url present in relayer config"

    grep -q "poll_interval_secs = $POLL_INTERVAL_SECS" "$configfile"
    check $? "poll_interval_secs in relayer config"

    grep -q "max_concurrent_withdrawals = $MAX_CONCURRENT_WITHDRAWALS" "$configfile"
    check $? "max_concurrent_withdrawals in relayer config"

    grep -q "\[ethereum\]" "$configfile"
    check $? "Ethereum section present"

    grep -q "\[monero\]" "$configfile"
    check $? "Monero section present"

    grep -q "\[zcash\]" "$configfile"
    check $? "Zcash section present"

    grep -q "\[litecoin\]" "$configfile"
    check $? "Litecoin section present"

    grep -q "\[aztec\]" "$configfile"
    check $? "Aztec section present"

    grep -q "\[relayer\]" "$configfile"
    check $? "Relayer section present"

    grep -q "timeout_blocks = $RELAYER_TIMEOUT_BLOCKS" "$configfile"
    check $? "timeout_blocks in relayer config"

    grep -q "fee_percentage = $RELAYER_FEE_PERCENTAGE" "$configfile"
    check $? "fee_percentage in relayer config"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: relayer config generation — chain enables
# ============================================================================
test_relayer_config_chains() {
    info "--- Relayer config generation (chain enables) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local configfile="$tmpdir/universal_relayer.toml"

    # All disabled by default
    generate_relayer_config "$configfile"
    grep -q "enabled = false" "$configfile"
    check $? "Chains default to disabled"

    # Enable specific chains
    ETH_ENABLED="true"
    XMR_ENABLED="true"
    generate_relayer_config "$configfile"

    grep -A1 "\[ethereum\]" "$configfile" | grep -q "enabled = true"
    check $? "Ethereum enabled when ETH_ENABLED=true"

    grep -A1 "\[monero\]" "$configfile" | grep -q "enabled = true"
    check $? "Monero enabled when XMR_ENABLED=true"

    grep -A1 "\[zcash\]" "$configfile" | grep -q "enabled = false"
    check $? "Zcash still disabled"

    # Restore defaults
    ETH_ENABLED="false"
    XMR_ENABLED="false"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: relayer config generation — custom DARKFID_URL
# ============================================================================
test_relayer_config_url() {
    info "--- Relayer config generation (custom DARKFID_URL) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local configfile="$tmpdir/universal_relayer.toml"

    DARKFID_URL="tcp://192.168.1.100:31345"
    generate_relayer_config "$configfile"

    grep -q 'darkfid_url = "tcp://192.168.1.100:31345"' "$configfile"
    check $? "Custom DARKFID_URL in relayer config"

    # Restore
    DARKFID_URL="tcp://127.0.0.1:${RPC_PORT}"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: relayer config generation — directory creation
# ============================================================================
test_relayer_config_directory() {
    info "--- Relayer config generation (directory creation) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local deepdir="$tmpdir/some/nested/relayer"
    local configfile="$deepdir/universal_relayer.toml"
    generate_relayer_config "$configfile"

    [ -d "$deepdir" ]
    check $? "Relayer config parent directory created"

    [ -f "$configfile" ]
    check $? "Relayer config file exists in created directory"

    grep -q "\[darkfi\]" "$configfile"
    check $? "Custom-path relayer config contains expected content"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: wallet preseed — env secret
# ============================================================================
test_preseed_env_secret() {
    info "--- Wallet preseed (env secret) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    DATADIR="$tmpdir"
    WALLET_ADDRESS="bridge_addr_abc123"
    WALLET_SECRET="bridge_secret_64_chars_abcdef1234567890abcdef12345678"
    WALLET_SECRET_FILE=""

    preseed_wallet

    [ -f "$DATADIR/mining_address" ]
    check $? "mining_address file created"

    [ "$(cat "$DATADIR/mining_address")" = "bridge_addr_abc123" ]
    check $? "mining_address has correct content"

    [ -f "$DATADIR/mining_secret" ]
    check $? "mining_secret file created"

    [ "$(cat "$DATADIR/mining_secret")" = "bridge_secret_64_chars_abcdef1234567890abcdef12345678" ]
    check $? "mining_secret has correct content"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: wallet preseed — file secret
# ============================================================================
test_preseed_file_secret() {
    info "--- Wallet preseed (file secret) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    echo "file_bridge_secret_xyz789" > "$tmpdir/secret_file"

    DATADIR="$tmpdir/data"
    WALLET_ADDRESS="addr_from_file"
    WALLET_SECRET=""
    WALLET_SECRET_FILE="$tmpdir/secret_file"

    preseed_wallet

    [ -f "$DATADIR/mining_address" ]
    check $? "mining_address created from file-based secret"

    [ "$(cat "$DATADIR/mining_secret")" = "file_bridge_secret_xyz789" ]
    check $? "mining_secret read from file correctly"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: wallet preseed — no overwrite
# ============================================================================
test_preseed_no_overwrite() {
    info "--- Wallet preseed (no overwrite) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    DATADIR="$tmpdir"
    WALLET_ADDRESS="new_addr"
    WALLET_SECRET="new_secret"

    echo "old_address" > "$DATADIR/mining_address"
    echo "old_secret" > "$DATADIR/mining_secret"

    preseed_wallet > /dev/null 2>&1

    [ "$(cat "$DATADIR/mining_address")" = "old_address" ]
    check $? "Existing mining_address not overwritten"

    [ "$(cat "$DATADIR/mining_secret")" = "old_secret" ]
    check $? "Existing mining_secret not overwritten"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: wallet preseed — no secret
# ============================================================================
test_preseed_no_secret() {
    info "--- Wallet preseed (no secret) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    DATADIR="$tmpdir"
    WALLET_ADDRESS=""
    WALLET_SECRET=""
    WALLET_SECRET_FILE=""

    preseed_wallet > /dev/null 2>&1

    [ ! -f "$DATADIR/mining_address" ]
    check $? "No mining_address when no secret"

    [ ! -f "$DATADIR/mining_secret" ]
    check $? "No mining_secret when no secret"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: error handling — unknown MODE
# ============================================================================
test_error_handling() {
    info "--- Error handling ---"

    local exit_code=0
    MODE="bogus_mode" bash "$SCRIPT_DIR/entrypoint.sh" >/dev/null 2>&1 || exit_code=$?
    [ "$exit_code" -ne 0 ]
    check $? "Unknown MODE causes non-zero exit"

    local output
    output=$(MODE="bogus_mode" bash "$SCRIPT_DIR/entrypoint.sh" 2>&1) || true
    echo "$output" | grep -qi "Unknown MODE"
    check $? "Unknown MODE prints error message"

    echo "$output" | grep -qi "full, relayer-only, lilith"
    check $? "Error message lists valid modes"
}

# ============================================================================
# Test: error handling — lilith mode reaches binary launch
# ============================================================================
test_lilith_mode_attempts_launch() {
    info "--- Lilith mode attempts launch ---"

    local output
    output=$(MODE="lilith" bash "$SCRIPT_DIR/entrypoint.sh" 2>&1) || true

    echo "$output" | grep -qi "lilith"
    check $? "Lilith mode identified in output"

    # Should print mode banner before failing on missing binary
    echo "$output" | grep -qi "P2P seed"
    check $? "Lilith mode prints P2P seed banner"
}

# ============================================================================
# Test: source-only guard
# ============================================================================
test_source_only_guard() {
    info "--- Source-only guard ---"

    # When ENTRYPOINT_SOURCE_ONLY is set and script is run directly,
    # the guard exits early after the banner but before mode dispatching
    local output
    output=$(ENTRYPOINT_SOURCE_ONLY=1 bash "$SCRIPT_DIR/entrypoint.sh" 2>&1) || true

    # Banner is printed before guard, but no mode-specific output
    echo "$output" | grep -q "DarkWow Bridge Node"
    check $? "Source-only mode prints banner before exiting"

    # Should NOT contain mode dispatching output
    ! echo "$output" | grep -qi "Starting\|Mode:"
    check $? "Source-only mode exits before mode dispatching"

    # Functions should still be defined after sourcing with guard
    type generate_relayer_config >/dev/null 2>&1
    check $? "generate_relayer_config defined after source-only guard"
}

# ============================================================================
# Test: relayer config — per-chain RPC URLs
# ============================================================================
test_relayer_config_chain_urls() {
    info "--- Relayer config per-chain RPC URLs ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local configfile="$tmpdir/universal_relayer.toml"

    ETH_NODE_URL="https://custom-eth.example.com"
    XMR_NODE_RPC_URL="http://custom-xmr.example.com:18081"
    ZEC_NODE_RPC_URL="http://custom-zec.example.com:8232"
    generate_relayer_config "$configfile"

    grep -q "node_url = \"https://custom-eth.example.com\"" "$configfile"
    check $? "Custom ETH_NODE_URL in config"

    grep -q "node_rpc_url = \"http://custom-xmr.example.com:18081\"" "$configfile"
    check $? "Custom XMR_NODE_RPC_URL in config"

    grep -q "node_rpc_url = \"http://custom-zec.example.com:8232\"" "$configfile"
    check $? "Custom ZEC_NODE_RPC_URL in config"

    # Restore
    ETH_NODE_URL=""
    XMR_NODE_RPC_URL=""
    ZEC_NODE_RPC_URL=""

    rm -rf "$tmpdir"
}

# ============================================================================
# Main test dispatch
# ============================================================================
echo "=== bridge-node entrypoint.sh Unit Tests ==="
echo ""

test_defaults
test_bridge_defaults
test_custom_vars
test_magic_bytes
test_dwowd_config_default
test_dwowd_config_seeds
test_dwowd_config_external_addr
test_dwowd_config_directory
test_relayer_config_default
test_relayer_config_chains
test_relayer_config_url
test_relayer_config_directory
test_relayer_config_chain_urls
test_preseed_env_secret
test_preseed_file_secret
test_preseed_no_overwrite
test_preseed_no_secret
test_error_handling
test_lilith_mode_attempts_launch
test_source_only_guard

report
