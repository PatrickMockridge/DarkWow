#!/bin/bash
set -e
ZKAS_BIN="./bin/zkas/zkas"
DIR="src/contract/multisig/proof"
echo "=== MultiSig ZK Circuit Compilation ==="
for c in create_group finalize sign; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin
    [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1
done
echo "=== Done ==="
