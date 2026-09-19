#!/bin/bash
# Game Room contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/game_room/proof"
echo "=== GameRoom Contract ZK Circuit Compilation Test ==="
for circuit in call claim close_pot contribute_entropy create_room deposit fold place_bet raise settle_pot withdraw; do
    echo "[Test] Compiling ${circuit}.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}.zk -o ${PROOF_DIR}/${circuit}.zk.bin
    echo "  OK ${circuit}.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}.zk.bin) bytes)"
done
echo "=== All GameRoom circuit compilation tests passed ==="
