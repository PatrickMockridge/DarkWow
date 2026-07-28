#!/bin/bash
# Darkbet Exchange contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/darkbet_exchange/proof"
echo "=== DarkbetExchange Contract ZK Circuit Compilation Test ==="
for circuit in add_liquidity buy_position cancel_order claim_winnings create_market match_orders place_back place_lay remove_liquidity resolve_market settle_market; do
    echo "[Test] Compiling ${circuit}_v1.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}_v1.zk -o ${PROOF_DIR}/${circuit}_v1.zk.bin
    echo "  OK ${circuit}_v1.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}_v1.zk.bin) bytes)"
done
echo "=== All DarkbetExchange circuit compilation tests passed ==="
