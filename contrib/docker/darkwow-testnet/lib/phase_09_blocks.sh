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
# RPC checks with retry, plus docker-log discriminators for the authority
# gate (pipeline_model.py L1).
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

    # ══ Genesis authority gate: ONLY node0 creates; everyone else syncs ══
    # The user directive this measures: "either a node creates its own
    # genesis on a separate network or it syncs to an existing one, but it
    # cannot do both." Hard failures — phase_gate stops the pipeline.
    local AUTHORITY_LOG="Genesis block created at height 1"
    local SKIP_LOG="Skipping genesis creation"
    local STARTUP_DEPLOY_LOG="Initializing 9 genesis contracts"   # deleted startup path — must appear NOWHERE
    local APPLIED_LOG="Genesis deployments applied: 9 contracts"  # block-execution path — must appear EVERYWHERE

    # (a) Positive: node0 is the genesis authority.
    if docker logs "$NODE0_NAME" 2>&1 | grep -q "$AUTHORITY_LOG"; then
        pass "node0 is the genesis authority"
    else
        fail "node0 did not create genesis (no '$AUTHORITY_LOG' log)"
    fi

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

    # (b) Negative: no other node created a genesis block; each declared
    # sync-only genesis.
    for node_spec in "${CHECK_LIST[@]}"; do
        local name="${node_spec%%:*}"
        if docker logs "$name" 2>&1 | grep -q "$AUTHORITY_LOG"; then
            fail "$name created a genesis block — only node0 may"
        else
            pass "$name did not create genesis"
        fi
        if docker logs "$name" 2>&1 | grep -q "$SKIP_LOG"; then
            pass "$name declared sync-only genesis"
        else
            fail "$name missing '$SKIP_LOG' log"
        fi
    done

    # (c) Deployment provenance: NO node deploys contracts at startup (the
    # genesis block carries them); EVERY node — node0 included — materializes
    # them by executing the genesis block (node0 at creation, others at sync).
    for node_spec in "${NODE0_NAME}:${NODE0_PORT}" "${CHECK_LIST[@]}"; do
        local name="${node_spec%%:*}"
        if docker logs "$name" 2>&1 | grep -qi "$STARTUP_DEPLOY_LOG"; then
            fail "$name deployed contracts at startup — genesis must carry deployments"
        else
            pass "$name: no startup contract deployment"
        fi
        if docker logs "$name" 2>&1 | grep -q "$APPLIED_LOG"; then
            pass "$name: genesis deployments applied via block execution"
        else
            fail "$name: genesis deployment execution log missing"
        fi
    done

    # (d) Cross-node genesis equality: block 1 identical everywhere.
    # Normalize with jq (.result only, fixed request id) before hashing.
    local ref_sum
    ref_sum=$(jsonrpc_get_block "$NODE0_NAME" "$NODE0_PORT" 1 | jq -cS '.result' 2>/dev/null | sha256sum | cut -d' ' -f1)
    local empty_sum
    empty_sum=$(printf '' | sha256sum | cut -d' ' -f1)
    local null_sum
    null_sum=$(printf 'null\n' | jq -cS '.' | sha256sum | cut -d' ' -f1)
    if [ -z "$ref_sum" ] || [ "$ref_sum" = "$empty_sum" ] || [ "$ref_sum" = "$null_sum" ]; then
        fail "node0 genesis block unreadable via RPC — cannot verify convergence"
    else
        for node_spec in "${CHECK_LIST[@]}"; do
            local name="${node_spec%%:*}"
            local port="${node_spec##*:}"
            local cmp_sum=""
            # Allow up to 60s for genesis sync lag on the compared node.
            for i in $(seq 1 20); do
                cmp_sum=$(jsonrpc_get_block "$name" "$port" 1 | jq -cS '.result' 2>/dev/null | sha256sum | cut -d' ' -f1)
                [ -n "$cmp_sum" ] && [ "$cmp_sum" != "$empty_sum" ] && [ "$cmp_sum" != "$null_sum" ] && break
                sleep 3
            done
            if [ "$cmp_sum" = "$ref_sum" ]; then
                pass "$name genesis block identical to node0"
            else
                fail "$name genesis block differs from node0 (ref=${ref_sum:0:16} got=${cmp_sum:0:16})"
            fi
        done
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
