#!/bin/bash
# DEX contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/dex/proof"
echo "=== DEX Contract ZK Circuit Compilation Test ==="
for circuit in accept_swap cancel_swap create_swap execute_swap execute_swap_fee execute_swap_slippage set_transparency_level update_config; do
    echo "[Test] Compiling ${circuit}.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}.zk -o ${PROOF_DIR}/${circuit}.zk.bin
    echo "  OK ${circuit}.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}.zk.bin) bytes)"
done
echo "=== All DEX circuit compilation tests passed ==="
