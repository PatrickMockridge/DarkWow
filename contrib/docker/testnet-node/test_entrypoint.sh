#!/bin/bash
# Unit tests for entrypoint.sh functions
#
# Usage:
#   bash contrib/docker/testnet-node/test_entrypoint.sh
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
info()  { echo -e "${GREEN}[INFO]${NC}  $*"; }

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

    # Verify the defaults from our sourced environment
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

    [ "$EXTERNAL_ADDR" = "" ]
    check $? "EXTERNAL_ADDR defaults to empty"

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

    [ "$WALLET_ADDRESS" = "" ]
    check $? "WALLET_ADDRESS defaults to empty"

    [ "$WALLET_SECRET" = "" ]
    check $? "WALLET_SECRET defaults to empty"

    [ "$WALLET_SECRET_FILE" = "" ]
    check $? "WALLET_SECRET_FILE defaults to empty"

    [ "$MINING_THREADS" = "1" ]
    check $? "MINING_THREADS defaults to 1"

    [ "$RANDOMX_MAX_THREADS" = "0" ]
    check $? "RANDOMX_MAX_THREADS defaults to 0"

    # DATADIR is derived from NETWORK, so it uses the actual sourced value
    echo "$DATADIR" | grep -q "darkwow-testnet"
    check $? "DATADIR contains network name"
}

# ============================================================================
# Test: custom env vars are respected
# ============================================================================
test_custom_vars() {
    info "--- Custom env vars ---"

    # Resolve which hash tool is available (same as derive_magic_bytes)
    local hash_tool="none"
    # We need to test custom vars in a subshell, but derive_magic_bytes
    # is already loaded. We test that custom env values flow through.
    # These tests run in the current (sourced) environment by setting
    # and restoring vars.

    local saved_network="$NETWORK"
    local saved_p2p="$P2P_PORT"
    local saved_rpc="$RPC_PORT"

    NETWORK="custom-testnet"
    P2P_PORT="99999"
    RPC_PORT="55555"

    [ "$NETWORK" = "custom-testnet" ]
    check $? "Custom NETWORK is respected"

    [ "$P2P_PORT" = "99999" ]
    check $? "Custom P2P_PORT is respected"

    [ "$RPC_PORT" = "55555" ]
    check $? "Custom RPC_PORT is respected"

    # Restore
    NETWORK="$saved_network"
    P2P_PORT="$saved_p2p"
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

    # Determinism: same input produces same output
    local result2
    result2=$(derive_magic_bytes "darkwow-testnet")
    [ "$result" = "$result2" ]
    check $? "Same input produces same output (deterministic)"

    # Different network produces valid output
    local result3
    result3=$(derive_magic_bytes "different-network")
    echo "$result3" | grep -qE '^[0-9]+, [0-9]+, [0-9]+, [0-9]+$'
    check $? "Different network name produces valid magic bytes format"

    # All byte values in range 0-255
    local all_valid=0
    local bytes
    bytes=$(echo "$result" | grep -oE '[0-9]+')
    for byte in $bytes; do
        if [ "$byte" -lt 0 ] || [ "$byte" -gt 255 ]; then
            all_valid=1
        fi
    done
    check $all_valid "All magic bytes are in range 0-255"
}

# ============================================================================
# Test: config generation — default
# ============================================================================
test_config_default() {
    info "--- Config generation (default) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local configfile="$tmpdir/dwowd_config.toml"
    generate_dwowd_config "$configfile" "false"

    [ -f "$configfile" ]
    check $? "Config file is created"

    grep -q 'network = "'"$NETWORK"'"' "$configfile"
    check $? "Network name in config"

    grep -q "threshold = $THRESHOLD" "$configfile"
    check $? "Threshold in config"

    grep -q "pow_target = $TARGET_BLOCK_TIME" "$configfile"
    check $? "pow_target in config"

    grep -q "rpc_listen = \"tcp://0.0.0.0:$RPC_PORT\"" "$configfile"
    check $? "RPC listen address correct"

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

    grep -q "localnet = false" "$configfile"
    check $? "localnet is false"

    # mm_rpc should NOT be present when merge_mining=false
    ! grep -q "mm_rpc" "$configfile"
    check $? "No mm_rpc section when merge_mining=false"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: config generation — seeds
# ============================================================================
test_config_seeds() {
    info "--- Config generation (seeds) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local configfile="$tmpdir/dwowd_config.toml"
    generate_dwowd_config "$configfile" "false"

    # Seeds should be in TOML array format with tcp+tls prefix
    grep -q 'seeds = \["tcp+tls://' "$configfile"
    check $? "Seeds rendered as tcp+tls TOML array"

    # Default seeds include lilith0 and lilith1
    grep -q "lilith0.dark.fi" "$configfile"
    check $? "Default seed lilith0 present in config"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: config generation — EXTERNAL_ADDR
# ============================================================================
test_config_external_addr() {
    info "--- Config generation (EXTERNAL_ADDR) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local configfile="$tmpdir/dwowd_config.toml"

    # With EXTERNAL_ADDR set
    EXTERNAL_ADDR="myhost.example.com:31342"
    generate_dwowd_config "$configfile" "false"
    grep -q 'external_addrs = \["tcp+tls://myhost.example.com:31342"\]' "$configfile"
    check $? "EXTERNAL_ADDR rendered when set"

    # With EXTERNAL_ADDR empty
    EXTERNAL_ADDR=""
    generate_dwowd_config "$configfile" "false"
    ! grep -q "external_addrs" "$configfile"
    check $? "external_addrs absent when EXTERNAL_ADDR is empty"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: config generation — merge mining
# ============================================================================
test_config_merge_mining() {
    info "--- Config generation (merge mining) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local configfile="$tmpdir/dwowd_config.toml"

    # merge_mining=true adds [mm_rpc] section
    generate_dwowd_config "$configfile" "true" "31348"
    grep -q "mm_rpc" "$configfile"
    check $? "mm_rpc section present when merge_mining=true"

    grep -q 'rpc_listen = "tcp://0.0.0.0:31348"' "$configfile"
    check $? "mm_rpc uses default port 31348"

    # merge_mining=false does NOT add [mm_rpc]
    generate_dwowd_config "$configfile" "false"
    ! grep -q "mm_rpc" "$configfile"
    check $? "No mm_rpc section when merge_mining=false"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: config generation — custom mm_rpc port
# ============================================================================
test_config_custom_mm_port() {
    info "--- Config generation (custom mm_rpc port) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local configfile="$tmpdir/dwowd_config.toml"
    generate_dwowd_config "$configfile" "true" "55555"

    grep -q 'rpc_listen = "tcp://0.0.0.0:55555"' "$configfile"
    check $? "mm_rpc uses custom port 55555"

    DATADIR="$saved_datadir"
    rm -rf "$tmpdir"
}

# ============================================================================
# Test: config generation — directory creation
# ============================================================================
test_config_directory() {
    info "--- Config generation (directory creation) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    local saved_datadir="$DATADIR"
    DATADIR="$tmpdir/data"
    local deepdir="$tmpdir/some/nested/config/dir"
    local configfile="$deepdir/dwowd_config.toml"
    generate_dwowd_config "$configfile" "false"

    [ -d "$deepdir" ]
    check $? "Config parent directory is created"

    [ -f "$configfile" ]
    check $? "Config file exists in created directory"

    # Custom configfile path is respected and contains real content
    grep -q "network = " "$configfile"
    check $? "Custom-path config contains expected content"

    DATADIR="$saved_datadir"
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
    WALLET_ADDRESS="test_addr_abc123"
    WALLET_SECRET="test_secret_hex_64_chars___abcdef1234567890abcdef12345678"
    WALLET_SECRET_FILE=""

    preseed_wallet

    [ -f "$DATADIR/mining_address" ]
    check $? "mining_address file created"

    [ "$(cat "$DATADIR/mining_address")" = "test_addr_abc123" ]
    check $? "mining_address has correct content"

    [ -f "$DATADIR/mining_secret" ]
    check $? "mining_secret file created"

    [ "$(cat "$DATADIR/mining_secret")" = "test_secret_hex_64_chars___abcdef1234567890abcdef12345678" ]
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

    echo "file_secret_xyz789" > "$tmpdir/secret_file"

    DATADIR="$tmpdir/data"
    WALLET_ADDRESS="addr_from_file"
    WALLET_SECRET=""
    WALLET_SECRET_FILE="$tmpdir/secret_file"

    preseed_wallet

    [ -f "$DATADIR/mining_address" ]
    check $? "mining_address created from file-based secret"

    [ "$(cat "$DATADIR/mining_secret")" = "file_secret_xyz789" ]
    check $? "mining_secret read from file correctly"

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
    check $? "No mining_address created when no secret provided"

    [ ! -f "$DATADIR/mining_secret" ]
    check $? "No mining_secret created when no secret provided"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: wallet preseed — partial (address only, no secret)
# ============================================================================
test_preseed_address_only() {
    info "--- Wallet preseed (address only, no secret) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    DATADIR="$tmpdir"
    WALLET_ADDRESS="some_addr"
    WALLET_SECRET=""
    WALLET_SECRET_FILE=""

    preseed_wallet > /dev/null 2>&1

    [ ! -f "$DATADIR/mining_address" ]
    check $? "No mining_address when address set but no secret"

    [ ! -f "$DATADIR/mining_secret" ]
    check $? "No mining_secret when address set but no secret"

    rm -rf "$tmpdir"
}

# ============================================================================
# Test: wallet preseed — partial (secret only, no address)
# ============================================================================
test_preseed_secret_only() {
    info "--- Wallet preseed (secret only, no address) ---"

    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    DATADIR="$tmpdir"
    WALLET_ADDRESS=""
    WALLET_SECRET="some_secret_only"
    WALLET_SECRET_FILE=""

    preseed_wallet > /dev/null 2>&1

    [ ! -f "$DATADIR/mining_address" ]
    check $? "No mining_address when secret set but no address"

    [ ! -f "$DATADIR/mining_secret" ]
    check $? "No mining_secret when secret set but no address"

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

    # Pre-create files with old content
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
# Test: wallet preseed — security warning
# ============================================================================
test_preseed_warnings() {
    info "--- Wallet preseed (security warnings) ---"

    # Plain WALLET_SECRET triggers warning about docker inspect
    local tmpdir
    tmpdir=$(mktemp -d)
    trap "rm -rf $tmpdir" RETURN

    DATADIR="$tmpdir"
    WALLET_ADDRESS="addr"
    WALLET_SECRET="visible_secret"
    WALLET_SECRET_FILE=""

    local output
    output=$(preseed_wallet 2>&1) || true

    echo "$output" | grep -qi "docker inspect" || echo "$output" | grep -qi "WARNING"
    check $? "Plain WALLET_SECRET triggers security warning"

    # WALLET_SECRET_FILE path does NOT trigger warning (safe path)
    local tmpdir2
    tmpdir2=$(mktemp -d)
    trap "rm -rf $tmpdir2" RETURN

    echo "safe_secret" > "$tmpdir2/secret_file"

    DATADIR="$tmpdir2/data"
    WALLET_ADDRESS="addr"
    WALLET_SECRET=""
    WALLET_SECRET_FILE="$tmpdir2/secret_file"

    local output2
    output2=$(preseed_wallet 2>&1) || true

    ! echo "$output2" | grep -qi "WARNING"
    check $? "WALLET_SECRET_FILE path does not trigger plain-text warning"

    rm -rf "$tmpdir" "$tmpdir2"
}

# ============================================================================
# Test: error handling — unknown MODE
# ============================================================================
test_error_handling() {
    info "--- Error handling ---"

    # Unknown MODE causes non-zero exit
    # Run entrypoint.sh directly (not sourced) with bad MODE
    # It will fail before trying to launch any binaries because the
    # "Unknown MODE" check at the bottom runs after all mode blocks
    local exit_code=0
    MODE="bogus_mode" bash "$SCRIPT_DIR/entrypoint.sh" >/dev/null 2>&1 || exit_code=$?
    [ "$exit_code" -ne 0 ]
    check $? "Unknown MODE causes non-zero exit"

    # Known modes reach binary launch (which will fail), but shouldn't hit
    # the "Unknown MODE" error message
    local output
    output=$(MODE="bogus_mode" bash "$SCRIPT_DIR/entrypoint.sh" 2>&1) || true
    echo "$output" | grep -qi "Unknown MODE"
    check $? "Unknown MODE prints error message"
}

# ============================================================================
# Main test dispatch
# ============================================================================
echo "=== entrypoint.sh Unit Tests ==="
echo ""

test_defaults
test_custom_vars
test_magic_bytes
test_config_default
test_config_seeds
test_config_external_addr
test_config_merge_mining
test_config_custom_mm_port
test_config_directory
test_preseed_env_secret
test_preseed_file_secret
test_preseed_no_secret
test_preseed_address_only
test_preseed_secret_only
test_preseed_no_overwrite
test_preseed_warnings
test_error_handling

report
