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
  - `consensus_linear_init_task` (pull loop) and `reorg_to_heavier_chain` (fork resolution) —
    `bin/dwowd/src/task/consensus_linear.rs`.
  - `accept_block` / `read_cumulative_from_overlay` — `bin/dwowd/src/block_acceptor.rs`.
  - `detect_reorg` / `disconnect_block` / `connect_block` / `competing_blocks` — `src/linear/src/chain_state.rs`.
  - `miner_task` (mine gate) — `bin/dwowd/src/lib.rs`.
- **Out of scope** (unchanged, covered by `sync-hazop.md`): the wallet pull (`bin/dww`), the unified
  `SyncPeer`/`SyncServer` wire (`src/linear/src/sync_connection.rs`), tx relay and block broadcast.

## 2. Guide words and nodes

Guide words: NO / NOT / MORE / LESS / PART OF / AS WELL AS / REVERSE / OTHER THAN / EARLY / LATE.

| Node | Function | Files |
|------|----------|-------|
| N1 | Peer discovery + dial | `linear_sync_client.rs:204-257` (`dial_sync_peers`, ≤ 8 peers), `consensus_linear.rs:342` |
| N2 | Tip collection → `max_peer_height` | `consensus_linear.rs:344-355` |
| N3 | Sync decision (`CaughtUp`/`Behind`) | `consensus_linear.rs:474-478` |
| N4 | Block pull + apply | `consensus_linear.rs:358-471`, `block_acceptor.rs:77+` |
| N5 | Fork detection + reorg | `consensus_linear.rs:137-296` (`reorg_to_heavier_chain`), `chain_state.rs:1455+` |
| N6 | Retry / pacing / peer rotation | `consensus_linear.rs:331` (30 s tick), `366-372`, `460-471` |
| N7 | Mine gate | `lib.rs:1259-1278` |

## 3. Findings (deviations, traced to file:line)

Each finding states the deviation the study examined, then its disposition in the current
implementation.

### F1 — EARLY `CaughtUp` (premature "caught up", the root cause)

- **Guide word**: EARLY (the node declares `CaughtUp` before it has any evidence of the best chain).
- **Hazard**: "no usable peer tips" is treated as "caught up at local height", so `miner_task`
  (`lib.rs:1271`) is unblocked and mines a **divergent fork** (blocks on its own tip). Once
  divergent, the cumulative supply commitment (`S_H = S_{H-1} + C_H`) differs from the canonical
  chain, and the node must reorg to rejoin (F2).
- **Disposition — closed**: `max_peer_height` starts at 0 (`consensus_linear.rs:345`) and the mine
  gate is `mine = caught_up && (authority || !sync_peers.is_empty())` (`:475`): a peerless
  non-authority node never mines. The loop re-ticks every 30 s (`:331`) and re-dials peers each pass.
- **Spec**: `node-startup-spec.md` §2 ("mine gate is CaughtUp on the canonical chain"; "A node must
  never mine while behind or on a divergent fork"); `sync-protocol.md` §18.1.1.
- **Production pattern**: Bitcoin `IsInitialBlockDownload()` returns true (no mining) until the best
  header chain is synced and within the time window; geth `syncing` is false only with a known best
  chain. "No peers / no tips" ⇒ IBD, never "done".

### F2 — NO reorg (fork never classified)

- **Guide word**: NO (reorg never occurs when the competing parent is not pre-stored).
- **Hazard**: the competing parent block is not already present locally (a divergent node mined its
  own block at the fork height), so the fork is never classified and the canonical block is rejected.
- **Disposition — closed**: on an `accept_block` failure that signals a fork, the sync path calls
  `reorg_to_heavier_chain` (`consensus_linear.rs:433`), which fetches the competing chain segment
  from the peer ancestor-by-ancestor with PoW validation (`:172`), decides by accumulated work —
  reorg only if the competing chain is **strictly heavier** (`:245`) — and then disconnects the local
  blocks and connects the peer's chain (`:285-293`).
- **Spec**: `consensus.md` §Fork Choice Rule (heaviest-chain wins); `node-startup-spec.md` §4
  (SHALL adopt the canonical heaviest chain).
- **Production pattern**: Bitcoin's reorg is triggered by chainwork on the *header* chain, not by
  whether a competing block happens to already be in memory; the missing segment is fetched.

### F3 — PART OF fork-pivot fetch (fetched, never stored)

- **Guide word**: PART OF (the reorg attempt is incomplete — it fetches the pivot then discards it).
- **Hazard**: a fork-pivot fetch whose result is discarded cannot change the outcome of the next
  retry — the retry loop is pure spin.
- **Disposition — closed**: the fetch-and-discard path no longer exists. `reorg_to_heavier_chain`
  validates first, then disconnects and **connects** the fetched competing blocks
  (`consensus_linear.rs:226-227,285-293`); nothing fetched is dropped.
- **Spec**: `consensus.md` §Reorg Depth (disconnect → connect competing → connect extension).
- **Production pattern**: Bitcoin fetches the missing block and *connects* it; it does not fetch-and-discard.

### F4 — MORE retries (unbounded retry)

- **Guide word**: MORE (the node retries the same rejected block forever).
- **Hazard**: a sync loop that retries a rejected block with no cap or escalation livelocks —
  e.g. `node1` at height 5 vs `node0` at 108+, repeating forever and never recovering.
- **Disposition — closed**: the outer loop is 30 s-paced (`consensus_linear.rs:331`); a pass breaks
  when nothing progressed (`:460-471`); a per-peer request failure `continue`s to the next peer
  (`:366-372`). There is no tight loop.
- **Spec**: `sync-protocol.md` §18.1 ("SHALL NOT … retry forever").
- **Production pattern**: Monero drops a peer after N bad blocks; Bitcoin backs off and disconnects a
  misbehaving peer. No implementation tight-loops on a permanently bad block.

### F5 — REVERSE deprioritisation (no persistent per-peer score)

- **Guide word**: REVERSE (the "deprioritise after N failures" state is not accumulated across passes).
- **Hazard**: a failure counter re-created each outer-loop cycle is wiped on every pass, so a
  failing peer is retried every cycle; combined with an unbounded retry (F4) this is a permanent
  tight loop.
- **Disposition — partially resolved**: the tight loop is closed — no counter exists at all and
  every pass is 30 s-paced (F4). **Residual**: `sync-protocol.md` §13.3's peer discipline ("a single
  persistent score (Bitcoin Core `Misbehaving()`)"; a peer that serves an invalid block is
  disconnected) is not yet implemented in `consensus_linear.rs`: a peer that serves a bad block is
  skipped mid-pass and re-dialed fresh each pass.
- **Spec**: `sync-protocol.md` §13.3.
- **Production pattern**: Bitcoin Core `Misbehaving()` — one persistent per-peer score, disconnect on
  invalid block.

## 4. Guide-word matrix

| Node × guide word | Deviation | Cause (`file:line`) | Safeguard (spec) | Recommendation |
|---|---|---|---|---|
| N2 × NO | No tips → `max_peer_height = 0` | `consensus_linear.rs:345` | node-startup-spec §2 | F1 — mine gate requires peers |
| N2 × EARLY | `CaughtUp` without peer evidence | `consensus_linear.rs:475` | node-startup-spec §2, sync-protocol §18.1.1 | F1 |
| N3 × OTHER THAN | Behind→CaughtUp without progress | `consensus_linear.rs:474-478` | node-startup-spec §2 | F1 |
| N4 × PART OF | Fork-pivot fetch discarded | `consensus_linear.rs:285-293` (connect path) | consensus §Reorg Depth | F3 — connect, don't discard |
| N5 × NO | Fork never classified | `consensus_linear.rs:433` (reorg on apply failure) | consensus §Fork Choice | F2 — reorg by accumulated work |
| N5 × LESS | Reorg limited depth | `consensus_linear.rs:137-296` (general depth) | consensus §Reorg Depth | F2 |
| N6 × MORE | Unbounded retry | `consensus_linear.rs:331,460-471` | sync-protocol §18.1 | F4 — 30 s pacing |
| N6 × REVERSE | No per-peer failure score | `consensus_linear.rs:342` (fresh dial each pass) | sync-protocol §13.3 | F5 — residual |

## 5. Resolution status (WYSIWYG)

| Finding | Status | Fix |
|---------|--------|-----|
| F1 — premature `CaughtUp` | FIXED | mine gate `caught_up && (authority || !sync_peers.is_empty())` (`consensus_linear.rs:475`) |
| F2 — reorg never classified | FIXED | `reorg_to_heavier_chain` by accumulated work (`consensus_linear.rs:137-296`) |
| F3 — pivot fetch discarded | FIXED | fetched competing chain validated then connected (`consensus_linear.rs:226-227,285-293`) |
| F4 — unbounded retry | FIXED | 30 s outer tick + per-pass progress break (`consensus_linear.rs:331,460-471`) |
| F5 — deprioritisation reset | PARTIALLY RESOLVED | tight loop closed (30 s pacing); §13.3 persistent per-peer score not yet implemented |

## 6. Python model (executable spec)

`contrib/model/chain_model.py` (`reorg_to_heavier_chain`, `chain_model.py:556`) and
`contrib/model/chain_validation_model.py` (`reorg_to_heavier_chain`, `:1198`) are the executable
specification of fork selection. The Rust `reorg_to_heavier_chain` + `activate_best_chain` SHALL
conform to them (walk back to the common ancestor, disconnect local blocks, connect the peer's
heavier segment). The regression guard is `test_temporary_divergence_then_reorg`
(`chain_validation_model.py:2186`): two nodes share genesis and block 2, node1 goes offline and
diverges at height 3, node0 pulls ahead, and node1 reorgs onto the heavier chain.
