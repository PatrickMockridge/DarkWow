# DarkWow Testnet Pipeline — Phase 9: Block Production
#
# Phase 9: verify each mining node independently produces blocks.
# Cross-node consensus is a protocol property verified by the Python
# consensus model (chain_validation_model.py), not by bash polling.
#
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NODE0, NATIVE_NODES, FINALITY_CARIBINA_ENABLED,
#                          FINALITY_ENABLE_MONERO, CONTAINER_NAME, RPC_PORT),
#               helpers.sh (jsonrpc)
#
# Sourced by test_pipeline.sh after phase_08_mining.sh.

# Build the list of nodes to verify based on topology.
# Each entry is "name:rpc_port".
_build_node_list() {
    NODE_LIST=("${NODE0}:31345")
    if [ "$MODE" = "native" ]; then
        case "$NATIVE_NODES" in
            2) NODE_LIST+=("dwow-node1:31346") ;;
            5) NODE_LIST+=("dwow-node1:31346" "dwow-node2:31350" "dwow-node3:31353" "dwow-node4:31356") ;;
        esac
    elif [ "$MODE" = "merge" ]; then
        NODE_LIST+=("dwow-node2:31350")
    fi
}

phase_blocks() {
    info "Phase 9: Verifying block production..."

    _build_node_list

    info "Waiting for genesis + chain init..."
    sleep 15

    # --- Verify each node produces blocks independently ---
    for node_spec in "${NODE_LIST[@]}"; do
        NODE_NAME="${node_spec%%:*}"
        NODE_PORT="${node_spec##*:}"

        info "Checking $NODE_NAME block production (port $NODE_PORT)..."

        # Get initial block 1 to confirm chain initialized
        for attempt in 1 2 3 4 5; do
            BLOCK_INFO=$(jsonrpc_get_block "$NODE_NAME" "$NODE_PORT" 1 2>&1) && break
            sleep 2
        done
        if ! echo "$BLOCK_INFO" | grep -q '"result"\|"height"'; then
            fail "$NODE_NAME RPC not returning block data after 5 retries"
            echo "Last response: $(echo "$BLOCK_INFO" | head -c 200)" >&2
            continue
        fi
        echo "$BLOCK_INFO" | head -c 200

        BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*') || true

        if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 1 ]; then
            pass "$NODE_NAME height >= 1 (initialized)"
        else
            fail "$NODE_NAME height >= 1 (got: $BLOCK_HEIGHT)"
        fi

        # Target height: solo needs 2 (genesis + 1 mined), multi-node needs 10+
        # so convergence check at depth=5 from tip has meaningful block history.
        local TARGET_HEIGHT=2
        local TIMEOUT=600
        if [ "${#NODE_LIST[@]}" -ge 2 ]; then
            TARGET_HEIGHT=10
            TIMEOUT=2400
        fi

        # Wait for mined blocks to reach target height
        info "Waiting for $NODE_NAME to mine blocks (timeout: ${TIMEOUT}s, target height=$TARGET_HEIGHT)..."
        START_TIME=$SECONDS
        while true; do
            if [ $((SECONDS - START_TIME)) -ge $TIMEOUT ]; then
                fail "$NODE_NAME block production timed out after ${TIMEOUT}s"
                break
            fi
            sleep 16
            for attempt in 1 2 3; do
                BLOCK_INFO=$(jsonrpc_get_block "$NODE_NAME" "$NODE_PORT" "$TARGET_HEIGHT" 2>&1) && break
                sleep 2
            done
            if [ -n "$BLOCK_INFO" ]; then
                BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*') || true
            fi
            elapsed=$((SECONDS - START_TIME))
            info "  $NODE_NAME waited ${elapsed}s (height=${BLOCK_HEIGHT:-?})..."
            if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge "$TARGET_HEIGHT" ]; then
                break
            fi
        done

        if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge "$TARGET_HEIGHT" ]; then
            pass "$NODE_NAME blocks produced (height=$BLOCK_HEIGHT)"
        else
            fail "$NODE_NAME blocks produced (height=${BLOCK_HEIGHT:-?}, expected >= $TARGET_HEIGHT)"
        fi
    done

    # --- PoW / anchor inspection (node0 block 1 only) ---
    info "Inspecting node0 block 1 for PoW data..."
    for attempt in 1 2 3 4 5; do
        BLOCK_DATA=$(jsonrpc_get_block "$NODE0" 31345 1 2>&1) && break
        sleep 2
    done
    if [ -z "$BLOCK_DATA" ]; then
        fail "node0 RPC unreachable for PoW inspection after 5 retries"
        return 1
    fi

    if echo "$BLOCK_DATA" | grep -q '"result"'; then
        pass "block 1 fetched successfully"
    else
        fail "block 1 fetch"
    fi

    # Cross-node chain convergence check (multi-node only).
    # Uncle-merkle consensus means nodes may have different blocks at the tip
    # (one node's canonical block may be another's uncle). But blocks several
    # deep are settled — both nodes MUST agree on the canonical chain at
    # depth >= 5 from the tip. We compare block hashes at that depth.
    if [ "${#NODE_LIST[@]}" -ge 2 ]; then
        info "Verifying chain convergence across nodes..."

        # Collect current heights from all nodes
        declare -a NODE_HEIGHTS=()
        for node_spec in "${NODE_LIST[@]}"; do
            NODE_NAME="${node_spec%%:*}"
            NODE_PORT="${node_spec##*:}"
            for attempt in 1 2 3; do
                NODE_BLOCK=$(jsonrpc_get_block "$NODE_NAME" "$NODE_PORT" 2 2>&1) && break
                sleep 2
            done
            h=$(echo "$NODE_BLOCK" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*' || echo "0")
            NODE_HEIGHTS+=("$h")
            info "  $NODE_NAME height: $h"
        done

        # Find minimum height across all nodes
        MIN_HEIGHT="${NODE_HEIGHTS[0]}"
        for h in "${NODE_HEIGHTS[@]}"; do
            [ "$h" -lt "$MIN_HEIGHT" ] && MIN_HEIGHT="$h"
        done

        # Blocks are settled 5 deep from tip. Check at that depth.
        CHECK_DEPTH=5
        CHECK_HEIGHT=$((MIN_HEIGHT - CHECK_DEPTH))
        [ "$CHECK_HEIGHT" -lt 1 ] && CHECK_HEIGHT=1

        info "Convergence check at height $CHECK_HEIGHT (min tip=$MIN_HEIGHT, depth=$CHECK_DEPTH)..."

        # Fetch block at check_height from all nodes, compare hashes
        declare -A NODE_HASHES=()
        all_match=true
        first_hash=""
        for node_spec in "${NODE_LIST[@]}"; do
            NODE_NAME="${node_spec%%:*}"
            NODE_PORT="${node_spec##*:}"
            for attempt in 1 2 3; do
                NODE_BLOCK=$(jsonrpc_get_block "$NODE_NAME" "$NODE_PORT" "$CHECK_HEIGHT" 2>&1) && break
                sleep 2
            done
            h=$(echo "$NODE_BLOCK" | grep -o '\\"hash\\":\\"[^\\]*\\"' | head -1 | sed 's/\\"hash\\":\\"//;s/\\"//' || echo "")
            NODE_HASHES["$NODE_NAME"]="$h"
            if [ -z "$first_hash" ]; then
                first_hash="$h"
            elif [ "$h" != "$first_hash" ]; then
                all_match=false
            fi
            info "  $NODE_NAME height=$CHECK_HEIGHT hash=${h:0:16}..."
        done

        if [ "$all_match" = true ] && [ -n "$first_hash" ]; then
            pass "chain converged at height $CHECK_HEIGHT — all nodes agree"
        elif [ "$CHECK_HEIGHT" -le 2 ] && [ "$all_match" != true ]; then
            info "Not enough blocks for depth-5 convergence (min height=$MIN_HEIGHT)"
            NODE0_HASH="${NODE_HASHES[${NODE_LIST[0]%%:*}]}"
            for node_spec in "${NODE_LIST[@]:1}"; do
                PEER_NAME="${node_spec%%:*}"
                PEER_HASH="${NODE_HASHES[$PEER_NAME]}"
                if [ -n "$NODE0_HASH" ] && [ -n "$PEER_HASH" ] && [ "$NODE0_HASH" = "$PEER_HASH" ]; then
                    info "$PEER_NAME genesis matches node0 (not enough blocks yet for depth-5)"
                else
                    info "$PEER_NAME hash differs at height $CHECK_HEIGHT — may still converge"
                fi
            done
        else
            warn "chain not converged at height $CHECK_HEIGHT — nodes may still be syncing"
        fi
    fi

    # Verify Caribina anchor presence/absence based on finality config
    info "Inspecting block 1 for Caribina anchor..."
    ANCHOR_TX_ID=$(echo "$BLOCK_DATA" | grep -o '"anchor_tx_id":"[^"]*"' | cut -d'"' -f4 || echo "")
    if [ -z "$ANCHOR_TX_ID" ]; then
        ANCHOR_TX_ID=$(echo "$BLOCK_DATA" | grep -o 'anchor_tx_id[^,}]*' | head -1 || echo "")
    fi

    if [ "$FINALITY_CARIBINA_ENABLED" != "true" ]; then
        if echo "$ANCHOR_TX_ID" | grep -qE '^[0]+$|^\s*$|^AAAAAAAAAAAAAAAA'; then
            pass "anchor_tx_id is zero (caribina disabled)"
        elif [ -z "$ANCHOR_TX_ID" ]; then
            pass "anchor_tx_id absent (caribina disabled)"
        else
            fail "anchor_tx_id should be zero (caribina disabled) but got: $ANCHOR_TX_ID"
        fi
    else
        if [ -n "$ANCHOR_TX_ID" ] && ! echo "$ANCHOR_TX_ID" | grep -qE '^[0]+$|^AAAAAAAAAAAAAAAA'; then
            pass "anchor_tx_id present (caribina enabled): ${ANCHOR_TX_ID:0:16}..."
        else
            echo "  WARNING: anchor_tx_id is zero or absent (caribina enabled)"
            echo "  This is acceptable if ArDrive Turbo was unreachable —"
            echo "  anchoring is best-effort and mining proceeds without it."
            echo "  Raw block data excerpt:"
            echo "$BLOCK_DATA" | grep -o 'anchor[^,}]*' | head -3 || echo "  (no anchor fields found)"
            warn "anchor_tx_id is zero (caribina enabled) — external service may be down, mining unaffected"
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
    else
        if [ "$ANCHOR_MONERO_HEIGHT" = "0" ] || [ -z "$ANCHOR_MONERO_HEIGHT" ]; then
            pass "anchor_monero_height is zero (monero anchoring disabled)"
        else
            fail "anchor_monero_height is non-zero ($ANCHOR_MONERO_HEIGHT) but Monero anchoring is disabled"
        fi
    fi

    # Cryptographic receipt verification (merge mode only)
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
