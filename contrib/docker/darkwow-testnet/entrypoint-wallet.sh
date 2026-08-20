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
# ── Peer config ──────────────────────────────────────────────────────
# The wallet connects DIRECTLY to its configured peers (ManualSession) and
# pulls GetTip/GetBlocks. No seed/hostlist exchange — that is mining-node
# machinery (net-node). PEER_ADDR lists the full nodes to pull from.
# ──────────────────────────────────────────────────────────────────────
PEER_ADDR="${PEER_ADDR:-tcp+tls://observer:31340,tcp+tls://node0:31342}"
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"

DWOW_RAYON_THREADS="${DWOW_RAYON_THREADS:-2}"
export RAYON_NUM_THREADS="${DWOW_RAYON_THREADS}"

CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dww/${NETWORK}}"
CACHEDIR="${CACHEDIR:-/root/.local/share/dwow/dww/${NETWORK}/cache}"

echo "  NETWORK=$NETWORK  INDEX=$WALLET_INDEX  P2P_PORT=$P2P_PORT"

mkdir -p "$CONFIGDIR" "$DATADIR" "$CACHEDIR"

# --- Build peers config line ---
# Pure bash loop — no sed. Produces valid TOML: { url = "tcp+tls://..." }
PEERS_LINE=""

if [ -n "$PEER_ADDR" ]; then
    PEER_LIST=""
    IFS=',' read -ra PEERS <<< "$PEER_ADDR"
    for peer in "${PEERS[@]}"; do
        peer=$(echo "$peer" | xargs)
        if [ -z "$PEER_LIST" ]; then
            PEER_LIST="{ url = \"${peer}\" }"
        else
            PEER_LIST="${PEER_LIST}, { url = \"${peer}\" }"
        fi
    done
    PEERS_LINE="peers = [${PEER_LIST}]"
    echo "  Peers: ${PEER_LIST}"
fi

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
${PEERS_LINE}
localnet = true
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
magic_bytes = [${MAGIC_BYTES}]
DWWEOF

# --- Wallet identity declaration (derive-on-boot) ---
# The wallet derives its identity from keys.toml on boot — no addresses table,
# no import step. Export the declaration so EVERY dwow_wallet invocation
# (initialize + daemon) resolves it via config: KEYS_FILE + WALLET_NAME
# (mirrors dwowd's --keys + NODE_NAME). Must be exported before any invocation.
export WALLET_NAME="${WALLET_NAME:-wallet-1}"
export KEYS_FILE="${KEYS_FILE:-/run/config/keys.toml}"
if [ ! -f "$KEYS_FILE" ]; then
    echo "  FATAL: keys.toml not found at $KEYS_FILE — the wallet must declare its key"
    exit 1
fi
echo "  Wallet identity: section [$WALLET_NAME] from $KEYS_FILE"

# --- Initialize wallet (DB schema + genesis contracts) ---
echo "  Initializing wallet (compiling genesis contracts, may take 2-3 min)..."
/app/darkwow wallet wallet initialize 2>&1

echo "  Wallet initialized. Starting daemon — P2P sync, continuous..."

# Enable tracing output so SeedErrorMessage codes, version handshake
# logs, and P2P diagnostics reach stderr → captured by docker logs.
export RUST_LOG=info

exec /app/darkwow wallet daemon
