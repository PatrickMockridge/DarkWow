#!/bin/bash
# Bearer Bond contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/bearer_bond/proof"
echo "=== BearerBond Contract ZK Circuit Compilation Test ==="
for circuit in blind_output burn prove_coverage redeem; do
    echo "[Test] Compiling ${circuit}_v1.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}_v1.zk -o ${PROOF_DIR}/${circuit}_v1.zk.bin
    echo "  OK ${circuit}_v1.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}_v1.zk.bin) bytes)"
done
echo "=== All BearerBond circuit compilation tests passed ==="
