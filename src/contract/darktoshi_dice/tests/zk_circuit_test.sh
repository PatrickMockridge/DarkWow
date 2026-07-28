#!/bin/bash
set -e
ZKAS_BIN="./bin/zkas/zkas"
DIR="src/contract/darktoshi_dice/proof"
echo "=== DarkToshiDice ZK Circuit Compilation ==="
for c in commit_bet house_close reveal_roll settle_bet; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin
    [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1
done
echo "=== Done ==="
