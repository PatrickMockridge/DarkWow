#!/bin/bash
# DarkFi Entry Point Script
# Handles config generation and invokes darkfid

set -e

NETWORK="${NETWORK:-linear-testnet}"
HOSTNAME="${HOSTNAME:-node0}"

echo "[entrypoint] Starting DarkFi entrypoint..."
echo "[entrypoint] NETWORK=$NETWORK HOSTNAME=$HOSTNAME"

# Determine node-specific settings based on hostname
case "$HOSTNAME" in
    node0)
        RPC_PORT=28345
        STRATUM_PORT=48347
        INBOUND_PORT=28340
        PEERS=""
        ;;
    node1)
        RPC_PORT=28346
        STRATUM_PORT=48447
        INBOUND_PORT=28341
        PEERS="peers = [\"tcp+tls://node0:28340\"]"
        ;;
    *)
        echo "[entrypoint] WARNING: Unknown hostname $HOSTNAME, using node0 defaults"
        RPC_PORT=28345
        STRATUM_PORT=48347
        INBOUND_PORT=28340
        PEERS=""
        ;;
esac

echo "[entrypoint] Generating config for $HOSTNAME (rpc=$RPC_PORT, stratum=$STRATUM_PORT, inbound=$INBOUND_PORT)..."
mkdir -p /root/.config/darkfi
cat > /root/.config/darkfi/darkfid_config.toml << EOF
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
rpc_listen = "tcp://0.0.0.0:$RPC_PORT"

[network_config."linear-testnet".stratum_rpc]
rpc_listen = "tcp://0.0.0.0:$STRATUM_PORT"

[network_config."linear-testnet".net]
localnet = true
active_profiles = ["tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:$INBOUND_PORT"]
$PEERS
EOF
echo "[entrypoint] Config generated successfully"

echo "[entrypoint] Starting darkfid..."
exec /app/darkfid --network "$NETWORK" "$@"
