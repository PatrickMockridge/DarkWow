#!/usr/bin/env bash
# DarkWow p2pool Chain Adaptor Entrypoint
#
# Starts the dwow-p2pool-adaptor which presents a dwowd node as a
# monerod-compatible daemon to p2pool. This allows p2pool to mine
# DarkWow blocks as the primary chain (no Monero required).
#
# Configuration via environment variables:
#   DWOWD_RPC       - dwowd JSON-RPC URL (default: node0:31345)
#   DWOWD_STRATUM   - dwowd stratum URL (default: node0:31347)
#   ADAPTOR_LISTEN  - where the adaptor listens for p2pool (default: 0.0.0.0:28081)
#   RUST_LOG        - logging level (default: info)

set -euo pipefail

DWOWD_RPC="${DWOWD_RPC:-node0:31345}"
DWOWD_STRATUM="${DWOWD_STRATUM:-node0:31347}"
ADAPTOR_LISTEN="${ADAPTOR_LISTEN:-0.0.0.0:28081}"

echo "=== dwow-p2pool-adaptor ==="
echo "dwowd RPC:      $DWOWD_RPC"
echo "dwowd stratum:  $DWOWD_STRATUM"
echo "Listen:         $ADAPTOR_LISTEN"

exec /app/dwow-p2pool-adaptor \
    --dwowd-rpc "$DWOWD_RPC" \
    --dwowd-stratum "$DWOWD_STRATUM" \
    --listen "$ADAPTOR_LISTEN" \
    -v
