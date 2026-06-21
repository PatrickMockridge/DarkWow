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

set -e -o pipefail

echo "=== DarkWow Wallet Container ==="

# --- Configuration from environment ---
WALLET_MODE="${WALLET_MODE:-interactive}"
WALLET_INDEX="${WALLET_INDEX:-1}"
NETWORK="${NETWORK:-darkwow-testnet}"
RPC_URL="${RPC_URL:-tcp://127.0.0.1:31345}"
WALLET_SECRET="${WALLET_SECRET:-}"
WALLET_SECRET_FILE="${WALLET_SECRET_FILE:-}"
WALLET_PASS="${WALLET_PASS:-walletpass}"
PRODUCTION="${PRODUCTION:-false}"

# P2P network settings — wallet participates as a full node
SEED_ADDR="${SEED_ADDR:-tcp+tls://lilith:31340}"
P2P_PORT="${P2P_PORT:-31360}"
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"

# Thread containment — prevents wallet containers from consuming all CPUs.
# Must match entrypoint.sh default. Controls both smol executor and rayon pool.
DWOW_RAYON_THREADS="${DWOW_RAYON_THREADS:-2}"
export RAYON_NUM_THREADS="${DWOW_RAYON_THREADS}"
CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"

# --- Wallet CLI wrapper — config is written to the binary's default path ---
# No -c flag needed: the entrypoint writes dww_config.toml to
# /root/.config/dwow/ which is the same default path the binary reads
# when -c is absent (config.rs:35). Matches dwowd entrypoint pattern.
wallet() {
    /app/dwow_wallet "$@"
}
DATADIR="${DATADIR:-/root/.local/share/dwow/dww/${NETWORK}/wallet-${WALLET_INDEX}}"
CACHEDIR="${CACHEDIR:-/root/.local/share/dwow/dww/${NETWORK}/wallet-${WALLET_INDEX}/cache}"

echo "  MODE=$WALLET_MODE  NETWORK=$NETWORK  INDEX=$WALLET_INDEX"
echo "  RPC=$RPC_URL  SEED=$SEED_ADDR  P2P_PORT=$P2P_PORT  DATA=$DATADIR"

# --- Generate dwow_wallet config (without [net] for init phase) ---
# The [net] section causes the binary to hide wallet subcommands
# (confirmed: 'wallet initialize' fails with 'Found argument wallet
# which wasn't expected' when [net] is present in config).
# Write config without [net] first so init/keygen/import work,
# then append [net] after init for P2P sync/scan.
mkdir -p "$CONFIGDIR" "$DATADIR" "$CACHEDIR"

CONFIGFILE="${CONFIGDIR}/dww_config.toml"

cat > "$CONFIGFILE" << DWWEOF
network = "${NETWORK}"

[network_config."${NETWORK}"]
chain_path = "${DATADIR}"
cache_path = "${CACHEDIR}"
wallet_path = "${DATADIR}/wallet.db"
wallet_pass = "${WALLET_PASS}"
production = ${PRODUCTION}
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

# --- Verify binary has expected subcommands ---
echo "  Verifying wallet binary..."
WALLET_VERSION=$(wallet --version 2>&1)
echo "  $WALLET_VERSION"
# Check subcommand by output, not exit code. clap v2 --help exits 1 in some builds.
if ! wallet wallet initialize --help 2>&1 | grep -q "Initialize wallet database"; then
    echo "  FATAL: wallet initialize subcommand not recognized."
    echo "  Binary commit: $(echo "$WALLET_VERSION" | grep -oP 'commit: \K\S+')"
    echo "  This is a stale Docker image. Fix: rebuild with --no-cache or --fresh."
    exit 1
fi
echo "  Binary OK — wallet subcommand smoke test passed"

# --- Initialize wallet (skip if wallet.db already exists — resumes from persisted state) ---
if [ -f "${DATADIR}/wallet.db" ]; then
    echo "  Wallet database exists — skipping initialize (resuming from persisted state)"
else
    echo "  Initializing wallet..."
    wallet wallet initialize 2>&1 || {
        echo "  FATAL: wallet initialize failed"
        echo "  Check that the wallet binary and wallet.sql are consistent."
        exit 1
    }
fi

# --- Generate or import keypair (skip if wallet.db exists — key already imported) ---
if [ -f "${DATADIR}/wallet.db" ]; then
    echo "  Wallet database exists — skipping key import (key already present)"
elif [ -n "$RESOLVED_SECRET" ]; then
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

# --- Append [net] section for P2P operations ---
# All local operations above complete. Now add network config so
# sync, scan, and broadcast have P2P connectivity.
cat >> "$CONFIGFILE" << NETEOF

[network_config."${NETWORK}".net]
seeds = ["${SEED_ADDR}"]
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
localnet = true
p2p_local = true
mining_easy = true
active_profiles = ["tcp+tls"]
outbound_connections = 4
inbound_connections = 32
magic_bytes = [${MAGIC_BYTES}]
NETEOF
echo "  [net] P2P config appended to $CONFIGFILE"

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
