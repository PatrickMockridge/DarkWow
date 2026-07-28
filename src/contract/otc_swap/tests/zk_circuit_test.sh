#!/bin/bash
set -e
ZKAS_BIN="./bin/zkas/zkas"
DIR="src/contract/otc_swap/proof"
echo "=== OtcSwap ZK Circuit Compilation ==="
for c in cancel_swap create_swap execute_swap fund_swap; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin
    [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1
done
echo "=== Done ==="
