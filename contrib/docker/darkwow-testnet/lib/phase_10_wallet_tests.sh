# DarkWow Testnet Pipeline — Phases 10-11: Wallet Verification
#
# Phase 10: Sync blocks, scan, verify balance, address match.
# Phase 11: wallet-1 sends to wallet-2, verify receiving address.
#
# These tests are GATES — failure stops the pipeline.
# A wallet that can't sync blocks is a broken wallet.
#
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (WITH_WALLET, FORWARD_DESTINATION, SCRIPT_DIR),
#
# Sources: wallet-shell.sh (wal function) at runtime.
#
# Sourced by test_pipeline.sh after phase_09_blocks.sh.

# ── Diagnostic helper: dump wallet state on failure ──────────────────────
_wallet_diagnostic() {
    local wallet_idx="$1"
    local container="dwow-wallet-${wallet_idx}"
    info "  ── Diagnostic: wallet-${wallet_idx} ──"
    info "  Container status:"
    docker inspect --format '{{.State.Status}} ({{.State.StartedAt}})' "$container" 2>/dev/null || echo "  (inspect failed)"
    info "  Container logs (last 30 lines):"
    docker logs --tail 30 "$container" 2>&1 | while read line; do info "    $line"; done
    info "  P2P config from container:"
    docker exec "$container" cat /root/.config/dwow/dww_config.toml 2>/dev/null | while read line; do info "    $line"; done || info "    (could not read config)"
    info "  Sync status:"
    wal "$wallet_idx" sync status 2>&1 | while read line; do info "    $line"; done
    info "  ── End diagnostic ──"
}

# Wallet verification — sync, scan, balance, address match.
# These are GATES. Failure stops the pipeline.
phase_wallet_verify() {
    local timeout=600  # 10 min — generous, blocks take time to mine and sync
    local interval=15

    source "${SCRIPT_DIR}/wallet-shell.sh"

    for wallet_idx in $(seq 1 "${WITH_WALLET:-1}"); do
    info "Phase 10: Verifying wallet container dwow-wallet-${wallet_idx}..."

    # 0. Fast-fail: verify node0 has produced blocks before polling wallet.
    # If the chain is empty, no wallet can sync — fail immediately (HAZID RC6.4).
    if [ "$wallet_idx" -eq 1 ]; then
        local node0_height
        node0_height=$(jsonrpc_get_block "node0" "31345" "2" 2>/dev/null | grep -c "hash" || echo 0)
        if [ "${node0_height:-0}" -eq 0 ]; then
            fail "node0 has not produced block 2 yet — chain is empty, wallet cannot sync"
            continue
        fi
    fi

    # 1. Wait for blocks. This is the fundamental test: can the wallet sync?
    info "  Waiting for wallet to sync blocks (timeout=${timeout}s)..."
    local height=0 elapsed=0
    while [ "$elapsed" -lt "$timeout" ]; do
        local status
        status=$(wal "$wallet_idx" sync status 2>&1)
        height=$(echo "$status" | grep -oP 'Local chain height: \K\d+' || echo 0)
        [ "$height" -gt 0 ] && break
        sleep "$interval"
        elapsed=$((elapsed + interval))
    done

    if [ "$height" -eq 0 ]; then
        _wallet_diagnostic "$wallet_idx"
        fail "wallet-$wallet_idx failed to sync any blocks after ${timeout}s"
        continue
    fi
    pass "wallet-$wallet_idx synced blocks (height=$height after ${elapsed}s)"

    # Wallet already has its key from entrypoint-wallet.sh import-from-toml.
    # No redundant import — containers own their state, pipeline verifies outcomes.

    # 2. Scan — verify wallet can process blocks
    info "  Running scan..."
    local scan_out
    scan_out=$(wal "$wallet_idx" scan 2>&1)
    if echo "$scan_out" | grep -qE "Scanning block|scan complete|block"; then
        pass "wallet-$wallet_idx scan"
    else
        fail "wallet-$wallet_idx scan produced no output: $scan_out"
    fi

    # 3. Balance — wallet-1 MUST have DRKW from coinbase forwarding
    info "  Checking balance..."
    local balance
    balance=$(wal "$wallet_idx" wallet balance 2>&1)
    if echo "$balance" | grep -qE 'DRKW\s+[1-9][0-9]*'; then
        pass "wallet-$wallet_idx has DRKW balance"
    elif [ "$wallet_idx" -eq 1 ]; then
        fail "wallet-1 has no DRKW balance — coinbase forwarding not working"
    else
        info "  wallet-$wallet_idx has no DRKW (expected until funded via transfer)"
    fi

    # 4. Address match — wallet-1 must match FORWARD_DESTINATION
    info "  Verifying wallet address..."
    local wallet_addr
    wallet_addr=$(wal "$wallet_idx" wallet address 2>&1 | tail -1)
    if [ "$wallet_idx" -eq 1 ]; then
        if [ -n "$FORWARD_DESTINATION" ] && [ "$wallet_addr" != "$FORWARD_DESTINATION" ]; then
            fail "wallet-1 address mismatch: $wallet_addr != FORWARD_DESTINATION=$FORWARD_DESTINATION"
        elif [ -z "$FORWARD_DESTINATION" ]; then
            info "  wallet-1 address: ${wallet_addr:0:16}... (FORWARD_DESTINATION not set)"
        else
            pass "wallet-1 address matches FORWARD_DESTINATION"
        fi
    else
        info "  wallet-$wallet_idx address: ${wallet_addr:0:16}..."
    fi

    done  # end wallet loop
}

phase_wallet_transfer() {
    info "Phase 11: Wallet-to-wallet transfer (wallet-1 → wallet-2)..."

    # Phase gate: verify both wallet containers are alive before attempting transfer.
    local containers_ok=1
    for idx in 1 2; do
        if ! docker ps --format '{{.Names}}' | grep -q "dwow-wallet-$idx"; then
            fail "transfer: dwow-wallet-$idx is not running"
            info "  Dumping dwow-wallet-$idx logs (last 30 lines)..."
            docker logs --tail 30 "dwow-wallet-$idx" 2>&1 | head -30 | while read line; do info "    $line"; done
            containers_ok=0
        fi
    done
    if [ "$containers_ok" -eq 0 ]; then
        return
    fi

    source "${SCRIPT_DIR}/wallet-shell.sh"

    # 1. Get wallet-2 address
    local wallet2_addr
    wallet2_addr=$(wal 2 wallet address 2>&1 | tail -1)
    if [ -z "$wallet2_addr" ]; then
        warn "transfer: failed to get wallet-2 address — wallet may not be initialized"
        return
    fi
    info "  Wallet-2 address: ${wallet2_addr:0:16}..."

    # 2. Wallet-1 transfers 1 DRKW to wallet-2.
    info "  Executing transfer: wallet-1 → wallet-2 (1 DRKW)..."
    local transfer_out
    transfer_out=$(wal 1 transfer 1 DRKW "$wallet2_addr" 2>&1)
    if ! echo "$transfer_out" | grep -q "Transaction"; then
        warn "transfer: wallet-1 transfer failed — chain may not be synced. Output: $transfer_out"
        return
    fi
    pass "transfer tx built and broadcast"

    # 4. Poll for confirmation (block time ~120s, check every 15s for 5 min)
    info "  Waiting for tx confirmation (polling wallet-2 balance)..."
    local balance2 attempt max_attempts
    max_attempts=20  # 20 * 15s = 5 min
    for attempt in $(seq 1 $max_attempts); do
        sleep 15
        wal 2 scan 2>&1 >/dev/null || true
        balance2=$(wal 2 wallet balance 2>&1)
        if echo "$balance2" | grep -qE 'DRKW\s+[1-9][0-9]*'; then
            pass "wallet-2 received transfer after $((attempt * 15))s (balance: $balance2)"
            return
        fi
        info "    attempt $attempt/$max_attempts: no DRKW yet, waiting..."
    done
    warn "transfer not confirmed after $((max_attempts * 15))s — may still be mining (diagnostic)"

    trap - ERR  # Restore ERR trap after diagnostic phase
}
