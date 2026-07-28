#!/bin/bash
# Game Room contract ZK circuit compilation test
set -e
ZKAS_BIN="./bin/zkas/zkas"
PROOF_DIR="src/contract/game_room/proof"
echo "=== GameRoom Contract ZK Circuit Compilation Test ==="
for circuit in call claim close_pot contribute_entropy create_room deposit fold place_bet raise settle_pot withdraw; do
    echo "[Test] Compiling ${circuit}_v1.zk..."
    $ZKAS_BIN ${PROOF_DIR}/${circuit}_v1.zk -o ${PROOF_DIR}/${circuit}_v1.zk.bin
    echo "  OK ${circuit}_v1.zk.bin ($(stat -c%s ${PROOF_DIR}/${circuit}_v1.zk.bin) bytes)"
done
echo "=== All GameRoom circuit compilation tests passed ==="
