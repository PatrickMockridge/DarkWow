# Consensus HAZID Report — Hazard Identification & Bow-Tie Analysis

> **Date:** 2026-07-16
> **Scope:** All consensus-critical code paths — determinism, arithmetic, atomicity
> **Methodology:** Systematic HAZID sweep, three independent agents, root-cause analysis,
> bow-tie diagrams for CATASTROPHIC and HIGH findings.

## Methodology

### Hazard Categories

| Category | What It Covers |
|----------|---------------|
| **DETERMINISM** | Non-deterministic functions in consensus paths — wall clock, randomness, relaxed atomics, non-canonical serialization |
| **ARITHMETIC** | Overflow, truncation, saturating defaults hiding errors, formula mismatches between spec and code |
| **ATOMICITY** | State inconsistency from partial updates, race conditions, TOCTOU gaps, missing recovery paths |
| **DATA-LOSS** | Destructive operations before fallible ones, tx loss on failure paths, silent error discards |
| **MISSING-IMPL** | Specified behavior not implemented, dead code in hot paths, verification functions never called |
| **TYPE-FRACTURE** | Raw bytes where typed newtypes should be used, type confusion across module boundaries |

### Severity Classification

| Level | Definition | Example |
|-------|-----------|---------|
| **CATASTROPHIC** | Chain fork, supply audit divergence, block rejection cascade, silent state corruption | Two nodes compute different rewards for same height; WASM execution skipped during reorg |
| **HIGH** | Silent data corruption, unrecoverable state, mining halt, nullifier replay, mempool tx massacre | Competing blocks lost on error path; stratum blocks never broadcast |
| **MEDIUM** | Incorrect balance, wrong fee, stale data, degraded security, recoverable inconsistency | Fee extraction silent zero; in-memory caches diverge from sled |
| **LOW** | Cosmetic, diagnostic, recoverable without chain impact | Unused code, log format drift, performance-only issues |

### Bow-Tie Structure

```
THREAT ──► [BARRIERS (prevention)] ──► TOP EVENT ──► [BARRIERS (mitigation)] ──► CONSEQUENCE
```

---

## Consolidated Hazard Register

### CATASTROPHIC (4)

| ID | Source | Summary | Type |
|----|--------|---------|------|
| **H-C1** | Determinism sweep | `reorganize_to()` commits peer blocks without executing WASM, updating contract state, or updating cumulative supply chain. If triggered by a fork, chain state is silently corrupted — contracts tree reflects pre-reorg state for blocks that are now canonical. | MISSING-IMPL |
| **H-C2** | Atomicity sweep | Stratum/merge-mining blocks never broadcast to P2P peers. `stratum_submit` and `mm_submit_solution` call `accept_block` but never `broadcast_block`. Mined blocks are committed locally but the network only discovers them via 30-second sync poll. | DATA-LOSS |
| **H-C3** | Arithmetic sweep | `expected_reward()` uses linear approximation `R_tail + (R0 - R_tail) * (1 - h/H)` but the spec documents exponential `R(h) = max(R0 * 2^(-h/H), R_tail)`. At the half-life (h=H), the linear formula pays ~0.80 DRKW vs the exponential's ~6.92 DRKW — ~8.7x underpayment. Total supply under linear is ~7.7M DRKW vs exponential's ~10.5M DRKW over first 4 years. | ARITHMETIC |
| **H-C4** | Determinism sweep | `serde_json::to_vec` fallback writes `vec![0u8; 32]` on serialization failure in competing block dedup path. If serialization fails, dedup hashes become zero-vectors — duplicate competing blocks accepted, valid ones silently dropped. | DATA-LOSS |

### HIGH (12)

| ID | Source | Summary | Type |
|----|--------|---------|------|
| **H-H1** | Determinism sweep | `Ordering::Relaxed` on all consensus-critical atomics (target, accumulated_work, timestamps). On ARM/RISC-V, different threads can observe different values indefinitely. | DETERMINISM |
| **H-H2** | Determinism sweep | `saturating_sub` on block timestamps in `adjust_target()` masks decreasing timestamps as zero-interval. Attacker controlling a mining majority could bias difficulty. | ARITHMETIC |
| **H-H3** | Determinism sweep | 50+ `.lock().unwrap()` with zero poison recovery across chain_state.rs. One panic in any locked section poisons ALL locks and brings down the entire node. | ATOMICITY |
| **H-H4** | Determinism sweep | In-memory caches (`coin_set`, `nullifier_set`, `uncle_coin_set`) diverge from sled. `uncle_coin_set` NEVER restored on restart — always empty, allowing duplicate uncle inclusion. | DATA-LOSS |
| **H-H5** | Determinism sweep | `serde_json` is non-canonical serialization format for block storage. Two nodes with different serde versions could have different sled bytes for semantically identical blocks. | DETERMINISM |
| **H-H6** | Determinism sweep | Uncle merge in `execution.rs` has no duplicate-key conflict detection. Two uncles writing the same contract state key — second silently overwrites first. | DETERMINISM |
| **H-H7** | Atomicity sweep | Mempool transaction loss on mining failure. `miner_task` never re-inserts mempool txs on error paths, unlike `miner_rpc.rs` which does. Two miners using different paths have different tx-recovery behavior. | ATOMICITY |
| **H-H8** | Atomicity sweep | Competing blocks permanently lost on coinbase/template generation failure. `take_competing_blocks()` is destructive and called before fallible operations in 3 of 4 call sites. | DATA-LOSS |
| **H-H9** | Atomicity sweep | Concurrent stratum logins overwrite template and config non-atomically. Two miners logging in simultaneously get mismatched template+config pairs. | ATOMICITY |
| **H-H10** | Atomicity sweep | Reorg doesn't re-admit disconnected-block transactions to mempool. Bitcoin Core re-admits them; DarkWow permanently loses them. | DATA-LOSS |
| **H-H11** | Arithmetic sweep | Dual supply tracking (contracts tree authoritative vs supply_chain tree mirrored) has zero automated reconciliation. `verify_cumulative_supply()` never called in production. A regression in the bridge function would silently desynchronize. | MISSING-IMPL |
| **H-H12** | Arithmetic sweep | Non-ZK fallback path in `generate_linear_block_template()` produces templates with all-zero cryptographic material. Currently blocked by validation layers, but represents dead code in the consensus hot path. | MISSING-IMPL |

### MEDIUM (18)

| ID | Source | Summary | Type |
|----|--------|---------|------|
| H-M1 | Determinism | `Clone` on PoWConsensus creates independent atomics — footgun-prone API | DETERMINISM |
| H-M2 | Determinism | `save_to_batch` reads `target` via Relaxed outside lock (currently safe, fragile) | DETERMINISM |
| H-M3 | Determinism | Deployooor post-processing bypasses duplicate-key detection during merge | DETERMINISM |
| H-M4 | Determinism | `unwrap_or_default` on ZK metadata decoding silently masks malformed proofs | MISSING-IMPL |
| H-M5 | Determinism | Error variant detection by string matching in `TxBackend::get_tx` | DETERMINISM |
| H-M6 | Determinism | `aggregate().unwrap_or_default()` in `accept_block()` silences overlay failure | MISSING-IMPL |
| H-M7 | Determinism | `saturating_add` on cumulative supply — silently saturates at u64::MAX | ARITHMETIC |
| H-M8 | Determinism | Competing block dedup hashes use header serialization not block hash | ATOMICITY |
| H-M9 | Determinism | `consensus.load()` failure silently discarded — node starts with corrupted state | DATA-LOSS |
| H-M10 | Atomicity | Template read/write race between stratum login and submit (no composite lock) | ATOMICITY |
| H-M11 | Atomicity | `mm_jobs` clear-all eviction at capacity (not FIFO) | DATA-LOSS |
| H-M12 | Atomicity | Sync state TOCTOU — miner mines during SYNC_BEHIND transition | ATOMICITY |
| H-M13 | Arithmetic | `saturating_sub` in `compute_reward()` hides pin reward overflow | ARITHMETIC |
| H-M14 | Arithmetic | `unwrap_or(0)` on WASM `total_supply` deserialization (contract entrypoint) | MISSING-IMPL |
| H-M15 | Arithmetic | Identity point as default sentinel conflates "not initialized" with "validly zero" | MISSING-IMPL |
| H-M16 | Arithmetic | `verify_entries()` checks only monotonicity, not Pedersen chain integrity | MISSING-IMPL |
| H-M17 | Arithmetic | `prev_coin` never updated from actual chain data in `verify_cumulative_supply()` | MISSING-IMPL |
| H-M18 | Arithmetic | `expected_reward(height: u32)` — u64→u32 truncation at ~16,000 years | ARITHMETIC |

---

## Root Cause Analysis

Six systemic root causes account for 28 of 34 findings:

### RC1: Error handling that silently discards failures
**Findings:** H-C4, H-H3, H-H7, H-H8, H-M4, H-M6, H-M9, H-M11, H-M14, H-M15 (10 findings)

Every `.unwrap()`, `.unwrap_or()`, `.unwrap_or_default()`, `let _ =`, and non-`?` error path in consensus code is a potential silent corruption vector. The pattern "failures are inconvenient, so substitute a default" is the single largest source of hazards.

**Recommended control:** Systematic audit of every non-propagated error in consensus paths. Replace `.unwrap_or(default)` with fail-closed `?` or explicit `match` with logging.

### RC2: Missing implementations of specified behavior  
**Findings:** H-C1, H-C2, H-H10, H-H11, H-M16, H-M17 (6 findings)

Functions exist in the specification but are not hooked up to the code (`verify_cumulative_supply`, broadcast after stratum submit, tx re-insertion on reorg), or functions are implemented but incomplete (`reorganize_to` skips WASM, `verify_entries` skips Pedersen check).

**Recommended control:** Spec-to-code traceability matrix. Every SHALL in the specification must map to a function in the code that is called in production.

### RC3: Non-deterministic primitives in consensus paths
**Findings:** H-H1, H-H2, H-H5, H-H6, H-M1, H-M2, H-M5, H-M8 (8 findings)

Relaxed atomics, saturating arithmetic masking violations, non-canonical serialization, and hash-map iteration order dependence create paths where two nodes can reach different conclusions from the same chain data.

**Recommended control:** `Ordering::Acquire`/`Release` on all consensus atomics. Replace `serde_json` with `dwow_serial` for block storage. Add deterministic tiebreakers to all HashMap iterations that feed consensus decisions.

### RC4: Destructive operations before fallible ones (no rollback)
**Findings:** H-H7, H-H8, H-H9, H-M10 (4 findings)

`take_competing_blocks()` removes data before subsequent fallible operations. If those operations fail, the data is lost. No savepoint/rollback mechanism exists.

**Recommended control:** Reverse the order — do all fallible work first, then destructively consume state. Or add explicit `put_competing_blocks()` recovery on every error path.

### RC5: Two sources of truth for the same state
**Findings:** H-H4, H-H11, H-M9 (3 findings)

In-memory caches diverge from sled. Supply chain tree diverges from contracts tree. Consensus state loaded from sled diverges from defaults. Two representations of the same fact can disagree with no detection.

**Recommended control:** Single source of truth for each state fact. If caching is needed for performance, add startup reconciliation that verifies cache == store.

### RC6: Spec/code formula mismatch
**Findings:** H-C3, H-M13, H-M18 (3 findings)

The documented exponential decay formula differs from the implemented linear approximation. Saturating arithmetic silently masks invariant violations that should be hard errors.

**Recommended control:** Either implement the exponential formula or update the spec to document the linear approximation. Replace `saturating_*` with `checked_*` in consensus paths where overflow is a bug, not a feature.

---

## Bow-Tie Analysis (CATASTROPHIC Findings)

### H-C1: `reorganize_to()` Skips WASM Execution

```
THREAT                              BARRIERS (prevention)           TOP EVENT                       BARRIERS (mitigation)           CONSEQUENCE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Fork occurs — peer chain            [NONE — no prevention]         reorganize_to() is called       [NONE — no mitigation]          Chain state corrupted:
has more accumulated work           reorganize_to has no           with incomplete                                                  contracts tree reflects
than canonical chain                safety gate — it trusts        implementation. Blocks                                          pre-reorg state for
                                    the caller to provide          are swapped without                                             blocks now canonical.
Peer sends competing blocks         a properly-built chain.        executing WASM, updating                                        Cumulative supply chain
via P2P or sync task                                               supply chain, or updating                                       diverges. Nullifier/
                                    try_reorg_from_competing       coin_set/nullifier_set.                                         coin_set caches stale.
Peer announces higher tip           (chain_state.rs:982) and       Chain state silently                                            Subsequent blocks built
via GetTip/Tip sync protocol       try_reorg_from_uncle_chains    corrupted.                                                     on corrupted foundation.
                                    (chain_state.rs:1251) are
The sync task detects a peer        the only callers. Neither      ─────────────────────                                            Catastrophic chain fork:
ahead and triggers a reorg          passes mempool reference.                                                                      nodes that reorged vs
to the peer's chain.                                                                                                                nodes that didn't have
                                    The function is in dead                                                                         different state forever.
                                    code — never triggered in
                                    normal operation (no known
                                    production fork scenario).
```

### H-C2: Stratum/Merge-Mined Blocks Never Broadcast

```
THREAT                              BARRIERS (prevention)           TOP EVENT                       BARRIERS (mitigation)           CONSEQUENCE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
External miner (xmrig)              [NONE — no prevention]         stratum_submit() or             Sync task polls every           Pool-mined blocks are
finds a valid nonce and             The broadcast is simply        mm_submit_solution()            30 seconds for new blocks.      invisible to the network
submits via stratum or              missing from both code         calls accept_block() but                                        for up to 30 seconds.
mm_rpc.                             paths.                         never broadcast_block().        Built-in miner broadcasts       During this window:
                                                                    Block is committed locally,     immediately.                    - Other miners waste
Pool operator runs a                miner_task calls               but no P2P peer knows                                          work on stale tips.
stratum server. p2pool              broadcast_block() at           about it.                       No detection mechanism          - Wallet users can't
operator runs mm_rpc.               line 1305 — this is the                                       — no alert, no log, no          confirm transactions.
                                    correct pattern.               ─────────────────────            metric.                         - Network hash rate
Merge-mining sidecar                                                      │                                                       appears fragmented.
(xmrig) submits share to                                                                                                          - Block propagation
p2pool. p2pool submits to                                                                                                         latency spikes from
mm_rpc when share meets                                                                                                           30s to 120s (sync poll
network difficulty.                                                                                                               + block time).
```

### H-C3: expected_reward() Formula Mismatch

```
THREAT                              BARRIERS (prevention)           TOP EVENT                       BARRIERS (mitigation)           CONSEQUENCE
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
A node implements the               [NONE — no prevention]         A node running the              Internal consistency:           If an external
documented exponential              The spec documents one         documented exponential          all current nodes run           implementation (exchange,
formula:                            formula, the code              formula computes different      the same linear code,           block explorer, audit
R(h) = max(R0*2^(-h/H),             implements another.            rewards than all other          so no fork today.               tool) uses the documented
R_tail)                             No compile-time or             nodes at every height.                                         exponential formula, it
                                     runtime check detects         The cumulative supply           WASM contract and               computes different rewards
The spec (consensus-                 the discrepancy.              chains diverge.                 Rust SDK agree on the           and diverges from the
coinbase.md) is the                                                   │                            linear formula.                 canonical chain.
authoritative document.             The Python model               ─────────────────────
A developer following the           (sim/crypto.py) matches               │                       The discrepancy is               Cumulative supply
spec would produce                   the Rust linear formula,             │                       invisible to current nodes.     diverges at block 1.
incompatible code.                   not the spec's exponential.         ▼                       Only external implementations    Every subsequent block
                                                                  At h=H (half-life):            would detect it.                 produces different
                                                                  linear pays ~0.80 DRKW                                         nullifiers, different
                                                                  exponential pays ~6.92 DRKW    No automated cross-check         commitments.
                                                                  ~8.7x underpayment             between spec and code.
                                                                                                  No CI test comparing             The two chains are
                                                                  Over 4 years: linear            expected_reward() output         forever incompatible.
                                                                  total supply ~7.7M DRKW        against spec formula.
                                                                  exponential ~10.5M DRKW
```

---

## Recommended Controls (New Barriers)

### Immediate (CATASTROPHIC + HIGH)

1. **H-C1**: Gate `reorganize_to()` behind a compile-time or runtime check that prevents activation until WASM execution and supply chain update are implemented. Add a `#[allow(dead_code)]` annotation with a `// FIXME: HAZID H-C1` comment.

2. **H-C2**: Add `broadcast_block()` call after `accept_block` in `stratum_submit` and `mm_submit_solution`, matching the `miner_task` and `miner_mine_linear` pattern.

3. **H-C3**: Either implement the exponential formula in `blockchain.rs` (change the linear approximation) or update `consensus-coinbase.md` to document the linear schedule. Add a CI test comparing `expected_reward()` output against the spec formula.

4. **H-H1**: Replace `Ordering::Relaxed` with `Ordering::Acquire`/`Release` pairs on all consensus atomics (`target`, `accumulated_work`, `timestamps`). One-line change per site.

5. **H-H8**: Move `take_competing_blocks()` after `build_linear_coinbase()`/`generate_linear_block_template()` in all three call sites (`prepare_block`, `stratum_login`, `mm_get_aux_block`).

6. **H-H7**: Add `mp.add(tx.clone()).await` re-insertion loop to `miner_task` error paths, matching `miner_mine_linear`.

7. **H-C4/H-H3**: Replace `.unwrap()` with `?` propagation or `unwrap_or_else(|e| e.into_inner())` in consensus paths. Add `SerializationFailed` error variant to `LinearError` (DONE in hardening pass, verify coverage).

### Near-Term (MEDIUM)

8. **H-H4**: Restore `uncle_coin_set` from sled on restart.
9. **H-H9**: Use composite lock for stratum template+config updates.
10. **H-H10**: Re-admit disconnected-block transactions to mempool after reorg.
11. **H-H11**: Call `verify_cumulative_supply()` periodically (every N blocks) and log warnings on divergence.
12. **H-H12**: Gate non-ZK fallback path behind `#[cfg(debug_assertions)]`.

---

## Verification

```bash
# Confirm all consensus paths compile
RAYON_NUM_THREADS=10 cargo check -p dwow_chain -p dwowd -p dwow_mempool 2>&1 | grep "^error" | wc -l

# Run all consensus tests
RAYON_NUM_THREADS=10 cargo test -p dwow_chain --lib 2>&1 | grep "test result"
RAYON_NUM_THREADS=10 cargo test -p dwow_mempool --lib 2>&1 | grep "test result"

# Count hazards addressed vs remaining
echo "Total: 34 findings (4 CATASTROPHIC, 12 HIGH, 18 MEDIUM)"
echo "Previously addressed in hardening pass: H-C4 (serde fallback), H-H3 partial (some unwraps), H-M6 (aggregate), H-M9 (mempool sled)"
echo "Remaining: 4 CATASTROPHIC, ~10 HIGH, ~16 MEDIUM"
```
