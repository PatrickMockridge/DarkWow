#!/bin/bash
# Wallet Container Shell Interface
# Source this file to interact with pipeline wallet containers.
#
# Pattern from test-wallet-transactions.sh:
#   wal() — execute a command inside a wallet container
#
# Usage:
#   source contrib/docker/darkwow-testnet/wallet-shell.sh
#   wal 1 sync init
#   wal 1 sync status
#   wal 1 scan
#   wal 1 wallet balance

# Execute a command inside wallet container N
wal() {
    local i=$1; shift
    if ! docker ps --format '{{.Names}}' | grep -q "^dwow-wallet-$i$"; then
        echo "ERROR: dwow-wallet-$i is not running" >&2
        return 1
    fi
    docker exec "dwow-wallet-$i" /app/darkwow wallet "$@" 2>&1 || true
}
