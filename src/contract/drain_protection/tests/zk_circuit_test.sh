#!/bin/bash
# DrainProtection contract ZK circuit compilation test
# This script verifies that the DrainProtection ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
DRAIN_PROTECTION_PROOF_DIR="src/contract/drain_protection/proof"
OUTPUT_DIR="src/contract/drain_protection/proof"

echo "=== DrainProtection Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: ExitV1
echo "[Test 1] Compiling exit.zk..."
$ZKAS_BIN ${DRAIN_PROTECTION_PROOF_DIR}/exit.zk -o ${OUTPUT_DIR}/exit.zk.bin
echo "  ✓ exit.zk compiled successfully"

# Test 2: Verify binary output exists
echo ""
echo "[Test 2] Verifying compiled binary..."
if [ -f "${OUTPUT_DIR}/exit.zk.bin" ]; then
    echo "  ✓ exit.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/exit.zk.bin) bytes)"
else
    echo "  ✗ exit.zk.bin missing"
    exit 1
fi

echo ""
echo "=== All DrainProtection circuit compilation tests passed ==="