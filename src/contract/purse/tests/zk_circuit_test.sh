#!/bin/bash
set -e
ZKAS_BIN="./zkas"
DIR="src/contract/purse/proof"
echo "=== Purse ZK Circuit Compilation ==="
for c in balance deposit withdraw; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}.zk -o ${DIR}/${c}.zk.bin
    [ -f "${DIR}/${c}.zk.bin" ] || exit 1
done
echo "=== Done ==="
