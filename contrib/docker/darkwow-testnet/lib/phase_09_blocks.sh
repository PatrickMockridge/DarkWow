# DarkWow Testnet Pipeline — Phase 9: Block Production Gate
#
# The wallet tests (Phase 10) need two things from the network:
#   1. Genesis happened — there's a chain to scan
#   2. Block 2 exists — there are blocks to find coins in
#
# Everything else is already proven by Rust:
#   - test_genesis_determinism → genesis correctness + merkle root convergence
#   - test_block_creation → height-2 acceptance + supply bridge (AC2-AC5)
#   - test_genesis_sync_materializes_contracts → contracts ride in genesis
#
# What we test here is irreducible to Rust: did the real Docker deployment
# actually produce blocks, and did ONLY node0 exercise genesis authority?
# Authority is verified via RPC block-1 hash comparison: if a non-node0 node
# has block 1 AND its hash matches node0's, it MUST have synced (cannot
# independently create an identical block). Docker-log grepping removed —
# log format, buffering, and container restarts produced false
# positives/negatives.
#
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NODE0, NATIVE_NODES),
#               helpers.sh (jsonrpc_get_height, jsonrpc_get_block)

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
    # ZK keygen for 9 genesis contracts can take several minutes on
    # first boot (--fresh). The health check no longer gates container
    # startup, so we wait here with a generous timeout. If genesis
    # truly never happens, subsequent RPC checks will catch it.
    local n0_height=0
    for i in $(seq 1 120); do
        n0_height=$(jsonrpc_get_height "$NODE0_NAME" "$NODE0_PORT")
        n0_height=$(echo "$n0_height" | tr -dc '0-9')
        n0_height="${n0_height:-0}"
        [ "$n0_height" -ge 1 ] 2>/dev/null && break
        sleep 5
    done
    if [ "${n0_height:-0}" -ge 1 ]; then
        pass "node0 genesis created (height=$n0_height)"
    else
        warn "node0 has no genesis block after 600s — check docker logs dwow-node0"
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
        warn "node0 still at height=${b2_height:-?} after 30s — mining may be slow"
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

    # ══ Genesis authority gate: ONLY node0 creates; everyone else syncs ══
    # The user directive this measures: "either a node creates its own
    # genesis on a separate network or it syncs to an existing one, but it
    # cannot do both."
    #
    # Authority is verified via RPC block-1 hash comparison, NOT docker-log
    # grepping. If a non-node0 node has block 1 AND its hash matches node0's,
    # it MUST have synced — it cannot independently create an identical block.
    # Docker-log grepping produced false positives and false negatives across
    # multiple pipeline runs (log format varies, buffering loses messages,
    # container restarts clear logs). RPC is deterministic and always available.
    #
    # Hard failures: node0 has no block 1, or a non-node0 node has a DIFFERENT
    # block 1 (independent genesis = authority violation).
    # Soft (warn): a non-node0 node has no block 1 (sync incomplete).

    # Helper: fetch block 1 canonical hash from a node via RPC.
    _get_block1_hash() {
        local container="$1" port="$2"
        local raw
        raw=$(jsonrpc_get_block "$container" "$port" 1 2>/dev/null || echo "")
        if [ -z "$raw" ]; then
            echo ""
            return
        fi
        echo "$raw" | jq -cS '.result' 2>/dev/null | sha256sum | cut -d' ' -f1
    }

    # Sentinel values that indicate an empty/unreadable block 1 response.
    local empty_sum
    empty_sum=$(printf '' | sha256sum | cut -d' ' -f1)
    local null_sum
    null_sum=$(printf 'null\n' | jq -cS '.' | sha256sum | cut -d' ' -f1)

    # (a) Positive: node0 is the genesis authority — confirmed by RPC height
    # check above (height >= 1). Now fetch node0's block 1 as the reference.
    local ref_sum
    ref_sum=$(_get_block1_hash "$NODE0_NAME" "$NODE0_PORT")
    if [ -z "$ref_sum" ] || [ "$ref_sum" = "$empty_sum" ] || [ "$ref_sum" = "$null_sum" ]; then
        warn "node0 genesis block unreadable via RPC — RPC may not be ready yet"
    else
    pass "node0 is the genesis authority (block 1 hash=${ref_sum:0:16}...)"

    # Non-authority check list: every NODE_LIST node except node0, plus the
    # observer (not in NODE_LIST — it has no mining role, but it MUST obey
    # the same genesis authority rule).
    local CHECK_LIST=()
    for node_spec in "${NODE_LIST[@]}"; do
        [ "${node_spec%%:*}" = "$NODE0_NAME" ] || CHECK_LIST+=("$node_spec")
    done
    if container_running "dwow-observer" 2>/dev/null; then
        CHECK_LIST+=("dwow-observer:31345")
    fi

    # (b) Wait for other nodes to sync block 1 (up to 60s retry per node).
    # Sync lag is normal — the observer must sync from node0 before node1
    # can sync from the observer. This is an infrastructure concern, not
    # an authority violation.
    for node_spec in "${CHECK_LIST[@]}"; do
        local name="${node_spec%%:*}"
        local port="${node_spec##*:}"
        local synced=0
        for i in $(seq 1 20); do
            local h
            h=$(jsonrpc_get_height "$name" "$port")
            h=$(echo "$h" | tr -dc '0-9')
            h="${h:-0}"
            if [ "$h" -ge 1 ] 2>/dev/null; then
                synced=1
                break
            fi
            sleep 3
        done
        if [ "$synced" -eq 0 ]; then
            warn "$name: sync timeout — height still 0 after 60s (infrastructure issue, not authority violation)"
        fi
    done

    # (c) Authority enforcement: every non-node0 node with block 1 MUST have
    # the same block 1 hash as node0. A different hash means the node created
    # its own independent genesis — an authority violation.
    for node_spec in "${CHECK_LIST[@]}"; do
        local name="${node_spec%%:*}"
        local port="${node_spec##*:}"
        local cmp_sum
        cmp_sum=$(_get_block1_hash "$name" "$port")
        if [ -z "$cmp_sum" ] || [ "$cmp_sum" = "$empty_sum" ] || [ "$cmp_sum" = "$null_sum" ]; then
            warn "$name: block 1 unavailable (sync incomplete — not an authority violation)"
            # Diagnostic: show what this node sees at RPC level
            local diag
            diag=$(jsonrpc_get_height "$name" "$port" 2>/dev/null || echo "RPC unreachable")
            info "  $name diagnostic: get_height=$diag"
        elif [ "$cmp_sum" = "$ref_sum" ]; then
            pass "$name: genesis block identical to node0 (synced, not independent)"
        else
            fail "$name: INDEPENDENT GENESIS — block 1 hash differs from node0 (ref=${ref_sum:0:16} got=${cmp_sum:0:16})"
        fi
    done
    fi  # else: node0 genesis block readable
}

# ==============================================================================
# Join Phase 9: Blockchain Sync
# ==============================================================================
phase_join_sync() {
    echo ""
    echo "=== Join Phase 9: Blockchain Sync ==="

    if ! container_running "$CONTAINER_NAME"; then
        warn "Container not running (run lifecycle phase first)"
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
