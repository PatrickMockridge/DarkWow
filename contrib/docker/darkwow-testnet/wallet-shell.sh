#!/bin/bash
# Wallet Container Shell Interface
# Source this file to interact with pipeline wallet containers.
#
# Pattern from test-wallet-transactions.sh:
#   wal() — execute a command inside a wallet container
#   WALLET_CONFIG — path to config inside the container
#
# Usage:
#   source contrib/docker/darkwow-testnet/wallet-shell.sh
#   wal 1 sync init
#   wal 1 sync status
#   wal 1 scan
#   wal 1 wallet balance

WALLET_CONFIG="/root/.config/dwow/drk.toml"

# Execute a command inside wallet container N
wal() {
    local i=$1; shift
    docker exec "dwow-wallet-$i" /app/dwow_wallet -c "$WALLET_CONFIG" "$@" 2>&1
}
