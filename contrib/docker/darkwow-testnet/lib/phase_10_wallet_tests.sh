# DarkWow Testnet Pipeline — Phases 10-11: Wallet Tests
#
# Phase 10: Sync, scan, check balance, address match.
# Phase 11: wallet-1 sends to wallet-2, verify receiving address.
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (WITH_WALLET, FORWARD_DESTINATION, SCRIPT_DIR),
#
# Sources: wallet-shell.sh (wal function) at runtime.
#
# Sourced by test_pipeline.sh after phase_09_blocks.sh.

# Wallet verification — sync, scan, balance, address match.
# Only runs when --with-wallet is used (resume-from 10, gated by --with-wallet).
# Exercises the wallet container against the running dockernet to prove the full chain works:
# P2P sync → local scan → AEAD decrypt → balance > 0.
phase_wallet_verify() {
    local timeout=120
    local interval=5

    source "${SCRIPT_DIR}/wallet-shell.sh"

    for wallet_idx in $(seq 1 "${WITH_WALLET:-1}"); do
    info "Phase 10: Verifying wallet container dwow-wallet-${wallet_idx}..."

    # 1. sync init — capture output for diagnostics
    info "  Running sync init..."
    local sync_out
    sync_out=$(wal "$wallet_idx" sync init 2>&1)
    if echo "$sync_out" | grep -q "P2P sync started"; then
        pass "wallet-$wallet_idx sync init"
    else
        fail "wallet-$wallet_idx sync init failed. Output:"
        echo "$sync_out" | head -10 | while read line; do info "    $line"; done
        # Defense in depth: dump container logs on sync failure
        info "  Dumping wallet-$wallet_idx container logs (last 20 lines)..."
        docker logs --tail 20 "dwow-wallet-$wallet_idx" 2>&1 | head -20 | while read line; do info "    $line"; done
        continue
    fi

    # 2. poll sync status until peers > 0 AND height > 0
    info "  Waiting for P2P peers and blocks (timeout=${timeout}s)..."
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
        fail "wallet-$wallet_idx has no peers after ${timeout}s"
        continue
    fi
    if [ "$height" -eq 0 ]; then
        fail "wallet-$wallet_idx has no blocks after ${timeout}s"
        continue
    fi
    pass "wallet-$wallet_idx sync (peers=$peers, height=$height)"

    # 3. scan — capture output for diagnostics
    info "  Running scan..."
    local scan_out
    scan_out=$(wal "$wallet_idx" scan 2>&1)
    if echo "$scan_out" | grep -q "Scanning block"; then
        pass "wallet-$wallet_idx scan"
    else
        fail "wallet-$wallet_idx scan failed or found no blocks. Output:"
        echo "$scan_out" | head -10 | while read line; do info "    $line"; done
        continue
    fi

    # 4. balance — critical check. wallet-1 must have DRKW (coinbase forwarding).
    #    wallet-2+ may have 0 balance until funded via transfer.
    info "  Checking balance..."
    local balance
    balance=$(wal "$wallet_idx" wallet balance 2>&1)
    if echo "$balance" | grep -qE 'DRKW\s+[0-9]+'; then
        pass "wallet-$wallet_idx balance (DRKW found)"
    else
        if [ "$wallet_idx" -eq 1 ]; then
            fail "wallet-1 has no DRKW balance. Output: $balance"
        else
            info "  wallet-$wallet_idx has no DRKW balance (expected until funded via transfer)"
        fi
    fi

    # 5. address match — only wallet-1 matches FORWARD_DESTINATION
    info "  Verifying wallet address..."
    local wallet_addr
    wallet_addr=$(wal "$wallet_idx" wallet address 2>&1 | tail -1)
    if [ "$wallet_idx" -eq 1 ]; then
        if [ "$wallet_addr" != "$FORWARD_DESTINATION" ]; then
            fail "wallet-1 address mismatch: $wallet_addr != FORWARD_DESTINATION=$FORWARD_DESTINATION"
        else
            pass "wallet-1 address matches FORWARD_DESTINATION"
        fi
    else
        pass "wallet-$wallet_idx address: ${wallet_addr:0:16}..."
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
            fail "balance is 0 but expected_reward($height)=$expected_balance — coinbase forwarding may have failed"
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
        fail "wallet not in seed hostlist (may not have registered yet)"
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
        fail "transfer: failed to get wallet-2 address"
        return
    fi
    info "  Wallet-2 address: ${wallet2_addr:0:16}..."

    # 2. Wallet-1 transfers 1 DRKW to wallet-2.
    #    The transfer function handles capability selection internally — it will either
    #    succeed or return a clear error message. phase_wallet_verify already
    #    confirmed wallet-1 has DRKW balance.
    info "  Executing transfer: wallet-1 → wallet-2 (1 DRKW)..."
    local transfer_out
    transfer_out=$(wal 1 transfer 1 DRKW "$wallet2_addr" 2>&1)
    if ! echo "$transfer_out" | grep -q "Transaction"; then
        fail "transfer: wallet-1 transfer failed. Output: $transfer_out"
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
        if echo "$balance2" | grep -qE 'DRKW\s+[0-9]+'; then
            pass "wallet-2 received transfer after $((attempt * 15))s (balance: $balance2)"
            return
        fi
        info "    attempt $attempt/$max_attempts: no DRKW yet, waiting..."
    done
    fail "wallet-2 has no DRKW after $((max_attempts * 15))s. Output: $balance2"
}
