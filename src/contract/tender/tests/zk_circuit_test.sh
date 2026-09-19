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
echo "[Test 1] Compiling create_tender.zk..."
$ZKAS_BIN ${TENDER_PROOF_DIR}/create_tender.zk -o ${OUTPUT_DIR}/create_tender.zk.bin
echo "  ✓ create_tender.zk compiled successfully"

# Test 2: SubmitBidV1
echo "[Test 2] Compiling submit_bid.zk..."
$ZKAS_BIN ${TENDER_PROOF_DIR}/submit_bid.zk -o ${OUTPUT_DIR}/submit_bid.zk.bin
echo "  ✓ submit_bid.zk compiled successfully"

# Test 3: RevealBidV1
echo "[Test 3] Compiling reveal_bid.zk..."
$ZKAS_BIN ${TENDER_PROOF_DIR}/reveal_bid.zk -o ${OUTPUT_DIR}/reveal_bid.zk.bin
echo "  ✓ reveal_bid.zk compiled successfully"

# Test 4: SelectWinnerV1
echo "[Test 4] Compiling select_winner.zk..."
$ZKAS_BIN ${TENDER_PROOF_DIR}/select_winner.zk -o ${OUTPUT_DIR}/select_winner.zk.bin
echo "  ✓ select_winner.zk compiled successfully"

# Test 5: Verify binary outputs exist
echo ""
echo "[Test 5] Verifying compiled binaries..."
for circuit in create_tender submit_bid reveal_bid select_winner; do
    if [ -f "${OUTPUT_DIR}/${circuit}.zk.bin" ]; then
        echo "  ✓ ${circuit}.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/${circuit}.zk.bin) bytes)"
    else
        echo "  ✗ ${circuit}.zk.bin missing"
        exit 1
    fi
done

echo ""
echo "=== All Tender circuit compilation tests passed ==="