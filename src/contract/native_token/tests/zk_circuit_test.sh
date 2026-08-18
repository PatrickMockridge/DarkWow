#!/bin/bash
set -e; ZKAS_BIN="./zkas"; DIR="src/contract/native_token/proof"
echo "=== NativeToken ZK Circuit Compilation ==="
for c in burn fee fee_collect mint; do echo "  $c..."; $ZKAS_BIN ${DIR}/${c}.zk -o ${DIR}/${c}.zk.bin; [ -f "${DIR}/${c}.zk.bin" ] || exit 1; done
echo "=== Done ==="
