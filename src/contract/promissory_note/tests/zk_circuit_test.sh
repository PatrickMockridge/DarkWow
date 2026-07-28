#!/bin/bash
set -e; ZKAS_BIN="./bin/zkas/zkas"; DIR="src/contract/promissory_note/proof"
echo "=== PromissoryNote ZK Circuit Compilation ==="
for c in blind_output burn mint redeem token_mint; do echo "  $c..."; $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin; [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1; done
echo "=== Done ==="
