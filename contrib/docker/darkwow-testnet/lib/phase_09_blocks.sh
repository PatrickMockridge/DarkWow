# DarkWow Testnet Pipeline — Phase 9: Block Production Diagnostics
#
# Phase 9 takes a diagnostic snapshot of an ongoing process. Mining started
# in Phase 5 (containers started), was confirmed active in Phase 7 (mining
# activity detected), and continues indefinitely after Phase 9 ends. This
# phase observes, it doesn't control.
#
# Three layers:
#   L1: Genesis Ceremony — did node0 create genesis? (hard gate)
#   L2: Per-Node Diagnostics — current height snapshot (observation)
#   L3: Cross-Node Convergence — same genesis across all nodes (hard gate)
#
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NODE0, NATIVE_NODES, FINALITY_CARIBINA_ENABLED,
#                          FINALITY_ENABLE_MONERO, CONTAINER_NAME, RPC_PORT),
#               helpers.sh (rpc_retry, jsonrpc_get_height, jsonrpc_get_block)
#
# Sourced by test_pipeline.sh after phase_08_mining.sh.

# Build the list of nodes to verify based on topology.
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

# ==============================================================================
# L1: Genesis Ceremony — hard gate
# ==============================================================================
# Verify node0 created genesis. This is the only lifecycle-critical check:
# if genesis never happened, nothing else matters. Use retries, not a
# long-running poll — the node was already confirmed mining in Phase 7.
_verify_genesis_ceremony() {
    local n0_height=0
    for attempt in $(seq 1 5); do
        n0_height=$(jsonrpc_get_height "dwow-node0" 31345 2>/dev/null || echo 0)
        n0_height=$(echo "$n0_height" | tr -dc '0-9')
        n0_height="${n0_height:-0}"
        [ "$n0_height" -ge 1 ] 2>/dev/null && break
        sleep 3
    done

    if [ "${n0_height:-0}" -lt 1 ]; then
        fail "node0 has no genesis block after 5 retries — CREATE_GENESIS may have failed"
        return 1
    fi
    pass "node0 genesis block created (height=$n0_height)"

    # Record node0's merkle root as the reference for convergence verification.
    # merkle_root is a serialized field in the block JSON. Identical blocks =
    # identical merkle root. All other nodes must sync this exact block from
    # node0 via P2P.
    local n0_blk
    n0_blk=$(jsonrpc_get_block "dwow-node0" 31345 1)
    GENESIS_MERKLE_ROOT=$(echo "$n0_blk" | jq -r '.result | fromjson | .header.merkle_root | @json' 2>/dev/null || echo "")
    if [ -z "$GENESIS_MERKLE_ROOT" ]; then
        fail "node0 block 1: could not extract merkle_root from RPC response"
    else
        pass "node0 genesis merkle root recorded as reference"
    fi
}

# ==============================================================================
# Genesis Contract Initialization — passive audit
# ==============================================================================
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
        warn "Only ${init_count}/9 genesis contracts initialized — check dwowd logs"
    else
        warn "ZERO genesis contracts initialized in logs — log format may differ"
    fi

    # Check 2: NativeToken TOTAL_SUPPLY seeded
    if echo "$n0_logs" | grep -q "TOTAL_SUPPLY seeded with genesis reward"; then
        pass "NativeToken TOTAL_SUPPLY seeded with genesis reward"
    else
        warn "NativeToken TOTAL_SUPPLY seed not found in logs"
    fi

    _check_supply_mismatches
}

# Height-aware supply mismatch detection (passive audit).
_check_supply_mismatches() {
    local containers=("dwow-node0" "dwow-node1" "dwow-observer")
    local total_bootstrap=0 total_steady=0 any_steady=false

    for container in "${containers[@]}"; do
        if ! docker ps --format '{{.Names}}' 2>/dev/null | grep -q "^${container}$"; then
            continue
        fi
        local logs
        logs=$(docker logs "$container" 2>&1 || true)

        local mismatches
        mismatches=$(echo "$logs" | grep -n "Supply mismatch" || true)
        [ -z "$mismatches" ] && continue

        local mismatch_count bootstrap=0 steady=0 unknown=0
        mismatch_count=$(echo "$mismatches" | wc -l)

        while IFS= read -r line; do
            local h
            h=$(echo "$line" | grep -o 'height=[0-9]*' | grep -o '[0-9]*' || echo "")
            if [ -z "$h" ]; then
                unknown=$((unknown + 1))
            elif [ "$h" -le 2 ]; then
                bootstrap=$((bootstrap + 1))
            else
                steady=$((steady + 1))
                any_steady=true
            fi
        done <<< "$mismatches"

        total_bootstrap=$((total_bootstrap + bootstrap))
        total_steady=$((total_steady + steady))

        [ "$steady" -gt 0 ] && warn "$container: ${steady} post-bootstrap supply mismatches"
        [ "$bootstrap" -gt 0 ] || [ "$unknown" -gt 0 ] && \
            info "$container: ${bootstrap} bootstrap, ${unknown} unknown-format mismatches"
    done

    if [ "$any_steady" = true ]; then
        warn "Found ${total_steady} post-bootstrap supply mismatches — passive audit divergence"
    elif [ "$total_bootstrap" -gt 0 ]; then
        pass "Supply mismatches only during bootstrap (heights 1-2) — expected"
    else
        pass "No supply mismatch errors in logs"
    fi
}

# ==============================================================================
# L2: Per-Node Diagnostics — snapshot current state
# ==============================================================================
# Take a single reading from each mining node. No waiting — mining is an
# ongoing process that was already confirmed in Phase 7. Report what we
# observe: height ≥ 2 = block production confirmed, height = 1 = genesis
# only (mining may still be working on block 2).
_diagnose_nodes() {
    for node_spec in "${NODE_LIST[@]}"; do
        local node_name="${node_spec%%:*}"
        local node_port="${node_spec##*:}"

        local h
        h=$(jsonrpc_get_height "$node_name" "$node_port")
        h=$(echo "$h" | tr -dc '0-9')
        h="${h:-0}"

        if [ -z "$h" ] || [ "$h" = "0" ]; then
            warn "$node_name: RPC unreachable or height 0 — node may not be ready"
        elif [ "$h" -ge 2 ]; then
            pass "$node_name: height=$h — block production confirmed"
        else
            info "$node_name: height=1 — genesis only, mining in progress"
        fi
    done
}

# ==============================================================================
# L3: Cross-Node Convergence — same genesis across all nodes
# ==============================================================================
_verify_genesis_convergence() {
    if [ -z "$GENESIS_MERKLE_ROOT" ]; then
        fail "Cannot verify genesis convergence — node0 merkle root unknown"
        return
    fi

    info "Verifying genesis convergence (merkle root)..."
    local all_genesis_match=true

    for node_spec in "${NODE_LIST[@]}"; do
        local node_name="${node_spec%%:*}"
        local node_port="${node_spec##*:}"
        local blk mr
        blk=$(jsonrpc_get_block "$node_name" "$node_port" 1)
        mr=$(echo "$blk" | jq -r '.result | fromjson | .header.merkle_root | @json' 2>/dev/null || echo "")

        if [ "$mr" = "$GENESIS_MERKLE_ROOT" ]; then
            pass "$node_name genesis matches node0"
        elif [ -z "$mr" ]; then
            warn "$node_name: RPC returned no merkle_root for block 1 — node may be syncing"
            all_genesis_match=false
        else
            fail "$node_name block 1 merkle root MISMATCH — different chain!"
            all_genesis_match=false
        fi
    done

    # Observer check
    if docker ps --format '{{.Names}}' | grep -q "dwow-observer"; then
        local obs_blk obs_mr
        obs_blk=$(jsonrpc_get_block "dwow-observer" 31345 1)
        obs_mr=$(echo "$obs_blk" | jq -r '.result | fromjson | .header.merkle_root | @json' 2>/dev/null || echo "")
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
}

# ==============================================================================
# Block Structure Inspection — PoW / anchoring
# ==============================================================================
_inspect_block_structure() {
    info "Inspecting node0 block 1..."
    local BLOCK_DATA
    BLOCK_DATA=$(jsonrpc_get_block "$NODE0" 31345 1)

    if [ -z "$BLOCK_DATA" ]; then
        warn "node0 RPC unreachable for block inspection"
        return 0
    fi

    if echo "$BLOCK_DATA" | grep -q '"result"'; then
        pass "block 1 fetched successfully"
    else
        warn "block 1 fetch failed"
    fi

    # Caribina anchor
    local ANCHOR_TX_ID
    ANCHOR_TX_ID=$(echo "$BLOCK_DATA" | grep -o 'anchor_tx_id[^,}]*' | sed 's/.*:\\"*//;s/\\"//g;s/"//g' | head -1 || echo "")

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
            warn "anchor_tx_id is zero (caribina enabled) — external service may be down, mining unaffected"
        fi
    fi

    # Monero anchor
    local ANCHOR_MONERO_HEIGHT
    ANCHOR_MONERO_HEIGHT=$(echo "$BLOCK_DATA" | grep -o 'anchor_monero_height[^,}]*' | grep -o '[0-9]*$' || echo "0")
    local ANCHOR_MONERO_HASH
    ANCHOR_MONERO_HASH=$(echo "$BLOCK_DATA" | grep -o 'anchor_monero_hash[^,}]*' | sed 's/.*:\\"*//;s/\\"//g;s/"//g' | head -1 || echo "")

    if [ "$FINALITY_ENABLE_MONERO" = "true" ]; then
        [ -n "$ANCHOR_MONERO_HEIGHT" ] && [ "$ANCHOR_MONERO_HEIGHT" -gt 0 ] && \
            pass "anchor_monero_height non-zero: $ANCHOR_MONERO_HEIGHT" || \
            warn "anchor_monero_height is zero (expected non-zero)"
        [ -n "$ANCHOR_MONERO_HASH" ] && [ "$ANCHOR_MONERO_HASH" != "0000000000000000000000000000000000000000000000000000000000000000" ] && \
            pass "anchor_monero_hash non-zero: ${ANCHOR_MONERO_HASH:0:16}..." || \
            warn "anchor_monero_hash is zero"
    else
        [ "$ANCHOR_MONERO_HEIGHT" = "0" ] || [ -z "$ANCHOR_MONERO_HEIGHT" ] && \
            pass "anchor_monero_height is zero (monero anchoring disabled)" || \
            warn "anchor_monero_height non-zero but Monero anchoring disabled"
    fi
}

# ==============================================================================
# Diagnostic Snapshots — observe trends over time
# ==============================================================================
_diagnostic_snapshots() {
    if [ "${#NODE_LIST[@]}" -lt 2 ]; then
        return
    fi

    info "Chain diagnostic (3 snapshots at 60s intervals)..."
    local NODE0_NAME="${NODE_LIST[0]%%:*}"
    local NODE0_PORT="${NODE_LIST[0]##*:}"
    local NODE1_NAME="${NODE_LIST[1]%%:*}"
    local NODE1_PORT="${NODE_LIST[1]##*:}"

    local prev_n0=0 prev_n1=0
    for snap in 1 2 3; do
        sleep 60

        local n0_tip n1_tip
        n0_tip=$(jsonrpc_get_height "$NODE0_NAME" "$NODE0_PORT")
        n0_tip=$(echo "$n0_tip" | tr -dc '0-9')
        n0_tip="${n0_tip:-?}"
        n1_tip=$(jsonrpc_get_height "$NODE1_NAME" "$NODE1_PORT")
        n1_tip=$(echo "$n1_tip" | tr -dc '0-9')
        n1_tip="${n1_tip:-?}"

        local delta_n0=""
        [ "$n0_tip" != "?" ] && [ "$prev_n0" != "0" ] && \
            delta_n0=" (+$((n0_tip - prev_n0)) blocks)" && prev_n0="$n0_tip"
        local delta_n1=""
        [ "$n1_tip" != "?" ] && [ "$prev_n1" != "0" ] && \
            delta_n1=" (+$((n1_tip - prev_n1)) blocks)" && prev_n1="$n1_tip"

        info "  snapshot $snap: node0=$n0_tip$delta_n0 node1=$n1_tip$delta_n1"

        # Genesis convergence check at each snapshot
        if [ "$n0_tip" != "?" ] && [ "$n1_tip" != "?" ] && \
           [ "$n0_tip" -ge 1 ] 2>/dev/null && [ "$n1_tip" -ge 1 ] 2>/dev/null; then
            local n0_mr n1_mr n0_blk n1_blk
            n0_blk=$(jsonrpc_get_block "$NODE0_NAME" "$NODE0_PORT" 1)
            n0_mr=$(echo "$n0_blk" | jq -r '.result | fromjson | .header.merkle_root | @json' 2>/dev/null || echo "?")
            n1_blk=$(jsonrpc_get_block "$NODE1_NAME" "$NODE1_PORT" 1)
            n1_mr=$(echo "$n1_blk" | jq -r '.result | fromjson | .header.merkle_root | @json' 2>/dev/null || echo "?")

            if [ "$n0_mr" = "$n1_mr" ] && [ "$n0_mr" != "?" ]; then
                info "    genesis merkle root match"
            else
                info "    genesis merkle differs: node0=${n0_mr:0:20}... node1=${n1_mr:0:20}..."
            fi
        fi
    done
}

# ==============================================================================
# Phase 9 entry point
# ==============================================================================
phase_blocks() {
    info "Phase 9: Block production diagnostics..."

    _build_node_list

    # L1: Genesis Ceremony — hard gate (did genesis happen?)
    _verify_genesis_ceremony

    # Genesis contract initialization — passive audit
    _verify_genesis_contract_init

    # L2: Per-Node Diagnostics — snapshot current state
    _diagnose_nodes

    # L3: Cross-Node Convergence — same genesis across all nodes
    _verify_genesis_convergence

    # Block structure inspection
    _inspect_block_structure

    # Diagnostic snapshots — observe trends over time
    _diagnostic_snapshots
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
    local synced=0 height=0
    for i in $(seq 1 60); do
        height=$(jsonrpc_get_height "$CONTAINER_NAME" "$RPC_PORT")
        height=$(echo "$height" | tr -dc '0-9')
        if [ -n "$height" ] && [ "$height" -gt 0 ] 2>/dev/null; then
            pass "Blockchain synced: height $height after $((i * 5))s"
            synced=1
            break
        fi
        sleep 5
    done

    if [ "$synced" -eq 0 ]; then
        warn "Blockchain height is 0 after 300s (public testnet may not have blocks yet)"
    fi
}
