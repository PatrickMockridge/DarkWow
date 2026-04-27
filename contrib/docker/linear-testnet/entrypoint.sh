#!/bin/bash
# DarkFi Entry Point Script
# Handles config generation and invokes darkfid

set -e

CONFIG_FILE="/config/darkfid.toml"
NETWORK="${NETWORK:-linear-testnet}"

echo "[entrypoint] Starting DarkFi entrypoint..."
echo "[entrypoint] NETWORK=$NETWORK CONFIG_FILE=$CONFIG_FILE"

if [ ! -f "$CONFIG_FILE" ]; then
    echo "[entrypoint] No config found at $CONFIG_FILE, generating default..."
    mkdir -p /config
    cat > "$CONFIG_FILE" << 'EOFCONFIG'
network = "linear-testnet"
[network_config."linear-testnet"]
database = "~/.local/share/darkfi/darkfid/linear-testnet"
threshold = 1
max_forks = 8
skip_sync = true
skip_fees = true
txs_batch_size = 50
[network_config."linear-testnet".pow]
target_block_time = 60
initial_difficulty = 255
min_difficulty = 1
max_difficulty = 4294967295
min_block_interval = 10
[network_config."linear-testnet".rpc]
rpc_listen = "tcp://0.0.0.0:28345"
[network_config."linear-testnet".stratum_rpc]
rpc_listen = "tcp://0.0.0.0:48347"
[network_config."linear-testnet".net]
localnet = true
active_profiles = ["tcp+tls"]
EOFCONFIG
    echo "[entrypoint] Config generated successfully"
else
    echo "[entrypoint] Config file exists at $CONFIG_FILE"
fi

echo "[entrypoint] Starting darkfid..."
exec /app/darkfid -c "$CONFIG_FILE" --network "$NETWORK" "$@"