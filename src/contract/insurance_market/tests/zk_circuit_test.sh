#!/bin/bash
set -e; ZKAS_BIN="./bin/zkas/zkas"; DIR="src/contract/insurance_market/proof"
echo "=== InsuranceMarket ZK Circuit Compilation ==="
for c in purchase_coverage purchase_coverage_with_capability purchase_coverage_with_dag underwrite_with_capability; do echo "  $c..."; $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin; [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1; done
echo "=== Done ==="
