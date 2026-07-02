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
SEED_ADDR="${SEED_ADDR:-}"
PEER_ADDR="${PEER_ADDR:-tcp+tls://observer:31340,tcp+tls://node0:31342}"
MAGIC_BYTES="${MAGIC_BYTES:-68,82,75,87}"

DWOW_RAYON_THREADS="${DWOW_RAYON_THREADS:-2}"
export RAYON_NUM_THREADS="${DWOW_RAYON_THREADS}"

CONFIGDIR="${CONFIGDIR:-/root/.config/dwow}"
DATADIR="${DATADIR:-/root/.local/share/dwow/dww/${NETWORK}}"
CACHEDIR="${CACHEDIR:-/root/.local/share/dwow/dww/${NETWORK}/cache}"

echo "  NETWORK=$NETWORK  INDEX=$WALLET_INDEX  SEED=$SEED_ADDR  P2P_PORT=$P2P_PORT"

mkdir -p "$CONFIGDIR" "$DATADIR" "$CACHEDIR"

# --- Build seeds / peers config lines ---
# Pure bash loop — no sed. Produces valid TOML: { url = "tcp+tls://..." }
SEEDS_LINE=""
PEERS_LINE=""

if [ -n "$SEED_ADDR" ]; then
    SEED_LIST=""
    IFS=',' read -ra SEEDS <<< "$SEED_ADDR"
    for seed in "${SEEDS[@]}"; do
        seed=$(echo "$seed" | xargs)
        if [ -z "$SEED_LIST" ]; then
            SEED_LIST="{ url = \"${seed}\" }"
        else
            SEED_LIST="${SEED_LIST}, { url = \"${seed}\" }"
        fi
    done
    SEEDS_LINE="seeds = [${SEED_LIST}]"
    echo "  Seeds: ${SEED_LIST}"
fi

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
${SEEDS_LINE}
${PEERS_LINE}
localnet = true
inbound = ["tcp+tls://0.0.0.0:${P2P_PORT}"]
magic_bytes = [${MAGIC_BYTES}]
DWWEOF

# --- Initialize wallet (DB schema + genesis contracts) ---
echo "  Initializing wallet (compiling genesis contracts, may take 2-3 min)..."
/app/dwow_wallet wallet initialize 2>&1

# --- Import keys from keys.toml (single deterministic entry point) ---
# Goes through AccountManager — the single key authority.
# Idempotent — safe on restart (INSERT OR IGNORE).
# Hard guardrail: if import fails, the wallet must not start.
# A wallet with zero secrets cannot decrypt coinbase.
WALLET_NAME="${WALLET_NAME:-wallet-1}"
KEYS_FILE="${KEYS_FILE:-/run/config/keys.toml}"
if [ -f "$KEYS_FILE" ]; then
    echo "  Importing keys from keys.toml [$WALLET_NAME]..."
    /app/dwow_wallet wallet import-from-toml "$WALLET_NAME" 2>&1 || {
        echo "  FATAL: Key import from keys.toml failed — cannot start without secrets"
        exit 1
    }
else
    # Fallback: legacy secret file path (backward compatibility).
    # Uses import-secrets which goes through AccountManager.
    SECRET_FILE="${SECRET_FILE:-/run/secrets/mining_secret}"
    if [ -f "$SECRET_FILE" ]; then
        echo "  Importing mining secret (legacy path)..."
        xxd -r -p "$SECRET_FILE" 2>/dev/null | bs58 2>/dev/null | \
            /app/dwow_wallet wallet import-secrets 2>&1 || {
            echo "  FATAL: Secret import failed — cannot start without secrets"
            exit 1
        }
    else
        echo "  No keys.toml or mining secret found — wallet will have zero secrets"
        echo "  FATAL: Wallet requires at least one secret key to decrypt coinbase"
        exit 1
    fi
fi

# Log derived public key for pipeline cross-verification (Layer 3 key identity)
echo "  Deriving public key for cross-verification..."
WALLET_PUBKEY=$(/app/dwow_wallet wallet address 2>/dev/null | tail -1 || echo "ADDRESS_LOOKUP_FAILED")
echo "WALLET_PUBKEY=$WALLET_PUBKEY"

echo "  Wallet initialized. Starting daemon — P2P sync, continuous..."

# Enable tracing output so SeedErrorMessage codes, version handshake
# logs, and P2P diagnostics reach stderr → captured by docker logs.
export RUST_LOG=info

exec /app/dwow_wallet daemon
