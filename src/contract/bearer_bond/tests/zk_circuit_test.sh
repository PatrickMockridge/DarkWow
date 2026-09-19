#!/bin/bash
# Bearer Bond contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/bearer_bond/proof"
echo "=== BearerBond Contract ZK Circuit Compilation Test ==="
for circuit in blind_output burn prove_coverage redeem; do
    echo "[Test] Compiling ${circuit}.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}.zk -o ${PROOF_DIR}/${circuit}.zk.bin
    echo "  OK ${circuit}.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}.zk.bin) bytes)"
done
echo "=== All BearerBond circuit compilation tests passed ==="
