#!/bin/bash
# DarkWow Merge Mining — p2pool entrypoint
#
# Bridges monerod and dwowd for merge mining. xmrig connects to p2pool's
# stratum port; p2pool submits found blocks to both monerod and dwowd.

set -e

MONERO_HOST="${MONERO_HOST:-monerod}"
MONERO_RPC_PORT="${MONERO_RPC_PORT:-28081}"
MONERO_ZMQ_PORT="${MONERO_ZMQ_PORT:-28083}"
DWOWD_MM_RPC="${DWOWD_MM_RPC:-node0:31348}"
STRATUM_PORT="${STRATUM_PORT:-3333}"
WALLET_ADDRESS="${WALLET_ADDRESS:-}"
MONERO_WALLET_ADDRESS="${MONERO_WALLET_ADDRESS:-}"

echo "=== p2pool Merge Mining ==="
echo "  Monero: $MONERO_HOST (RPC=$MONERO_RPC_PORT ZMQ=$MONERO_ZMQ_PORT)"
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

exec p2pool \
    --host "${MONERO_HOST}" \
    --rpc-port "${MONERO_RPC_PORT}" \
    --zmq-port "${MONERO_ZMQ_PORT}" \
    --wallet "${MONERO_WALLET_ADDRESS}" \
    --stratum "0.0.0.0:${STRATUM_PORT}" \
    --data-dir /root/.p2pool \
    --no-igd \
    --merge-mine "${DWOWD_MM_RPC}" "${WALLET_ADDRESS}"
