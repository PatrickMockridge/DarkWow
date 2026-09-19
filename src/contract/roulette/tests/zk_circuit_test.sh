#!/bin/bash
set -e; ZKAS_BIN="./bin/zkas/zkas"; DIR="src/contract/roulette/proof"
echo "=== Roulette ZK Circuit Compilation ==="
for c in house_close place_bet settle_bet spin_wheel; do echo "  $c..."; $ZKAS_BIN ${DIR}/${c}.zk -o ${DIR}/${c}.zk.bin; [ -f "${DIR}/${c}.zk.bin" ] || exit 1; done
echo "=== Done ==="
