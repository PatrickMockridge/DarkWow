#!/bin/bash
# DAO-Escrow contract ZK circuit compilation test
# This script verifies that the DAO-Escrow ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
DAO_ESCROW_PROOF_DIR="src/contract/dao_escrow/proof"
OUTPUT_DIR="src/contract/dao_escrow/proof"

echo "=== DAO-Escrow Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: InitV1
echo "[Test 1] Compiling init_v1.zk..."
$ZKAS_BIN ${DAO_ESCROW_PROOF_DIR}/init_v1.zk -o ${OUTPUT_DIR}/init_v1.zk.bin
echo "  ✓ init_v1.zk compiled successfully"

# Test 2: PayPremiumV1
echo "[Test 2] Compiling pay_premium_v1.zk..."
$ZKAS_BIN ${DAO_ESCROW_PROOF_DIR}/pay_premium_v1.zk -o ${OUTPUT_DIR}/pay_premium_v1.zk.bin
echo "  ✓ pay_premium_v1.zk compiled successfully"

# Test 3: Verify binary outputs exist
echo ""
echo "[Test 3] Verifying compiled binaries..."
if [ -f "${OUTPUT_DIR}/init_v1.zk.bin" ]; then
    echo "  ✓ init_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/init_v1.zk.bin) bytes)"
else
    echo "  ✗ init_v1.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/pay_premium_v1.zk.bin" ]; then
    echo "  ✓ pay_premium_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/pay_premium_v1.zk.bin) bytes)"
else
    echo "  ✗ pay_premium_v1.zk.bin missing"
    exit 1
fi

echo ""
echo "=== All DAO-Escrow circuit compilation tests passed ==="