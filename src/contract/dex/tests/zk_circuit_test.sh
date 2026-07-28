#!/bin/bash
# DEX contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/dex/proof"
echo "=== DEX Contract ZK Circuit Compilation Test ==="
for circuit in accept_swap cancel_swap create_swap execute_swap execute_swap_fee execute_swap_slippage set_transparency_level update_config; do
    echo "[Test] Compiling ${circuit}_v1.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}_v1.zk -o ${PROOF_DIR}/${circuit}_v1.zk.bin
    echo "  OK ${circuit}_v1.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}_v1.zk.bin) bytes)"
done
echo "=== All DEX circuit compilation tests passed ==="
