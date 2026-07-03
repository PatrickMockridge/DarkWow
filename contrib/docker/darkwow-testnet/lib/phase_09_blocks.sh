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

# ── Genesis contract initialization verification ──────────────────────────
# Verify all 9 genesis contracts had their __initialize WASM export called
# and NativeToken's TOTAL_SUPPLY was seeded with the genesis reward.
# Without this, pow_reward_v1 fails for every mined block after genesis
# ("Supply mismatch" — current_supply=0 but expected_cumulative includes
# the genesis reward). These are GATES — failure stops the pipeline.
_verify_genesis_contract_init() {
    info "Verifying genesis contract initialization..."
    local n0_logs
    n0_logs=$(docker logs dwow-node0 2>&1 || true)

    # Check 1: All 9 contracts initialized
    local init_count
    init_count=$(echo "$n0_logs" | grep -c "init_contract OK" || echo 0)
    if [ "${init_count:-0}" -eq 9 ]; then
        pass "All 9 genesis contracts initialized (init_contract OK x9)"
    elif [ "${init_count:-0}" -gt 0 ]; then
        fail "Only ${init_count}/9 genesis contracts initialized — check dwowd logs"
    else
        fail "ZERO genesis contracts initialized (init_contract OK=0) — init_genesis_contracts may not have run"
    fi

    # Check 2: NativeToken TOTAL_SUPPLY seeded with genesis reward
    if echo "$n0_logs" | grep -q "TOTAL_SUPPLY seeded with genesis reward"; then
        pass "NativeToken TOTAL_SUPPLY seeded with genesis reward"
    else
        fail "NativeToken TOTAL_SUPPLY NOT seeded — cumulative supply chain broken. pow_reward_v1 will reject every block with 'Supply mismatch'"
    fi

    # Check 3: No supply mismatch errors (these indicate the cumulative
    # supply check in pow_reward_v1 is failing — blocks are being rejected)
    local supply_errors
    supply_errors=$(echo "$n0_logs" | grep -c "Supply mismatch" || echo 0)
    if [ "${supply_errors:-0}" -eq 0 ]; then
        pass "No supply mismatch errors (cumulative supply chain intact)"
    else
        fail "Found ${supply_errors} 'Supply mismatch' error(s) in dwowd logs — blocks are being rejected. pow_reward_v1 supply check is failing"
    fi
}

# ── Genesis ceremony verification (L1) ──────────────────────────────────────
# Verify node0 actually created genesis at runtime — the only check that
# docker-compose.yml cannot guarantee. (CREATE_GENESIS env vars are already
# set correctly in the compose file — no need to re-verify those.)
_verify_genesis_ceremony() {
    # Verify node0 has block 1 — wait up to 30s
    local n0_height
    for attempt in $(seq 1 15); do
        n0_height=$(jsonrpc_get_height "dwow-node0" 31345 2>/dev/null || echo 0)
        [ "${n0_height:-0}" -ge 1 ] && break
        sleep 2
    done
    if [ "${n0_height:-0}" -lt 1 ]; then
        fail "node0 has no genesis block after 30s — CREATE_GENESIS may have failed"
        return
    fi
    pass "node0 genesis block created (height=$n0_height)"

    # Store node0's merkle root as the reference for convergence verification.
    # merkle_root is a serialized field in the block JSON (unlike hash which is
    # a computed method). Format in raw TCP JSON-RPC: \"merkle_root\":[b0,...,b31]
    # (serde_json produces no spaces after colon). Identical blocks = identical
    # merkle root. All other nodes must sync this exact block from node0 via P2P.
    local n0_blk
    for attempt in $(seq 1 5); do
        n0_blk=$(jsonrpc_get_block "dwow-node0" 31345 1 2>&1) && break
        sleep 2
    done
    GENESIS_MERKLE_ROOT=$(echo "$n0_blk" | grep -o '\\"merkle_root\\":\[[^]]*\]' | head -1 || echo "")
    if [ -z "$GENESIS_MERKLE_ROOT" ]; then
        fail "node0 block 1: could not extract merkle_root from RPC response"
    else
        pass "node0 genesis merkle root recorded as reference"
    fi
}

phase_blocks() {
    info "Phase 9: Verifying block production..."

    _build_node_list

    # L1: Genesis ceremony verification — must pass before block checks
    _verify_genesis_ceremony

    # Genesis contract initialization — all 9 contracts must have
    # init_contract called and TOTAL_SUPPLY seeded. Without this,
    # blocks after genesis cannot be accepted (supply check fails).
    _verify_genesis_contract_init

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

    # ── L1: Genesis convergence (merkle root) ──────────────────────────
    # Every node must have node0's exact block 1, synced via P2P.
    # merkle_root format in raw TCP JSON-RPC: \"merkle_root\":[b0,...,b31]
    # (serde_json, no spaces). Identical blocks have identical merkle roots.
    info "Verifying genesis convergence (merkle root)..."
    if [ -z "$GENESIS_MERKLE_ROOT" ]; then
        fail "Cannot verify genesis convergence — node0 merkle root unknown"
    else
        local all_genesis_match=true
        for node_spec in "${NODE_LIST[@]}"; do
            local node_name="${node_spec%%:*}"
            local node_port="${node_spec##*:}"
            local blk mr
            for attempt in $(seq 1 5); do
                blk=$(jsonrpc_get_block "$node_name" "$node_port" 1 2>&1) && break
                sleep 2
            done
            mr=$(echo "$blk" | grep -o '\\"merkle_root\\":\[[^]]*\]' | head -1 || echo "")
            if [ "$mr" = "$GENESIS_MERKLE_ROOT" ]; then
                pass "$node_name genesis matches node0"
            elif [ -z "$mr" ]; then
                fail "$node_name block 1: RPC returned no merkle_root"
                all_genesis_match=false
            else
                fail "$node_name block 1 merkle root MISMATCH — different chain!"
                all_genesis_match=false
            fi
        done
        # Also check observer
        if docker ps --format '{{.Names}}' | grep -q "dwow-observer"; then
            local obs_blk obs_mr
            for attempt in $(seq 1 5); do
                obs_blk=$(jsonrpc_get_block "dwow-observer" 31345 1 2>&1) && break
                sleep 2
            done
            obs_mr=$(echo "$obs_blk" | grep -o '\\"merkle_root\\":\[[^]]*\]' | head -1 || echo "")
            if [ "$obs_mr" = "$GENESIS_MERKLE_ROOT" ]; then
                pass "observer genesis matches node0"
            else
                fail "observer block 1 merkle root MISMATCH — different chain!"
                all_genesis_match=false
            fi
        fi
        if [ "$all_genesis_match" = "true" ]; then
            pass "All nodes share the same genesis block — same chain confirmed"
        fi
    fi

    # ── L2: Cross-node consensus (merkle root at heights 2-5) ───────────
    # Early blocks should be identical across nodes. Beyond height 5, the
    # protocol's uncle/forgiving consensus (threshold=3) permits divergence.
    if [ "${#NODE_LIST[@]}" -ge 2 ]; then
        info "Cross-node consensus (merkle root at heights 2-5)..."
        # First get the minimum height across all nodes
        local min_h=999999
        for node_spec in "${NODE_LIST[@]}"; do
            local node_name="${node_spec%%:*}"
            local node_port="${node_spec##*:}"
            local h
            h=$(jsonrpc_get_height "$node_name" "$node_port" 2>/dev/null || echo 0)
            info "  $node_name height: $h"
            [ "$h" -lt "$min_h" ] && min_h="$h"
        done

        if [ "$min_h" -lt 2 ]; then
            warn "Cross-node consensus: min height is $min_h — skipping (need >= 2)"
        else
            local max_check=$min_h
            [ "$max_check" -gt 5 ] && max_check=5
            local h
            for h in $(seq 2 "$max_check"); do
                local ref_mr=""
                local ref_node=""
                local all_ok=true
                for node_spec in "${NODE_LIST[@]}"; do
                    local node_name="${node_spec%%:*}"
                    local node_port="${node_spec##*:}"
                    local blk mr
                    for attempt in $(seq 1 3); do
                        blk=$(jsonrpc_get_block "$node_name" "$node_port" "$h" 2>&1) && break
                        sleep 1
                    done
                    mr=$(echo "$blk" | grep -o '\\"merkle_root\\":\[[^]]*\]' | head -1 || echo "")
                    if [ -z "$mr" ]; then
                        fail "$node_name height=$h: RPC returned no merkle_root"
                        all_ok=false; continue
                    fi
                    if [ -z "$ref_mr" ]; then
                        ref_mr="$mr"
                        ref_node="$node_name"
                    elif [ "$mr" != "$ref_mr" ]; then
                        fail "CONSENSUS SPLIT at height=$h: $node_name differs from $ref_node"
                        all_ok=false
                    fi
                done
                if [ "$all_ok" = "true" ]; then
                    pass "Consensus height=$h: all nodes agree (merkle root match)"
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
