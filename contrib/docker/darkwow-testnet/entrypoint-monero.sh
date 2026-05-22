#!/bin/bash
# DarkWow Merge Mining — monerod entrypoint
#
# Starts a Monero node. Supports three modes via env vars:
#   OFFLINE=true               — offline with fixed difficulty (local devnet)
#   OFFLINE=false MONERO_NETWORK=testnet  — Monero public testnet
#   OFFLINE=false MONERO_NETWORK=mainnet  — Monero mainnet
#
# The pre-built p2pool binary (GitHub releases) supports both --testnet and
# --mainnet flags. Monero testnet uses port 28081 (same as our default).

set -e

OFFLINE="${OFFLINE:-true}"
MONERO_NETWORK="${MONERO_NETWORK:-testnet}"
FIXED_DIFFICULTY="${FIXED_DIFFICULTY:-20000}"
RPC_PORT="${RPC_PORT:-28081}"
ZMQ_PORT="${ZMQ_PORT:-28083}"
MONERO_ADD_PEERS="${MONERO_ADD_PEERS:-}"

echo "=== Monero Node ==="
echo "  OFFLINE=$OFFLINE  NETWORK=$MONERO_NETWORK  RPC=$RPC_PORT  ZMQ=$ZMQ_PORT"

ARGS="--non-interactive --no-igd --data-dir /root/.bitmonero --log-level 1 --hide-my-port"
ARGS="$ARGS --zmq-pub tcp://0.0.0.0:${ZMQ_PORT}"
ARGS="$ARGS --rpc-bind-ip 0.0.0.0 --rpc-bind-port ${RPC_PORT} --confirm-external-bind"

if [ "$MONERO_NETWORK" = "testnet" ]; then
    ARGS="$ARGS --testnet"
fi

if [ "$OFFLINE" = "true" ]; then
    echo "  Mode: offline (fixed difficulty $FIXED_DIFFICULTY)"
    ARGS="$ARGS --offline --fixed-difficulty ${FIXED_DIFFICULTY} --disable-rpc-ban"
else
    echo "  Mode: online — syncing Monero $MONERO_NETWORK"
    # Add bootstrap peers for faster sync
    if [ -n "$MONERO_ADD_PEERS" ]; then
        IFS=',' read -ra PEERS <<< "$MONERO_ADD_PEERS"
        for peer in "${PEERS[@]}"; do
            peer=$(echo "$peer" | xargs)
            ARGS="$ARGS --add-peer $peer"
        done
        echo "  Bootstrap peers: $MONERO_ADD_PEERS"
    fi
fi

exec monerod $ARGS
