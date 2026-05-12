#!/usr/bin/env bash
# DarkWow-Native p2pool Entrypoint
#
# Starts p2pool pointed at the dwow-p2pool-adaptor (which presents dwowd as
# a monerod-compatible daemon). This runs p2pool in DarkWow-native mode:
# miners hash DarkWow block headers, rewards are DRKW only.
#
# Unlike merge mining mode, this does NOT connect to a real monerod and does
# NOT use the --merge-mine flag. From p2pool's perspective, it's just mining
# on a "Monero-compatible" chain (the adaptor makes dwowd look like monerod).
#
# Environment variables:
#   MONERO_HOST     - adaptor host (default: adaptor)
#   MONERO_RPC_PORT - adaptor RPC port (default: 28081)
#   STRATUM_PORT    - p2pool stratum port for xmrig (default: 3333)
#   WALLET_ADDRESS  - DarkWow wallet for mining rewards

set -euo pipefail

MONERO_HOST="${MONERO_HOST:-adaptor}"
MONERO_RPC_PORT="${MONERO_RPC_PORT:-28081}"
STRATUM_PORT="${STRATUM_PORT:-3333}"

DARKFI_WALLET="${WALLET_ADDRESS:-}"

# p2pool requires a Monero wallet address, even though we're mining DarkWow.
# The adaptor ignores it for block rewards. Use the DarkWow address or a dummy.
MONERO_ADDR="${MONERO_WALLET_ADDRESS:-9wviCeWe2D8ZwF6BxR3BPfKAKE5uufTkjmdVFpj2HRuzunmKZQz}"

echo "=== p2pool (DarkWow-native) ==="
echo "Adaptor host:   $MONERO_HOST:$MONERO_RPC_PORT"
echo "Stratum:        0.0.0.0:$STRATUM_PORT"
echo "DarkWow wallet: $DARKFI_WALLET"
echo ""

# No --zmq-port flag — the adaptor doesn't support ZMQ yet. p2pool will poll
# get_block_template on its own interval (ZMQ is disabled by default).
exec p2pool \
    --host "$MONERO_HOST" \
    --rpc-port "$MONERO_RPC_PORT" \
    --wallet "$MONERO_ADDR" \
    --stratum "0.0.0.0:$STRATUM_PORT" \
    --data-dir /root/.p2pool \
    --no-igd \
    --mini \
    --no-upnp
