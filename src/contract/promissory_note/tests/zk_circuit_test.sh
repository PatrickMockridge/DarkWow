#!/bin/bash
set -e; ZKAS_BIN="./zkas"; DIR="src/contract/promissory_note/proof"
echo "=== PromissoryNote ZK Circuit Compilation ==="
for c in issue redeem register_type revoke transfer; do echo "  $c..."; $ZKAS_BIN ${DIR}/${c}.zk -o ${DIR}/${c}.zk.bin; [ -f "${DIR}/${c}.zk.bin" ] || exit 1; done
echo "=== Done ==="
