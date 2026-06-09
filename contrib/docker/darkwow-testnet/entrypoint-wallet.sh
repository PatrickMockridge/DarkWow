#!/bin/bash
# DarkWow Wallet Container Entrypoint
#
# Generates dwow_wallet wallet config from environment variables at container start.
# Supports two modes via WALLET_MODE env var:
#   test        — auto-init, scan, run position, assert output, exit with status
#   interactive — init wallet, then sleep infinity for docker exec access
#
# WALLET_INDEX (default 1) is used when running multiple wallet containers
# to give each its own data directory.
#
# Usage:
#   docker run -e WALLET_INDEX=1 darkwow-wallet       # wallet-1 (interactive)
#   docker run -e WALLET_INDEX=2 darkwow-wallet       # wallet-2 (interactive)
#   WALLET_MODE=test docker compose --profile wallet up -d  # CI/test

set -e

echo "=== DarkWow Wallet Container ==="

# --- Configuration from environment ---
WALLET_MODE="${WALLET_MODE:-interactive}"
WALLET_INDEX="${WALLET_INDEX:-1}"
NETWORK="${NETWORK:-darkwow-testnet}"
RPC_URL="${RPC_URL:-tcp://node0:31345}"
WALLET_SECRET="${WALLET_SECRET:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
WALLET_PASS="${WALLET_PASS:-walletpass}"

# Thread containment — prevents wallet containers from consuming all CPUs.
# Must match entrypoint.sh default. Controls both smol executor and rayon pool.
DWOW_RAYON_THREADS="${DWOW_RAYON_THREADS:-2}"
export RAYON_NUM_THREADS="${DWOW_RAYON_THREADS}"
CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dww/${NETWORK}/wallet-${WALLET_INDEX}}"
CACHEDIR="${CACHEDIR:-/root/.local/share/dwow/dww/${NETWORK}/wallet-${WALLET_INDEX}/cache}"

echo "  MODE=$WALLET_MODE  NETWORK=$NETWORK  INDEX=$WALLET_INDEX"
echo "  RPC=$RPC_URL  DATA=$DATADIR"

# --- Generate dwow_wallet config ---
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
    RESOLVED_SECRET=$(cat "$WALLET_SECRET_FILE" | tr -d '[:space:]')
    echo "  Wallet secret loaded from file"
elif [ -n "$WALLET_SECRET" ]; then
    echo "  WARNING: WALLET_SECRET from environment is visible in docker inspect."
    echo "  Use WALLET_SECRET_FILE instead for production."
    RESOLVED_SECRET="$(echo "$WALLET_SECRET" | tr -d '[:space:]')"
elif [ -f /run/secrets/mining_secret ]; then
    RESOLVED_SECRET=$(cat /run/secrets/mining_secret | tr -d '[:space:]')
    WALLET_SECRET_FILE=/run/secrets/mining_secret
    echo "  Wallet secret loaded from /run/secrets/mining_secret"
fi

# --- Initialize wallet ---
echo "  Initializing wallet..."
/app/dwow_wallet -c "$CONFIGFILE" wallet initialize 2>&1 || {
    echo "  WARNING: wallet initialize failed (may already be initialized)"
}

# --- Generate or import keypair ---
if [ -n "$RESOLVED_SECRET" ]; then
    echo "  Importing wallet key..."
    # dwow_wallet wallet import-secrets reads bs58-encoded secrets from stdin.
    # The secret from keygen/pipeline is hex; convert via xxd -r -p | bs58.
    echo -n "$RESOLVED_SECRET" | xxd -r -p 2>/dev/null | bs58 2>/dev/null | \
        /app/dwow_wallet -c "$CONFIGFILE" wallet import-secrets 2>&1 || {
        echo "  WARNING: wallet import-secrets failed (key may already exist)"
    }
else
    echo "  Generating new keypair..."
    /app/dwow_wallet -c "$CONFIGFILE" wallet keygen 2>&1 || {
        echo "  WARNING: wallet keygen failed (key may already exist)"
    }
fi

# --- Display wallet address ---
echo "  Wallet address:"
/app/dwow_wallet -c "$CONFIGFILE" wallet address 2>&1 || echo "  (could not retrieve address)"

# ============================================================================
# MODE: test — scan, resolve position, verify output, exit
# ============================================================================
if [ "$WALLET_MODE" = "test" ]; then
    echo ""
    echo "=== Test Mode ==="

    FAIL=0

    # Scan blockchain for coins
    echo "  Scanning blockchain..."
    /app/dwow_wallet -c "$CONFIGFILE" scan 2>&1
    echo "  Scan complete."

    # Run position resolution
    echo "  Running position resolution..."
    POS_OUTPUT=$(/app/dwow_wallet -c "$CONFIGFILE" position 2>&1)
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
CONTAINER="dwow-wallet-${WALLET_INDEX}"
echo "  Wallet ready. Use 'docker exec ${CONTAINER} /app/dwow_wallet -c $CONFIGFILE <command>'"
echo "  Examples:"
echo "    docker exec ${CONTAINER} /app/dwow_wallet -c $CONFIGFILE position"
echo "    docker exec ${CONTAINER} /app/dwow_wallet -c $CONFIGFILE wallet balance"
echo "    docker exec ${CONTAINER} /app/dwow_wallet -c $CONFIGFILE scan"
echo ""

# Sleep indefinitely so the container stays up for docker exec
exec sleep infinity
