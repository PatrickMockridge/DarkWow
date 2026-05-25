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

ARGS="--non-interactive --no-igd --data-dir /root/.bitmonero --log-level 1"
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

# Start monerod in background, wait for RPC, then start mining
monerod $ARGS &
MONEROD_PID=$!

echo "  Waiting for monerod RPC..."
for i in $(seq 1 60); do
    if curl -s --max-time 2 http://127.0.0.1:${RPC_PORT}/json_rpc \
        -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","method":"get_info","id":1}' >/dev/null 2>&1; then
        echo "  monerod RPC ready (attempt $i)"
        break
    fi
    sleep 1
done

MONERO_MINING_THREADS="${MONERO_MINING_THREADS:-1}"
echo "  Starting mining with ${MONERO_MINING_THREADS} thread(s)..."
curl -s http://127.0.0.1:${RPC_PORT}/json_rpc \
    -H 'Content-Type: application/json' \
    -d "{\"jsonrpc\":\"2.0\",\"id\":\"0\",\"method\":\"start_mining\",\"params\":{\"threads_count\":${MONERO_MINING_THREADS},\"do_background_mining\":true,\"ignore_battery\":false}}" >/dev/null 2>&1

echo "  monerod mining started. Keeping in foreground."
wait $MONEROD_PID
