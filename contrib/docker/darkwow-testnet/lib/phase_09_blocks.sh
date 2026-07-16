# DarkWow Testnet Pipeline — Phase 9: Block Production Gate
#
# The wallet tests (Phase 10) need two things from the network:
#   1. Genesis happened — there's a chain to scan
#   2. Block 2 exists — there are blocks to find coins in
#
# Everything else is already proven by Rust:
#   - test_genesis_determinism → genesis correctness + merkle root convergence
#   - test_block_creation → height-2 acceptance + supply bridge (AC2-AC5)
#   - init_genesis_contracts → contract initialization
#
# What we test here is irreducible to Rust: did the real Docker deployment
# actually produce blocks? Single RPC check, retry with 3s sleep, 30s max.
#
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NODE0, NATIVE_NODES),
#               helpers.sh (jsonrpc_get_height)

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
    info "Phase 9: Block production gate..."

    _build_node_list

    local NODE0_NAME="${NODE0}:31345"
    NODE0_NAME="${NODE0_NAME%%:*}"
    local NODE0_PORT=31345

    # ── Genesis: did node0 create genesis? ──────────────────────────
    # Irreducible Docker check — cannot test in Rust. If genesis
    # never happened, nothing else matters.
    local n0_height=0
    for i in $(seq 1 10); do
        n0_height=$(jsonrpc_get_height "$NODE0_NAME" "$NODE0_PORT")
        n0_height=$(echo "$n0_height" | tr -dc '0-9')
        n0_height="${n0_height:-0}"
        [ "$n0_height" -ge 1 ] 2>/dev/null && break
        sleep 3
    done
    if [ "${n0_height:-0}" -ge 1 ]; then
        pass "node0 genesis created (height=$n0_height)"
    else
        fail "node0 has no genesis block after 30s — CREATE_GENESIS may have failed"
        return 1
    fi

    # ── Block production: did node0 mine at least block 2? ─────────
    # Wallet tests (Phase 10) need blocks to scan. If block 2 doesn't
    # exist, the wallet will find zero coins. This is a soft gate:
    # warn, don't fail — wallet tests self-validate.
    local b2_height=0
    for i in $(seq 1 10); do
        b2_height=$(jsonrpc_get_height "$NODE0_NAME" "$NODE0_PORT")
        b2_height=$(echo "$b2_height" | tr -dc '0-9')
        b2_height="${b2_height:-0}"
        [ "$b2_height" -ge 2 ] 2>/dev/null && break
        sleep 3
    done
    if [ "${b2_height:-0}" -ge 2 ]; then
        pass "node0 height=$b2_height — block production confirmed"
    else
        if [ "$MODE" = "merge" ]; then
            fail "node0 still at height=${b2_height:-?} after 30s — merge mining should produce blocks quickly"
        else
            warn "node0 still at height=${b2_height:-?} after 30s — mining may be slow"
        fi
    fi

    # ── Other nodes: alive check, observational only ───────────────
    for node_spec in "${NODE_LIST[@]}"; do
        local node_name="${node_spec%%:*}"
        local node_port="${node_spec##*:}"
        [ "$node_name" = "$NODE0_NAME" ] && continue  # already checked above

        local h
        h=$(jsonrpc_get_height "$node_name" "$node_port")
        h=$(echo "$h" | tr -dc '0-9')
        h="${h:-0}"
        if [ "$h" -ge 1 ] 2>/dev/null; then
            pass "$node_name: height=$h"
        else
            warn "$node_name: RPC unreachable or height=0"
        fi
    done
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
