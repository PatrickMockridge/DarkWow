#!/bin/bash
# DarkWow Wallet Container Entrypoint
#
# Writes a SINGLE config with [net] at the default path so all commands
# — local and P2P — work without -c flags. The [net] section is optional:
# local commands ignore it; P2P commands use it. No two-config split.
#
# HAZOP-WALLET-001: Two-config design caused subcommands to disappear when
# the wrong config was used. Single config eliminates this failure mode.
set -e -o pipefail

echo "=== DarkWow Wallet Container ==="

# --- Configuration from environment ---
NETWORK="${NETWORK:-darkwow-testnet}"
WALLET_INDEX="${WALLET_INDEX:-1}"
WALLET_PASS="${WALLET_PASS:-walletpass}"
PRODUCTION="${PRODUCTION:-false}"

P2P_PORT="${P2P_PORT:-31360}"
SEED_ADDR="${SEED_ADDR:-tcp+tls://lilith:31340}"
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"

DWOW_RAYON_THREADS="${DWOW_RAYON_THREADS:-2}"
export RAYON_NUM_THREADS="${DWOW_RAYON_THREADS}"

CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dww/${NETWORK}}"
CACHEDIR="${CACHEDIR:-/root/.local/share/dwow/dww/${NETWORK}/cache}"

echo "  NETWORK=$NETWORK  INDEX=$WALLET_INDEX  SEED=$SEED_ADDR  P2P_PORT=$P2P_PORT"

mkdir -p "$CONFIGDIR" "$DATADIR" "$CACHEDIR"

# --- Write single config WITH [net] ---
# The [net] section is optional — local commands (wallet address, keygen)
# ignore it. P2P commands (sync, scan, transfer) use it. One config for
# everything. No -c flag needed. No two-config confusion.
cat > "${CONFIGDIR}/dww_config.toml" << DWWEOF
network = "${NETWORK}"

[network_config."${NETWORK}"]
chain_path = "${DATADIR}"
cache_path = "${CACHEDIR}"
wallet_path = "${DATADIR}/wallet.db"
wallet_pass = "${WALLET_PASS}"
production = ${PRODUCTION}
history_path = "${DATADIR}/history.txt"

[network_config."${NETWORK}".net]
seeds = [{ url = "${SEED_ADDR}" }]
localnet = true
magic_bytes = [${MAGIC_BYTES}]
DWWEOF

# --- Initialize wallet (DB schema + genesis contracts) ---
echo "  Initializing wallet (compiling genesis contracts, may take 2-3 min)..."
/app/dwow_wallet wallet initialize 2>&1

# --- Import mining secret if available ---
SECRET_FILE="${SECRET_FILE:-/run/secrets/mining_secret}"
if [ -f "$SECRET_FILE" ]; then
    echo "  Importing mining secret..."
    xxd -r -p "$SECRET_FILE" | bs58 | /app/dwow_wallet wallet import-secrets 2>&1
    echo "  Mining secret imported."
else
    echo "  No mining secret found at $SECRET_FILE — skipping key import."
fi

echo "  Wallet initialized. Starting daemon — P2P sync, continuous..."
exec /app/dwow_wallet daemon
