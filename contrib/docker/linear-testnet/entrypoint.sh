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
        SEEDS="seeds = [\"tcp+tls://lilith:18345\"]"
        PEERS="peers = [\"tcp+tls://node1:28341\"]"
        EXTERNAL_ADDRS="external_addrs = [\"tcp+tls://node0:28340\"]"
        ;;
    node1)
        RPC_PORT=28346
        STRATUM_PORT=48447
        INBOUND_PORT=28341
        SEEDS="seeds = [\"tcp+tls://lilith:18345\"]"
        PEERS="peers = [\"tcp+tls://node0:28340\"]"
        EXTERNAL_ADDRS="external_addrs = [\"tcp+tls://node1:28341\"]"
        ;;
    *)
        echo "[entrypoint] WARNING: Unknown hostname $HOSTNAME, using node0 defaults"
        RPC_PORT=28345
        STRATUM_PORT=48347
        INBOUND_PORT=28340
        SEEDS="seeds = [\"tcp+tls://lilith:18345\"]"
        PEERS="peers = [\"tcp+tls://node1:28341\"]"
        EXTERNAL_ADDRS="external_addrs = [\"tcp+tls://node0:28340\"]"
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
skip_fees = false
txs_batch_size = 50
pow_target = 1

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
magic_bytes = [163, 139, 113, 101]
hostlist = "/root/.local/share/darkfi/darkfid/linear-testnet/hostlist.tsv"
$SEEDS
$PEERS
$EXTERNAL_ADDRS

[network_config."linear-testnet".net.profiles."tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:$INBOUND_PORT"]
EOF
echo "[entrypoint] Config generated successfully"

echo "[entrypoint] Starting darkfid..."
exec /app/darkfid "$@" &

DARKFID_PID=$!

# Start xmrig on mining nodes
if [ "$HOSTNAME" = "node0" ] || [ "$HOSTNAME" = "node1" ]; then
    STRATUM_PORT=$(if [ "$HOSTNAME" = "node0" ]; then echo "48347"; else echo "48447"; fi)
    DATADIR="/root/.local/share/darkfi/darkfid/linear-testnet"
    MINER_ADDRESS_FILE="$DATADIR/mining_address"

    # Three-tier address resolution:
    # 1. Explicit WALLET_ADDRESS env var (operator-provided)
    # 2. Persisted file from prior darkfid run
    # 3. Wait for darkfid to auto-generate a keypair on first run
    if [ -n "$WALLET_ADDRESS" ]; then
        echo "[entrypoint] Using provided WALLET_ADDRESS: $WALLET_ADDRESS"
    elif [ -f "$MINER_ADDRESS_FILE" ]; then
        WALLET_ADDRESS=$(cat "$MINER_ADDRESS_FILE")
        echo "[entrypoint] Using persisted mining address: $WALLET_ADDRESS"
    else
        echo "[entrypoint] Waiting for darkfid to generate mining address..."
        for i in $(seq 1 30); do
            if [ -f "$MINER_ADDRESS_FILE" ]; then
                WALLET_ADDRESS=$(cat "$MINER_ADDRESS_FILE")
                echo "[entrypoint] Generated mining address: $WALLET_ADDRESS"
                break
            fi
            sleep 1
        done
    fi

    if [ -n "$WALLET_ADDRESS" ]; then
        echo "[entrypoint] Starting xmrig on $HOSTNAME (stratum port $STRATUM_PORT)..."
        xmrig \
            -o "stratum+tcp://127.0.0.1:${STRATUM_PORT}" \
            -u "$WALLET_ADDRESS" \
            -a rx/0 \
            -t 1 &
    else
        echo "[entrypoint] WARNING: No mining address available, xmrig not started"
    fi
fi

wait $DARKFID_PID
