#!/bin/bash
set -e
ZKAS_BIN="./zkas"
DIR="src/contract/attestation/proof"
echo "=== Attestation ZK Circuit Compilation ==="
for c in attest_slash check_not_revoked commit_fee_schedule consume_claim create_attestation create_claim delegate_attestation update_delegation verify_chain verify_claim; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}.zk -o ${DIR}/${c}.zk.bin
    [ -f "${DIR}/${c}.zk.bin" ] || exit 1
done
echo "=== Done ==="
