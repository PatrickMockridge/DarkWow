#!/bin/bash
# Baccarat contract ZK circuit compilation test
# This script verifies that the baccarat ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
BACCARAT_PROOF_DIR="src/contract/baccarat/proof"
OUTPUT_DIR="src/contract/baccarat/proof"

echo "=== Baccarat Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: CommitBetV1
echo "[Test 1] Compiling commit_bet_v1.zk..."
$ZKAS_BIN ${BACCARAT_PROOF_DIR}/commit_bet_v1.zk -o ${OUTPUT_DIR}/commit_bet_v1.zk.bin
echo "  ✓ commit_bet_v1.zk compiled successfully"

# Test 2: SettleBetV1
echo "[Test 2] Compiling settle_bet_v1.zk..."
$ZKAS_BIN ${BACCARAT_PROOF_DIR}/settle_bet_v1.zk -o ${OUTPUT_DIR}/settle_bet_v1.zk.bin
echo "  ✓ settle_bet_v1.zk compiled successfully"

# Test 3: Verify binary outputs exist
echo ""
echo "[Test 3] Verifying compiled binaries..."
if [ -f "${OUTPUT_DIR}/commit_bet_v1.zk.bin" ]; then
    echo "  ✓ commit_bet_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/commit_bet_v1.zk.bin) bytes)"
else
    echo "  ✗ commit_bet_v1.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/settle_bet_v1.zk.bin" ]; then
    echo "  ✓ settle_bet_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/settle_bet_v1.zk.bin) bytes)"
else
    echo "  ✗ settle_bet_v1.zk.bin missing"
    exit 1
fi

echo ""
echo "=== All Baccarat circuit compilation tests passed ==="