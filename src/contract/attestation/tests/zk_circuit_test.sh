#!/bin/bash
set -e
ZKAS_BIN="./bin/zkas/zkas"
DIR="src/contract/attestation/proof"
echo "=== Attestation ZK Circuit Compilation ==="
for c in attest_slash commit_fee_schedule consume_claim create_attestation create_claim delegate_attestation update_delegation verify_claim; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin
    [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1
done
echo "=== Done ==="
