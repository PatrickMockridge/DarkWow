#!/bin/bash
# Oracle contract ZK circuit compilation test
# This script verifies that the oracle ZK circuits compile correctly.

set -e

ZKAS_BIN="./zkas"
ORACLE_PROOF_DIR="src/contract/oracle/proof"

echo "=== Oracle Contract ZK Circuit Compilation Test ==="
echo ""

for c in register_oracle push_value attest_value aggregate push_value_commitment; do
    echo "  $c..."
    $ZKAS_BIN ${ORACLE_PROOF_DIR}/${c}.zk -o ${ORACLE_PROOF_DIR}/${c}.zk.bin
    [ -f "${ORACLE_PROOF_DIR}/${c}.zk.bin" ] || exit 1
done

echo ""
echo "=== All Oracle circuit compilation tests passed ==="
