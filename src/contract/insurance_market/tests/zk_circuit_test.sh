#!/bin/bash
set -e
ZKAS_BIN="./bin/zkas/zkas"
DIR="src/contract/insurance_market/proof"
echo "=== InsuranceMarket ZK Circuit Compilation ==="
for c in purchase_coverage purchase_coverage_dag purchase_coverage_v1 underwrite; do
    echo "  $c..."
    $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin
    [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1
done
echo "=== Done ==="
