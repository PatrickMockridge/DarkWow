# DarkWow Testnet Pipeline — Phase 9: Block Production / Blockchain Sync
#
# Phase 9 local: verify genesis block, height increment, anchor validation.
# Phase 9 join: verify blockchain sync by checking height advances.
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NODE0, NATIVE_NODES, FINALITY_CARIBINA_ENABLED,
#                          FINALITY_ENABLE_MONERO, CONTAINER_NAME, RPC_PORT),
#               helpers.sh (jsonrpc)
#
# Sourced by test_pipeline.sh after phase_08_mining.sh.

phase_blocks() {
    info "Phase 9: Verifying block production..."

    info "Waiting for genesis + mined blocks..."
    WAIT_SECS=15
    for i in $(seq 1 $WAIT_SECS); do
        sleep 1
        if [ $((i % 10)) -eq 0 ]; then
            info "  waited ${i}s / ${WAIT_SECS}s..."
        fi
    done

    for attempt in 1 2 3 4 5; do
        BLOCK_INFO=$(jsonrpc_get_block "$NODE0" 31345 1 2>&1) && break
        sleep 2
    done
    if ! echo "$BLOCK_INFO" | grep -q '"result"\|"height"'; then
        echo "[FATAL] RPC response contains no block data after 5 retries — node0 may be down" >&2
        echo "Last response: $(echo "$BLOCK_INFO" | head -c 200)" >&2
        exit 1
    fi
    echo "$BLOCK_INFO" | head -c 200

    BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*') || true
    info "Initial block height: $BLOCK_HEIGHT"

    if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 1 ]; then
        pass "block height >= 1 (initialized)"
    else
        fail "block height >= 1 (got: $BLOCK_HEIGHT)"
    fi

    info "Waiting for additional blocks (timeout: 600s)..."
    BLOCK_HEIGHT=""
    START_TIME=$SECONDS
    BLOCK_TIMEOUT=600
    while true; do
        if [ $((SECONDS - START_TIME)) -ge $BLOCK_TIMEOUT ]; then
            fail "Block production timed out after ${BLOCK_TIMEOUT}s — check mining threads"
            break
        fi
        sleep 16
        BLOCK_HEIGHT=""
        # Try node0 first (merge-mined blocks appear here via mm_rpc)
        for attempt in 1 2 3; do
            BLOCK_INFO=$(jsonrpc_get_block "$NODE0" 31345 2 2>&1) && break
            sleep 2
        done
        # Fallback: try node2 (native-mined blocks, or blocks received via P2P)
        if [ -z "$BLOCK_INFO" ] && [ "$MODE" = "merge" ]; then
            for attempt in 1 2 3; do
                BLOCK_INFO=$(jsonrpc_get_block dwow-node2 31350 2 2>&1) && break
                sleep 2
            done
        fi
        if [ -n "$BLOCK_INFO" ]; then
            BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*') || true
        fi
        elapsed=$((SECONDS - START_TIME))
        info "  waited ${elapsed}s (height=${BLOCK_HEIGHT:-?})..."
        if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
            break
        fi
    done

    info "Block height after waiting: ${BLOCK_HEIGHT:-?}"

    if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
        pass "$MODE blocks produced (height=$BLOCK_HEIGHT)"

        # Cross-node verification based on node count
        if [ "$NATIVE_NODES" = "1" ]; then
            info "Solo mode — skipping cross-node consensus check"
        elif [ "$NATIVE_NODES" = "5" ]; then
            # 5-node consensus: verify nodes 1-4 have blocks
            NODE_RPC_PORTS=(31346 31350 31353 31356)
            for i in $(seq 0 3); do
                node_num=$((i + 1))
                port=${NODE_RPC_PORTS[$i]}
                info "Checking node$node_num block height..."
                NODE_BLOCK=$(jsonrpc_get_block "dwow-node$node_num" "$port" 2 2>/dev/null || true)
                NODE_HEIGHT=$(echo "$NODE_BLOCK" | grep -o '"height":[0-9]*' | head -1 | grep -o '[0-9]*' || true)
                if [ -n "$NODE_HEIGHT" ] && [ "$NODE_HEIGHT" -ge 2 ]; then
                    pass "node$node_num at height $NODE_HEIGHT"
                else
                    fail "node$node_num does not have block at height 2"
                fi
            done
        else
            # 2-node mode: verify node1 has blocks.
            # The daemon handles P2P peer discovery internally — seeds are
            # configured in the [net] TOML section. This check just confirms
            # consensus was achieved.
            info "Verifying cross-node consensus (node1 sees same blocks)..."
            NODE1_BLOCK=$(jsonrpc_get_block dwow-node1 31346 2 2>/dev/null || true)
            NODE1_HEIGHT=$(echo "$NODE1_BLOCK" | grep -o '"height":[0-9]*' | head -1 | grep -o '[0-9]*' || true)
            if [ -n "$NODE1_HEIGHT" ] && [ "$NODE1_HEIGHT" -ge 2 ]; then
                pass "node1 sees block at height $NODE1_HEIGHT (consensus confirmed)"
            else
                fail "node1 does not see block at height 2 (consensus check failed)"
            fi
        fi
    else
        fail "$MODE blocks produced (height=${BLOCK_HEIGHT:-?}, expected >= 2)"
    fi

    if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 1 ]; then
        info "Inspecting block 1 for PoW data..."
        for attempt in 1 2 3 4 5; do
            BLOCK_DATA=$(jsonrpc_get_block "$NODE0" 31345 1 2>&1) && break
            sleep 2
        done
        if [ -z "$BLOCK_DATA" ]; then
            echo "[FATAL] docker exec failed after 5 retries — cannot reach node0 RPC for PoW inspection" >&2
            exit 1
        fi

        if echo "$BLOCK_DATA" | grep -q '"result"'; then
            pass "block 1 fetched successfully"
        else
            fail "block 1 fetch"
        fi

        # Verify Caribina anchor presence/absence based on finality config
        info "Inspecting block 1 for Caribina anchor..."
        ANCHOR_TX_ID=$(echo "$BLOCK_DATA" | grep -o '"anchor_tx_id":"[^"]*"' | cut -d'"' -f4 || echo "")
        if [ -z "$ANCHOR_TX_ID" ]; then
            # Try to detect anchor as a hex/base58 field if JSON format differs
            ANCHOR_TX_ID=$(echo "$BLOCK_DATA" | grep -o 'anchor_tx_id[^,}]*' | head -1 || echo "")
        fi

        if [ "$FINALITY_CARIBINA_ENABLED" != "true" ]; then
            # Caribina disabled — anchor should be zero/absent
            if echo "$ANCHOR_TX_ID" | grep -qE '^[0]+$|^\s*$|^AAAAAAAAAAAAAAAA'; then
                pass "anchor_tx_id is zero (caribina disabled)"
            elif [ -z "$ANCHOR_TX_ID" ]; then
                pass "anchor_tx_id absent (caribina disabled)"
            else
                fail "anchor_tx_id should be zero (caribina disabled) but got: $ANCHOR_TX_ID"
            fi
        else
            # Caribina enabled (default) — anchor should be non-zero
            if [ -n "$ANCHOR_TX_ID" ] && ! echo "$ANCHOR_TX_ID" | grep -qE '^[0]+$|^AAAAAAAAAAAAAAAA'; then
                pass "anchor_tx_id present (caribina enabled): ${ANCHOR_TX_ID:0:16}..."
            else
                echo "  WARNING: anchor_tx_id is zero or absent (caribina enabled)"
                echo "  This is acceptable if ArDrive Turbo was unreachable —"
                echo "  anchoring is best-effort and mining proceeds without it."
                echo "  Raw block data excerpt:"
                echo "$BLOCK_DATA" | grep -o 'anchor[^,}]*' | head -3 || echo "  (no anchor fields found)"
                fail "anchor_tx_id should be non-zero (caribina enabled)"
            fi
        fi
    fi

    # Verify Monero anchor presence/absence based on finality config
    info "Inspecting block 1 for Monero anchor..."
    ANCHOR_MONERO_HEIGHT=$(echo "$BLOCK_DATA" | grep -o '"anchor_monero_height":[0-9]*' | grep -o '[0-9]*$' || echo "0")
    ANCHOR_MONERO_HASH=$(echo "$BLOCK_DATA" | grep -o '"anchor_monero_hash":"[^"]*"' | cut -d'"' -f4 || echo "")

    if [ "$FINALITY_ENABLE_MONERO" = "true" ]; then
        if [ -n "$ANCHOR_MONERO_HEIGHT" ] && [ "$ANCHOR_MONERO_HEIGHT" -gt 0 ]; then
            pass "anchor_monero_height non-zero (monero anchoring): $ANCHOR_MONERO_HEIGHT"
        else
            fail "anchor_monero_height is zero (expected non-zero with Monero anchoring enabled)"
        fi

        if [ -n "$ANCHOR_MONERO_HASH" ] && \
           [ "$ANCHOR_MONERO_HASH" != "0000000000000000000000000000000000000000000000000000000000000000" ]; then
            pass "anchor_monero_hash non-zero: ${ANCHOR_MONERO_HASH:0:16}..."
        else
            fail "anchor_monero_hash is zero (expected non-zero with Monero anchoring enabled)"
        fi

        # Verify node1 also sees Monero anchors via P2P propagation
        info "Verifying node1 P2P propagation of Monero anchor..."
        for attempt in 1 2 3 4 5; do
            NODE1_BLOCK=$(jsonrpc_get_block dwow-node1 31346 1 2>/dev/null) && break
            sleep 2
        done
        N1_ANCHOR_HEIGHT=$(echo "$NODE1_BLOCK" | grep -o '"anchor_monero_height":[0-9]*' | grep -o '[0-9]*$' || echo "0")
        if [ -n "$N1_ANCHOR_HEIGHT" ] && [ "$N1_ANCHOR_HEIGHT" -gt 0 ]; then
            pass "node1 sees Monero anchor at height $N1_ANCHOR_HEIGHT (P2P verification)"
        else
            fail "node1 Monero anchor missing (P2P sync may be incomplete)"
        fi
    else
        if [ "$ANCHOR_MONERO_HEIGHT" = "0" ] || [ -z "$ANCHOR_MONERO_HEIGHT" ]; then
            pass "anchor_monero_height is zero (monero anchoring disabled)"
        else
            fail "anchor_monero_height is non-zero ($ANCHOR_MONERO_HEIGHT) but Monero anchoring is disabled"
        fi
    fi

    # Cryptographic receipt verification (merge mode only)
    # Polls until receipts appear — merge mining is slower than native
    # because xmrig must find a Monero share meeting the target.
    if [ "$MODE" = "merge" ]; then
        info "Verifying cryptographic receipts (polling — merge mining pace)..."
        if ! container_running "$NODE0"; then
            fail "node0 not running — cannot verify merge mining receipts"
            return 1
        fi
        MM_DONE=false
        MM_START=$SECONDS
        while [ $((SECONDS - MM_START)) -lt 1800 ]; do
            NODE0_LOGS=$(docker logs "$NODE0" 2>&1 | sed 's/\x1b\[[0-9;]*m//g' || true)
            MM_SUBMIT_COUNT=$(echo "$NODE0_LOGS" | grep -c "Got solution submission" 2>/dev/null || echo 0)
            MM_AUX_VERIFIED=$(echo "$NODE0_LOGS" | grep -c "Aux merkle proof verified" 2>/dev/null || echo 0)
            MM_COINBASE_VERIFIED=$(echo "$NODE0_LOGS" | grep -c "Coinbase merkle proof verified" 2>/dev/null || echo 0)
            MM_ACCEPTED=$(echo "$NODE0_LOGS" | grep -c "Merge-mined block.*accepted" 2>/dev/null || echo 0)

            if [ "$MM_ACCEPTED" -gt 0 ]; then
                MM_DONE=true
                break
            fi
            elapsed=$((SECONDS - MM_START))
            info "  merge receipts: ${elapsed}s (submits=$MM_SUBMIT_COUNT accepted=$MM_ACCEPTED)..."
            sleep 30
        done

        if [ "$MM_SUBMIT_COUNT" -gt 0 ]; then
            pass "mm_submit_solution received ($MM_SUBMIT_COUNT submissions)"
        else
            fail "no mm_submit_solution received"
        fi

        if [ "$MM_AUX_VERIFIED" -gt 0 ]; then
            pass "aux merkle proof verified ($MM_AUX_VERIFIED)"
        else
            fail "aux merkle proof not verified"
        fi

        if [ "$MM_COINBASE_VERIFIED" -gt 0 ]; then
            pass "coinbase merkle proof verified ($MM_COINBASE_VERIFIED)"
        else
            fail "coinbase merkle proof not verified"
        fi

        if [ "$MM_ACCEPTED" -gt 0 ]; then
            pass "merge-mined block accepted ($MM_ACCEPTED)"
        else
            fail "no merge-mined block accepted"
        fi
    fi
}

# ==============================================================================
# Join Phase 9: Blockchain Sync
# ==============================================================================
phase_join_sync() {
    echo ""
    echo "=== Join Phase 9: Blockchain Sync ==="

    if ! container_running "$CONTAINER_NAME"; then
        fail "Container not running (run lifecycle phase first)"
        return 0
    fi

    echo "  Checking blockchain sync..."
    local info
    info=$(jsonrpc "$RPC_PORT" "blockchain.get_height")

    echo "  Waiting for block height > 0 (up to 300s)..."
    local synced=0
    local height=0
    for i in $(seq 1 60); do
        info=$(jsonrpc "$RPC_PORT" "blockchain.get_height")
        if echo "$info" | grep -q '"height"'; then
            height=$(echo "$info" | grep -o '"height":[0-9]*' | grep -o '[0-9]*' || echo "0")
            if [ -n "$height" ] && [ "$height" -gt 0 ] 2>/dev/null; then
                pass "Blockchain synced: height $height after $((i * 5))s"
                synced=1
                break
            fi
        fi
        sleep 5
    done

    if [ "$synced" -eq 0 ]; then
        echo "  Last blockchain.get_height response:"
        jsonrpc "$RPC_PORT" "blockchain.get_height" | head -1
        fail "Blockchain height is 0 after 300s (public testnet may not have blocks yet)"
    fi
}
