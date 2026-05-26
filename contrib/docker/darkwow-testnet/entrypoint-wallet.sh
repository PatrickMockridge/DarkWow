#!/bin/bash
# DarkWow Wallet Container Entrypoint
#
# Generates dww wallet config from environment variables at container start.
# Supports two modes via WALLET_MODE env var:
#   test        — auto-init, scan, run position, assert output, exit with status
#   interactive — init wallet, then sleep infinity for docker exec access
#
# Usage:
#   docker compose --profile native up -d wallet         # interactive (default)
#   WALLET_MODE=test docker compose --profile native up -d wallet  # CI/test

set -e

echo "=== DarkWow Wallet Container ==="

# --- Configuration from environment ---
WALLET_MODE="${WALLET_MODE:-interactive}"
NETWORK="${NETWORK:-darkwow-testnet}"
RPC_URL="${RPC_URL:-tcp://node0:31345}"
WALLET_SECRET="${WALLET_SECRET:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
WALLET_PASS="${WALLET_PASS:-walletpass}"
CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dww/${NETWORK}}"
CACHEDIR="${CACHEDIR:-/root/.local/share/dwow/dww/${NETWORK}/cache}"

echo "  MODE=$WALLET_MODE  NETWORK=$NETWORK"
echo "  RPC=$RPC_URL  DATA=$DATADIR"

# --- Generate dww config ---
mkdir -p "$CONFIGDIR" "$DATADIR" "$CACHEDIR"

CONFIGFILE="${CONFIGDIR}/drk.toml"

cat > "$CONFIGFILE" << DWWEOF
network = "${NETWORK}"

[network_config."${NETWORK}"]
cache_path = "${CACHEDIR}"
wallet_path = "${DATADIR}/wallet.db"
wallet_pass = "${WALLET_PASS}"
endpoint = "${RPC_URL}"
history_path = "${DATADIR}/history.txt"
DWWEOF

echo "  Config written to $CONFIGFILE"

# --- Resolve wallet secret ---
RESOLVED_SECRET=""
if [ -n "$WALLET_SECRET_FILE" ] && [ -f "$WALLET_SECRET_FILE" ]; then
    RESOLVED_SECRET=$(cat "$WALLET_SECRET_FILE")
    echo "  Wallet secret loaded from file"
elif [ -n "$WALLET_SECRET" ]; then
    echo "  WARNING: WALLET_SECRET from environment is visible in docker inspect."
    echo "  Use WALLET_SECRET_FILE instead for production."
    RESOLVED_SECRET="$WALLET_SECRET"
fi

# --- Initialize wallet ---
echo "  Initializing wallet..."
/app/dww -c "$CONFIGFILE" wallet init 2>&1 || true

# --- Generate or import keypair ---
if [ -n "$RESOLVED_SECRET" ]; then
    echo "  Importing wallet key..."
    # dww wallet import — write the secret to a temp file and import
    echo "$RESOLVED_SECRET" > /tmp/wallet_secret_hex
    /app/dww -c "$CONFIGFILE" wallet import /tmp/wallet_secret_hex 2>&1 || {
        echo "  WARNING: wallet import failed (may already exist or be unsupported)"
    }
    rm -f /tmp/wallet_secret_hex
else
    echo "  Generating new keypair..."
    /app/dww -c "$CONFIGFILE" wallet keygen 2>&1 || {
        echo "  WARNING: wallet keygen failed (key may already exist)"
    }
fi

# --- Display wallet address ---
echo "  Wallet address:"
/app/dww -c "$CONFIGFILE" wallet address 2>&1 || echo "  (could not retrieve address)"

# ============================================================================
# MODE: test — scan, resolve position, verify output, exit
# ============================================================================
if [ "$WALLET_MODE" = "test" ]; then
    echo ""
    echo "=== Test Mode ==="

    FAIL=0

    # Scan blockchain for coins
    echo "  Scanning blockchain..."
    /app/dww -c "$CONFIGFILE" scan 2>&1
    echo "  Scan complete."

    # Run position resolution
    echo "  Running position resolution..."
    POS_OUTPUT=$(/app/dww -c "$CONFIGFILE" position 2>&1)
    echo "$POS_OUTPUT"

    # Verify position output
    if echo "$POS_OUTPUT" | grep -q "Capabilities"; then
        echo "  PASS: position shows Capabilities section"
    else
        echo "  FAIL: position missing Capabilities section"
        FAIL=1
    fi

    if echo "$POS_OUTPUT" | grep -q "Descriptors loaded"; then
        echo "  PASS: position reports descriptors count"
    else
        echo "  FAIL: position missing descriptors count"
        FAIL=1
    fi

    if echo "$POS_OUTPUT" | grep -q "Coin worth"; then
        echo "  PASS: position shows coin capabilities"
    else
        echo "  WARN: position shows no coin capabilities (wallet may have no coins yet)"
    fi

    if echo "$POS_OUTPUT" | grep -q "No actions available\|Available Actions"; then
        echo "  PASS: position reports actions status"
    else
        echo "  FAIL: position missing actions status"
        FAIL=1
    fi

    echo ""
    if [ "$FAIL" -eq 0 ]; then
        echo "=== Wallet test PASSED ==="
        exit 0
    else
        echo "=== Wallet test FAILED ==="
        exit 1
    fi
fi

# ============================================================================
# MODE: interactive (default) — keep container alive for docker exec
# ============================================================================
echo ""
echo "=== Interactive Mode ==="
echo "  Wallet ready. Use 'docker exec dwow-wallet dww -c $CONFIGFILE <command>'"
echo "  Examples:"
echo "    docker exec dwow-wallet dww -c $CONFIGFILE position"
echo "    docker exec dwow-wallet dww -c $CONFIGFILE wallet balance"
echo "    docker exec dwow-wallet dww -c $CONFIGFILE scan"
echo ""

# Sleep indefinitely so the container stays up for docker exec
exec sleep infinity
