#!/bin/bash
set -e
ZKAS_BIN="./bin/zkas/zkas"
DIR="src/contract/box/proof"
echo "=== Box ZK Circuit Compilation ==="
for c in put take; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}.zk -o ${DIR}/${c}.zk.bin
    [ -f "${DIR}/${c}.zk.bin" ] || exit 1
done
echo "=== Done ==="
