#!/bin/bash
# Oracle contract ZK circuit compilation test
# This script verifies that the oracle ZK circuits compile correctly.

set -e

ZKAS_BIN="./bin/zkas/zkas"
ORACLE_PROOF_DIR="src/contract/oracle/proof"
OUTPUT_DIR="src/contract/oracle/proof"

echo "=== Oracle Contract ZK Circuit Compilation Test ==="
echo ""

# Test 1: RegisterOracleV1
echo "[Test 1] Compiling register_oracle_v1.zk..."
$ZKAS_BIN ${ORACLE_PROOF_DIR}/register_oracle_v1.zk -o ${OUTPUT_DIR}/register_oracle_v1.zk.bin
echo "  ✓ register_oracle_v1.zk compiled successfully"

# Test 2: PushValueV1
echo "[Test 2] Compiling push_value_v1.zk..."
$ZKAS_BIN ${ORACLE_PROOF_DIR}/push_value_v1.zk -o ${OUTPUT_DIR}/push_value_v1.zk.bin
echo "  ✓ push_value_v1.zk compiled successfully"

# Test 3: AttestValueV1
echo "[Test 3] Compiling attest_value_v1.zk..."
$ZKAS_BIN ${ORACLE_PROOF_DIR}/attest_value_v1.zk -o ${OUTPUT_DIR}/attest_value_v1.zk.bin
echo "  ✓ attest_value_v1.zk compiled successfully"

# Test 4: Verify binary outputs exist
echo ""
echo "[Test 4] Verifying compiled binaries..."
for circuit in register_oracle push_value attest_value; do
    if [ -f "${OUTPUT_DIR}/${circuit}_v1.zk.bin" ]; then
        echo "  ✓ ${circuit}_v1.zk.bin exists ($(stat -c%s ${OUTPUT_DIR}/${circuit}_v1.zk.bin) bytes)"
    else
        echo "  ✗ ${circuit}_v1.zk.bin missing"
        exit 1
    fi
done

echo ""
echo "=== All Oracle circuit compilation tests passed ==="