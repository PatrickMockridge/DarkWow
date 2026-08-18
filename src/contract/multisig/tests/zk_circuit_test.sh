#!/bin/bash
set -e
ZKAS_BIN="./zkas"
DIR="src/contract/multisig/proof"
echo "=== MultiSig ZK Circuit Compilation ==="
for c in create_group finalize sign; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}.zk -o ${DIR}/${c}.zk.bin
    [ -f "${DIR}/${c}.zk.bin" ] || exit 1
done
echo "=== Done ==="
