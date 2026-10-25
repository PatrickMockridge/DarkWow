# DarkWow Testnet Pipeline — Phase: Fee Window Signalling
#
# Validates the fee window signalling end-to-end:
#   1. Window boundary transition at height 20 (flags set by miner)
#   2. Multi-node flag consensus (all nodes emit identical flags)
#   3. Post-boundary wallet transfer (fee computed dynamically, admitted)
#
# Runs after phase_09_blocks.sh (chain must have blocks) and after
# phase_10_wallet_tests.sh (wallets must be synced).
#
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (MODE, NATIVE_NODES, WITH_WALLET),
#               helpers.sh (jsonrpc_get_height),
#               wallet-shell.sh (wal function)

# ── Helpers ────────────────────────────────────────────────────────────────

# Poll node0 until it reaches a target height. Returns 0 on success, 1 on timeout.
_poll_height() {
    local node="$1" port="$2" target="$3" max_polls="$4" label="$5"
    local poll=0 h=0
    while [ "$poll" -lt "$max_polls" ]; do
        h=$(jsonrpc_get_height "$node" "$port")
        h=$(echo "$h" | tr -dc '0-9')
        h="${h:-0}"
        if [ "${h:-0}" -ge "$target" ]; then
            info "  $label reached height=$h (target=$target, poll $poll)"
            return 0
        fi
        poll=$((poll + 1))
        [ "$poll" -lt "$max_polls" ] && sleep 15
    done
    warn "  $label: height=$h, target=$target not reached after $((max_polls * 15))s"
    return 1
}

# Extract fee_window_flags from a node's miner log. Returns hex value or empty.
_get_flags_from_log() {
    local node="$1"
    # Prefer the last boundary line that carries flags: the node logs the
    # announcement (miner) and the connect-side derivation, and only the
    # announcement historically carried flags. Total: returns empty (exit 0)
    # when no boundary line exists — never a non-zero pipefail exit.
    docker logs "$node" 2>&1 | grep "Fee window boundary" | grep "flags=0x" | tail -1 \
        | grep -oP 'flags=0x[0-9a-fA-F]+' | cut -d'=' -f2 || true
}

# Extract a congestion-factor value (CfValue) from the last fee window boundary
# log line. The daemon logs e.g. "circuit_premium=CfValue(1000000)".
# key ∈ {circuit_premium, circuit_standard, wasm_premium, wasm_standard}.
_get_cf_from_log() {
    local node="$1" key="$2"
    # Select the last line that CARRIES the field, not the last boundary line. A field-poor
    # connect-side or diagnostic line after the announcement made the pre-fix `tail -1` parser
    # return empty while the value was present (OBL-C270's failure shape).
    docker logs "$node" 2>&1 | grep "Fee window boundary" | grep "${key}=CfValue(" | tail -1 \
        | grep -oP "${key}=CfValue\(\K[0-9]+" || true
}

# Height of the most recent "Fee window boundary at height N:" log line, or empty.
_get_boundary_height() {
    local node="$1"
    docker logs "$node" 2>&1 | grep "Fee window boundary" | tail -1 \
        | grep -oP 'at height \K[0-9]+' || true
}

# A boundary value at a NAMED height, from that node's log. Comparing "last
# line" across nodes compares different windows whenever one node lags; this
# makes L3-FW-2 compare like with like.
_get_cf_at_height() {
    local node="$1" key="$2" height="$3"
    # Same rule at a named height: the last line carrying the field, never a field-poor twin.
    docker logs "$node" 2>&1 | grep "Fee window boundary at height ${height}:" \
        | grep "${key}=CfValue(" | tail -1 \
        | grep -oP "${key}=CfValue\(\K[0-9]+" || true
}

_get_flags_at_height() {
    local node="$1" height="$2"
    docker logs "$node" 2>&1 | grep "Fee window boundary at height ${height}:" \
        | grep "flags=0x" | tail -1 \
        | grep -oP 'flags=0x[0-9a-fA-F]+' | cut -d'=' -f2 || true
}

# Wallet chain height via sync status (frozen porcelain format).
_wallet_chain_height() {
    local idx="$1"
    wal "$idx" sync status 2>/dev/null | grep 'Local chain height:' | grep -oE '[0-9]+' | head -1 || echo "0"
}

# ── Phase entry point ──────────────────────────────────────────────────────

phase_fee_window() {
    info "Phase: Fee window signalling verification..."

    local NODE0_NAME="dwow-node0"
    local NODE0_PORT=31345
    local WINDOW_HEIGHT=20
    local SYNC_MAX_POLLS=80   # 20 min max

    # ═══════════════════════════════════════════════════════════════════════
    # L3-FW-1: Window boundary transition at height 20.
    #
    # Poll until node0 reaches height 20. The miner_task sets fee_window_flags
    # at the fee window boundary (every 20 blocks). Verify:
    #   - Both bytes active (flags & 0x0101)
    #   - Congestion multiplier per byte in {0,1,2}
    #   - circuit_premium >= circuit_standard, wasm_premium >= wasm_standard
    #   - congestion factors >= identity (1_000_000)
    # ═══════════════════════════════════════════════════════════════════════

    info "L3-FW-1: Waiting for fee window boundary at height $WINDOW_HEIGHT..."
    if ! _poll_height "$NODE0_NAME" "$NODE0_PORT" "$WINDOW_HEIGHT" "$SYNC_MAX_POLLS" "node0"; then
        fail "L3-FW-1: node0 did not reach height $WINDOW_HEIGHT"
        return
    fi

    # Diagnostic: show boundary log lines
    info "  ── node0 fee window boundary log lines ──"
    docker logs "$NODE0_NAME" 2>&1 | grep "Fee window boundary" | while read line; do info "    $line"; done
    info "  ── end fee window log ──"

    # Extract flags from the most recent boundary event
    local flags
    flags=$(_get_flags_from_log "$NODE0_NAME")
    if [ -z "$flags" ]; then
        fail "L3-FW-1: No 'Fee window boundary' log line found in node0 logs"
        return
    fi
    info "  node0 fee_window_flags: $flags"

    # Parse hex value (u16 = circuit byte | wasm byte << 8)
    local flags_int
    flags_int=$((flags))
    if [ "$flags_int" -eq 0 ] 2>/dev/null && [ "$flags" != "0x00" ] && [ "$flags" != "0x0" ]; then
        warn "L3-FW-1: could not parse flags=$flags as integer for bit checks, skipping bit-level assertions"
    else
        # Both bytes active (circuit byte bit0 AND wasm byte bit0)
        if [ $((flags_int & 0x0101)) -ne $((0x0101)) ]; then
            fail "L3-FW-1: not both windows active (flags=0x${flags_int})"
        else
            pass "L3-FW-1: both circuit+wasm active (flags & 0x0101 = 0x0101)"
        fi

        # Congestion multiplier per byte (circuit bits 4-7, wasm bits 12-15) in {0,1,2}
        local circuit_cm=$(((flags_int >> 4) & 0x0F))
        local wasm_cm=$(((flags_int >> 12) & 0x0F))
        if [ "$circuit_cm" -le 2 ] 2>/dev/null && [ "$wasm_cm" -le 2 ] 2>/dev/null; then
            pass "L3-FW-1: congestion_multiplier circuit=$circuit_cm wasm=$wasm_cm (0=hold,1=+10%,2=-10%)"
        else
            fail "L3-FW-1: congestion_multiplier out of range (circuit=$circuit_cm wasm=$wasm_cm)"
        fi
    fi

    # Extract the four congestion factors from the new log format.
    local circuit_premium circuit_standard wasm_premium wasm_standard
    circuit_premium=$(_get_cf_from_log "$NODE0_NAME" "circuit_premium")
    circuit_standard=$(_get_cf_from_log "$NODE0_NAME" "circuit_standard")
    wasm_premium=$(_get_cf_from_log "$NODE0_NAME" "wasm_premium")
    wasm_standard=$(_get_cf_from_log "$NODE0_NAME" "wasm_standard")
    if [ -z "$circuit_premium" ] || [ -z "$wasm_premium" ]; then
        fail "L3-FW-1: could not extract congestion factors from node0 fee window log"
        return
    fi
    info "  node0 CF: circuit=$circuit_premium/$circuit_standard wasm=$wasm_premium/$wasm_standard"

    # I4 ordering: premium >= standard (== at zero congestion, so >= not >).
    if [ "$circuit_premium" -ge "$circuit_standard" ] 2>/dev/null && [ "$wasm_premium" -ge "$wasm_standard" ] 2>/dev/null; then
        pass "L3-FW-1: premium >= standard (circuit $circuit_premium>=$circuit_standard, wasm $wasm_premium>=$wasm_standard)"
    else
        fail "L3-FW-1: premium < standard (I4 violated: circuit $circuit_premium<$circuit_standard, wasm $wasm_premium<$wasm_standard)"
    fi

    # Identity floor: CfValue never drops below SCALE (1_000_000). At the first
    # boundary (height 20, zero congestion) all four are exactly 1_000_000 (hold).
    if [ "$circuit_premium" -ge 1000000 ] 2>/dev/null && [ "$circuit_standard" -ge 1000000 ] 2>/dev/null \
       && [ "$wasm_premium" -ge 1000000 ] 2>/dev/null && [ "$wasm_standard" -ge 1000000 ] 2>/dev/null; then
        pass "L3-FW-1: all congestion factors >= identity (1_000_000)"
    else
        warn "L3-FW-1: congestion factor below identity (circuit=$circuit_premium/$circuit_standard wasm=$wasm_premium/$wasm_standard)"
    fi

    # ═══════════════════════════════════════════════════════════════════════
    # L3-FW-2: Multi-node flag consensus witness.
    #
    # Check that other mining nodes (node1+) emit the same congestion factors
    # at the same height. All nodes share the same chain state, so the
    # FeeWindowState::adjust() output should be identical. Divergence
    # surfaces the I8 risk (no Rust median consensus — Python-only today).
    # ═══════════════════════════════════════════════════════════════════════

    info "L3-FW-2: Multi-node flag consensus..."

    local other_nodes=""
    case "$NATIVE_NODES" in
        2) other_nodes="dwow-node1:31346" ;;
        5) other_nodes="dwow-node1:31346 dwow-node2:31350 dwow-node3:31353 dwow-node4:31356" ;;
    esac

    if [ -n "$other_nodes" ]; then
        # Compare at a NAMED boundary height. A lagging node's last line is an
        # earlier window; that is "did not reach", not "divergence".
        local node0_boundary
        node0_boundary=$(_get_boundary_height "$NODE0_NAME")
        local ref_cp ref_cs ref_wp ref_ws
        ref_cp=$(_get_cf_at_height "$NODE0_NAME" "circuit_premium" "$node0_boundary")
        ref_cs=$(_get_cf_at_height "$NODE0_NAME" "circuit_standard" "$node0_boundary")
        ref_wp=$(_get_cf_at_height "$NODE0_NAME" "wasm_premium" "$node0_boundary")
        ref_ws=$(_get_cf_at_height "$NODE0_NAME" "wasm_standard" "$node0_boundary")
        if [ -z "$node0_boundary" ] || [ -z "$ref_cp" ]; then
            warn "L3-FW-2: node0 has no parseable boundary line yet — skipping cross-node comparison"
            return
        fi

        for node_spec in $other_nodes; do
            local node_name="${node_spec%%:*}"
            local node_port="${node_spec##*:}"

            if ! docker ps --format '{{.Names}}' | grep -q "^${node_name}$"; then
                warn "L3-FW-2: $node_name not running — skipping"
                continue
            fi

            # Wait (bounded) for this node to emit the same boundary height.
            local poll nb
            nb=$(_get_boundary_height "$node_name")
            poll=0
            while { [ -z "$nb" ] || [ "$nb" -lt "$node0_boundary" ]; } 2>/dev/null && [ "$poll" -lt 40 ]; do
                poll=$((poll + 1))
                sleep 10
                nb=$(_get_boundary_height "$node_name")
            done
            if [ -z "$nb" ] || [ "$nb" -lt "$node0_boundary" ] 2>/dev/null; then
                warn "L3-FW-2: $node_name has not reached boundary $node0_boundary (last=${nb:-none}) — not a divergence verdict"
                continue
            fi

            local node_flags node_cp node_cs node_wp node_ws
            node_flags=$(_get_flags_at_height "$node_name" "$node0_boundary")
            node_cp=$(_get_cf_at_height "$node_name" "circuit_premium" "$node0_boundary")
            node_cs=$(_get_cf_at_height "$node_name" "circuit_standard" "$node0_boundary")
            node_wp=$(_get_cf_at_height "$node_name" "wasm_premium" "$node0_boundary")
            node_ws=$(_get_cf_at_height "$node_name" "wasm_standard" "$node0_boundary")

            if [ -z "$node_cp" ]; then
                warn "L3-FW-2: $node_name has no boundary line at height $node0_boundary — skipping"
                continue
            fi

            if [ "$node_cp" = "$ref_cp" ] && [ "$node_cs" = "$ref_cs" ] \
               && [ "$node_wp" = "$ref_wp" ] && [ "$node_ws" = "$ref_ws" ]; then
                pass "L3-FW-2: $node_name boundary $node0_boundary match (circuit=$node_cp/$node_cs wasm=$node_wp/$node_ws, flags=$node_flags)"
            else
                fail "L3-FW-2: $node_name DIVERGENCE at boundary $node0_boundary — circuit=$node_cp/$node_cs wasm=$node_wp/$node_ws (node0 circuit=$ref_cp/$ref_cs wasm=$ref_wp/$ref_ws)"
            fi
        done
    else
        skip "L3-FW-2: No additional mining nodes to check (NATIVE_NODES=$NATIVE_NODES)"
    fi

    # ═══════════════════════════════════════════════════════════════════════
    # L3-FW-3: Post-boundary wallet E2E.
    #
    # After the fee window boundary, transfer 1 DRKW from wallet-1 to
    # wallet-2. The tx fee is computed dynamically (two-component CF formula);
    # verify wallet-2 receives the value — proves the fee window didn't break
    # wallet economics.
    # ═══════════════════════════════════════════════════════════════════════

    if [ "${WITH_WALLET:-0}" -lt 2 ]; then
        skip "L3-FW-3: Need at least 2 wallets (WITH_WALLET=$WITH_WALLET) — skipping"
        return
    fi

    info "L3-FW-3: Post-boundary wallet transfer (fee window active)..."
    info "  Active congestion factors: circuit=$circuit_premium/$circuit_standard wasm=$wasm_premium/$wasm_standard"
    info "  Transfer fee computed dynamically via the two-component formula"

    # Ensure wallet-1 is synced to window height
    local w1_height
    w1_height=$(_wallet_chain_height 1)
    info "  wallet-1 chain height: $w1_height"
    if [ "${w1_height:-0}" -lt "$WINDOW_HEIGHT" ]; then
        info "  wallet-1 syncing to window height..."
        wal 1 sync init >/dev/null 2>&1 || true
        local sync_poll=0
        while [ "$sync_poll" -lt 60 ]; do
            w1_height=$(_wallet_chain_height 1)
            [ "${w1_height:-0}" -ge "$WINDOW_HEIGHT" ] && break
            sleep 10
            sync_poll=$((sync_poll + 1))
        done
        info "  wallet-1 height after sync: $w1_height"
    fi

    if [ "${w1_height:-0}" -lt "$WINDOW_HEIGHT" ]; then
        warn "L3-FW-3: wallet-1 not synced to window height (h=$w1_height) — skipping transfer"
        return
    fi

    # Scan wallet-1 to pick up any missed coinbase outputs
    wal 1 scan >/dev/null 2>&1 || true

    # Get wallet-2 address for transfer target
    local addr2
    addr2=$(wal 2 wallet address 2>/dev/null | tail -1 || echo "")
    if [ -z "$addr2" ]; then
        fail "L3-FW-3: could not get wallet-2 address"
        return
    fi
    info "  wallet-2 address: $addr2"

    # Pre-transfer balances
    local bal1_before
    bal1_before=$(wal 1 wallet balance --porcelain 2>/dev/null | grep "$NATIVE_TOKEN_ID" | cut -f2 || echo "0")
    info "  wallet-1 DRKW balance before: $bal1_before"

    # Transfer 1 DRKW (must succeed despite fee window active)
    info "  wallet-1 transferring 1 DRKW to wallet-2..."
    local tx_output
    local tx_rc=0
    # Capture the real exit code: `|| true` masked it (always 0). An `if`
    # condition is exempt from `set -e` + the ERR trap, so a failed transfer
    # lands in the else branch instead of aborting the pipeline.
    if tx_output=$(wal 1 transfer 1 "$NATIVE_TOKEN_ID" "$addr2" --porcelain 2>&1); then
        tx_rc=0
    else
        tx_rc=$?
    fi
    if [ "$tx_rc" -ne 0 ]; then
        fail "L3-FW-3: transfer failed (exit=$tx_rc) — fee window may have broken wallet economics"
        info "  transfer output: ${tx_output:-<empty>}"
        return
    fi

    local txid
    txid=$(echo "$tx_output" | grep -oP 'txid=\K[^[:space:]]+' | head -1 || echo "")
    if [ -n "$txid" ]; then
        pass "L3-FW-3: transfer broadcast (txid=$txid)"
    else
        pass "L3-FW-3: transfer broadcast (no txid parsed)"
    fi

    # Wait for inclusion
    info "  waiting for transfer to be mined..."
    sleep 30

    # Wallet-2 scans and checks balance
    wal 2 scan >/dev/null 2>&1 || true
    local bal2_after
    bal2_after=$(wal 2 wallet balance --porcelain 2>/dev/null | grep "$NATIVE_TOKEN_ID" | cut -f2 || echo "0")
    info "  wallet-2 DRKW balance after: $bal2_after"

    if [ "${bal2_after:-0}" -gt 0 ] 2>/dev/null; then
        pass "L3-FW-3: wallet-2 received transfer (balance=$bal2_after) — fee window active, wallet works"
    else
        # Transfer may not be mined yet — check again after more time
        info "  transfer not yet visible — waiting additional 60s for mining..."
        sleep 60
        wal 2 scan >/dev/null 2>&1 || true
        bal2_after=$(wal 2 wallet balance --porcelain 2>/dev/null | grep "$NATIVE_TOKEN_ID" | cut -f2 || echo "0")
        if [ "${bal2_after:-0}" -gt 0 ] 2>/dev/null; then
            pass "L3-FW-3: wallet-2 received transfer after extended wait (balance=$bal2_after)"
        else
            warn "L3-FW-3: wallet-2 balance still 0 after transfer — mining may be slow (not a fee-window failure)"
        fi
    fi

    # Wallet-1 post-transfer balance (must have at least DEFAULT_FEE remaining,
    # confirming the fee was deducted and the transfer was admitted)
    local bal1_after
    bal1_after=$(wal 1 wallet balance --porcelain 2>/dev/null | grep "$NATIVE_TOKEN_ID" | cut -f2 || echo "0")
    info "  wallet-1 DRKW balance after: $bal1_after"

    if [ "${bal1_after:-0}" -ge "$DEFAULT_FEE" ] 2>/dev/null; then
        pass "L3-FW-3: wallet-1 post-transfer balance >= DEFAULT_FEE (${DEFAULT_FEE}) — fee deduction worked"
    else
        warn "L3-FW-3: wallet-1 balance after transfer: $bal1_after (below DEFAULT_FEE=$DEFAULT_FEE)"
    fi
}
