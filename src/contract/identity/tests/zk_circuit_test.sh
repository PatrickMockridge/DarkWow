#!/bin/bash
# Identity contract ZK circuit compilation test
# This script verifies that the identity ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
IDENTITY_PROOF_DIR="src/contract/identity/proof"
OUTPUT_DIR="src/contract/identity/proof"

echo "=== Identity Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: CreateClaimV1 (Level 0 - zk_only)
echo "[Test 1] Compiling create_claim_v1.zk (Level 0 - safemath assert)..."
$ZKAS_BIN ${OUTPUT_DIR}/create_claim_v1.zk -o ${OUTPUT_DIR}/create_claim_v1.zk.bin
echo "  ✓ create_claim_v1.zk compiled successfully"

# Test 2: CreateClaimV1L1 (Level 1 - bounded equation)
echo "[Test 2] Compiling create_claim_v1_l1.zk (Level 1 - bounded equation)..."
$ZKAS_BIN ${OUTPUT_DIR}/create_claim_v1_l1.zk -o ${OUTPUT_DIR}/create_claim_v1_l1.zk.bin
echo "  ✓ create_claim_v1_l1.zk compiled successfully"

# Test 3: Verify binary outputs exist
echo ""
echo "[Test 3] Verifying compiled binaries..."
if [ -f "${OUTPUT_DIR}/create_claim_v1.zk.bin" ]; then
    echo "  ✓ create_claim_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/create_claim_v1.zk.bin) bytes)"
else
    echo "  ✗ create_claim_v1.zk.bin missing"
    exit 1
fi

if [ -f "${OUTPUT_DIR}/create_claim_v1_l1.zk.bin" ]; then
    echo "  ✓ create_claim_v1_l1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/create_claim_v1_l1.zk.bin) bytes)"
else
    echo "  ✗ create_claim_v1_l1.zk.bin missing"
    exit 1
fi

# Test 4: IssueCredentialV1 (expected to fail - known bug)
echo ""
echo "[Test 4] Testing issue_credential_v1.zk (expected to fail - known issue)..."
if $ZKAS_BIN ${OUTPUT_DIR}/issue_credential_v1.zk -o ${OUTPUT_DIR}/issue_credential_v1.zk.bin 2>/dev/null; then
    echo "  ✓ issue_credential_v1.zk compiled (unexpected)"
else
    echo "  ✗ issue_credential_v1.zk failed to compile (expected - ISSUER_PK constant not supported)"
    echo "    See: https://codeberg.org/rusticml/darkfi-safemath/issues/1"
fi

echo ""
echo "=== All compilation tests passed ==="