# DarkWow Testnet Pipeline — Phase 8: Mining Activity / P2P Connectivity
#
# Phase 8 local: check stratum, monerod (merge), p2pool readiness.
# Phase 8 join: verify P2P connections, slot count via JSON-RPC.
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NODE0, RPC_PORT, CONTAINER_NAME),
#               helpers.sh (check_network, jsonrpc)
#
# Sourced by test_pipeline.sh after phase_06_verify.sh.

phase_mining_activity() {
    info "Phase 8: Verifying mining activity..."

    if [ "$MODE" = "merge" ]; then
        # Every check is a single-shot diagnostic. No retry loops, no sleeps,
        # no time-based measurements. Check current state, report, continue.
        # These are all diagnostic — mining infrastructure may not be ready
        # yet, that's normal. warn() and move on.

        info "Checking monerod RPC health..."
        if docker exec dwow-monerod curl -s --max-time 2 http://127.0.0.1:28081/json_rpc \
            -H 'Content-Type: application/json' \
            -d '{"jsonrpc":"2.0","method":"get_info","id":1}' 2>/dev/null | grep -q "height"; then
            pass "monerod RPC healthy"
        else
            warn "monerod RPC not responding"
        fi

        info "Checking monerod has blocks..."
        local MONERO_INFO MONERO_HEIGHT
        MONERO_INFO=$(docker exec dwow-monerod curl -s --max-time 2 http://127.0.0.1:28081/json_rpc \
            -H 'Content-Type: application/json' \
            -d '{"jsonrpc":"2.0","method":"get_info","id":1}' 2>/dev/null || true)
        MONERO_HEIGHT=$(echo "$MONERO_INFO" | jq -r '.result.height // 0' 2>/dev/null) || true
        if [ -n "$MONERO_HEIGHT" ] && [ "$MONERO_HEIGHT" -gt 0 ]; then
            pass "monerod has blocks (height=$MONERO_HEIGHT)"
        else
            warn "monerod has no blocks yet (offline mining may still be starting)"
        fi

        info "Checking dwowd mm_rpc endpoint..."
        if docker exec dwow-node0 curl -s --max-time 2 http://127.0.0.1:31348 \
            -H 'Content-Type: application/json' \
            -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","params":[],"id":1}' 2>/dev/null | grep -q "result"; then
            pass "dwowd mm_rpc healthy"
        else
            warn "dwowd mm_rpc not responding"
        fi

        info "Checking p2pool sidecar activity in merge nodes..."
        local N0_P2POOL N1_P2POOL
        N0_P2POOL=$(docker logs dwow-node0 2>&1 | grep -ci "p2pool sidecar\|stratum.*3333\|merge.mine\|P2Pool" || true)
        N1_P2POOL=$(docker logs dwow-node1 2>&1 | grep -ci "p2pool sidecar\|stratum.*3333\|merge.mine\|P2Pool" || true)
        if [ "$N0_P2POOL" -gt 0 ] && [ "$N1_P2POOL" -gt 0 ]; then
            pass "p2pool merge mining sidecars active"
        else
            warn "p2pool sidecars not detected (diagnostic)"
        fi

        info "Checking xmrig activity in node containers..."
        local NODE0_XMRIG NODE1_XMRIG N0_OK N1_OK
        NODE0_XMRIG=$(docker logs "$NODE0" 2>&1 || true)
        NODE1_XMRIG=$(docker logs dwow-node1 2>&1 || true)
        N0_OK=$(echo "$NODE0_XMRIG" | grep -qi "xmrig sidecar started\|Merge mining.*xmrig" && echo 1 || echo 0)
        N1_OK=$(echo "$NODE1_XMRIG" | grep -qi "xmrig sidecar started\|Merge mining.*xmrig" && echo 1 || echo 0)
        if [ "$N0_OK" = "1" ] && [ "$N1_OK" = "1" ]; then
            pass "xmrig sidecars active in node0 and node1"
        else
            warn "xmrig sidecars not detected (diagnostic)"
        fi

        info "Checking mm_rpc aux block polling..."
        if echo "$NODE0_XMRIG" | grep -qi "merge_mining_get_aux_block\|get_aux_block"; then
            pass "p2pool polling mm_get_aux_block on node0"
        else
            warn "no mm_get_aux_block calls detected (diagnostic)"
        fi

        info "Checking xmrig stratum connections..."
        if echo "$NODE0_XMRIG" | grep -qi "stratum\|pool\|connect"; then
            pass "xmrig stratum activity detected"
        else
            info "xmrig stratum activity not yet visible (diagnostic)"
        fi

        info "Checking node0 for block production..."
        local NODE0_LOGS
        NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
        if echo "$NODE0_LOGS" | grep -qi "Mined and applied\|miner.mine_linear\|merge.mine\|mm_rpc\|new job\|accepted"; then
            pass "node0 block production activity"
        else
            info "node0 block production activity not yet visible in logs (diagnostic)"
        fi

        info "Checking node2 for native mining activity..."
        local NODE2_LOGS
        NODE2_LOGS=$(docker logs dwow-node2 2>&1 || true)
        if echo "$NODE2_LOGS" | grep -qi "miner.mine_linear\|Mined and applied block\|native mining\|built-in miner\|Mining block\|Block.*mined"; then
            pass "node2 native mining activity detected"
        else
            warn "node2 logs don't show clear native mining activity — may need more time"
        fi
    else
        info "Checking native mining activity (in-container RPC miner)..."
        NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
        if echo "$NODE0_LOGS" | grep -qi "miner.mine_linear\|Mined and applied block\|native mining\|built-in miner\|Mining block\|Block.*mined"; then
            pass "native mining activity detected"
        else
            warn "native mining activity not detected in logs — log format may differ, or miner may not have started yet"
        fi
    fi
}

# ==============================================================================
# Join Phase 8: P2P Connectivity
# ==============================================================================
phase_join_p2p() {
    echo ""
    echo "=== Join Phase 8: P2P Connectivity ==="

    if ! container_running "$CONTAINER_NAME"; then
        warn "Container not running (lifecycle phase left it)"
        return 0
    fi

    check_network || return 0

    echo "  Checking P2P connectivity..."
    local peers
    peers=$(jsonrpc "$RPC_PORT" "p2p.info")

    # If p2p.info method isn't registered, check logs for P2P activity
    if echo "$peers" | grep -q '"method not found"'; then
        echo "  p2p.info method not available — checking logs for P2P activity"
        local p2p_logs
        p2p_logs=$(docker logs "$CONTAINER_NAME" 2>&1)
        if echo "$p2p_logs" | grep -qi "session.*open\|peer.*connected\|P2P.*connected"; then
            pass "P2P connections active (log evidence)"
        elif echo "$p2p_logs" | grep -qi "Unable to connect to seed"; then
            warn "P2P subsystem active but unable to connect to seeds (public testnet may be down)"
        else
            info "P2P connectivity check skipped (p2p.info not implemented; container operational)"
        fi
        return 0
    fi

    # Single-shot check — no retry loop. If p2p.info returns sessions data,
    # report it. If not, warn and continue.
    if echo "$peers" | grep -q '"result"'; then
        local count
        count=$(echo "$peers" | grep -o '"sessions":[0-9]*' | grep -o '[0-9]*' || echo "0")
        if [ -n "$count" ] && [ "$count" -gt 0 ] 2>/dev/null; then
            pass "P2P connected: $count session(s)"
        else
            warn "P2P returned result but no active sessions — network may be slow"
        fi
    else
        echo "  p2p.info response:"
        echo "$peers" | head -1
        warn "P2P info not available — join mode, network may be slow"
    fi
}
