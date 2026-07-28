#!/bin/bash
set -e; ZKAS_BIN="./bin/zkas/zkas"; DIR="src/contract/darkbet_exchange/proof"
echo "=== DarkbetExchange ZK Circuit Compilation ==="
for c in add_liquidity buy_position cancel_order claim_winnings create_market match_orders place_back place_lay remove_liquidity resolve_market; do echo "  $c..."; $ZKAS_BIN ${DIR}/${c}_v1.zk -o ${DIR}/${c}_v1.zk.bin; [ -f "${DIR}/${c}_v1.zk.bin" ] || exit 1; done
echo "=== Done ==="
