#!/bin/bash
set -e
ZKAS_BIN="./bin/zkas/zkas"
DIR="src/contract/lottery/proof"
echo "=== Lottery ZK Circuit Compilation ==="
for c in claim_prize commit_ticket draw_winners expire_lottery initialize reveal_ticket; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin
    [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1
done
echo "=== Done ==="
