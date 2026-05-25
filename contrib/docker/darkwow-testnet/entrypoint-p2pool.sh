#!/bin/bash
# DarkWow Merge Mining — p2pool entrypoint
#
# Bridges monerod and dwowd for merge mining. xmrig connects to p2pool's
# stratum port; p2pool submits found blocks to both monerod and dwowd.
#
# Environment:
#   MONERO_HOST           - monerod hostname (default: monerod)
#   MONERO_RPC_PORT       - monerod RPC port (default: 28081)
#   MONERO_ZMQ_PORT       - monerod ZMQ pub port (default: 28083)
#   MONERO_NETWORK        - "testnet" or "mainnet" (default: testnet)
#   DWOWD_MM_RPC          - dwowd merge mining RPC (default: node0:31348)
#   STRATUM_PORT          - p2pool stratum port (default: 3333)
#   WALLET_ADDRESS        - DarkWow wallet for aux chain rewards
#   MONERO_WALLET_ADDRESS - Monero wallet for parent chain rewards

set -e

MONERO_HOST="${MONERO_HOST:-monerod}"
MONERO_RPC_PORT="${MONERO_RPC_PORT:-28081}"
MONERO_ZMQ_PORT="${MONERO_ZMQ_PORT:-28083}"
MONERO_NETWORK="${MONERO_NETWORK:-testnet}"
DWOWD_MM_RPC="${DWOWD_MM_RPC:-node0:31348}"
STRATUM_PORT="${STRATUM_PORT:-3333}"
WALLET_ADDRESS="${WALLET_ADDRESS:-}"
MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"

echo "=== p2pool Merge Mining ==="
echo "  Monero: $MONERO_HOST (RPC=$MONERO_RPC_PORT ZMQ=$MONERO_ZMQ_PORT network=$MONERO_NETWORK)"
echo "  dwowd mm_rpc: $DWOWD_MM_RPC"
echo "  Stratum: 0.0.0.0:$STRATUM_PORT"
echo "  DarkWow wallet: $WALLET_ADDRESS"
echo "  Monero wallet: $MONERO_WALLET_ADDRESS"

if [ -z "$WALLET_ADDRESS" ]; then
    echo "WARNING: WALLET_ADDRESS not set — DarkWow coinbase rewards will use dwowd default"
fi

if [ -z "$MONERO_WALLET_ADDRESS" ]; then
    echo "WARNING: MONERO_WALLET_ADDRESS not set — using dummy address for offline mode"
    MONERO_WALLET_ADDRESS="9wenrVcFffvbTR4nEQ7KAbDMw7bq6B7uwgsraJzFVLkq9SiqMYFf72544RyXLaXKmZYfYNdcdZWpKaBv5dD8xkpS5djBZPM"
fi

NETWORK_FLAG=""
if [ "$MONERO_NETWORK" = "testnet" ]; then
    NETWORK_FLAG="--mini"
fi

# Start p2pool in background (not exec — we start xmrig too)
p2pool \
    --host "${MONERO_HOST}" \
    --rpc-port "${MONERO_RPC_PORT}" \
    --zmq-port "${MONERO_ZMQ_PORT}" \
    --wallet "${MONERO_WALLET_ADDRESS}" \
    --stratum "0.0.0.0:${STRATUM_PORT}" \
    --data-dir /root/.p2pool \
    --no-igd \
    $NETWORK_FLAG \
    --merge-mine "${DWOWD_MM_RPC}" "${WALLET_ADDRESS}" &
P2POOL_PID=$!

# Wait for p2pool's stratum port to be ready before starting xmrig
echo "Waiting for p2pool stratum on 127.0.0.1:${STRATUM_PORT}..."
for i in $(seq 1 60); do
    if timeout 1 bash -c "echo >/dev/tcp/127.0.0.1/${STRATUM_PORT}" 2>/dev/null; then
        echo "p2pool stratum ready (attempt $i)"
        break
    fi
    sleep 1
done

# Start xmrig hasher connected to local p2pool stratum
MINING_THREADS="${MINING_THREADS:-${XMERGE_THREADS:-1}}"
echo "Starting xmrig (127.0.0.1:${STRATUM_PORT}, ${MINING_THREADS} threads)..."
xmrig \
    -o "127.0.0.1:${STRATUM_PORT}" \
    -a rx/0 \
    -t "${MINING_THREADS}" \
    --keepalive &
XMRIG_PID=$!

echo "p2pool mining node running (p2pool PID=$P2POOL_PID, xmrig PID=$XMRIG_PID)"
wait $P2POOL_PID
