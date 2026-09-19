#!/bin/bash
# Bridge contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/bridge/proof"
echo "=== Bridge Contract ZK Circuit Compilation Test ==="
for circuit in azt_deposit deposit ltc_deposit update_config withdraw xmr_deposit zec_deposit; do
    echo "[Test] Compiling ${circuit}.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}.zk -o ${PROOF_DIR}/${circuit}.zk.bin
    echo "  OK ${circuit}.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}.zk.bin) bytes)"
done
echo "=== All Bridge circuit compilation tests passed ==="
