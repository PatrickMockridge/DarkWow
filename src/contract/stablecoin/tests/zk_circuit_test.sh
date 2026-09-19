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
echo "[Test 1] Compiling open_position.zk..."
$ZKAS_BIN ${STABLECOIN_PROOF_DIR}/open_position.zk -o ${OUTPUT_DIR}/open_position.zk.bin
echo "  ✓ open_position.zk compiled successfully"

# Test 2: MintStableV1
echo "[Test 2] Compiling mint_stable.zk..."
$ZKAS_BIN ${STABLECOIN_PROOF_DIR}/mint_stable.zk -o ${OUTPUT_DIR}/mint_stable.zk.bin
echo "  ✓ mint_stable.zk compiled successfully"

# Test 3: LiquidateV1
echo "[Test 3] Compiling liquidate.zk..."
$ZKAS_BIN ${STABLECOIN_PROOF_DIR}/liquidate.zk -o ${OUTPUT_DIR}/liquidate.zk.bin
echo "  ✓ liquidate.zk compiled successfully"

# Test 4: Verify binary outputs exist
echo ""
echo "[Test 4] Verifying compiled binaries..."
if [ -f "${OUTPUT_DIR}/open_position.zk.bin" ]; then
    echo "  ✓ open_position.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/open_position.zk.bin) bytes)"
else
    echo "  ✗ open_position.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/mint_stable.zk.bin" ]; then
    echo "  ✓ mint_stable.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/mint_stable.zk.bin) bytes)"
else
    echo "  ✗ mint_stable.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/liquidate.zk.bin" ]; then
    echo "  ✓ liquidate.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/liquidate.zk.bin) bytes)"
else
    echo "  ✗ liquidate.zk.bin missing"
    exit 1
fi

echo ""
echo "=== All Stablecoin circuit compilation tests passed ==="