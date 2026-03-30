#!/bin/bash
# Stablecoin contract ZK circuit compilation test
# This script verifies that the stablecoin ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
STABLECOIN_PROOF_DIR="src/contract/stablecoin/proof"
OUTPUT_DIR="src/contract/stablecoin/proof"

echo "=== Stablecoin Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: OpenPositionV1
echo "[Test 1] Compiling open_position_v1.zk..."
$ZKAS_BIN ${STABLECOIN_PROOF_DIR}/open_position_v1.zk -o ${OUTPUT_DIR}/open_position_v1.zk.bin
echo "  ✓ open_position_v1.zk compiled successfully"

# Test 2: MintStableV1
echo "[Test 2] Compiling mint_stable_v1.zk..."
$ZKAS_BIN ${STABLECOIN_PROOF_DIR}/mint_stable_v1.zk -o ${OUTPUT_DIR}/mint_stable_v1.zk.bin
echo "  ✓ mint_stable_v1.zk compiled successfully"

# Test 3: LiquidateV1
echo "[Test 3] Compiling liquidate_v1.zk..."
$ZKAS_BIN ${STABLECOIN_PROOF_DIR}/liquidate_v1.zk -o ${OUTPUT_DIR}/liquidate_v1.zk.bin
echo "  ✓ liquidate_v1.zk compiled successfully"

# Test 4: Verify binary outputs exist
echo ""
echo "[Test 4] Verifying compiled binaries..."
if [ -f "${OUTPUT_DIR}/open_position_v1.zk.bin" ]; then
    echo "  ✓ open_position_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/open_position_v1.zk.bin) bytes)"
else
    echo "  ✗ open_position_v1.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/mint_stable_v1.zk.bin" ]; then
    echo "  ✓ mint_stable_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/mint_stable_v1.zk.bin) bytes)"
else
    echo "  ✗ mint_stable_v1.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/liquidate_v1.zk.bin" ]; then
    echo "  ✓ liquidate_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/liquidate_v1.zk.bin) bytes)"
else
    echo "  ✗ liquidate_v1.zk.bin missing"
    exit 1
fi

echo ""
echo "=== All Stablecoin circuit compilation tests passed ==="