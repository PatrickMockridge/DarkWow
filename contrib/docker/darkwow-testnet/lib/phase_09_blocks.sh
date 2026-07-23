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
#
# SYNCHRONIZATION GATE: Phase 9 is the single synchronization point for the
# entire pipeline. It polls node0 height until block 2 exists (genesis + first
# mined block), then runs invariant checks once. The poll bound (20 min) is
# derived from system constants — ZK keygen (~5 min) + genesis creation (~30s)
# + block 2 mining (~30s) + P2P mesh (~30s) × 2x safety margin. After this
# gate, all subsequent phases can run single-shot checks against a known-ready
# chain.
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

# Helper: fetch block 1 canonical hash from a node via RPC.
# Returns empty string if the block is unreadable.
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

phase_blocks() {
    info "Phase 9: Block production gate..."

    _build_node_list

    local NODE0_NAME="${NODE0}:31345"
    NODE0_NAME="${NODE0_NAME%%:*}"
    local NODE0_PORT=31345

    # ═══════════════════════════════════════════════════════════════════
    # SYNCHRONIZATION GATE: poll node0 until block 2 exists.
    #
    # This is the ONE place the pipeline waits for a state transition.
    # Every check after this gate is a single-shot observation of
    # invariant vs diagnostic state. The bound is derived from system
    # constants, not invented:
    #   ZK keygen (Mint_V1 + FeeCollect_V1):     ~5 min
    #   Genesis creation (coinbase + 9 deploys): ~30s
    #   Block 2 mining (3× target_block_time):   ~30s
    #   P2P mesh formation (observer + node1):    ~30s
    #   Safety margin: 2× → 20 min (120 × 10s)
    # ═══════════════════════════════════════════════════════════════════
    local SYNC_MAX_POLLS=120   # 20 minutes max
    local SYNC_INTERVAL=10     # poll every 10 seconds
    local n0_height=0
    local poll=0

    info "Waiting for node0 to produce block 2 (ZK keygen may take minutes on first boot)..."
    while [ "$poll" -lt "$SYNC_MAX_POLLS" ]; do
        n0_height=$(jsonrpc_get_height "$NODE0_NAME" "$NODE0_PORT")
        n0_height=$(echo "$n0_height" | tr -dc '0-9')
        n0_height="${n0_height:-0}"
        if [ "${n0_height:-0}" -ge 2 ]; then
            pass "node0 reached height=$n0_height after $((poll * SYNC_INTERVAL))s"
            break
        fi
        poll=$((poll + 1))
        if [ "$poll" -lt "$SYNC_MAX_POLLS" ]; then
            info "  node0 height=$n0_height — waiting for block 2 (poll $poll/$SYNC_MAX_POLLS, ${SYNC_INTERVAL}s)..."
            # Periodic diagnostic (every 12 polls = 2 minutes): show recent
            # miner/sync activity so operators can see WHAT the node is doing,
            # not just its height. Distinguishes between ZK keygen, CaughtUp
            # wait, and actual mining.
            if [ $((poll % 12)) -eq 0 ]; then
                info "  ── node0 diagnostic (poll $poll, elapsed $((poll * SYNC_INTERVAL))s) ──"
                docker logs dwow-node0 --tail 20 2>&1 | grep -E 'sync_state|miner_task|consensus_linear|ZK|Mining' | tail -5 | while read line; do info "    $line"; done || true
                info "  ── end diagnostic ──"
            fi
            sleep "$SYNC_INTERVAL"
        fi
    done

    if [ "${n0_height:-0}" -lt 2 ]; then
        fail "node0 never produced block 2 after $((SYNC_MAX_POLLS * SYNC_INTERVAL))s (height=$n0_height)"
        info "  ZK keygen may have failed, or mining is not running."
        info "  ── TIMEOUT DIAGNOSTIC: node0 full log tail ──"
        docker logs dwow-node0 --tail 100 2>&1 | while read line; do info "    $line"; done || true
        info "  ── TIMEOUT DIAGNOSTIC: sync_state transitions ──"
        docker logs dwow-node0 --tail 500 2>&1 | grep -E 'sync_state' | while read line; do info "    $line"; done || info "    (no sync_state transitions found)"
        info "  ── TIMEOUT DIAGNOSTIC: miner activity ──"
        docker logs dwow-node0 --tail 500 2>&1 | grep -E 'miner_task|CaughtUp|Mining|ZK' | while read line; do info "    $line"; done || info "    (no miner activity found)"
        info "  ── END TIMEOUT DIAGNOSTIC ──"
        return
    fi

    # ── Diagnostic: display current heights ──────────────────────────
    info "node0 height=$n0_height — chain is ready for wallet testing"

    # ── Other nodes: alive check, observational only ─────────────────
    for node_spec in "${NODE_LIST[@]}"; do
        local node_name="${node_spec%%:*}"
        local node_port="${node_spec##*:}"
        [ "$node_name" = "$NODE0_NAME" ] && continue

        local h
        h=$(jsonrpc_get_height "$node_name" "$node_port")
        h=$(echo "$h" | tr -dc '0-9')
        h="${h:-0}"
        if [ "$h" -ge 1 ] 2>/dev/null; then
            pass "$node_name: height=$h"
        else
            warn "$node_name: RPC unreachable or height=0 (sync lag is normal)"
        fi
    done

    # ═══════════════════════════════════════════════════════════════════
    # GENESIS AUTHORITY GATE: ONLY node0 creates; everyone else syncs.
    #
    # Authority is verified via RPC block-1 hash comparison. If a
    # non-node0 node has block 1 AND its hash matches node0's, it MUST
    # have synced — it cannot independently create an identical block.
    # ═══════════════════════════════════════════════════════════════════

    # Sentinel values that indicate an empty/unreadable block 1 response.
    local empty_sum
    empty_sum=$(printf '' | sha256sum | cut -d' ' -f1)
    local null_sum
    null_sum=$(printf 'null\n' | jq -cS '.' | sha256sum | cut -d' ' -f1)

    # (a) Fetch node0's block 1 as the reference hash.
    #     Retry up to 3×2s for transient RPC unavailability (node busy
    #     mining). If still unreadable, fail — the authority gate cannot
    #     execute without a reference hash.
    local ref_sum
    local ref_attempt=0
    while [ "$ref_attempt" -lt 3 ]; do
        ref_sum=$(_get_block1_hash "$NODE0_NAME" "$NODE0_PORT")
        if [ -n "$ref_sum" ] && [ "$ref_sum" != "$empty_sum" ] && [ "$ref_sum" != "$null_sum" ]; then
            break
        fi
        ref_attempt=$((ref_attempt + 1))
        [ "$ref_attempt" -lt 3 ] && sleep 2
    done

    if [ -z "$ref_sum" ] || [ "$ref_sum" = "$empty_sum" ] || [ "$ref_sum" = "$null_sum" ]; then
        fail "node0 block 1 unreadable via RPC after 3 attempts — authority gate cannot execute"
        return
    fi
    pass "node0 is the genesis authority (block 1 hash=${ref_sum:0:16}...)"

    # Genesis determinism: verify block 0 (genesis) hash against precomputed
    # constant. The block-1 authority comparison ensures nodes share the same
    # chain but does NOT verify the chain started from the correct genesis.
    # Two nodes with identically corrupted code could produce matching block-1
    # hashes from different genesis blocks. This check catches that.
    # GATE: Genesis determinism — verifies the chain started from the correct
    # genesis. Two nodes with identically corrupted code could produce matching
    # block-1 hashes from different genesis blocks. This check catches that.
    local GENESIS_HASH_FILE="${REPO_ROOT}/genesis_hash.txt"
    if [ -f "$GENESIS_HASH_FILE" ]; then
        local expected_genesis_hash
        expected_genesis_hash=$(tr -d '[:space:]' < "$GENESIS_HASH_FILE")
        local actual_genesis_hash
        actual_genesis_hash=$(jsonrpc_get_block "dwow-node0" "$RPC_PORT" 0 \
            | jq -cS '.result' | openssl sha256 | awk '{print $2}')
        if [ "$actual_genesis_hash" = "$expected_genesis_hash" ]; then
            pass "Genesis block 0 hash matches precomputed constant (determinism verified at runtime)"
        else
            fail "GENESIS DETERMINISM VIOLATED: expected $expected_genesis_hash, got $actual_genesis_hash"
        fi
    else
        fail "GENESIS DETERMINISM CHECK IMPOSSIBLE: genesis_hash.txt not found at $GENESIS_HASH_FILE. Build the project first (make) or run from repo root."
    fi

    # (b) Build check list: all NODE_LIST nodes except node0, plus the
    #     observer (not in NODE_LIST — no mining role, but MUST obey the
    #     same genesis authority rule).
    local CHECK_LIST=()
    for node_spec in "${NODE_LIST[@]}"; do
        [ "${node_spec%%:*}" = "$NODE0_NAME" ] || CHECK_LIST+=("$node_spec")
    done
    if container_running "dwow-observer" 2>/dev/null; then
        CHECK_LIST+=("dwow-observer:31345")
    fi

    # (c) Authority enforcement: every non-node0 node with block 1 MUST
    #     have the same block 1 hash as node0. A different hash means the
    #     node created its own independent genesis — an authority violation.
    for node_spec in "${CHECK_LIST[@]}"; do
        local name="${node_spec%%:*}"
        local port="${node_spec##*:}"

        # Sync check: diagnostic only — sync lag is normal
        local sync_h
        sync_h=$(jsonrpc_get_height "$name" "$port")
        sync_h=$(echo "$sync_h" | tr -dc '0-9')
        sync_h="${sync_h:-0}"
        if [ "$sync_h" -ge 1 ] 2>/dev/null; then
            info "$name: height=$sync_h (synced)"
        else
            warn "$name: height=$sync_h (not synced yet — sync lag is normal)"
        fi

        # Authority: block-1 hash comparison
        local cmp_sum
        cmp_sum=$(_get_block1_hash "$name" "$port")
        if [ -z "$cmp_sum" ] || [ "$cmp_sum" = "$empty_sum" ] || [ "$cmp_sum" = "$null_sum" ]; then
            warn "$name: block 1 unavailable (sync incomplete — not an authority violation)"
            local diag
            diag=$(jsonrpc_get_height "$name" "$port" 2>/dev/null || echo "RPC unreachable")
            info "  $name diagnostic: get_height=$diag"
        elif [ "$cmp_sum" = "$ref_sum" ]; then
            pass "$name: genesis block identical to node0 (synced, not independent)"
        else
            fail "$name: INDEPENDENT GENESIS — block 1 hash differs from node0 (ref=${ref_sum:0:16} got=${cmp_sum:0:16})"
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
        warn "Container not running (run lifecycle phase first)"
        return 0
    fi

    echo "  Checking blockchain sync..."
    local height
    height=$(jsonrpc_get_height "$CONTAINER_NAME" "$RPC_PORT")
    height=$(echo "$height" | tr -dc '0-9')
    if [ -n "$height" ] && [ "$height" -gt 0 ] 2>/dev/null; then
        pass "Blockchain synced: height $height"
    else
        warn "Blockchain height is 0 (public testnet may not have blocks yet)"
    fi
}
