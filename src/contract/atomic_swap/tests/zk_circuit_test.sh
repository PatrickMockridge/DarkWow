#!/bin/bash
# Atomic Swap contract ZK circuit compilation test
# This script verifies that the atomic swap ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
ATOMIC_SWAP_PROOF_DIR="src/contract/atomic_swap/proof"
OUTPUT_DIR="src/contract/atomic_swap/proof"

echo "=== Atomic Swap Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: CreateSwapV1
echo "[Test 1] Compiling create_swap_v1.zk..."
$ZKAS_BIN ${ATOMIC_SWAP_PROOF_DIR}/create_swap_v1.zk -o ${OUTPUT_DIR}/create_swap_v1.zk.bin
echo "  ✓ create_swap_v1.zk compiled successfully"

# Test 2: ClaimV1
echo "[Test 2] Compiling claim_v1.zk..."
$ZKAS_BIN ${ATOMIC_SWAP_PROOF_DIR}/claim_v1.zk -o ${OUTPUT_DIR}/claim_v1.zk.bin
echo "  ✓ claim_v1.zk compiled successfully"

# Test 3: RefundV1
echo "[Test 3] Compiling refund_v1.zk..."
$ZKAS_BIN ${ATOMIC_SWAP_PROOF_DIR}/refund_v1.zk -o ${OUTPUT_DIR}/refund_v1.zk.bin
echo "  ✓ refund_v1.zk compiled successfully"

# Test 4: Verify binary outputs exist
echo ""
echo "[Test 4] Verifying compiled binaries..."
if [ -f "${OUTPUT_DIR}/create_swap_v1.zk.bin" ]; then
    echo "  ✓ create_swap_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/create_swap_v1.zk.bin) bytes)"
else
    echo "  ✗ create_swap_v1.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/claim_v1.zk.bin" ]; then
    echo "  ✓ claim_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/claim_v1.zk.bin) bytes)"
else
    echo "  ✗ claim_v1.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/refund_v1.zk.bin" ]; then
    echo "  ✓ refund_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/refund_v1.zk.bin) bytes)"
else
    echo "  ✗ refund_v1.zk.bin missing"
    exit 1
fi

echo ""
echo "=== All Atomic Swap circuit compilation tests passed ==="