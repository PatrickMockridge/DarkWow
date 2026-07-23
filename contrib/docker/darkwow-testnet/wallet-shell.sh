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
    local outfile="/tmp/wal_out_$$"
    local errfile="/tmp/wal_err_$$"
    local rc=0
    docker exec "dwow-wallet-$i" /app/darkwow wallet "$@" >"$outfile" 2>"$errfile" || rc=$?
    cat "$outfile"
    if [ "$rc" -ne 0 ]; then
        echo "WAL_ERROR: wallet-$i exit=$rc stderr=$(head -3 "$errfile" 2>/dev/null | tr '\n' ' ')" >&2
        rm -f "$outfile" "$errfile"
        return "$rc"
    fi
    # stderr on success is normal (Rust diagnostics, progress messages).
    # Log it for visibility but don't treat it as an error.
    if [ -s "$errfile" ]; then
        echo "WAL_DIAG: wallet-$i stderr=$(head -3 "$errfile" 2>/dev/null | tr '\n' ' ')" >&2
    fi
    rm -f "$outfile" "$errfile"
    return 0
}
