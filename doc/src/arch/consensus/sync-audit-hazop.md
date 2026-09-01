# Miner Sync — Adversarial Audit + HAZOP

Status: **remediation in progress.** Changes A (block classification + reorg) and B (uncle reward
wiring + single reward source of truth) are implemented and verified; C/D/E remain. This is the
analysis-first record for the sync/consensus surface. Scope: the code that moves, validates, and
applies blocks on the pull-sync and relay paths.

## 1. Production-pattern baseline (what "correct" looks like)

Every finding below is judged against these mature-chain behaviours:

- **Bitcoin Core** — `IsInitialBlockDownload` (mine only when caught up, and "caught up" is a *local*
  property, not a peer-count property); `AcceptBlock` returns *already-known* vs *invalid* as distinct
  outcomes, and checks `LookupBlockIndex(hash)` **first** (zero work for a known block); `Misbehaving()` is a
  persistent, severity-graded score (not reset by one good block, not a binary ban); `CBlockUndo` is written
  **atomically with** the block so a disconnect can always reverse it; `ActivateBestChain` validates a
  candidate chain **before** disconnecting the tip.
- **Ethereum** — distinguishes "invalid block" (penalize) from "known/competing" (benign); peer badness is
  graded, not an all-or-nothing ban.
- **Monero** — `get_blocks` carries a block-id chain so the fork point is found in one round-trip; peers on a
  different `NETWORK_ID` are refused up front.
- **Polkadot (BABE/GRANDPA)** — fork choice runs at *every* block import (not only on a failed apply); lighter
  forks are stashed, not executed against the wrong state.

Two DarkWow-specific obligations follow from these: (a) a fresh L1 chain has few peers, so peer discipline
must tolerate a slow-but-honest peer and must not ban on slowness; (b) the genesis authority is the canonical
tip by construction and must mine without peer evidence.

## 2. HAZOP — guide-word mapping

Guide words applied to the sync state variables (`sync_state`, `peer_scores`, `peer_tips`, `local/peer
height`, `BlockConnectOutcome`, `total_reward`, the WASM overlay, `contracts_undo`).

| Guide word | Parameter | Deviation → finding |
|---|---|---|
| NO | peer_tips | No compatible tips → non-authority node sets `Behind` even when fully synced → M2.1 |
| NO | peer_scores | Timeout/empty response never scores → dead peer retried forever → H5 |
| NO | contracts_undo | Crash before undo stored → block never fully reversible → H4 |
| NO | liveness check | `has_full_node_peers` is session-type only → zombie channel counts as "peers available" → M6.1 |
| LESS | peer_scores | One good block wipes accumulated score → misbehaving peer never banned → H6 |
| MORE | peer_tips | Inflated (but valid) peer height → node stuck `Behind` forever → H7 |
| MORE | fork walk | Unbounded 1-block-at-a-time walk, no depth cap → H9 |
| MORE | varint | Unbounded continuation bytes → shift panic/UB → M7.1 |
| REVERSE | disconnect vs validate | Reorg disconnects canonical chain *before* validating competitor → C1 |
| OTHER THAN | block class | Duplicate / competing / invalid conflated; tip-duplicate re-runs WASM → H1, M2 |
| OTHER THAN | reward anchor | Host check vs WASM check on different quantities → H3 |
| OTHER THAN | peer predicate | Docker-gateway `contains` filter drops real peers → H8 |
| BEFORE | duplicate check | Duplicate check runs *after* full ZK verification → H1 (also M2) |
| BEFORE | undo capture | `contracts_undo` captured *after* commit → H4 |
| AFTER | disconnect | Disconnect does not reverse uncles / uncle_commitment_set / supply cache / fee_window → M7 |
| PART OF | uncle data | P2P relay sends `uncles=[]` → uncle-bearing block cannot propagate → H2 |
| AS WELL AS | equal-height fork | Fork choice only fires on a failed apply, not at import → M1.3 |

## 3. Consolidated findings (ranked)

### CRITICAL

- **C1 — Reorg disconnects before validating; work comparison is forgeable.**
  `consensus_linear.rs:173-301` + `block_acceptor.rs:625-692`. The fork walk fetches peer blocks one at a time
  with no validation (height-contiguity only), sums `chain_work` over peer-controlled `target` fields, and
  `reorganize_to_chain` disconnects every block down to the fork point **before** validating a single
  competing block. A malicious peer truncates a node's valid chain and parks it `Behind`. Precedent: Bitcoin
  `ActivateBestChain` validates before disconnect and never truncates on an invalid candidate.

### HIGH

- **H1 — Tip-duplicate (height == current) re-runs WASM and bans the relayer.** `block_acceptor.rs:263` only
  catches `height < current`; a relay of the current tip falls through to `execute_block` → `pow_reward_v1`
  "Duplicate commitment" → `linear_broadcast.rs:461-477` bans the relaying peer. The duplicate check is also
  not first (it runs after full ZK verification), and `linear_broadcast` bans on *any* `Err` without consulting
  `LinearError::consensus_phase()`. Precedent: Bitcoin `AcceptBlock` returns "known" via a single
  `LookupBlockIndex(hash)` before any work.
- **H2 — Uncle-bearing blocks cannot propagate over P2P.** `linear_broadcast.rs:427` passes `uncles=[]` into
  `accept_block`, so the host reward check computes `total_pin=0` and rejects any block whose
  `total_reward = expected − Σpin`; the producing miner is banned by every peer.
- **H3 — Host and WASM reward checks enforce different quantities; no anchor to uncle notes.**
  Host (`block_acceptor.rs:220-244`) checks `header.total_reward`; WASM (`entrypoint/mod.rs:1007`) checks the
  coinbase clear-input value. Nothing verifies `Σ UncleMintV1` pins match the claimed pins or that the coinbase
  note value equals `total_reward` — a miner can mint full base and emit no uncle notes (reward theft).
- **H4 — `contracts_undo` is captured after the atomic commit.** `block_acceptor.rs:377` vs `:384-387`. A crash
  in the window leaves a canonical block with no undo; a later disconnect restores only the 3 singletons, not
  the per-contract keys. Precedent: Bitcoin writes `CBlockUndo` atomically with the block.
- **H5 — Timeouts and empty responses never score a peer.** `consensus_linear.rs:675-679,815-820,462-464`. A
  dead/stale peer is retried every pass forever. (Note: the earlier "malice-only" fix over-corrected — the
  correct rule is *separate* dead-peer backoff/disconnect from malice scoring, not "never score slowness".)
- **H6 — One good block wipes the accumulated ban score.** `consensus_linear.rs:765,786` (`peer_scores.remove`
  on any `Ok`). A peer serving 2 invalid then 1 valid block can never reach the threshold of 3.
- **H7 — An inflated (but valid) peer height parks the node `Behind` forever.** `consensus_linear.rs:587-590`
  + `sync_boundary.rs:70-75` (only `u64::MAX` rejected). A lying tip height keeps `local < max_peer` forever.
- **H8 — Docker-gateway filter uses substring `contains`.** `linear_sync_client.rs:94,146,198`. Matches
  `172.18.0.10-19` etc., silently dropping real peers on the docker bridge.
- **H9 — Unbounded fork walk, one round-trip per height, no depth cap.** `consensus_linear.rs:190-232`. A
  stalling peer can pin a node for `local_height × 30s` per pass.

### MEDIUM

- **M1.3** Equal-height heavier fork never detected (fork choice only on failed apply).
- **M2.1** A fully-synced non-authority node with zero peers is stuck `Behind` forever ("no peers" ≠ "behind").
- **M2** Duplicate check is height-only (not hash) and runs after full ZK verification.
- **M2.3** CaughtUp reachable with a same-magic/different-genesis peer under the default
  `GenesisValidationMode::Off`.
- **M3.3** Ban is per-session and fully resets on CaughtUp; the peer is never disconnected from P2P.
- **M4.1** Pull loop treats `CompetingStored`/`UncleExtended` as chain progress and clears the peer's score.
- **M4** Lighter uncle-chain extension is rejected at WASM, never stored (`UncleExtended` dead path).
- **M5.1** A single empty-response peer ends the whole pull pass.
- **M5.2** Sync dials are sequential, unbounded, no dead-peer backoff.
- **M6.1** `has_full_node_peers` is session-type only (no liveness).
- **M7.1** `varint_decode` has no continuation bound → shift panic/UB.
- **M7.2** SyncServer has no handshake/idle read timeout; unlimited detached tasks.
- **M7.3** Client handshake read has no timeout.
- **M7** `disconnect_block` does not reverse the uncles tree, `uncle_commitment_set`, the supply in-memory
  cache, or fee_window state.
- **M8** Reorg recursion (`perform_reorg`) has no depth cap.
- **M10** Cumulative-supply overlay is a passive mirror with no host-side re-derivation.

### LOW

- GenesisAuthority::new() is a public unguarded constructor (type claim ceremonial); `skip_sync` sets CaughtUp
  with zero evidence; `WaitingForGenesis` never set in the peers-at-0 path; miner startup timeout logs only;
  peer-count logs conflate P2P vs sync peers.

## 4. Spec + model reconciliation required (`python-model-is-the-spec`)

Before any fix, `doc/src/arch/sync-protocol.md` and `contrib/model/sync_model.py` must state the correct
behaviour for: (a) already-known block as a distinct, benign, hash-based outcome (before any validation);
(b) the split of peer discipline into *malice* (score/ban) vs *deadness* (backoff/disconnect) vs *slowness*
(tolerate); (c) validate-before-disconnect reorg with a bounded fork walk and a PoW-anchored work metric;
(d) an equal-height fork-choice rule; (e) authority mines with zero peer evidence and a synced join node is
CaughtUp with zero peers; (f) atomic `CBlockUndo` capture; (g) symmetric disconnect.

## 5. Fixes (to be sequenced after spec/model)

The highest-leverage cluster, in order: H1+C1 (block classification + validate-before-disconnect) → H2+H3
(uncle reward wiring + single source of truth for the reward) → H4 (atomic undo) → H5+H6+H7 (peer discipline)
→ M7 (symmetric disconnect) → the remaining MEDIUM transport/robustness items.

Verification: `cargo check -p dwowd --tests` clean, then one `test_pipeline.sh --mode native --with-wallet 2`
run asserting node1 tracks node0 with no peer bans.

## 6. Resolution log (Management-of-Change)

Each change is closed only when the spec clause is written, the Python model is reconciled, the code is
implemented, and `cargo check -p dwowd --tests` (+ targeted unit test) is green.

| Change | Findings | Status | Precedent |
|---|---|---|---|
| A — block classification + reorg safety | C1, H1, H9, M1.3, M2, M8 | **Done** (commit `5c00d94ada`) | Bitcoin `AcceptBlock` known-vs-invalid + `ActivateBestChain` validate-before-disconnect |
| B — uncle reward wiring + single reward source of truth | H2, H3 | **Done** (this change) | Bitcoin/Zcash: coinbase spendable value is public & consensus-bound, never prover-asserted |
| C — atomic undo + symmetric disconnect | H4, M7 | **Done** (this change) | Bitcoin `CBlockUndo` written atomically with the block |
| D — peer discipline (malice/deadness/slowness split) | H5, H6, H7, M3.3, M6.1, M2.1 | **Done** (this change) | Bitcoin `Misbehaving()` graded, persistent; Monero NETWORK_ID refusal |
| E — transport robustness | M7.1–M7.4, M5.1, M5.2, M2.3, M4.1, M4, M10 | Pending | Monero block-id chain; Ethereum graded peer badness |

### Change B detail (H2 + H3)

**H2 — uncle data on the wire.** `BlockBroadcast` now carries `uncles: Vec<UncleBlock>` so a receiver can
recompute `Σ pin`; `broadcast_block(p2p, block, uncles)` and every caller updated.

**H3 — single reward source of truth (CRITICAL over-mint fix).** The prior design left the Mint_V2
`effective_value` witness unconstrained (only `range_check`), so a miner could set the spendable coinbase note
to the FULL base while also emitting uncle notes — over-minting by `Σ pin`, or emit no uncle notes at all
(reward theft). Fix:

- `mint.zk`: `constrain_instance(effective_value)` — the reduced spendable value is now a public input.
- Client (`TransferMintRevealed`, `PoWRewardRevealed`): 9 → 10 public inputs.
- `PoWRewardParamsV1` / `UncleMintParamsV1`: carry `effective_value` in the clear params (host-readable).
- `pow_reward_v1`: reject `effective_value > input.value`; `uncle_mint_v1`: reject `effective_value != value`.
- Host (`block_acceptor`): verify `coinbase.effective_value == header.total_reward` AND
  `Σ uncle.effective_value == Σ pin` — the spendable-note mass balance.
- Spec: `uncle_merkle.md` §"Spendable-note mass balance"; model `uncle_fork_model.py` already enforced the
  `canonical + Σ pin == base` invariant (reconciled, green).

Verification: `cargo check -p dwowd --tests` clean; `test_uncle_note_persisted_and_reversed` green;
`make -C src/contract/native_token all` regenerated `mint.zk.bin` + WASM with the 10-input circuit.

### Change C detail (H4 + M7)

**H4 — atomic `CBlockUndo`.** `accept_block` previously wrote `store.contracts_undo` as a **post-commit**
sled insert — a crash in that window left a canonical block whose contracts-tree writes could never be
reversed. `connect_block` now takes `contracts_undo: Option<Vec<u8>>` and writes it into the same cross-tree
sled transaction as the block/contracts/supply/commitment/nullifier sets; `accept_block` passes it in and the
post-commit insert is removed.

**M7 — symmetric disconnect.** `disconnect_block` now reverses every transition `connect_block` performed:

- a new `uncles_by_height` sled tree records each canonical block's uncle-header hashes at connect time;
  `disconnect_block` replays it to remove the displaced uncles from the `uncles` tree (a reorged-out uncle
  can now be re-included, closing the phantom-uncle fairness gap);
- the in-memory `uncle_commitment_set` is rolled back (`retain` at the displaced height);
- the cumulative-supply in-memory cache is rolled back via `supply_chain::rollback_cache` to the predecessor.
- `fee_window` is stateless per block (initialized `Default` and never mutated in `connect_block` — it is
  only persisted), so there is no per-block transition to reverse.

Spec: `sync-protocol.md` §19.4 (atomic undo) + §19.6 (symmetric disconnect).

### Change D detail (H5/H6/H8/M2.1/M3.3/M6.1)

- **H5 — deadness split from malice.** A new `dead_peers` counter (hoisted like `peer_scores`) increments
  on a tip-request timeout and is cleared on a successful tip; a peer with 3 consecutive dead passes is
  skipped in the round-robin, never scored as malice.
- **H6 — no score-reset-on-good.** A successful `accept_block`/reorg no longer wipes the peer's score;
  the score is graded and persistent.
- **M3.3 — no score-reset-on-CaughtUp.** `peer_scores.clear()` on CaughtUp removed; a peer that served 3
  invalid blocks stays skipped across sessions.
- **H8 — docker-gateway exact match.** The `contains("172.18.0.1")` filter matched `172.18.0.10`/`.100`;
  replaced with an exact `host_str()` comparison.
- **M2.1 — zero-peer CaughtUp.** A non-authority node that holds genesis and has no peers now returns
  `ProceedSolo` (CaughtUp) instead of `Retry` (permanent `Behind`).
- **M6.1 — liveness.** `filtered_peers`/`has_full_node_peers` now require `!channel.is_stopped()` so a
  zombie (session-established but dead) channel is not treated as a sync source.
