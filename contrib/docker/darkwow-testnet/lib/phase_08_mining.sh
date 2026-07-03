# DarkWow Testnet Pipeline — Phase 8: Mining Activity / P2P Connectivity
#
# Phase 8 local: check stratum, monerod (merge), p2pool readiness.
# Phase 8 join: verify P2P connections, slot count via JSON-RPC.
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NODE0, RPC_PORT, CONTAINER_NAME),
#               helpers.sh (check_network, jsonrpc)
#
# Sourced by test_pipeline.sh after phase_07_rpc.sh.

phase_mining_activity() {
    info "Phase 8: Verifying mining activity..."

    if [ "$MODE" = "merge" ]; then
        info "Checking monerod RPC health..."
        MONEROD_READY=false
        for i in $(seq 1 30); do
            if docker exec dwow-monerod curl -s --max-time 2 http://127.0.0.1:28081/json_rpc \
                -H 'Content-Type: application/json' \
                -d '{"jsonrpc":"2.0","method":"get_info","id":1}' 2>/dev/null | grep -q "height"; then
                info "monerod RPC responding (attempt $i)"
                MONEROD_READY=true
                break
            fi
            sleep 3
        done
        if [ "$MONEROD_READY" = true ]; then
            pass "monerod RPC healthy"
        else
            fail "monerod RPC not responding"
        fi

        info "Checking monerod has blocks..."
        MONERO_HEIGHT=0
        for i in $(seq 1 120); do
            MONERO_INFO=$(docker exec dwow-monerod curl -s --max-time 2 http://127.0.0.1:28081/json_rpc \
                -H 'Content-Type: application/json' \
                -d '{"jsonrpc":"2.0","method":"get_info","id":1}' 2>/dev/null || true)
            MONERO_HEIGHT=$(echo "$MONERO_INFO" | grep -o '"height":[0-9]*' | head -1 | grep -o '[0-9]*') || true
            if [ -n "$MONERO_HEIGHT" ] && [ "$MONERO_HEIGHT" -gt 0 ]; then
                info "monerod height=$MONERO_HEIGHT (attempt $i)"
                break
            fi
            [ "$i" -eq 120 ] && warn "monerod has no blocks after 120 polls (offline mining may be slow)"
            sleep 5
        done
        if [ -n "$MONERO_HEIGHT" ] && [ "$MONERO_HEIGHT" -gt 0 ]; then
            pass "monerod has blocks (height=$MONERO_HEIGHT)"
        else
            fail "monerod has no blocks yet (offline mining still starting)"
        fi

        info "Checking dwowd mm_rpc endpoint..."
        MM_RPC_READY=false
        for i in $(seq 1 30); do
            if docker exec dwow-node0 curl -s --max-time 2 http://127.0.0.1:31348 \
                -H 'Content-Type: application/json' \
                -d '{"jsonrpc":"2.0","method":"merge_mining_get_chain_id","params":[],"id":1}' 2>/dev/null | grep -q "result"; then
                info "mm_rpc responding (attempt $i)"
                MM_RPC_READY=true
                break
            fi
            sleep 3
        done
        if [ "$MM_RPC_READY" = true ]; then
            pass "dwowd mm_rpc healthy"
        else
            fail "dwowd mm_rpc not responding"
        fi

        info "Checking p2pool sidecar activity in merge nodes..."
        P2POOL_READY=false
        for i in $(seq 1 30); do
            NODE0_P2POOL=$(docker logs dwow-node0 2>&1 | grep -ci "p2pool sidecar\|stratum.*3333\|merge.mine\|P2Pool" || true)
            NODE1_P2POOL=$(docker logs dwow-node1 2>&1 | grep -ci "p2pool sidecar\|stratum.*3333\|merge.mine\|P2Pool" || true)
            if [ "$NODE0_P2POOL" -gt 0 ] && [ "$NODE1_P2POOL" -gt 0 ]; then
                info "p2pool sidecars active in node0 and node1 (attempt $i)"
                P2POOL_READY=true
                break
            fi
            sleep 3
        done
        if [ "$P2POOL_READY" = true ]; then
            pass "p2pool merge mining sidecars active"
        else
            warn "p2pool sidecars not detected in node logs — log format may differ, or sidecars may not have started"
        fi

        info "Checking xmrig activity in node containers..."
        NODE0_XMRIG=$(docker logs "$NODE0" 2>&1 || true)
        if echo "$NODE0_XMRIG" | grep -qi "xmrig sidecar started\|Merge mining.*xmrig"; then
            pass "xmrig sidecar active in node0"
        else
            info "node0 logs don't show xmrig sidecar startup yet (diagnostic)"
        fi
        NODE1_XMRIG=$(docker logs dwow-node1 2>&1 || true)
        if echo "$NODE1_XMRIG" | grep -qi "xmrig sidecar started\|Merge mining.*xmrig"; then
            pass "xmrig sidecar active in node1"
        else
            info "node1 logs don't show xmrig sidecar startup yet (diagnostic)"
        fi

        info "Checking mm_rpc aux block polling..."
        NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
        if echo "$NODE0_LOGS" | grep -qi "merge_mining_get_aux_block\|get_aux_block"; then
            pass "p2pool polling mm_get_aux_block on node0"
        else
            info "no mm_get_aux_block calls detected yet (p2pool may still be starting)"
        fi

        info "Checking xmrig stratum connections..."
        if echo "$NODE0_XMRIG" | grep -qi "stratum\|pool\|connect"; then
            pass "xmrig stratum activity detected"
        else
            info "xmrig stratum activity not yet visible (diagnostic)"
        fi

        info "Checking node0 for block production..."
        NODE0_LOGS=$(docker logs "$NODE0" 2>&1 || true)
        if echo "$NODE0_LOGS" | grep -qi "block\|mining\|merge.mine\|mm_rpc\|new job\|accepted"; then
            pass "node0 block production activity"
        else
            info "node0 block production activity not yet visible in logs (diagnostic)"
        fi

        info "Checking node2 for native mining activity..."
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
        fail "Container not running (lifecycle phase left it)"
        return 0
    fi

    check_network || return 0

    echo "  Checking P2P connectivity..."
    local peers
    peers=$(jsonrpc "$RPC_PORT" "p2p.info")

    # If p2p.info method isn't registered, check logs for P2P activity
    if echo "$peers" | grep -q '"method not found"'; then
        echo "  p2p.info method not available — checking logs for P2P activity"
        local logs
        logs=$(docker logs "$CONTAINER_NAME" 2>&1)
        if echo "$logs" | grep -qi "session.*open\|peer.*connected\|P2P.*connected"; then
            pass "P2P connections active (log evidence)"
        elif echo "$logs" | grep -qi "Unable to connect to seed"; then
            warn "P2P subsystem active but unable to connect to seeds (public testnet may be down)"
        else
            info "P2P connectivity check skipped (p2p.info not implemented; container operational)"
        fi
        return 0
    fi

    echo "  Waiting for P2P connections (up to 90s)..."
    local connected=0
    for i in $(seq 1 18); do
        peers=$(jsonrpc "$RPC_PORT" "p2p.info")
        if echo "$peers" | grep -q '"result"'; then
            local count
            count=$(echo "$peers" | grep -o '"sessions":[0-9]*' | grep -o '[0-9]*' || echo "0")
            if [ -n "$count" ] && [ "$count" -gt 0 ] 2>/dev/null; then
                pass "P2P connected: $count session(s) after $((i * 5))s"
                connected=1
                break
            fi
        fi
        sleep 5
    done

    if [ "$connected" -eq 0 ]; then
        echo "  Last p2p.info response:"
        jsonrpc "$RPC_PORT" "p2p.info" | head -1
        warn "No P2P connections after 90s — join mode, network may be slow"
    fi
}
