#!/bin/bash
# Auction contract ZK circuit compilation test
# This script verifies that the auction ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
AUCTION_PROOF_DIR="src/contract/auction/proof"
OUTPUT_DIR="src/contract/auction/proof"

echo "=== Auction Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: CreateAuctionV1
echo "[Test 1] Compiling create_auction_v1.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/create_auction_v1.zk -o ${OUTPUT_DIR}/create_auction_v1.zk.bin
echo "  ✓ create_auction_v1.zk compiled successfully"

# Test 2: PlaceBidV1
echo "[Test 2] Compiling place_bid_v1.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/place_bid_v1.zk -o ${OUTPUT_DIR}/place_bid_v1.zk.bin
echo "  ✓ place_bid_v1.zk compiled successfully"

# Test 3: CloseAuctionV1
echo "[Test 3] Compiling close_auction_v1.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/close_auction_v1.zk -o ${OUTPUT_DIR}/close_auction_v1.zk.bin
echo "  ✓ close_auction_v1.zk compiled successfully"

# Test 4: ClaimWinningsV1
echo "[Test 4] Compiling claim_winnings_v1.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/claim_winnings_v1.zk -o ${OUTPUT_DIR}/claim_winnings_v1.zk.bin
echo "  ✓ claim_winnings_v1.zk compiled successfully"

# Test 5: SettleAuctionV1
echo "[Test 5] Compiling settle_auction_v1.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/settle_auction_v1.zk -o ${OUTPUT_DIR}/settle_auction_v1.zk.bin
echo "  ✓ settle_auction_v1.zk compiled successfully"

# Test 6: RefundBidV1
echo "[Test 6] Compiling refund_bid_v1.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/refund_bid_v1.zk -o ${OUTPUT_DIR}/refund_bid_v1.zk.bin
echo "  ✓ refund_bid_v1.zk compiled successfully"

# Test 7: Verify binary outputs exist
echo ""
echo "[Test 7] Verifying compiled binaries..."
for circuit in create_auction place_bid close_auction claim_winnings settle_auction refund_bid; do
    if [ -f "${OUTPUT_DIR}/${circuit}_v1.zk.bin" ]; then
        echo "  ✓ ${circuit}_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/${circuit}_v1.zk.bin) bytes)"
    else
        echo "  ✗ ${circuit}_v1.zk.bin missing"
        exit 1
    fi
done

echo ""
echo "=== All Auction circuit compilation tests passed ==="