#!/bin/bash
# DarkWow Merge Mining — monerod entrypoint
#
# Starts a Monero testnet node. In offline mode (default for local devnet),
# uses fixed difficulty so blocks are found quickly without syncing the real
# Monero testnet. Set OFFLINE=false for public testnet merge mining.

set -e

OFFLINE="${OFFLINE:-true}"
FIXED_DIFFICULTY="${FIXED_DIFFICULTY:-20000}"
RPC_PORT="${RPC_PORT:-28081}"
ZMQ_PORT="${ZMQ_PORT:-28083}"

echo "=== Monero Node (testnet) ==="
echo "  OFFLINE=$OFFLINE  RPC=$RPC_PORT  ZMQ=$ZMQ_PORT"

ARGS="--testnet --no-igd --data-dir /root/.bitmonero --log-level 1 --hide-my-port"
ARGS="$ARGS --zmq-pub tcp://0.0.0.0:${ZMQ_PORT}"
ARGS="$ARGS --rpc-bind-ip 0.0.0.0 --rpc-bind-port ${RPC_PORT} --confirm-external-bind"

if [ "$OFFLINE" = "true" ]; then
    echo "  Mode: offline (fixed difficulty $FIXED_DIFFICULTY)"
    ARGS="$ARGS --offline --fixed-difficulty ${FIXED_DIFFICULTY} --disable-rpc-ban"
else
    echo "  Mode: online (syncing public Monero testnet)"
fi

exec monerod $ARGS
