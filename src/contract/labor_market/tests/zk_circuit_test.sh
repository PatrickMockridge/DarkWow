#!/bin/bash
# Labor Market contract ZK circuit compilation test
# This script verifies that the Labor Market ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
LABOR_MARKET_PROOF_DIR="src/contract/labor_market/proof"
OUTPUT_DIR="src/contract/labor_market/proof"

echo "=== Labor Market Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: CreateJobV1
echo "[Test 1] Compiling create_job.zk..."
$ZKAS_BIN ${LABOR_MARKET_PROOF_DIR}/create_job.zk -o ${OUTPUT_DIR}/create_job.zk.bin
echo "  ✓ create_job.zk compiled successfully"

# Test 2: AcceptJobV1
echo "[Test 2] Compiling accept_job.zk..."
$ZKAS_BIN ${LABOR_MARKET_PROOF_DIR}/accept_job.zk -o ${OUTPUT_DIR}/accept_job.zk.bin
echo "  ✓ accept_job.zk compiled successfully"

# Test 3: SubmitDeliverableV1
echo "[Test 3] Compiling submit_deliverable.zk..."
$ZKAS_BIN ${LABOR_MARKET_PROOF_DIR}/submit_deliverable.zk -o ${OUTPUT_DIR}/submit_deliverable.zk.bin
echo "  ✓ submit_deliverable.zk compiled successfully"

# Test 4: SubmitGitDeliverableV1
echo "[Test 4] Compiling submit_git_deliverable.zk..."
$ZKAS_BIN ${LABOR_MARKET_PROOF_DIR}/submit_git_deliverable.zk -o ${OUTPUT_DIR}/submit_git_deliverable.zk.bin
echo "  ✓ submit_git_deliverable.zk compiled successfully"

# Test 5: ConfirmDeliveryV1
echo "[Test 5] Compiling confirm_delivery.zk..."
$ZKAS_BIN ${LABOR_MARKET_PROOF_DIR}/confirm_delivery.zk -o ${OUTPUT_DIR}/confirm_delivery.zk.bin
echo "  ✓ confirm_delivery.zk compiled successfully"

# Test 6: DisputeV1
echo "[Test 6] Compiling dispute.zk..."
$ZKAS_BIN ${LABOR_MARKET_PROOF_DIR}/dispute.zk -o ${OUTPUT_DIR}/dispute.zk.bin
echo "  ✓ dispute.zk compiled successfully"

# Test 7: RefundV1
echo "[Test 7] Compiling refund.zk..."
$ZKAS_BIN ${LABOR_MARKET_PROOF_DIR}/refund.zk -o ${OUTPUT_DIR}/refund.zk.bin
echo "  ✓ refund.zk compiled successfully"

# Test 8: MilestonePaymentV1
echo "[Test 8] Compiling milestone_payment.zk..."
$ZKAS_BIN ${LABOR_MARKET_PROOF_DIR}/milestone_payment.zk -o ${OUTPUT_DIR}/milestone_payment.zk.bin
echo "  ✓ milestone_payment.zk compiled successfully"

# Test 9: Verify binary outputs exist
echo ""
echo "[Test 9] Verifying compiled binaries..."
for circuit in create_job accept_job submit_deliverable submit_git_deliverable confirm_delivery dispute refund milestone_payment; do
    if [ -f "${OUTPUT_DIR}/${circuit}.zk.bin" ]; then
        echo "  ✓ ${circuit}.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/${circuit}.zk.bin) bytes)"
    else
        echo "  ✗ ${circuit}.zk.bin missing"
        exit 1
    fi
done

echo ""
echo "=== All Labor Market circuit compilation tests passed ==="