#!/bin/bash
# Identity contract ZK circuit compilation test
# This script verifies that the identity ZK circuits compile correctly.

set -e

ZKAS_BIN="./zkas"
IDENTITY_PROOF_DIR="src/contract/identity/proof"

echo "=== Identity Contract ZK Circuit Compilation Test ==="
echo ""

for c in issue_credential verify_capability; do
    echo "  $c..."
    $ZKAS_BIN ${IDENTITY_PROOF_DIR}/${c}.zk -o ${IDENTITY_PROOF_DIR}/${c}.zk.bin
    [ -f "${IDENTITY_PROOF_DIR}/${c}.zk.bin" ] || exit 1
done

echo ""
echo "=== All Identity circuit compilation tests passed ==="
