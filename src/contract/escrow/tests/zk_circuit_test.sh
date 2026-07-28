#!/bin/bash
set -e
ZKAS_BIN="./bin/zkas/zkas"
DIR="src/contract/escrow/proof"
echo "=== Escrow ZK Circuit Compilation ==="
for c in cancel_escrow claim_escrow create_escrow fund_escrow refund_escrow; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin
    [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1
done
echo "=== Done ==="
