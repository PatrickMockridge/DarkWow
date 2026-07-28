#!/bin/bash
# Bridge contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/bridge/proof"
echo "=== Bridge Contract ZK Circuit Compilation Test ==="
for circuit in azt_deposit deposit ltc_deposit update_config withdraw xmr_deposit zec_deposit; do
    echo "[Test] Compiling ${circuit}_v1.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}_v1.zk -o ${PROOF_DIR}/${circuit}_v1.zk.bin
    echo "  OK ${circuit}_v1.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}_v1.zk.bin) bytes)"
done
echo "=== All Bridge circuit compilation tests passed ==="
