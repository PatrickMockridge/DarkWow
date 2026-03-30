#!/bin/bash
# Tender contract ZK circuit compilation test
# This script verifies that the tender ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
TENDER_PROOF_DIR="src/contract/tender/proof"
OUTPUT_DIR="src/contract/tender/proof"

echo "=== Tender Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: CreateTenderV1
echo "[Test 1] Compiling create_tender_v1.zk..."
$ZKAS_BIN ${TENDER_PROOF_DIR}/create_tender_v1.zk -o ${OUTPUT_DIR}/create_tender_v1.zk.bin
echo "  ✓ create_tender_v1.zk compiled successfully"

# Test 2: SubmitBidV1
echo "[Test 2] Compiling submit_bid_v1.zk..."
$ZKAS_BIN ${TENDER_PROOF_DIR}/submit_bid_v1.zk -o ${OUTPUT_DIR}/submit_bid_v1.zk.bin
echo "  ✓ submit_bid_v1.zk compiled successfully"

# Test 3: RevealBidV1
echo "[Test 3] Compiling reveal_bid_v1.zk..."
$ZKAS_BIN ${TENDER_PROOF_DIR}/reveal_bid_v1.zk -o ${OUTPUT_DIR}/reveal_bid_v1.zk.bin
echo "  ✓ reveal_bid_v1.zk compiled successfully"

# Test 4: SelectWinnerV1
echo "[Test 4] Compiling select_winner_v1.zk..."
$ZKAS_BIN ${TENDER_PROOF_DIR}/select_winner_v1.zk -o ${OUTPUT_DIR}/select_winner_v1.zk.bin
echo "  ✓ select_winner_v1.zk compiled successfully"

# Test 5: Verify binary outputs exist
echo ""
echo "[Test 5] Verifying compiled binaries..."
for circuit in create_tender submit_bid reveal_bid select_winner; do
    if [ -f "${OUTPUT_DIR}/${circuit}_v1.zk.bin" ]; then
        echo "  ✓ ${circuit}_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/${circuit}_v1.zk.bin) bytes)"
    else
        echo "  ✗ ${circuit}_v1.zk.bin missing"
        exit 1
    fi
done

echo ""
echo "=== All Tender circuit compilation tests passed ==="