# Node Sync HAZOP + Guide-Word Study

This is the exhaustive Hazard and Operability study of DarkWow's **mining/observer node** sync
path — the client-side pull that a node uses to join and stay on the canonical chain. It is the
sibling of `sync-hazop.md` (which covered the wallet/connection layer and explicitly deferred this
path) and the study that produced the fixes in the node-sync remediation.

Authoritative specs: `sync-protocol.md`, `consensus/consensus.md` (§Fork Choice Rule, §Reorg Depth),
`consensus/node-startup-spec.md`. Production reference patterns: Bitcoin Core
(`ActivateBestChain`/`DisconnectTip`/`ConnectTip`, `IsInitialBlockDownload`), Monero chain sync,
geth `eth/downloader`. Uses SHALL / MUST / SHALL NOT / MUST NOT per RFC 2119.

All `file:line` citations are on `linear-master`.

---

## 1. System boundary and scope

- **In scope**: the node's block-sync + fork-resolution path, from peer discovery through block
  application to the mine gate:
  - `consensus_linear_init_task` — `bin/dwowd/src/task/consensus_linear.rs` (SyncClient for node).
  - `accept_block` / `perform_reorg` / `read_cumulative_from_overlay` — `bin/dwowd/src/block_acceptor.rs`.
  - `detect_reorg` / `disconnect_block` / `connect_block` / `competing_blocks` — `src/linear/src/chain_state.rs`.
  - `miner_task` (mine gate) — `bin/dwowd/src/lib.rs`.
  - `wait_for_peers_or_proceed` — `bin/dwowd/src/proto/linear_sync_client.rs`.
- **Out of scope** (unchanged, covered by `sync-hazop.md`): the wallet pull (`bin/dww`), the unified
  `SyncPeer`/`SyncServer` wire (`src/linear/src/sync_connection.rs`), tx relay and block broadcast.

## 2. Guide words and nodes

Guide words: NO / NOT / MORE / LESS / PART OF / AS WELL AS / REVERSE / OTHER THAN / EARLY / LATE.

| Node | Function | Files |
|------|----------|-------|
| N1 | Peer discovery + dial | `linear_sync_client.rs:221-288`, `consensus_linear.rs:247` |
| N2 | Tip collection → `max_peer_height` | `consensus_linear.rs:291-428` |
| N3 | Sync decision (`CaughtUp`/`Behind`/`Syncing`) | `consensus_linear.rs:432-722` |
| N4 | Block pull + apply | `consensus_linear.rs:453-635`, `block_acceptor.rs` |
| N5 | Fork detection + reorg | `chain_state.rs:1545-1595,1597+`, `block_acceptor.rs:247-253,368-456` |
| N6 | Retry / backoff / peer deprioritisation | `consensus_linear.rs:459-460,568-634,689-698` |
| N7 | Mine gate | `lib.rs:1251-1310` |

## 3. Findings (deviations, traced to file:line)

### F1 — EARLY `CaughtUp` (premature "caught up", the root cause)

- **Guide word**: EARLY (the node declares `CaughtUp` before it has any evidence of the best chain).
- **Cause**: `consensus_linear.rs:425-428` —
  ```rust
  let mut max_peer_height: BlockHeight = compatible_peers.iter()
      .map(|(_, pt)| pt.height)
      .max()
      .unwrap_or(local_height);
  ```
  The `unwrap_or(local_height)` makes "no usable peer tips" indistinguishable from "caught up at
  local height". The B1 guard (`:312-319`) only retries when `peer_tips.is_empty() && !sync_peers.is_empty()`;
  when `sync_peers` itself is empty (every `port+2` sync dial failed) or every tip request fails, the
  node falls through the sync skip (`:440` needs `max_peer_height > local_height`) to the
  `CaughtUp` branch (`:718-719`).
- **Consequence**: `miner_task` (`lib.rs:1264,1297`) is unblocked and mines a **divergent fork**
  (blocks 2–5 on its own tip). Once divergent, the cumulative supply commitment
  (`S_H = S_{H-1} + C_H`) differs from the canonical chain, and the node can never rejoin (F2/F3).
- **Spec violated**: `node-startup-spec.md` §2 ("mine gate is CaughtUp on the canonical chain";
  "A node must never mine while behind or on a divergent fork").
- **Production pattern**: Bitcoin `IsInitialBlockDownload()` returns true (no mining) until the best
  header chain is synced and within the time window; geth `syncing` is false only with a known best
  chain. "No peers / no tips" ⇒ IBD, never "done".
- **Fix**: `CaughtUp` requires positive evidence of a peer tip; empty `sync_peers`/`compatible_peers`
  ⇒ `Behind` (or `WaitingForGenesis` at height 0) + retry.

### F2 — NO reorg (fork never classified)

- **Guide word**: NO (reorg never occurs when the competing parent is not pre-stored).
- **Cause**: `detect_reorg` (`chain_state.rs:1545-1595`) requires the competing parent block to already
  be present in `competing_blocks.get(&current_height)` (`:1553-1571`). A divergent node never stores the
  canonical peer's block at the fork height (it mined its own), so the uncle-parent lookup returns
  `None` and `detect_reorg` returns `Ok(None)` at `:1571` — the heaviest-chain comparison (`:1586-1591`)
  is never reached.
- **Consequence**: `accept_block` falls through to WASM (`block_acceptor.rs:259`), where `pow_reward_v1`
  rejects the canonical block with `old_cumulative_commit does not match on-chain state` before
  `connect_block` can classify or store it.
- **Spec violated**: `consensus.md` §Fork Choice Rule (heaviest-chain wins); `node-startup-spec.md` §4
  (SHALL adopt the canonical heaviest chain).
- **Production pattern**: Bitcoin's reorg is triggered by chainwork on the *header* chain, not by
  whether a competing block happens to already be in memory; the missing segment is fetched.
- **Fix**: reorg decision by accumulated work on a fetched competing chain (F3/F5 below).

### F3 — PART OF fork-pivot fetch (pivot fetched, never stored)

- **Guide word**: PART OF (the reorg attempt is incomplete — it fetches the pivot then discards it).
- **Cause**: `consensus_linear.rs:579-617` (the "Gap #2" retry) fetches the pivot block and calls
  `accept_block` with the result discarded (`let _ = accept_block(...)` at `:596-599`). The pivot is
  never stored in `competing_blocks`, so the subsequent retry of the original block still sees
  `detect_reorg == None` and fails identically (`Retry after fork-pivot fetch still failed at height 6`).
- **Consequence**: the retry loop is pure spin — it cannot change the outcome.
- **Spec violated**: `consensus.md` §Reorg Depth (disconnect → connect competing → connect extension).
- **Production pattern**: Bitcoin fetches the missing block and *connects* it; it does not fetch-and-discard.
- **Fix**: fold into the general-depth reorg (F5) — fetch the competing chain, store/connect it, reorg.

### F4 — MORE retries (unbounded retry)

- **Guide word**: MORE (the node retries the same rejected block forever).
- **Cause**: `consensus_linear.rs:689-698` — on "sync incomplete" the outer loop stores `Behind`,
  sleeps 2 s, and `continue`s with no cap or escalation. The outer loop (`:203 … :731`) has no bound.
- **Consequence**: the observed livelock — `node1` at height 5 vs `node0` at 108+, repeating every 2 s
  indefinitely, never recovering.
- **Spec violated**: `sync-protocol.md` §18.1 ("SHALL NOT … retry forever").
- **Production pattern**: Monero drops a peer after N bad blocks; Bitcoin backs off and disconnects a
  misbehaving peer. No implementation tight-loops on a permanently bad block.
- **Fix**: bound retries and escalate (backoff + `error!`); never `continue` unconditionally.

### F5 — REVERSE deprioritisation (failure counter resets each cycle)

- **Guide word**: REVERSE (the "deprioritise after 3 failures" counter is reset instead of accumulated).
- **Cause**: `consensus_linear.rs:459-460` — `channel_failures` is re-created inside the
  `if max_peer_height > local_height` block, so it is wiped on every outer-loop cycle; the §13.3
  "3 consecutive failures → deprioritise for the sync pass" never persists across passes.
- **Consequence**: the failing peer is retried every cycle; combined with F4 this is a permanent
  tight loop.
- **Spec violated**: `sync-protocol.md` §13.3, §14 (quarantine a barb-violating peer, not retried forever).
- **Fix**: persist per-peer failure scores across cycles (or graduate to §14 quarantine).

## 4. Guide-word matrix

| Node × guide word | Deviation | Cause (`file:line`) | Safeguard (spec) | Recommendation |
|---|---|---|---|---|
| N2 × NO | No tips yet `max_peer_height = local_height` | `consensus_linear.rs:428` | §2, sync-protocol §18 | F1 — require positive tip evidence |
| N2 × EARLY | `CaughtUp` at local height | `consensus_linear.rs:718-719` | §2 | F1 |
| N3 × OTHER THAN | Behind→CaughtUp without progress | `consensus_linear.rs:701-722` | §2 | F1 |
| N4 × PART OF | Pivot fetched, result discarded | `consensus_linear.rs:596-599` | consensus §Reorg Depth | F3 |
| N5 × NO | `detect_reorg` returns `None` | `chain_state.rs:1571` | consensus §Fork Choice | F2 |
| N5 × LESS | Reorg only 1-deep / pre-stored | `chain_state.rs:1548,1553` | consensus §Reorg Depth | F5 — general depth |
| N6 × MORE | Unbounded retry | `consensus_linear.rs:689-698` | sync-protocol §18.1 | F4 |
| N6 × REVERSE | Failure counter reset | `consensus_linear.rs:459-460` | sync-protocol §13.3 | F5 |

## 5. Spec↔code citation drift (noted, not a functional bug)

- `consensus_linear.rs:311` comments cite "§4.2.4" of `sync-protocol.md`, which has no such section.
  The comment should reference the actual clause (the B1 no-tip guard).

## 6. Resolution status (WYSIWYG)

| Finding | Status | Fix |
|---------|--------|-----|
| F1 — premature `CaughtUp` | FIXED | `CaughtUp` requires positive peer-tip evidence (`consensus_linear.rs`) |
| F2 — reorg never classified | FIXED | reorg by accumulated work on a fetched competing chain (`chain_state.rs`) |
| F3 — pivot fetch discarded | FIXED | folded into general-depth reorg (`consensus_linear.rs`) |
| F4 — unbounded retry | FIXED | bounded retry + backoff (`consensus_linear.rs`) |
| F5 — deprioritisation reset | FIXED | persistent per-peer failure score (`consensus_linear.rs`) |
| Citation drift | FIXED | corrected comment reference |

## 7. Python model (executable spec)

`contrib/model/chain_model.py:532` (`reorganize_to`) and
`contrib/model/chain_validation_model.py:1199` (`reorganize_to`) are the executable specification of
fork selection. The Rust `reorganize_to_chain` SHALL conform to them (walk back to the common
ancestor, disconnect local blocks, connect the peer's heavier segment). A model test reproducing the
node1 divergence (diverge at h=2, peer at h=6, reorg converges) is the regression guard.
