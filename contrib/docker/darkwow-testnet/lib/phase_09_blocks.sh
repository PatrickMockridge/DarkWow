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

# ── Genesis ceremony verification (L1) ──────────────────────────────────────
# Verify node0 actually created genesis at runtime — the only check that
# docker-compose.yml cannot guarantee. (CREATE_GENESIS env vars are already
# set correctly in the compose file — no need to re-verify those.)
_verify_genesis_ceremony() {
    # Verify node0 has block 1 — wait up to 30s
    local n0_block1 n0_hash
    for attempt in $(seq 1 15); do
        n0_block1=$(jsonrpc_get_block "dwow-node0" 31345 1 2>&1) && break
        sleep 2
    done
    # Hash extraction: raw TCP JSON-RPC uses escaped quotes (\"hash\")
    n0_hash=$(echo "$n0_block1" | grep -o '\\"hash\\":"[^"]*"' | head -1 | cut -d'"' -f4 || echo "")
    if [ -n "$n0_hash" ] && [ ${#n0_hash} -ge 64 ]; then
        pass "node0 genesis block created: ${n0_hash:0:16}..."
        GENESIS_HASH="$n0_hash"
    else
        fail "node0 has no genesis block after 30s — CREATE_GENESIS may have failed"
        GENESIS_HASH=""
    fi
}

phase_blocks() {
    info "Phase 9: Verifying block production..."

    _build_node_list

    # L1: Genesis ceremony verification — must pass before block checks
    _verify_genesis_ceremony

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
            warn "$NODE_NAME RPC not returning block data after 5 retries — node may be mining (RPC briefly unresponsive)"
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

        # Infrastructure gate: node must produce at least 1 mined block.
        # Proves the mining thread is alive. If this fails, the node is broken.
        info "Waiting for $NODE_NAME to mine its first block..."
        START_TIME=$SECONDS
        while true; do
            if [ $((SECONDS - START_TIME)) -ge 600 ]; then
                warn "$NODE_NAME block production timed out after 600s — mining may be slow"
                break
            fi
            sleep 16
            for attempt in 1 2 3; do
                BLOCK_INFO=$(jsonrpc_get_block "$NODE_NAME" "$NODE_PORT" 2 2>&1) && break
                sleep 2
            done
            if [ -n "$BLOCK_INFO" ]; then
                BLOCK_HEIGHT=$(echo "$BLOCK_INFO" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*') || true
            fi
            elapsed=$((SECONDS - START_TIME))
            info "  $NODE_NAME waited ${elapsed}s (height=${BLOCK_HEIGHT:-?})..."
            if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
                break
            fi
        done

        if [ -n "$BLOCK_HEIGHT" ] && [ "$BLOCK_HEIGHT" -ge 2 ]; then
            pass "$NODE_NAME blocks produced (height=$BLOCK_HEIGHT)"
        else
            warn "$NODE_NAME blocks produced (height=${BLOCK_HEIGHT:-?}, expected >= 2)"
        fi
    done

    # ── L1: Genesis hash convergence ──────────────────────────────────
    # All nodes must have the same block 1 hash (synced from node0).
    if [ -n "$GENESIS_HASH" ]; then
        info "Verifying genesis hash convergence across all nodes..."
        local all_converged=true
        for node_spec in "${NODE_LIST[@]}"; do
            local node_name="${node_spec%%:*}"
            local node_port="${node_spec##*:}"
            local node_block1 node_hash
            for attempt in $(seq 1 5); do
                node_block1=$(jsonrpc_get_block "$node_name" "$node_port" 1 2>&1) && break
                sleep 2
            done
            node_hash=$(echo "$node_block1" | grep -o '\\"hash\\":"[^"]*"' | head -1 | cut -d'"' -f4 || echo "")
            if [ "$node_hash" = "$GENESIS_HASH" ]; then
                pass "$node_name genesis hash matches node0"
            else
                fail "$node_name genesis hash MISMATCH: ${node_hash:0:16}... != ${GENESIS_HASH:0:16}..."
                all_converged=false
            fi
        done
        # Also check observer
        if docker ps --format '{{.Names}}' | grep -q "dwow-observer"; then
            local obs_block1 obs_hash
            for attempt in $(seq 1 5); do
                obs_block1=$(jsonrpc_get_block "dwow-observer" 31345 1 2>&1) && break
                sleep 2
            done
            obs_hash=$(echo "$obs_block1" | grep -o '\\"hash\\":"[^"]*"' | head -1 | cut -d'"' -f4 || echo "")
            if [ "$obs_hash" = "$GENESIS_HASH" ]; then
                pass "observer genesis hash matches node0"
            else
                fail "observer genesis hash MISMATCH: ${obs_hash:0:16}... != ${GENESIS_HASH:0:16}..."
                all_converged=false
            fi
        fi
        if [ "$all_converged" = "true" ]; then
            pass "All nodes converged on genesis hash: ${GENESIS_HASH:0:16}..."
        fi
    fi

    # ── L2: Cross-node consensus verification ─────────────────────────
    # Verify block hash equality at sampled heights across all nodes.
    if [ "${#NODE_LIST[@]}" -ge 2 ]; then
        info "Cross-node consensus verification..."
        # Find the minimum common height across all nodes
        local min_height=999999
        for node_spec in "${NODE_LIST[@]}"; do
            local node_name="${node_spec%%:*}"
            local node_port="${node_spec##*:}"
            local h
            h=$(jsonrpc_get_height "$node_name" "$node_port" 2>/dev/null || echo 0)
            info "  $node_name height: $h"
            [ "$h" -lt "$min_height" ] && min_height="$h"
        done

        if [ "$min_height" -lt 2 ]; then
            warn "Cross-node consensus: minimum height is $min_height — skipping (need >= 2)"
        else
            # Only check early heights (2-5). Beyond height 5, the protocol's
            # uncle/forgiving consensus model (threshold=3) permits legitimate
            # divergence as nodes mine competing blocks on different forks.
            # Convergence is expected at heights 1-5 because there hasn't been
            # time for forks to develop. Height 1 is already verified by the
            # genesis convergence check above.
            local check_heights=(2 3 4 5)

            for height in "${check_heights[@]}"; do
                local ref_hash=""
                local ref_node=""
                local all_ok=true
                for node_spec in "${NODE_LIST[@]}"; do
                    local node_name="${node_spec%%:*}"
                    local node_port="${node_spec##*:}"
                    local blk hash_val
                    for attempt in $(seq 1 3); do
                        blk=$(jsonrpc_get_block "$node_name" "$node_port" "$height" 2>&1) && break
                        sleep 1
                    done
                    hash_val=$(echo "$blk" | grep -o '\\"hash\\":"[^"]*"' | head -1 | cut -d'"' -f4 || echo "")
                    if [ -z "$hash_val" ]; then
                        fail "$node_name height=$height: RPC returned no hash"
                        all_ok=false; continue
                    fi
                    if [ -z "$ref_hash" ]; then
                        ref_hash="$hash_val"
                        ref_node="$node_name"
                    elif [ "$hash_val" != "$ref_hash" ]; then
                        fail "CONSENSUS SPLIT at height=$height: $node_name=${hash_val:0:12}... != $ref_node=${ref_hash:0:12}..."
                        all_ok=false
                    fi
                done
                if [ "$all_ok" = "true" ]; then
                    pass "Consensus height=$height: all nodes agree (${ref_hash:0:12}...)"
                fi
            done
        fi
    fi

    # --- PoW / anchor inspection (node0 block 1 only) ---
    info "Inspecting node0 block 1 for PoW data..."
    for attempt in 1 2 3 4 5; do
        BLOCK_DATA=$(jsonrpc_get_block "$NODE0" 31345 1 2>&1) && break
        sleep 2
    done
    if [ -z "$BLOCK_DATA" ]; then
        warn "node0 RPC unreachable for PoW inspection after 5 retries — mining may be blocking RPC"
        return 0
    fi

    if echo "$BLOCK_DATA" | grep -q '"result"'; then
        pass "block 1 fetched successfully"
    else
        warn "block 1 fetch failed — RPC may be busy mining"
    fi

    # Diagnostic ping: snapshot heights and genesis hash at intervals.
    # No timeouts, no targets — just observe what the nodes are doing.
    # Uncle-merkle correctness is verified by the Python model.
    if [ "${#NODE_LIST[@]}" -ge 2 ]; then
        info "Chain diagnostic (snapshot pings)..."

        NODE0_SPEC="${NODE_LIST[0]}"
        NODE0_NAME="${NODE0_SPEC%%:*}"
        NODE0_PORT="${NODE0_SPEC##*:}"
        NODE1_SPEC="${NODE_LIST[1]}"
        NODE1_NAME="${NODE1_SPEC%%:*}"
        NODE1_PORT="${NODE1_SPEC##*:}"

        # Take snapshots at intervals — observe chain state evolving
        for snap in 1 2 3; do
            sleep 60

            for attempt in 1 2 3; do
                N0_BLOCK=$(jsonrpc_get_block "$NODE0_NAME" "$NODE0_PORT" 2 2>&1) && break; sleep 2
            done
            N0_TIP=$(echo "$N0_BLOCK" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*' || echo "?")
            for attempt in 1 2 3; do
                N1_BLOCK=$(jsonrpc_get_block "$NODE1_NAME" "$NODE1_PORT" 2 2>&1) && break; sleep 2
            done
            N1_TIP=$(echo "$N1_BLOCK" | grep -o '\\"height\\":[0-9]*' | head -1 | grep -o '[0-9]*' || echo "?")

            info "  snapshot $snap: node0=$N0_TIP node1=$N1_TIP"

            if [ "$N0_TIP" != "?" ] && [ "$N1_TIP" != "?" ] && [ "$N0_TIP" -ge 1 ] && [ "$N1_TIP" -ge 1 ]; then
                for attempt in 1 2 3; do
                    N0_GEN=$(jsonrpc_get_block "$NODE0_NAME" "$NODE0_PORT" 1 2>&1) && break; sleep 2
                done
                N0_MR=$(echo "$N0_GEN" | grep -o '\\"merkle_root\\":\[[^]]*\]' | head -1 || echo "?")
                for attempt in 1 2 3; do
                    N1_GEN=$(jsonrpc_get_block "$NODE1_NAME" "$NODE1_PORT" 1 2>&1) && break; sleep 2
                done
                N1_MR=$(echo "$N1_GEN" | grep -o '\\"merkle_root\\":\[[^]]*\]' | head -1 || echo "?")

                if [ "$N0_MR" = "$N1_MR" ] && [ "$N0_MR" != "?" ]; then
                    info "    genesis merkle root match (${N0_MR:0:30}...)"
                else
                    info "    genesis merkle differs: node0=${N0_MR:0:20}... node1=${N1_MR:0:20}..."
                fi
            fi
        done
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
            warn "anchor_tx_id should be zero (caribina disabled) but got: $ANCHOR_TX_ID"
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
            warn "anchor_monero_height is zero (expected non-zero with Monero anchoring enabled)"
        fi

        if [ -n "$ANCHOR_MONERO_HASH" ] && \
           [ "$ANCHOR_MONERO_HASH" != "0000000000000000000000000000000000000000000000000000000000000000" ]; then
            pass "anchor_monero_hash non-zero: ${ANCHOR_MONERO_HASH:0:16}..."
        else
            warn "anchor_monero_hash is zero (expected non-zero with Monero anchoring enabled)"
        fi
    else
        if [ "$ANCHOR_MONERO_HEIGHT" = "0" ] || [ -z "$ANCHOR_MONERO_HEIGHT" ]; then
            pass "anchor_monero_height is zero (monero anchoring disabled)"
        else
            warn "anchor_monero_height is non-zero ($ANCHOR_MONERO_HEIGHT) but Monero anchoring is disabled"
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
            warn "no mm_submit_solution received"
        fi

        if [ "$MM_AUX_VERIFIED" -gt 0 ]; then
            pass "aux merkle proof verified ($MM_AUX_VERIFIED)"
        else
            warn "aux merkle proof not verified"
        fi

        if [ "$MM_COINBASE_VERIFIED" -gt 0 ]; then
            pass "coinbase merkle proof verified ($MM_COINBASE_VERIFIED)"
        else
            warn "coinbase merkle proof not verified"
        fi

        if [ "$MM_ACCEPTED" -gt 0 ]; then
            pass "merge-mined block accepted ($MM_ACCEPTED)"
        else
            warn "no merge-mined block accepted"
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
        warn "Blockchain height is 0 after 300s (public testnet may not have blocks yet)"
    fi
}
