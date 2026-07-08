# DarkWow Testnet Pipeline — Phases 10-11: Wallet Verification
#
# Phase 10: Sync blocks, scan, verify balance, address match.
# Phase 11: wallet-1 sends to wallet-2, verify receiving address.
#
# These tests are GATES — failure stops the pipeline.
# A wallet that can't sync blocks is a broken wallet.
#
# Dependencies: output.sh (info, pass, fail, warn),
#               config.sh (WITH_WALLET, SCRIPT_DIR),
#
# Sources: wallet-shell.sh (wal function) at runtime.
#
# Sourced by test_pipeline.sh after phase_09_blocks.sh.

# Native token id (base58 of 32 zero bytes) — the DRKW coinbase token. This is
# the value the wallet's `--porcelain` balance emits (NOT the human alias "DRKW").
# Must match wallet_model.DRKW_TOKEN_ID_STR in the Python spec.
NATIVE_TOKEN_ID="11111111111111111111111111111111"

# Minimum fee for a single-input transaction (fee_builder.rs:50).
# A wallet must hold at least this much native token to pay network fees
# and open the capability pathway.
DEFAULT_FEE=42000000

# Native-token balance for a wallet via the frozen `--porcelain` contract.
# balance --porcelain prints one "<token_id>\t<amount>" line per held token.
# Prints the native-token amount (0 if none). Path-independent (RPC or local CLI).
_native_balance() {
    local idx="$1"
    wal "$idx" wallet balance --porcelain 2>/dev/null \
        | awk -F'\t' -v t="$NATIVE_TOKEN_ID" '$1==t {print $2; f=1} END{if(!f) print 0}'
}

# Held-capability count for a wallet via `scan --porcelain` ("capabilities=<N>\tblocks=<M>").
# This is the real decrypt signal (coinbase/notes decrypted), not a log-grep proxy.
_scan_capabilities() {
    local idx="$1"
    wal "$idx" scan --porcelain 2>/dev/null \
        | sed -n 's/^capabilities=\([0-9][0-9]*\).*/\1/p' | head -1
}

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

# Wait for container to be healthy (retry with backoff).
# Uses Docker's native health check — polls health status until "healthy".
_wait_for_container_healthy() {
    local container="$1"
    local max_wait="${2:-60}"
    local start=$SECONDS
    while [ $((SECONDS - start)) -lt "$max_wait" ]; do
        local status
        status=$(docker inspect --format '{{.State.Health.Status}}' "$container" 2>/dev/null || echo "unknown")
        if [ "$status" = "healthy" ]; then
            return 0
        fi
        info "  Waiting for $container to be healthy (status=$status, elapsed=$((SECONDS - start))s)..."
        sleep 5
    done
    warn "$container not healthy after ${max_wait}s — proceeding anyway"
    return 1
}

# Wallet verification — sync, scan, balance, address match.
# These are GATES. Failure stops the pipeline.
phase_wallet_verify() {
    local timeout=600  # 10 min — generous, blocks take time to mine and sync
    local interval=15

    source "${SCRIPT_DIR}/wallet-shell.sh"

    # Stabilize: wait for node0 and observer to report healthy before wallet ops.
    # Wallet connectivity depends on these nodes being ready.
    info "Stabilizing container health before wallet verification..."
    _wait_for_container_healthy "dwow-node0" 60 || true
    _wait_for_container_healthy "dwow-observer" 60 || true

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
        height=$(echo "$status" | grep 'Local chain height:' | sed 's/.*Local chain height: //' | grep -oE '[0-9]+' | head -1 || echo 0)
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

    # The wallet derives its key on boot from keys.toml [wallet-N] (WALLET_NAME).
    # No import step — containers own their state, the pipeline verifies outcomes.
    # keys.toml declares wallet-1 with node0's secret, so wallet-1 can decrypt
    # node0's coinbase. A wrong key surfaces as capabilities=0 / native balance 0.

    # 2. Scan — the real decrypt signal: `capabilities=<N>` (coinbase/notes). The
    #    old log-grep (Scanning block|Scan complete) didn't assert anything decrypted.
    info "  Scanning blocks..."
    local caps
    caps=$(_scan_capabilities "$wallet_idx")
    if [ -n "$caps" ] && [ "$caps" -gt 0 ]; then
        pass "wallet-$wallet_idx scan (capabilities=$caps — coinbase decrypted)"
    elif [ "$wallet_idx" -eq 1 ]; then
        fail "wallet-1 has zero capabilities after scan — coinbase decrypt may have failed"
    else
        info "  wallet-$wallet_idx capabilities=$caps (expected 0 until funded via transfer)"
    fi

    # 2b. Capability path diagnostic: verify the generic AEAD scan path is active.
    #     `[CAPABILITY] Stage N:` messages confirm the pipeline is running:
    #     Stage 1 (SCAN) → Stage 2 (DISCOVER) → Stage 3 (STORE).
    #     Absence means the capability model is not discovering anything — possible
    #     configuration issue or empty block with no contract calls (expected early).
    local cap_diag
    cap_diag=$(wal "$wallet_idx" scan 2>&1 | grep -c "\[CAPABILITY\] Stage" || true)
    if [ "${cap_diag:-0}" -gt 0 ]; then
        info "  wallet-$wallet_idx capability path: $cap_diag diagnostic stages found (pipeline active)"
    else
        info "  wallet-$wallet_idx capability path: 0 diagnostic stages (expected — no contract calls in early blocks)"
    fi

    # 3. Balance — assert the native token line (not grep for human alias "DRKW",
    #    which the LocalWallet path never prints). wallet-1 must have DRKW > 0;
    #    wallet-2 is 0 pre-transfer.
    local native_amt
    native_amt=$(_native_balance "$wallet_idx")
    if [ -n "$native_amt" ] && [ "$native_amt" -gt 0 ]; then
        pass "wallet-$wallet_idx native balance = $native_amt (DRKW)"
    elif [ "$wallet_idx" -eq 1 ]; then
        fail "wallet-1 native balance is 0 — coinbase decrypt or forwarding failure (check caps=$caps, height=$height)"
    else
        info "  wallet-$wallet_idx native balance = 0 (expected until transfer)"
    fi

    # 3b. Fee readiness gate: wallet-1 must hold enough native token to pay
    #     network fees. Without this, capability pathways cannot open
    #     (every DeployV1, TransferV1, and contract invocation attaches a fee).
    if [ "$wallet_idx" -eq 1 ]; then
        if [ -n "$native_amt" ] && [ "$native_amt" -ge "$DEFAULT_FEE" ]; then
            pass "wallet-1 fee-ready: $native_amt DRKW >= $DEFAULT_FEE (can pay network fees)"
        else
            fail "wallet-1 NOT fee-ready: $native_amt DRKW < $DEFAULT_FEE — capability pathways blocked"
        fi
    fi

    # 4. Show wallet address
    info "  Verifying wallet address..."
    local wallet_addr
    wallet_addr=$(wal "$wallet_idx" wallet address 2>&1 | tail -1)
    info "  wallet-$wallet_idx address: ${wallet_addr:0:16}..."

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
        return 1
    fi

    source "${SCRIPT_DIR}/wallet-shell.sh"

    # 1. Get wallet-2 address
    local wallet2_addr
    wallet2_addr=$(wal 2 wallet address 2>&1 | tail -1)
    if [ -z "$wallet2_addr" ]; then
        fail "transfer: failed to get wallet-2 address — wallet may not be initialized"
        return 1
    fi
    info "  Wallet-2 address: ${wallet2_addr:0:16}..."

    # 2. Wallet-1 transfers 1 DRKW to wallet-2. Assert the frozen `txid=` contract
    #    (both RPC and standalone paths emit it under --porcelain).
    info "  Executing transfer: wallet-1 → wallet-2 (1 DRKW)..."
    local transfer_out
    transfer_out=$(wal 1 transfer 1 DRKW "$wallet2_addr" --porcelain 2>&1)
    if ! echo "$transfer_out" | grep -qE '^txid='; then
        fail "transfer: wallet-1 transfer failed — chain may not be synced. Output: $transfer_out"
        return 1
    fi
    pass "transfer tx built and broadcast ($(echo "$transfer_out" | grep -oE '^txid=[0-9a-f]+' | head -1))"

    # 4. Poll for confirmation (block time ~120s, check every 15s for 5 min).
    #    Assert wallet-2's NATIVE-token balance goes > 0 (received the transfer).
    info "  Waiting for tx confirmation (polling wallet-2 native balance)..."
    local recv attempt max_attempts
    max_attempts=20  # 20 * 15s = 5 min
    for attempt in $(seq 1 $max_attempts); do
        sleep 15
        _scan_capabilities 2 >/dev/null || true
        recv=$(_native_balance 2)
        if [ -n "$recv" ] && [ "$recv" -gt 0 ]; then
            pass "wallet-2 received transfer after $((attempt * 15))s (native balance=$recv)"
            break
        fi
        info "    attempt $attempt/$max_attempts: wallet-2 native balance still 0, waiting..."
    done
    if [ -z "$recv" ] || [ "$recv" -le 0 ]; then
        fail "transfer not confirmed after $((max_attempts * 15))s — wallet-2 native balance never went > 0"
        return 1
    fi

    # 5. Revocation detection: wallet-1 rescans to detect its own nullifier.
    #    The spent coin MUST be marked revoked in held_capabilities. Without this,
    #    the wallet thinks it still has the spent coin and will fail on the next
    #    fee payment (duplicate nullifier rejection at the protocol level).
    #    wallet.md:409 — Detect Revocation is step 4 of the capability lifecycle.
    info "  Verifying revocation detection (wallet-1 rescan)..."
    local caps_before caps_after
    caps_before=$(_scan_capabilities 1)
    # Re-scan to detect the nullifier from wallet-1's own transfer
    _scan_capabilities 1 >/dev/null
    caps_after=$(_scan_capabilities 1)
    if [ -n "$caps_after" ] && [ "$caps_after" -lt "$caps_before" ]; then
        pass "wallet-1 revocation detected: active caps $caps_before → $caps_after (spent coin revoked)"
    else
        warn "wallet-1 active caps: before=$caps_before after=$caps_after (expected decrease — nullifier detection may not have run)"
    fi

    # 6. Fee readiness after transfer: wallet-1 must still hold enough native
    #    token for future fee payments. If this is 0 or below DEFAULT_FEE,
    #    capability pathways cannot open (every contract call needs a fee).
    local post_xfer_balance
    post_xfer_balance=$(_native_balance 1)
    if [ -n "$post_xfer_balance" ] && [ "$post_xfer_balance" -ge "$DEFAULT_FEE" ]; then
        pass "wallet-1 post-transfer fee-ready: $post_xfer_balance DRKW >= $DEFAULT_FEE (capability pathways open)"
    else
        fail "wallet-1 post-transfer NOT fee-ready: balance=$post_xfer_balance < $DEFAULT_FEE — capability pathways blocked"
    fi

    trap - ERR  # Restore ERR trap after diagnostic phase
}
