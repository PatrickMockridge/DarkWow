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
echo "[Test 1] Compiling create_auction.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/create_auction.zk -o ${OUTPUT_DIR}/create_auction.zk.bin
echo "  ✓ create_auction.zk compiled successfully"

# Test 2: PlaceBidV1
echo "[Test 2] Compiling place_bid.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/place_bid.zk -o ${OUTPUT_DIR}/place_bid.zk.bin
echo "  ✓ place_bid.zk compiled successfully"

# Test 3: CloseAuctionV1
echo "[Test 3] Compiling close_auction.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/close_auction.zk -o ${OUTPUT_DIR}/close_auction.zk.bin
echo "  ✓ close_auction.zk compiled successfully"

# Test 4: ClaimWinningsV1
echo "[Test 4] Compiling claim_winnings.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/claim_winnings.zk -o ${OUTPUT_DIR}/claim_winnings.zk.bin
echo "  ✓ claim_winnings.zk compiled successfully"

# Test 5: SettleAuctionV1
echo "[Test 5] Compiling settle_auction.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/settle_auction.zk -o ${OUTPUT_DIR}/settle_auction.zk.bin
echo "  ✓ settle_auction.zk compiled successfully"

# Test 6: RefundBidV1
echo "[Test 6] Compiling refund_bid.zk..."
$ZKAS_BIN ${AUCTION_PROOF_DIR}/refund_bid.zk -o ${OUTPUT_DIR}/refund_bid.zk.bin
echo "  ✓ refund_bid.zk compiled successfully"

# Test 7: Verify binary outputs exist
echo ""
echo "[Test 7] Verifying compiled binaries..."
for circuit in create_auction place_bid close_auction claim_winnings settle_auction refund_bid; do
    if [ -f "${OUTPUT_DIR}/${circuit}.zk.bin" ]; then
        echo "  ✓ ${circuit}.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/${circuit}.zk.bin) bytes)"
    else
        echo "  ✗ ${circuit}.zk.bin missing"
        exit 1
    fi
done

echo ""
echo "=== All Auction circuit compilation tests passed ==="