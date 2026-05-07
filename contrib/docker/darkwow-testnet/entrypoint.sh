#!/bin/bash
# DarkWow Testnet Entry Point Script
# Provisional name: "DarkWow" (token ticker: DRKW)
# Handles config generation and invokes dwowd

set -e

NETWORK="${NETWORK:-darkwow-testnet}"
HOSTNAME="${HOSTNAME:-node0}"

echo "[entrypoint] Starting DarkWow entrypoint..."
echo "[entrypoint] NETWORK=$NETWORK HOSTNAME=$HOSTNAME"

# Determine node-specific settings based on hostname
case "$HOSTNAME" in
    node0)
        RPC_PORT=31345
        STRATUM_PORT=31347
        INBOUND_PORT=31342
        SEEDS="seeds = [\"tcp+tls://lilith:31340\"]"
        PEERS="peers = [\"tcp+tls://node1:31343\"]"
        EXTERNAL_ADDRS="external_addrs = [\"tcp+tls://node0:31342\"]"
        ;;
    node1)
        RPC_PORT=31346
        STRATUM_PORT=31348
        INBOUND_PORT=31343
        SEEDS="seeds = [\"tcp+tls://lilith:31340\"]"
        PEERS="peers = [\"tcp+tls://node0:31342\"]"
        EXTERNAL_ADDRS="external_addrs = [\"tcp+tls://node1:31343\"]"
        ;;
    *)
        echo "[entrypoint] WARNING: Unknown hostname $HOSTNAME, using node0 defaults"
        RPC_PORT=31345
        STRATUM_PORT=31347
        INBOUND_PORT=31342
        SEEDS="seeds = [\"tcp+tls://lilith:31340\"]"
        PEERS="peers = [\"tcp+tls://node1:31343\"]"
        EXTERNAL_ADDRS="external_addrs = [\"tcp+tls://node0:31342\"]"
        ;;
esac

echo "[entrypoint] Generating config for $HOSTNAME (rpc=$RPC_PORT, stratum=$STRATUM_PORT, inbound=$INBOUND_PORT)..."
mkdir -p /root/.config/dwow
cat > /root/.config/dwow/dwowd_config.toml << EOF
network = "darkwow-testnet"

[network_config."darkwow-testnet"]
database = "~/.local/share/dwow/dwowd/darkwow-testnet"
threshold = 3
max_forks = 8
skip_sync = false
skip_fees = false
txs_batch_size = 50
pow_target = 120

[network_config."darkwow-testnet".pow]
target_block_time = 120
initial_difficulty = 255
min_difficulty = 1
max_difficulty = 4294967295
min_block_interval = 10

[network_config."darkwow-testnet".rpc]
rpc_listen = "tcp://0.0.0.0:$RPC_PORT"

[network_config."darkwow-testnet".stratum_rpc]
rpc_listen = "tcp://0.0.0.0:$STRATUM_PORT"

[network_config."darkwow-testnet".net]
localnet = false
active_profiles = ["tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:$INBOUND_PORT"]
magic_bytes = [68, 82, 75, 87]
hostlist = "/root/.local/share/dwow/dwowd/darkwow-testnet/hostlist.tsv"
$SEEDS
$PEERS
$EXTERNAL_ADDRS

[network_config."darkwow-testnet".net.profiles."tcp+tls"]
inbound = ["tcp+tls://0.0.0.0:$INBOUND_PORT"]
EOF
echo "[entrypoint] Config generated successfully"

echo "[entrypoint] Starting dwowd..."
/app/dwowd "$@" &

DARKFID_PID=$!

# Start xmrig on mining nodes
if [ "$HOSTNAME" = "node0" ] || [ "$HOSTNAME" = "node1" ]; then
    STRATUM_PORT=$(if [ "$HOSTNAME" = "node0" ]; then echo "31347"; else echo "31348"; fi)
    DATADIR="/root/.local/share/dwow/dwowd/darkwow-testnet"
    MINER_ADDRESS_FILE="$DATADIR/mining_address"

    # Three-tier address resolution:
    # 1. Explicit WALLET_ADDRESS env var (operator-provided)
    # 2. Persisted file from prior dwowd run
    # 3. Wait for dwowd to auto-generate a keypair on first run
    if [ -n "$WALLET_ADDRESS" ]; then
        echo "[entrypoint] Using provided WALLET_ADDRESS: $WALLET_ADDRESS"
    elif [ -f "$MINER_ADDRESS_FILE" ]; then
        WALLET_ADDRESS=$(cat "$MINER_ADDRESS_FILE")
        echo "[entrypoint] Using persisted mining address: $WALLET_ADDRESS"
    else
        echo "[entrypoint] Waiting for dwowd to generate mining address..."
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
