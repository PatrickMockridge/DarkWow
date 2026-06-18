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
RPC_URL="${RPC_URL:-tcp://127.0.0.1:31345}"
WALLET_SECRET="${WALLET_SECRET:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
WALLET_PASS="${WALLET_PASS:-walletpass}"

# P2P network settings — wallet participates as a full node
SEED_ADDR="${SEED_ADDR:-tcp+tls://lilith:31340}"
P2P_PORT="${P2P_PORT:-31360}"
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"

# Thread containment — prevents wallet containers from consuming all CPUs.
# Must match entrypoint.sh default. Controls both smol executor and rayon pool.
DWOW_RAYON_THREADS="${DWOW_RAYON_THREADS:-2}"
export RAYON_NUM_THREADS="${DWOW_RAYON_THREADS}"
CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"

# --- Wallet CLI wrapper — ensures consistent -n and -c flags on every call ---
wallet() {
    /app/dwow_wallet -n "${NETWORK}" -c "${CONFIGFILE}" "$@"
}
DATADIR="${DATADIR:-/root/.local/share/dwow/dww/${NETWORK}/wallet-${WALLET_INDEX}}"
CACHEDIR="${CACHEDIR:-/root/.local/share/dwow/dww/${NETWORK}/wallet-${WALLET_INDEX}/cache}"

echo "  MODE=$WALLET_MODE  NETWORK=$NETWORK  INDEX=$WALLET_INDEX"
echo "  RPC=$RPC_URL  SEED=$SEED_ADDR  P2P_PORT=$P2P_PORT  DATA=$DATADIR"

# --- Generate dwow_wallet config ---
mkdir -p "$CONFIGDIR" "$DATADIR" "$CACHEDIR"

CONFIGFILE="${CONFIGDIR}/dww_config.toml"

cat > "$CONFIGFILE" << DWWEOF
network = "${NETWORK}"

[network_config."${NETWORK}"]
database = "${DATADIR}"
cache_path = "${CACHEDIR}"
wallet_path = "${DATADIR}/wallet.db"
wallet_pass = "${WALLET_PASS}"
endpoint = "${RPC_URL}"
history_path = "${DATADIR}/history.txt"

[network_config."${NETWORK}".net]
seeds = ["${SEED_ADDR}"]
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
localnet = true
active_profiles = ["tcp+tls"]
outbound_connections = 4
inbound_connections = 32
magic_bytes = [${MAGIC_BYTES}]
DWWEOF

echo "  Config written to $CONFIGFILE"

# --- Validate config is parseable by the binary ---
echo "  Validating config..."
if ! /app/dwow_wallet -n "${NETWORK}" -c "${CONFIGFILE}" --help > /dev/null 2>&1; then
    echo "  FATAL: Binary cannot parse config at ${CONFIGFILE}"
    echo "  Config contents:"
    cat "${CONFIGFILE}"
    exit 1
fi
echo "  Config OK — binary accepts it"

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

# --- Verify binary has expected subcommands ---
echo "  Verifying wallet binary..."
if ! wallet --help 2>&1 | grep -q "wallet"; then
    echo "  FATAL: wallet does not recognize 'wallet' subcommand."
    echo "  The Docker image was built from a source tree that lacks the wallet subcommand."
    echo "  This is a non-deterministic build. Check:"
    echo "    1. git branch and commit hash match between host and Docker build"
    echo "    2. Docker build is not using a cached layer from an older commit"
    echo "    3. The base image (darkwow-base:24.04) has a consistent Rust toolchain"
    exit 1
fi
echo "  Binary OK — wallet subcommand found"

# --- Initialize wallet ---
echo "  Initializing wallet..."
wallet wallet initialize 2>&1 || {
    echo "  FATAL: wallet initialize failed"
    echo "  Check that the wallet binary and wallet.sql are consistent."
    exit 1
}

# --- Generate or import keypair ---
if [ -n "$RESOLVED_SECRET" ]; then
    # Validate secret length before importing
    SECRET_LEN=$(echo -n "$RESOLVED_SECRET" | wc -c)
    if [ "$SECRET_LEN" -ne 64 ]; then
        echo "  ERROR: Secret must be 64 hex characters (32 bytes), got $SECRET_LEN"
        echo "  Check /tmp/dwow_mining_secret or WALLET_SECRET/WALLET_SECRET_FILE"
        exit 1
    fi
    echo "  Importing wallet key..."
    # dwow_wallet wallet import-secrets reads bs58-encoded secrets from stdin.
    # The secret from keygen/pipeline is hex; convert via xxd -r -p | bs58.
    SECRET_BS58=$(echo -n "$RESOLVED_SECRET" | xxd -r -p | bs58 2>&1)
    if [ -z "$SECRET_BS58" ]; then
        echo "  FATAL: Failed to convert hex secret to bs58"
        echo "  xxd -r -p output length: $(echo -n "$RESOLVED_SECRET" | xxd -r -p | wc -c)"
        exit 1
    fi
    echo "$SECRET_BS58" | wallet wallet import-secrets 2>&1 || {
        echo "  FATAL: wallet import-secrets failed — container cannot decrypt coinbase"
        echo "  The wallet secret may not match FORWARD_DESTINATION."
        echo "  Check /tmp/dwow_mining_secret on the host."
        exit 1
    }
else
    echo "  Generating new keypair..."
    wallet wallet keygen 2>&1 || {
        echo "  FATAL: wallet keygen failed"
        exit 1
    }
fi

# --- Display wallet address ---
echo "  Wallet address:"
wallet wallet address 2>&1 || echo "  (could not retrieve address)"

# ============================================================================
# MODE: test — scan, resolve position, verify output, exit
# ============================================================================
if [ "$WALLET_MODE" = "test" ]; then
    echo ""
    echo "=== Test Mode ==="

    FAIL=0

    # Scan blockchain for coins
    echo "  Scanning blockchain..."
    wallet scan 2>&1
    echo "  Scan complete."

    # Run position resolution
    echo "  Running position resolution..."
    POS_OUTPUT=$(wallet position 2>&1)
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
echo "  Wallet ready. Use 'docker exec ${CONTAINER} wallet <command>'"
echo "  Examples:"
echo "    docker exec ${CONTAINER} wallet position"
echo "    docker exec ${CONTAINER} wallet wallet balance"
echo "    docker exec ${CONTAINER} wallet scan"
echo "    docker exec ${CONTAINER} wallet sync init"
echo ""

# Sleep indefinitely so the container stays up for docker exec
exec sleep infinity
