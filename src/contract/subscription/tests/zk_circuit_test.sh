#!/bin/bash
# Subscription contract ZK circuit compilation test
# This script verifies that the subscription ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
SUBSCRIPTION_PROOF_DIR="src/contract/subscription/proof"
OUTPUT_DIR="src/contract/subscription/proof"

echo "=== Subscription Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: SubscribeV1
echo "[Test 1] Compiling subscribe_v1.zk..."
$ZKAS_BIN ${SUBSCRIPTION_PROOF_DIR}/subscribe_v1.zk -o ${OUTPUT_DIR}/subscribe_v1.zk.bin
echo "  ✓ subscribe_v1.zk compiled successfully"

# Test 2: VerifyAccessV1
echo "[Test 2] Compiling verify_access_v1.zk..."
$ZKAS_BIN ${SUBSCRIPTION_PROOF_DIR}/verify_access_v1.zk -o ${OUTPUT_DIR}/verify_access_v1.zk.bin
echo "  ✓ verify_access_v1.zk compiled successfully"

# Test 3: Verify binary outputs exist
echo ""
echo "[Test 3] Verifying compiled binaries..."
if [ -f "${OUTPUT_DIR}/subscribe_v1.zk.bin" ]; then
    echo "  ✓ subscribe_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/subscribe_v1.zk.bin) bytes)"
else
    echo "  ✗ subscribe_v1.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/verify_access_v1.zk.bin" ]; then
    echo "  ✓ verify_access_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/verify_access_v1.zk.bin) bytes)"
else
    echo "  ✗ verify_access_v1.zk.bin missing"
    exit 1
fi

echo ""
echo "=== All Subscription circuit compilation tests passed ==="