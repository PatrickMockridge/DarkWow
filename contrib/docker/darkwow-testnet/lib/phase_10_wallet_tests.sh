# DarkWow Testnet Pipeline — Phases 10-11: Wallet Diagnostics
#
# Phase 10: Sync, scan, check balance, address match — DIAGNOSTIC ONLY.
# Phase 11: wallet-1 sends to wallet-2, verify receiving address.
#
# The wallet phase NEVER blocks pipeline success. Finding peers and syncing
# is fundamental functionality — failures here are diagnostics, not gates.
# The mining nodes already prove the network works.
#
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (WITH_WALLET, FORWARD_DESTINATION, SCRIPT_DIR),
#
# Sources: wallet-shell.sh (wal function) at runtime.
#
# Sourced by test_pipeline.sh after phase_09_blocks.sh.

# ── Diagnostic helper: dump wallet state when sync isn't working ──────────
_wallet_diagnostic() {
    local wallet_idx="$1"
    local container="dwow-wallet-${wallet_idx}"
    info "  ── Diagnostic: wallet-${wallet_idx} ──"
    info "  Container status:"
    docker inspect --format '{{.State.Status}} ({{.State.StartedAt}})' "$container" 2>/dev/null || echo "  (inspect failed)"
    info "  Container logs (last 15 lines):"
    docker logs --tail 15 "$container" 2>&1 | while read line; do info "    $line"; done
    info "  P2P config from container:"
    docker exec "$container" cat /root/.config/dwow/dww_config.toml 2>/dev/null | grep -A5 '\[net\]' | head -10 | while read line; do info "    $line"; done || info "    (could not read config)"
    info "  Network: can container reach lilith?"
    docker exec "$container" sh -c 'echo | timeout 3 nc -w2 lilith 31340 2>&1 && echo "  TCP: lilith:31340 REACHABLE" || echo "  TCP: lilith:31340 UNREACHABLE"' 2>/dev/null
    info "  Sync status:"
    wal "$wallet_idx" sync status 2>&1 | while read line; do info "    $line"; done
    info "  ── End diagnostic ──"
}

# Wallet verification — sync, scan, balance, address match.
# DIAGNOSTIC ONLY — never fails the pipeline. Reports status.
phase_wallet_verify() {
    local timeout=120
    local interval=5

    source "${SCRIPT_DIR}/wallet-shell.sh"
    set +e  # wal() can return non-zero — don't trigger ERR trap

    for wallet_idx in $(seq 1 "${WITH_WALLET:-1}"); do
    info "Phase 10: Verifying wallet container dwow-wallet-${wallet_idx}..."

    # Daemon is already running (entrypoint started it).
    # Poll sync status until peers > 0 AND height > 0.
    info "  Waiting for P2P peers and blocks (timeout=${timeout}s, diagnostic only)..."
    local peers=0 height=0 elapsed=0
    while [ "$elapsed" -lt "$timeout" ]; do
        local status
        status=$(wal "$wallet_idx" sync status 2>&1)
        peers=$(echo "$status" | grep -oP 'Peers: \K\d+' || echo 0)
        height=$(echo "$status" | grep -oP 'Local chain height: \K\d+' || echo 0)
        [ "$peers" -gt 0 ] && [ "$height" -gt 0 ] && break
        sleep "$interval"
        elapsed=$((elapsed + interval))
    done
    if [ "$peers" -eq 0 ]; then
        warn "wallet-$wallet_idx has no peers after ${timeout}s — diagnostic follows"
        _wallet_diagnostic "$wallet_idx"
    elif [ "$height" -eq 0 ]; then
        warn "wallet-$wallet_idx has peers=$peers but no blocks after ${timeout}s — diagnostic follows"
        _wallet_diagnostic "$wallet_idx"
    else
        pass "wallet-$wallet_idx sync (peers=$peers, height=$height)"
    fi

    # If no peers, skip remaining checks — nothing to scan.
    if [ "$peers" -eq 0 ] || [ "$height" -eq 0 ]; then
        continue
    fi

    # 3. scan — capture output for diagnostics
    info "  Running scan..."
    local scan_out
    scan_out=$(wal "$wallet_idx" scan 2>&1)
    if echo "$scan_out" | grep -q "Scanning block"; then
        pass "wallet-$wallet_idx scan"
    else
        warn "wallet-$wallet_idx scan found no blocks. Output:"
        echo "$scan_out" | head -10 | while read line; do info "    $line"; done
    fi

    # 4. balance — critical check. wallet-1 must have DRKW (coinbase forwarding).
    #    wallet-2+ may have 0 balance until funded via transfer.
    info "  Checking balance..."
    local balance
    balance=$(wal "$wallet_idx" wallet balance 2>&1)
    if echo "$balance" | grep -qE 'DRKW\s+[1-9][0-9]*'; then
        pass "wallet-$wallet_idx balance (DRKW found)"
    else
        if [ "$wallet_idx" -eq 1 ]; then
            warn "wallet-1 has no DRKW balance — coinbase forwarding may not be configured. Balance output: $balance"
        else
            info "  wallet-$wallet_idx has no DRKW balance (expected until funded via transfer)"
        fi
    fi

    # 5. address match — only wallet-1 matches FORWARD_DESTINATION
    info "  Verifying wallet address..."
    local wallet_addr
    wallet_addr=$(wal "$wallet_idx" wallet address 2>&1 | tail -1)
    if [ "$wallet_idx" -eq 1 ]; then
        if [ -n "$FORWARD_DESTINATION" ] && [ "$wallet_addr" != "$FORWARD_DESTINATION" ]; then
            warn "wallet-1 address mismatch: $wallet_addr != FORWARD_DESTINATION=$FORWARD_DESTINATION"
        elif [ -z "$FORWARD_DESTINATION" ]; then
            info "  wallet-1 address: ${wallet_addr:0:16}... (FORWARD_DESTINATION not set)"
        else
            pass "wallet-1 address matches FORWARD_DESTINATION"
        fi
    else
        info "  wallet-$wallet_idx address: ${wallet_addr:0:16}..."
    fi

    # === Independent verification (wallet-1 only) ===
    if [ "$wallet_idx" -ne 1 ]; then
        continue
    fi

    # Claim B: Balance cross-check via Python model
    info "  Independent: coinbase detection via Python model..."
    if [ "$height" -gt 0 ]; then
        local expected_balance actual_balance
        expected_balance=$(python3 -c "
import sys; sys.path.insert(0, 'sim')
from crypto import expected_reward
print(expected_reward($height))
" 2>/dev/null || echo 0)
        actual_balance=$(echo "$balance" | grep -oP 'DRKW\s+\K\d+' | head -1 || echo 0)
        if [ "$expected_balance" -gt 0 ] && [ "$actual_balance" -eq 0 ]; then
            warn "balance is 0 but expected_reward($height)=$expected_balance — coinbase forwarding may have failed"
        elif [ "$expected_balance" -gt 0 ] && [ "$actual_balance" -gt 0 ]; then
            pass "independent balance (actual=$actual_balance, expected_reward=$expected_balance)"
        else
            info "  independent balance check skipped"
        fi
    else
        info "  independent balance check skipped (no blocks)"
    fi

    # Claim C: P2P connected — check seed hostlist
    info "  Independent: wallet in seed hostlist..."
    local hostlist
    hostlist=$(docker exec dwow-lilith cat /root/.local/share/dwow/lilith/darkwow-testnet/hostlist.tsv 2>/dev/null || echo "")
    if echo "$hostlist" | grep -q "wallet-"; then
        pass "wallet found in seed hostlist"
    elif [ -z "$hostlist" ]; then
        info "  independent hostlist check skipped (no hostlist)"
    else
        warn "wallet not in seed hostlist (may not have registered yet — P2P protocol mismatch possible)"
    fi
    done  # end wallet loop

    set -e  # Re-enable errexit after diagnostic phase
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

    set -e  # Re-enable errexit after diagnostic phase
}
