#!/bin/bash
# DarkWow Wallet Container Entrypoint
#
# Writes config WITHOUT [net] to the default path so docker exec commands
# always work (wallet address, wallet balance). Saves a copy with [net]
# for P2P operations (sync, scan, transfer) accessed via -c flag.
#
# No subcommands run here — DWW() in the pipeline handles init and key
# import before the container starts. The entrypoint just prepares config
# and keeps the container alive.
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
DATADIR="${DATADIR:-/root/.local/share/dwow/dww/${NETWORK}/wallet-${WALLET_INDEX}}"
CACHEDIR="${CACHEDIR:-/root/.local/share/dwow/dww/${NETWORK}/wallet-${WALLET_INDEX}/cache}"

echo "  NETWORK=$NETWORK  INDEX=$WALLET_INDEX  SEED=$SEED_ADDR  P2P_PORT=$P2P_PORT"

mkdir -p "$CONFIGDIR" "$DATADIR" "$CACHEDIR"

# --- Write default config WITHOUT [net] (docker exec always works) ---
cat > "${CONFIGDIR}/dww_config.toml" << DWWEOF
network = "${NETWORK}"

[network_config."${NETWORK}"]
chain_path = "${DATADIR}"
cache_path = "${CACHEDIR}"
wallet_path = "${DATADIR}/wallet.db"
wallet_pass = "${WALLET_PASS}"
production = ${PRODUCTION}
history_path = "${DATADIR}/history.txt"
DWWEOF

# --- Write P2P config WITH [net] (for sync, scan, transfer) ---
cat > "${CONFIGDIR}/dww_config_p2p.toml" << DWWEOF
network = "${NETWORK}"

[network_config."${NETWORK}"]
chain_path = "${DATADIR}"
cache_path = "${CACHEDIR}"
wallet_path = "${DATADIR}/wallet.db"
wallet_pass = "${WALLET_PASS}"
production = ${PRODUCTION}
history_path = "${DATADIR}/history.txt"

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
DWWEOF

echo "  Wallet ready. Use 'docker exec dwow-wallet-${WALLET_INDEX} <command>'"
echo "  Local commands (default config, no [net]): wallet address, wallet balance"
echo "  P2P commands (-c /root/.config/dwow/dww_config_p2p.toml): sync, scan, transfer"

exec sleep infinity
