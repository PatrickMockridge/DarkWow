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

# Wait for dwowd mm_rpc to be ready before starting p2pool.
# p2pool calls merge_mining_get_chain_id only at startup — if mm_rpc isn't
# ready yet, p2pool gets "empty response" and never retries, silently disabling
# merge mining. Poll the endpoint ourselves first.
echo "Waiting for dwowd mm_rpc at ${DWOWD_MM_RPC}..."
for i in $(seq 1 60); do
    CHAIN_ID=$(curl -s --max-time 2 -X POST "http://${DWOWD_MM_RPC}" \
        -H 'Content-Type: application/json' \
        -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","params":[],"id":1}' 2>/dev/null \
        | grep -o '"chain_id":"[^"]*"' | cut -d'"' -f4)
    if [ -n "$CHAIN_ID" ]; then
        echo "dwowd mm_rpc ready, chain_id=$CHAIN_ID (attempt $i)"
        break
    fi
    [ "$i" -eq 60 ] && echo "WARNING: mm_rpc not ready after 60s, starting p2pool anyway"
    sleep 1
done

# Start p2pool in background (not exec — we start xmrig too).
# Redirect to a log file so we can grep for the StratumServer readiness message.
P2POOL_LOG="/tmp/p2pool.log"
p2pool \
    --host "${MONERO_HOST}" \
    --rpc-port "${MONERO_RPC_PORT}" \
    --zmq-port "${MONERO_ZMQ_PORT}" \
    --wallet "${MONERO_WALLET_ADDRESS}" \
    --stratum "0.0.0.0:${STRATUM_PORT}" \
    --data-dir /root/.p2pool \
    --no-randomx \
    --no-igd \
    $NETWORK_FLAG \
    --merge-mine "${DWOWD_MM_RPC}" "${WALLET_ADDRESS}" \
    > "$P2POOL_LOG" 2>&1 &
P2POOL_PID=$!

# Wait for p2pool's stratum server to start.
# Log-based detection — matches the bare-metal test pattern. RandomX dataset
# initialization can take minutes in Docker without huge pages, so port scanning
# fails prematurely. The "StratumServer" log line is emitted once stratum is up.
echo "Waiting for p2pool stratum on 0.0.0.0:${STRATUM_PORT}..."
for i in $(seq 1 300); do
    if grep -qi "StratumServer\|stratum.*listening" "$P2POOL_LOG" 2>/dev/null; then
        echo "p2pool stratum ready (attempt $i)"
        break
    fi
    # Also tail the log so docker logs shows p2pool output in real time
    if [ "$i" -eq 1 ]; then
        tail -f "$P2POOL_LOG" &
        TAIL_PID=$!
    fi
    sleep 1
done
# Stop the background tail
[ -n "${TAIL_PID:-}" ] && kill "$TAIL_PID" 2>/dev/null || true

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
# Tail the p2pool log to docker logs for visibility
tail -f "$P2POOL_LOG" &
wait $P2POOL_PID
