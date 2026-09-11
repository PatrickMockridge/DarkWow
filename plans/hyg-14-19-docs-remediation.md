# HYG-14…19 — Documentation Hygiene: Genesis, Consensus & Wallet

**Date:** 2026-09-11 · **Base:** `linear-master` @ d3dd07a68 · **Status:** HYG-14 ✅ applied (fab1e9a2a, 32636852e) · HYG-15 ✅ applied (c7f1a94cf) · HYG-16 ✅ applied (063aeadff) · HYG-17 ✅ applied (5f3efe2b2) · HYG-18 ✅ applied (75183565f) · HYG-19 ✅ applied (0b553a52e; supply_chain desktop verify outstanding) · HYG-20 ✅ applied (this batch)

## Method

Three independent doc-vs-code reviews (genesis, consensus, wallet) plus a central
book-structure and link audit. Every finding carries code-side file:line evidence.
Finding IDs used below: **G**=genesis, **C**=consensus, **W**=wallet, **S**=structure,
**L**=links.

## Overall assessment

The deep specs are in better shape than expected: `genesis.md`/`consensus-coinbase.md`
contract IDs, deployment order, keys.toml ceremony, seeds, magic bytes and maturity
constants all verify against code, and the normative consensus docs were genuinely
rewritten for `reorg_to_heavier_chain`. The damage concentrates in:

1. **Consensus-critical numbers that are wrong** — INITIAL_REWARD stated as 1.383 DRKW
   (it is ~13.84 — a 10× unit error), the emission formula written as `2^(-h/H)` when
   the code (and its own docstring, incorrectly) uses `2^(-(h-1)/H)`, a 4×-too-hard
   `initial_target` in a config example, and a "21M supply cap" that the code
   explicitly says is *not* a cap.
2. **Fossil layers in the middle docs** — `chain_architecture.md`, HAZID/sync side
   docs, docker READMEs and both python models still describe pre-rewrite artifacts
   (`src/validator/`, `wait_for_peers_or_proceed`, `insert_validated_block`, sled
   7-tree claim, zero-reward genesis, `depth`/`pin_offered` era).
3. **Wallet operational docs one generation behind** — they predate the `daemon`
   command + unix-socket RPC, the `darkwow` launcher/`account` CLI, and the SQLite
   consolidation (sled trees, `addresses` table, `seeds` key all gone).
4. **Book rot** — 9 in-scope docs absent from `SUMMARY.md` (6 with zero inbound
   links), two stale index files, and ~30 broken links in 4 systematic classes.

No UNVERIFIED/FIXME/TODO markers remain in any in-scope doc file (they survive only
as code comments, already tracked in the desktop runbook).

---

## HYG-14 — Consensus-critical numeric corrections (do first)

Wrong numbers users would copy; each is a one-line-to-one-section fix.

| # | File | Change | Src |
|---|------|--------|-----|
| 1 | `doc/src/arch/consensus-coinbase.md` §17.7 | `INITIAL_REWARD` "(1.383 DRKW)" → "(~13.84 DRKW)"; cite `src/sdk/src/blockchain.rs:865 (reward::INITIAL_REWARD)` not the dead `:606` | G1 |
| 2 | `doc/src/arch/consensus-coinbase.md` §4.2, §4.4 | Emission formula → `R(h) = R₀ × 2^(-(h-1)/H)` for h ≥ 2 (R(1) = R₀, R(0) = 0), floor at `TAIL_REWARD`; recompute the R(2) example and any cumulative-supply figures | G2 |
| 3 | `src/sdk/src/blockchain.rs` docstring (code comment) | Same formula fix in `expected_reward`'s doc comment — it currently states `2^(-h/H)` while the implementation uses `exp = height - 1` | G2 |
| 4 | `doc/src/arch/consensus-coinbase.md` §10 | Config example `initial_target = 16777215` → `268435455 # 0x0FFFFFFF` | G3 |
| 5 | `contrib/docker/darkwow-testnet/README.md` params table | "Initial difficulty 255 (auto-adjusting)" → "Initial target 268435455 (0x0FFFFFFF, auto-adjusting)" | G4 |
| 6 | `doc/src/arch/consensus-coinbase.md` §4.1 | "Supply cap 21,000,000 DRKW — Same as Bitcoin" → "Reference supply 21,000,000 DRKW (tail-emission reference, NOT a hard cap; perpetual ~0.80 DRKW/block tail)" | G8 |
| 7 | `doc/src/arch/consensus/consensus.md` §Reorg Depth | State `MAX_REORG_DEPTH = 100` (`bin/dwowd/src/task/consensus_linear.rs:128`) | C20 |

Single-sourcing (fixes the four-copy drift of the same emission numbers —
`genesis.md`, `consensus-coinbase.md` §4, root `README.md`, docker README):

| # | File | Change | Src |
|---|------|--------|-----|
| 8 | `doc/src/arch/genesis.md` | Keep the full emission/supply numbers here as the canonical doc section; others link to it; note `sim/crypto.py` as the executable source of truth | G16 |

## HYG-15 — Book structure & link rot

### Structure

| # | File | Change | Src |
|---|------|--------|-----|
| 1 | `doc/src/SUMMARY.md` | "Sync Module" → add `sync-protocol.md`, `sync-conformance.md`, `sync-hazop.md`, `sync-red-team-audit.md` | S1 |
| 2 | `doc/src/SUMMARY.md` | Consensus section → add `node-startup-spec.md`, `sync-audit-hazop.md`, `node-sync-hazop.md`, `l1-capability-tests-phase-trace.md`, `l1-capability-write-path-trace.md` (mark the two traces as historical snapshots) | S1 |
| 3 | `doc/src/SUMMARY.md` | Audit Reports → add `arch/audit/l1-capability-tests-phase-hazop.md`, `arch/audit/l1-write-path-hazop.md` | S2 |
| 4 | `doc/src/arch/audit/README.md` | Add the two l1 HAZOP docs to the index | S2 |
| 5 | `doc/src/arch/consensus/README.md` | Index all 17 files in the directory (missing: safety, fee-spec, transfer-spec, node-startup-spec, sync-audit-hazop, node-sync-hazop, both l1 traces) | S3,C18 |
| 6 | `doc/src/arch/README.md` | Directory tree: drop the 4 removed redirect stubs (`pipeline.md`, `test_harness_guide.md`, `genesis_harness.md`, `localnet_contract_testing.md`); drop `legacy/wallet.md` + `legacy/consensus_dag.md` (contradicts its own line 89); expand `consensus/` tree to the real files; dedupe the double `dao_escrow.md` line | S4 |

### Broken links — by class

| # | Class | Files → fixes | Src |
|---|-------|---------------|-----|
| 7 | Wrong relative paths | `about/for-dummies.md`→`../contracts.md`; `consensus/consensus.md`→`../../about/…`, `fee-spec.md`, `../sync-protocol.md`, `../type-system.md`; `consensus-coinbase.md`→`type-system.md`; `fee-spec.md`→`../type-system.md`, `../ocap.md`; `transfer-spec.md`→`../consensus-coinbase.md`; `wallet-vs-daemon.md`→`type-system.md`; `building_sdks_apps.md`→`../arch/sc/tx-lifetime.md`; `slashing.md`→`../contract/dao_escrow.md` and delete `./economic_security.md`+`./trust_models.md`; `misc/faq.md`→`nodes/tor-guide.md`; fix malformed `intro.md/#community` in `misc/faq.md` + `start-here.md` | L6 |
| 8 | "N. Title" slugs (`n--title`, not `n-title`) | `type-system.md` anchors in `sync.md`, `sync-protocol.md`, `scaling.md`, `wallet-vs-daemon.md`; `sync-red-team-audit.md`→`sync-protocol.md#17--…`; `testnet/merge-mining.md`→`consensus-coinbase.md#12--mining-network-architecture` | L2 |
| 9 | `&` in headings (3 dashes) | `consensus-coinbase.md`→`consensus.md#execution-ordering---atomicity-layers`; `consensus.md`→`uncle_merkle.md#uncle-minting---maturity` | L3 |
| 10 | `→` in headings | `dev/testing/wallet-testing.md`→`level-3-localnet.md` anchor (slug has 3 dashes around each arrow) | L4 |
| 11 | Removed sections | `consensus-coinbase.md#anchoring-finality-gadget` (from `consensus.md`, `caribina.md`, `testnet/merge-mining.md`) → repoint or delete; `#merge-mining-competition` (from `testnet/merge-mining.md`); `wallet.md#p2p-network-connectivity` (from `wallet-vs-daemon.md`); `dwowd_jsonrpc.md#merge-mining-xmr` (from `testnet/merge-mining.md`); `sync-protocol.md#15/#16` (from `sync-conformance.md`, `sync-red-team-audit.md`) → point at §12 Conformance / §13 Async Production Logic | L5 |
| 12 | Source-code links off by one level | `consensus/consensus.md` ×5 → 4 ups; `consensus-coinbase.md` ×8 → 3 ups; `stratum.md` ×1 → 4 ups. (Fix depth for GitHub; note these can never resolve in the rendered mdbook — alternatively convert to inline code paths.) | L1 |
| 13 | `consensus/linear_zkvm.md`→`../wallet_scanning.md` | Target file doesn't exist anywhere → point at `../wallet.md` (or delete the link) | L6 |

### HYG-15 execution notes (2026-09-11)

All 13 rows applied. Verification: custom python link-checker (absolute-path
resolution + exact pulldown-cmark slug emulation + proper fenced-code parser)
over the in-scope set — **zero broken links remain in scope** (the only flag,
`start-here.md → ../index.html`, is valid: it is the rendered book root).

Repoint decisions for removed sections (row 11):
- `#anchoring-finality-gadget` → `arch/caribina.md` (Caribina is the finality
  widget doc; from `consensus.md`, `caribina.md`, `testnet/merge-mining.md`)
- `#merge-mining-competition` → `arch/merge-mining.md#mining-competition-model`
- `wallet.md#p2p-network-connectivity` → `arch/sync.md#p2p-protocol`
- `dwowd_jsonrpc.md#merge-mining-xmr` → `arch/consensus/merge-mining-ffi.md#3--p2pool-bridge-protocol--mm_rpc`
- `sync-protocol.md#16` → `#12--conformance`; `#15` → `#11--reuse` (ban-policy
  content now lives there); `#1` → `#1--the-sync-process--ρ-calculus` (ρ kept by
  pulldown-cmark, unlike the old link); `#17` → `#17--wallet-follows-the-longest-chain`
- `level-3-localnet.md#coinbase-reward-forwarding` → `consensus-coinbase.md#13--wallet-integration---user-sovereignty`
  (the section itself is now "Removed", §13 carries the mechanics)

Beyond the table: same-class fixes in `p2p-network.md` (×2), `legacy/event_graph.md`
(×2), `zk/post-quantum-proving-system.md`, `block-explorer.md` (×2),
`python-simulations.md` (consensus.md `#supply-audit-capability`), and `ai-index.md`
(×4 — links to the removed redirect stubs now point at `level-2-heavyweight.md`,
`genesis.md`, `level-3-localnet.md`).

### HYG-16 execution notes (2026-09-11)

All 9 plan items applied:
1. **genesis.md Genesis Block table** [G5, G19, G20]: `commitment_merkle_root` and
   `nullifier_root` rows now say `[0u8; 32]` at genesis with a "Decorative roots"
   note (never computed or verified by the acceptor; `nullifier_root` is a blake3
   root, not an SMT). Added 10 missing header rows (`version`, `merkle_root`,
   `uncle_merkle_root`, `randomx_key` = `blake3(height.to_le_bytes())`, `miner`,
   `anchor_monero_height/hash`, `finality_flags`, `fee_window_flags`, `pow_source`).
2. **consensus-coinbase.md** [G5, G14]: 7 host-nullifier "SMT" phrasings → "host
   nullifier set" (contract-SMT mentions left — those are the real `nullifiers_db`
   SMT). §17.6 cite fixed `src/linear/src/lib.rs:56` → `:70`; check cite added
   (`src/linear/src/chain_state.rs:1055`).
3. **Docker README** [G12, G9]: `FORWARD_DESTINATION` row marked vestigial (captured
   in entrypoint.sh:54, passed through compose, no consumer — coinbase binds to the
   declared key via `NODE_NAME` + `--keys keys.toml`); the misleading
   `FORWARD_DESTINATION=... ./test_pipeline.sh` example removed; prerequisites
   bullet rewritten around the declared mining key; "four WASM contracts (29 …)"
   → "all 32 WASM contracts" (Dockerfile builds 32).
4. **testnet-mining.md** [G13, C6]: fake `threshold`/`pow_target`/`recipient`/
   `txs_batch_size`/`skip_fees` keys removed; real `[network_config."…".pow]`
   `target_block_time = 120` section added; Step 1 rewritten ("Declare a Mining
   Key" — `darkwow account generate` + keys.toml + `NODE_NAME`); Step 4 command
   now `NODE_NAME=node0 dwowd -c dwowd_config.toml --keys keys.toml`; troubleshooting
   item repointed.
5. **genev README** [G15, G17]: full rewrite — genevd is the event graph daemon
   (JSON-RPC `tcp://127.0.0.1:28880`, methods `add`/`list`/`eg_get_info`/
   `dnet_subscribe_events`/`dnet_switch`/`deg_*`), genev is the `add <nick> <title>
   <text>` / `list` CLI; documents `script/tmux_sessions.sh` (4 daemons + 4 CLIs).
6. **genev_config.toml**: `#replay_datastore = "…/replayed_darkirc_db"` residue →
   `replayed_genev_db` (matches the `--replay-datastore` default in genevd).
7. **merge_mining_model_README.md** [G10, G18]: `src/validator/*` mapping table
   rewritten — `reorg_to_heavier_chain` (bin/dwowd/src/task/consensus_linear.rs:137),
   `compute_reward` (src/linear/src/block.rs:514), `expected_reward`
   (src/sdk/src/blockchain.rs:924); model-only helpers labeled `(model-internal)`;
   broken `mining-tokenomics.md` links repointed to `merge-mining.md#mining-competition-model`
   + `caribina.md`.
8. **merge_mining_model.py** docstring: source-map block updated (comment-only —
   no behavior change, no re-run needed on this VM).
9. Same-file residue scan: no history narration left in the batch.

User style directive applied this batch and recorded: docs describe code as-is
now — no "legacy"/"removed"/"predates" narration. (Open question for HYG-19: the
model's `block_rank` fork-choice internals still diverge from the current
heaviest-chain Rust path — model behavior untouched here.)

### HYG-20 — Repo-wide link sweep (follow-up, out of scope for HYG-15)

Full-repo scan (all 268 docs) leaves **188 out-of-scope broken links**, dominated by:
- `contract/*.md → safety.md` (target lives in `dev/contracts/`, ~35 files)
- `arch/audit/comprehensive-security-audit.md` — ~48 source-code links at wrong
  depth (historical snapshot; decide fix vs. mark-as-historical)
- `dev/contracts.md`, `dev/contracts/safety.md`, `dev/testing/*` — wrong-depth
  source/arch links; `promissory_note_intermediaries.md` ×10
- `arch/zk/*`, `arch/quantum-os.md`, `arch/privacy-model.md` — wrong-depth
  `proofs/` + `src/` links; `arch/monero.md` → `atomic_swap.md` (file absent)
- `misc/darkirc/darkirc.md`, `testnet/node.md` → `../index.html#build` (book-root
  anchor), `zkas/writing-zk-proofs.md` → bare anchor

## HYG-16 — Genesis / coinbase / supply content sweep

| # | File | Change | Src |
|---|------|--------|-----|
| 1 | `doc/src/arch/genesis.md` Genesis Block table | `commitment_merkle_root` / `nullifier_root` → "all-zeros `[0u8;32]` at genesis; not verified or updated by the acceptor" (code: `bin/dwowd/src/lib.rs:558-559`); add one sentence that the roots are currently decorative | G5,G20 |
| 2 | `doc/src/arch/genesis.md` Genesis Block table | Add `miner = [0u8;32]`, `pow_source = Native`, plus the missing header rows (`randomx_key`, `uncle_merkle_root`, `anchor_monero_*`, `finality_flags`, `fee_window_flags`) | G19 |
| 3 | `doc/src/arch/consensus-coinbase.md` | Drop "SMT" phrasing for the nullifier root (code says "blake3 root over the nullifier set, not an SMT"); refresh §17 file:line cites (`COINBASE_MATURITY` at `src/linear/src/lib.rs:70`, entrypoint ranges) | G5,G14 |
| 4 | `contrib/docker/darkwow-testnet/README.md` | FORWARD_DESTINATION row → delete or mark "captured for compatibility; ignored" (entrypoint captures but never applies it) | G12 |
| 5 | `contrib/docker/darkwow-testnet/README.md` | "four WASM contracts" → 32 (Dockerfile builds the full `make all` set) | G9 |
| 6 | `doc/src/testnet/testnet-mining.md` | Remove `recipient` key (not parsed; coinbase pays the node's declared key); replace `threshold`/`pow_target`/`txs_batch_size` with the real `[network_config."…".pow] target_block_time` key | G13,C6 |
| 7 | `bin/genev/README.md` | Rewrite for the actual tool (a generic `GenEvent` JSON-RPC viewer at `tcp://127.0.0.1:28880` — no genesis inspection exists); fix "Building: make" → per-subdir make or `cargo build -p genevd -p genev-cli` | G15,G17 |
| 8 | `contrib/docker/darkwow-testnet/merge_mining_model_README.md` | Rewrite the 1:1 mapping table against `reorg_to_heavier_chain` (`bin/dwowd/src/task/consensus_linear.rs:137`) and `src/linear` types; drop the `src/validator/` claims; repoint the two broken `mining-tokenomics.md` links → `doc/src/arch/consensus-coinbase.md` | G10,G18 |
| 9 | (code) `bin/genev/script/*.toml` | Remove darkirc residue (`replayed_darkirc_db`) from genevd config | G15 |

## HYG-17 — Consensus content sweep

| # | File | Change | Src |
|---|------|--------|-----|
| 1 | `doc/src/arch/consensus/chain_architecture.md` | Replace the `get_next_work_required` snippet (self-declared-target attack surface) with the real signature `(&self, store: &LinearStore, height) -> Result<BlockTarget,_>` + cache-walk-recompute description (`src/linear/src/consensus.rs:358-424`) | C1 |
| 2 | `doc/src/arch/consensus/chain_architecture.md` | Sled trees 7 → 12 (`src/linear/src/store.rs:52-81`); refresh `MiningState` diagram (`sync_complete` gone, `mm_jobs` = `HashMap<JobId,…>`, add `template_height`/`miner_config`); `SyncState` = only `CaughtUp|Behind` | C9 |
| 3 | `doc/src/arch/consensus/chain_architecture.md` | Delete the `ConsensusPhase` paragraph (enum + `err.phase()` removed); mark the IBD row implemented (`consensus_linear_init_task`, caught-up + separate mining gate) | C10 |
| 4 | `doc/src/arch/consensus/consensus.md` | `BlockConnectOutcome` → five variants, `AlreadyKnown`, no `ReorgAvailable`; point exhaustive-match example at `ReorgSignal::{Heavier,Lighter,None}` | C2 |
| 5 | `doc/src/arch/consensus/consensus.md` | `GenesisAuthority` → flag-gated (`CREATE_GENESIS`), `new()` infallible; drop "from_key(secret)" and "(Change 3 planned)" | C3 |
| 6 | `doc/src/arch/consensus/consensus.md` | Phase 0.5 → "FeeV2 (0x08) fee calls" (FeeV1 removed) | C15 |
| 7 | `doc/src/arch/consensus/node-startup-spec.md` | Step 2 → replace `wait_for_peers_or_proceed` with the inline gate `caught_up = height >= max_peer_height; mine = caught_up && (authority || !sync_peers.is_empty())` | C4 |
| 8 | `doc/src/arch/consensus/node-startup-spec.md` | Replace line-number cites (several point past EOF) with function names, per the spec's own WYSIWYG policy | C14 |
| 9 | `doc/src/arch/sync-conformance.md` | Refresh the File→clause table: `sync_boundary.rs` = `PeerTip` only; no `channel_failures`; no `wait_for_peers_or_proceed` | C5 |
| 10 | `doc/src/arch/consensus/stratum.md` | Mining blob 227/228 → **260 bytes** (`MINING_BLOB_LEN`, `src/linear/src/block.rs:235-273`), add `miner[32]` rows; step 6 → `accept_block()` + `last_block_time.set_now()` + `mempool.mark_mined()` (no `insert_validated_block`) | C7,C8 |
| 11 | (code) `bin/dwowd/src/rpc/stratum.rs:315` | Fix the stale "227-byte" comment to 260 | C7 |
| 12 | `doc/src/arch/consensus/linear_blockchain.md` §Confirmation | Depth-based confirmation is **not implemented** (`threshold` is a vestigial TOML key); finality = anchors + heaviest-chain | C13 |
| 13 | `doc/src/arch/consensus/uncle_merkle.md` Constants | `MAX_COMPETING_BLOCKS` "Defined in" → `block.rs`; delete the local-duplication note (fixed by the recent re-export) | C11 |
| 14 | `doc/src/arch/consensus/hazid-report.md` | Mark H-C3 + H-H1 resolved (reward formula + atomics match code); narrow H-C2 to the stratum path (mm path now broadcasts) | C12 |
| 15 | `doc/src/arch/consensus/linear_zkvm.md` Key Files | `bin/dwowd/src/execution.rs`, `zk.rs`, `src/linear_wasm_adapter.rs`, `src/validator/verification.rs` → `src/linear/src/execution.rs`, `src/linear/src/zk_verifier.rs` | C17 |
| 16 | Fork-choice consolidation | One normative fork-choice section in `consensus.md`; `sync-protocol.md` §19, `node-startup-spec.md` §4, `sync-audit-hazop.md` §6, `hazid-report.md` H-C1, `linear_blockchain.md` link to it. Align wording: "strictly greater" (not "first-seen"), walk via `request_blocks(cursor,1)` not `header.previous` | C16 |
| 17 | `doc/src/arch/consensus/uncle_merkle.md` + `node-startup-spec.md` | Add the devnet-wipe warning: the uncle format change (`src/linear/src/block.rs:121-128`) broke the sled `uncles`-tree and JSON wire formats — devnet must start from a wiped sled DB | C19 |
| 18 | (code) `src/linear/src/lib.rs:24-27` | Fix crate doc "without uncle blocks, fork consensus" (stale) | C-extra |

**Applied 2026-09-11** (items 1–18 above, plus same-class fixes surfaced during the
edit/verify pass):

- **Fork-choice consolidation** landed: `consensus.md` §Fork Choice Rule is normative —
  "strictly more accumulated work" comparison (not first-seen), walk via
  `peer.request_blocks(cursor, 1)` + per-step PoW validation, finality guard inside
  `detect_reorg` before the comparison. `sync-protocol.md` §19, `node-startup-spec.md` §4,
  `sync-audit-hazop.md` §6 (Change A row), `hazid-report.md` H-C1, and `linear_blockchain.md`
  §Confirmation Model all link to it (anchors verified against the mdbook slug rule:
  every non-alphanumeric → dash, no dedup — §19 = `19--fork-selection--heaviest-chain---reorg`).
- `hazid-report.md`: H-C3 + H-H1 marked RESOLVED, H-C2 narrowed to the stratum path
  (`mm_submit_solution` broadcasts — `mm_rpc.rs`); register rows, bow-ties, controls #2/#3/#4,
  RC2 paragraph, and the Verification echo lines all updated. H-C1 row + bow-tie de-cited
  (function names now), heading retitled, H-M16 trimmed.
- Style directive applied across the sweep: `consensus.md` history paragraphs (opening
  "legacy fork/overlay … fully removed" + the `[REMOVED]` status row + "supersedes the
  per-call isolated-overlay model" / "former same-block … superseded") rewritten as
  current state, and `chain_architecture.md`'s "replaces the old dual-instance pattern"
  sentence dropped; remaining `FeeV1` → `FeeV2 (0x08)` and `nullifier SMT` → nullifier set
  mentions in `consensus.md` Phases 5–6 / atomicity tables / cheat-detection table fixed;
  `sync-protocol.md` "legacy P2P stack/transport/rail" mentions dropped (the
  `dwow_core::net` rail is current code).
- `uncle_merkle.md`: Motivation section rewritten current-state; P2-9 note reworded
  ("Depth is not stored — derived via `UncleBlock::depth_for`"); devnet-wipe warning added;
  Constants table cites fixed (`block.rs`, `src/linear/src/lib.rs`) + duplicate-const note
  deleted; "Comparison with Original Design" section removed.
- `linear_zkvm.md`: Key Files + Components tables now the real
  `src/linear/src/execution.rs` (`execute_block`, `genesis_contracts` [9],
  `apply_genesis_deployments`) and `src/linear/src/zk_verifier.rs`
  (`verify_core_tx_with_tables`, `verify_single_tx`, `load_zkbin`, `decode_and_reconcile`);
  note/Context/heading history scrubbed.
- `linear_blockchain.md`: "LinearBlockAdapter *(archived)*" and "Comparison with
  Fork-Based Consensus" sections removed; UncleBlock/UncleProof structs now match code.
- Code: `src/linear/src/lib.rs` crate doc rewritten; `stratum.rs:315` 227→260 comment;
  `bin/darkwow/src/main.rs` `(lib.rs:1264)` → `(lib.rs:1271)`.
- **Out of batch** (noted for follow-up): `node-sync-hazop.md`
  (`wait_for_peers_or_proceed`, `channel_failures`), `fee-spec.md` FeeV1 history,
  `merge-mining-ffi.md` 228-byte blob — none are in the HYG-17…19 tables; fold into a
  follow-up batch (candidate HYG-20 alongside the link sweep).

## HYG-18 — Wallet content sweep

| # | File | Change | Src |
|---|------|--------|-----|
| 1 | `doc/src/arch/wallet.md` §6.3 | Delete step 7 (Schnorr signing) — contradicts §0 (signatures removed); fix step 8's `Transaction` fields → `{inputs, outputs, contract_calls, nullifiers, witness}` | W1 |
| 2 | `doc/src/arch/wallet.md` §6.5, §0.1.4 | `cap_selection.rs` → the Pending filter lives in `lib.rs:1457`; refresh the module map to the real ~18 files | W12 |
| 3 | `doc/src/arch/wallet.md` §0.1.1 | Fix dependency graph: `Transaction` defined in dwow_chain (`src/linear/src/transaction.rs:295`), not dwow-sdk; loosen the "dwow-accounts has zero deps" claim | W13 |
| 4 | `doc/src/arch/wallet-vs-daemon.md` | Rewrite the wallet runtime column: three dispatch categories + `daemon` container mode (persistent sync+scan tasks, unix-socket JSON-RPC server) — not "one command → one process, stateless" | W2 |
| 5 | `doc/src/arch/wallet-vs-daemon.md` | Command taxonomy: 3 categories (no `LocalStdin`), ~22 variants, Network = Broadcast/Scan/Sync{Init,Status}/Daemon — no `mine` command | W3 |
| 6 | `doc/src/arch/wallet-vs-daemon.md` Wallet-Only table | Drop nonexistent `cache.rs`/`transfer.rs`; CapabilityResolver is one generic resolver; add args/config/ffi/integrity/p2p_wallet/rpc_server/prover_impl | W4 |
| 7 | `doc/src/arch/wallet-vs-daemon.md` Shared/feature tables | `LinearStore` (sled) is daemon-only — wallet stores synced blocks in SQLite `chain_blocks`; dwowd features are `net-node`+`rpc`, wallet is `net-wallet` | W14 |
| 8 | `doc/src/arch/wallet-vs-daemon.md` ProcessNet | Delete the duplicated bullet; remove the leftover `TransportQuic` edit fragment | W15,W16 |
| 9 | `doc/src/arch/key-management.md` | Correct rotation semantics (`generate()` never repoints default; `set_default` rejects index ≠ 0); document the `darkwow account` CLI (generate/import-hex/import-base58/export/list); `AccountManager::open` is 3-arg and dwowd hard-fails without `NODE_NAME` | W9 |
| 10 | `doc/src/arch/key-management.md` Wallet Key Flow | Rewrite: no `import-from-toml`; `addresses` table removed; `select_commitments`/`build_transfer` don't exist — identity is `--keys`/`KEYS_FILE` + `WALLET_NAME`, derived at boot, never stored | W10 |
| 11 | `doc/src/arch/key-management.md` | Drop "only hardened derivation implemented" — non-hardened path exists | W11 |
| 12 | `doc/src/dev/testing/wallet-testing.md` | Launcher is `/app/darkwow wallet …` (not `/app/dwow_wallet`); seed service is `observer` (not "lilith"); config key is `peers` (no `seeds`); feature is `net-wallet`; drop the "DRKW alias registered at init" claim; balance prints tab-separated, empty = "No retained balances found" | W5,W6 |
| 13 | `doc/src/dev/wallet-ffi.md` | Replace the `derive_key` row with `dwow_wallet_derive_address(handle, contract_id, height, out_address, out_len)` (0 = error/NULL — opposite convention); add the ~15 missing symbols (`open_persistent`, cap_*, `invoke_contract`, zkas_store/load, `generate_proof`, …); update the symbol count and the ZK section (no longer "Planned") | W7,W17 |
| 14 | `doc/src/testnet/payment.md` | Rewrite the tutorial: no interactive shell; `transfer <amount> DRKW <addr>` auto-broadcasts with confirmation; `broadcast` reads binary stdin; current `wallet coins` columns | W8 |
| 15 | `doc/src/clients/dwowd_jsonrpc.md` | Add `blockchain.get_sync_state`, `login`, `submit`, `merge_mining_get_aux_block`, `merge_mining_get_chain_id`, `merge_mining_submit_solution` (or explicitly scope the doc as main-chain only) | W18 |
| 16 | `bin/dww/README.md` | Add the subcommand reference (incl. `daemon`, `position`, `diagnostic`, `redeem`, `burn`, `contract invoke`) and the `darkwow` launcher (`node`/`wallet`/`account`) | W19 |
| 17 | `doc/src/crypto/key-recovery.md` | Add a status marker — the t-of-n scheme has no implementation in the repo (spec-only) | W20 |
| 18 | `doc/src/ui/ui.md` + `bin/app/README.md` | Either link ui.md to the real `bin/app` or mark it aspirational; fix the README's `darkwallet.apk` → `darkfi-app.apk` and drop the `cargo-limit` step (Makefile never invokes it) | W20 |

### HYG-18 execution notes

All 18 items applied; 11 files changed (335+/206-). Deviations from the plan
as written, each resolved by code evidence:

- **Item 1 (Transaction fields):** the plan prescribed `{inputs, outputs,
  contract_calls, nullifiers, witness}`, but the wallet assembles
  `dwow_core::tx::Transaction` = `{calls, proofs, tx_commitment, nullifiers}`
  (`src/tx/mod.rs:90`) — no `signatures`, no `inputs/outputs/witness`. The
  7-field Transaction the plan cited is the chain-level block transaction
  (`src/linear/src/transaction.rs:295`), a separate type. Docs follow the
  code: §6.3 step 8 now lists the real four fields; §0.1.3 spells out that
  wallet-assembly `Transaction`/`ContractCallLeaf` live in `dwow_core::tx`
  and `ContractCall` in dwow-sdk `tx.rs`.
- **Item 5 (command count):** 24 command paths, not ~22 (verified against
  `args.rs` `WalletCommand`/subcommand enums; `coins` is an alias of
  `capabilities`, `args.rs:367`).
- **Item 9/10:** `darkwow account` CLI is generate/import-hex/import-base58/
  from-seed/export/list (`bin/darkwow/src/account.rs`); vault at
  `~/.dwow/lifecycle.json` with `DWOW_KEY_PASSPHRASE` REQUIRED
  (`crates/dwow-accounts/src/lib.rs:550`).
- **Item 12:** the "seed" node's name IS lilith — its role is what changed
  (observer-role dwowd, not a separate seed binary). Docs say "lilith
  observer node" and `peers = [...]` (config key verified in `config.rs`).
  Also fixed a stale failure-mode row: `Token not found: DRKW` doesn't exist
  in the wallet source — the real non-DRKW error is
  `no held capability found for asset_id '…'`.
- **Item 13:** beyond `derive_address`, added `open_persistent`,
  `caps_by_asset`, `resolve_transfer_contract`, `invoke_contract`,
  `zkas_store`/`zkas_load`/`zkas_list` (list = stub "not yet implemented"),
  `generate_proof`; symbol count 55 → 61.
- **Item 14:** there is NO confirmation prompt on `transfer` — it prints
  base64 and auto-broadcasts (`dispatch.rs:549`, `confirm=false`); the
  prompt exists only for Contract Deploy/Invoke/Lock and Secrets. The
  tutorial says so.
- **Item 16:** `bin/dww/README.md` got the launcher section + full command
  table including `redeem`/`burn` → error directing
  `contract invoke <cid> redeem`, and `daemon`'s unix-socket.
- **Item 18:** per user guidance ("the app is inherited from upstream, and
  will likely change substantially"), `bin/app` edits stayed minimal —
  just `darkwallet.apk` → `darkfi-app.apk` and the cargo-limit step.
  `ui.md` marked aspirational with a pointer to `bin/app/README.md`.

## HYG-19 — Python models (requires desktop runs)

The two model fixes change numeric output; re-run both on the desktop and diff
against `sim/crypto.py` (canonical) before committing.

| # | File | Change | Src |
|---|------|--------|-----|
| 1 | `contrib/docker/darkwow-testnet/merge_mining_model.py` | `expected_reward`: h=0 → 0, h=1 → `INITIAL_REWARD`, else `2^(-(h-1)/H)`; delete the nonexistent `BASE_REWARD` uncle reference; fix the "exact match … blockchain.rs:108-119" comment | G6 |
| 2 | `contrib/model/supply_chain_model.py` | Return `INITIAL_REWARD` at height 1 (drop the zero-reward genesis); re-run cumulative-supply output | G7 |

**Verify:** desktop `python3` runs of both models vs `sim/crypto.py`; no test-suite runs on this VM.

### HYG-19 execution notes

Both edits applied and committed with an `Untested:` trailer. Verification split:

- **Item 1 (`merge_mining_model.py`) — VM-verified green.** The model is
  stdlib-only, so it ran on this VM (same precedent as HYG-12's chain_model):
  all 13 self-tests pass with `reward(1) = 1_383_764_049` (= `INITIAL_REWARD`)
  and `reward(0) = 0`. Also deleted the nonexistent `BASE_REWARD` constant and
  rewrote `compute_reward_distribution`'s docstring (the "Rust hardcodes
  BASE_REWARD" claim was false — base comes from `expected_reward(height)` at
  the call site; Rust's split pays pin rewards, `src/linear/src/block.rs:508`).
- **Item 2 (`supply_chain_model.py`) — desktop verify outstanding.** `blake3`
  is not installed on this VM, so the model cannot run here. `expected_reward`
  now matches Rust exactly (h=0 → 0, h=1 → `INITIAL_REWARD`, else the
  fixed-point loop over `h-1`), header refs corrected (`:924`, `:1153`), and
  `test_genesis_zero_reward` was rewritten as `test_genesis_reward_schedule`
  (genesis pays the full reward, S_2 = C_1 + C_2, supply(1) = `INITIAL_REWARD`)
  — the old test embodied the zero-reward-genesis model that contradicts
  `blockchain.rs:924`. The height-1 validation path was hand-traced against
  `execute_pow_reward` steps A–D (all symbolic checks self-consistent).
  **Desktop command:** `python3 contrib/model/supply_chain_model.py`
  (needs `pip install blake3`); also re-run
  `python3 contrib/docker/darkwow-testnet/merge_mining_model.py` and diff both
  against `sim/crypto.py` (canonical).

### HYG-19 VM verification run (2026-09-11, this VM — edit-only, stdlib models)

All stdlib-only models re-run here; two real bugs found and fixed:

- **`contrib/docker/darkwow-testnet/merge_mining_model.py`** — the h=0/h=1
  cases were fixed, but the decay was float math (`2.0 ** (-exp/H)`), which
  drifts from the canonical integer fixed-point from h=2 on (diff vs
  `sim/crypto.py`: 8/10 spot heights mismatched). Replaced with the
  closed-form `_fixed_pow_decay` (same as `sim/crypto.py` /
  `blockchain.rs::fixed_pow_decay`). Now **23/23 green** and **0 mismatches
  across 408 heights** (0…400 + spot checks incl. h=1_051_920, 2_103_840).
- **`contrib/model/supply_chain_model.py`** — same bug class: the iterative
  per-step `(reward * DECAY_FP) >> 32` loop floors at every step and drifts
  from h=3 on (found by extracting the pure-int `expected_reward`/
  `expected_cumulative_supply` and executing them without importing the
  module — `blake3` blocks the import, not the math). Replaced with the
  closed-form exponentiation. **0 mismatches across 408 heights** for both
  functions. The desktop run is now expected to be a clean re-verification
  of the blake3-dependent parts only (`hash_state_id` sled keys, the
  self-tests).
- **`contrib/model/dockernet_model.py`** — `KeyedMiningNode.__init__`
  referenced `self._account_mgr` before `_init_account_manager()` ever set
  it on the no-`AccountManager` (standalone) path → `AttributeError` (FM1
  failed). Initialized `self._account_mgr = None` first. Now **ALL TESTS
  PASSED** (FM2–FM4 + the wallet phase skip without `cryptography`).

VM run results (stdlib-only): `chain_model` 13/13 · `chain_validation_model`
40/40 · `merge_mining_model` 7/7 · `fee_model` 38/38 · `fee_window_model`
66/66 · `uncle_fork_model` all · `sync_model` 31/31 · `pipeline_model` all ·
`capability_discovery` verified · `proof_of_token_balance` 9/9 ·
`vm_state_model` exit 0 (diagnostic: 2/5 crash paths found is its expected
output) · docker `merge_mining_model` 23/23 · `dockernet_model` ALL PASSED.

Desktop-only (unchanged): `supply_chain_model.py` full run (`blake3`),
`wallet_model.py` + dependents (`wallet_simulation`, `dex_lock_model`,
`key_management`, `nullifier_lifecycle`, `test_oracle`,
`transaction_lifecycle`) — `cryptography` not installed on this VM.

### HYG-19 VM verification run, part 2 (2026-09-11 — packages installed, everything runs here)

`sudo apt-get install python3-pip python3-cryptography` +
`pip install --break-system-packages blake3 base58` unlocked the remaining
models on this VM (network + sudo are available — the "no pip" constraint is
retired). Result: **every python model now runs green on this VM; the
desktop-only python work is eliminated.**

Final table (all run from the repo root — several suites fail when run from
`contrib/model/` because they resolve `src/contract/*/manifest.toml`
relative to the cwd):

| Model | Result |
|---|---|
| `chain_model` 13/13 · `chain_validation_model` 40/40 · `merge_mining_model` 7/7 | green |
| `fee_model` 38/38 · `fee_window_model` 66/66 | green |
| `wallet_model` **95/95** (was 100/100 in test-audit.md — stale count, fixed) | green |
| `wallet_simulation` 12/12 · `key_management` 24/24 · `nullifier_lifecycle` 18/18 | green |
| `transaction_lifecycle` 21/21 · `test_oracle` 4/4 · `dex_lock_model` all | green |
| `uncle_fork_model` all · `sync_model` 31/31 · `pipeline_model` all | green |
| `capability_discovery` verified · `proof_of_token_balance` 9/9 | green |
| `supply_chain_model` **PYTHON SPECIFICATION COMPLETE** (first full VM run) | green |
| `dockernet_model` **ALL TESTS PASSED** incl. the wallet phase (previously skipped) | green |
| docker `merge_mining_model` 23/23 | green |
| `vm_state_model` exit 0 (diagnostic — "2/5 tests found crash paths" is its expected output) | n/a |

Three more real fixes in this pass:

- **`wallet_model.py`** — `_DERIVED_RULE_ARITY` was missing `leaf_increment`
  (arity 3) and `increment` (arity 1); both exist in the Rust prover
  (`src/sdk/src/prover.rs:276-277`) and `purse/manifest.toml` uses
  `derived:leaf_increment:0,5,7`. The manifest-conformance test now parses
  box + purse manifests and passes (95/95).
- **`supply_chain_model.py`** — `test_genesis_non_determinism_detection`
  validated block 1 on both nodes but never committed it, so block 2 saw
  `TOTAL_SUPPLY = 0` (test bug, not model bug). Added
  `node_a/node_b.commit_atomic(result_a/result_b)` after the block-1
  asserts.
- **test-audit.md** — `wallet_model.py 100/100` → 95/95 (the suite defines
  95 runnable tests; the old number predates a consolidation).

## HYG-20 — Link-sweep follow-up + stale-file sweeps

**Status:** ✅ applied · `/tmp/hyg20/check_links.py` reports **broken links: 0**
across 268 files (from 209 at HYG-15). Re-run after every edit; still 0.

### 20.1 The three HYG-17-flagged stale files

- **`consensus/node-sync-hazop.md`** — full rewrite to current code: nodes N1–N7
  (`dial_sync_peers` `linear_sync_client.rs:204-257`, tip collection
  `consensus_linear.rs:344-355`, decision `:474-478`, pull `:358-471`, reorg
  `:137-296`, pacing `:331,366-372,460-471`, mine gate `lib.rs:1259-1278`);
  findings F1–F4 FIXED, F5 PARTIALLY RESOLVED (no per-peer failure counter
  exists and every pass is 30 s-paced (F4); residual: sync-protocol.md §13.3
  peer discipline (`Misbehaving()`) not yet implemented). Old §5 (citation
  drift) removed; §6 cites `chain_model.py:556` +
  `test_temporary_divergence_then_reorg` (`chain_validation_model.py:2186`).
- **`consensus/fee-spec.md`** — current-state sweep, all code-verified: §3
  (FeeV1 history) and §13.6 (SPEC-5) deleted; GS-5 row + FeeV1/DEFAULT_*/K_REF/
  MAX_SCALE §10 rows deleted; PRICE_LOW/MEDIUM/HIGH redefined as 1×/2×/4×
  multipliers; `fee = gas × CF × tier × risk` in ~8 places; `compute_fee_v3`
  in ~5; §5.5 "Seven Spending Questions" + §5.6 Fee_V2 circuit table relocated
  from §3; §12.9/§12.10 pseudocode rewritten to match `lib.rs:1355-1382`.
- **Blob 228→260** — `monero-merge-mining.md` blob section rewritten (260 bytes
  = `BlockHeader::MINING_BLOB_LEN` `block.rs:273`: 0..227 header core, 227
  pow_source disc, 228..260 miner pubkey; nonce at byte 39 for xmrig rx/0);
  `merge-mining-ffi.md:428` label fixed; `uncle_merkle.md` "grows 228 → 260"
  narration replaced.
- **`sync-hazop.md`** — stale follow-up ("still uses the legacy
  LinearSyncClient… deferred") replaced: the node's client-side pull runs over
  the unified `SyncPeer` rail (`dial_sync_peers`), covered by
  `consensus/node-sync-hazop.md`.

### 20.2 Adjacent stale-content sweep (docs-current-state-only, every edit code-verified)

- **`net-node-boundary.md`** — §2 nonexistent `SyncDecision` enum replaced
  with the real decision (`consensus_linear.rs:474-478`); §3 P2-7 history →
  current `SyncState` (CaughtUp=2/Behind=3, `lib.rs:169`); barb catalog:
  `BlocksBatch`/`SyncDecision` rows deleted, `linear_sync.rs` → `sync_types.rs`;
  Obligation 3 → `TIP_TIMEOUT` 5 s / `BLOCKS_TIMEOUT` 30 s
  (`sync_connection.rs:66,76`) + `dial_sync_peers` 15 s-per-dead-peer;
  §9 witness table → the 6 tests that exist in `consensus_coordination.rs`
  + honest gap note (obligations #2/#3 have no runtime witness).
- **`type-system.md`** — 8 REMOVED barb rows deleted (BarbIds don't exist);
  `AccumulatorPoint`/`ThresholdAmount` rows deleted (types gone); §2.3.1
  "Planned newtypes"/audit narration → applied list; `gas × gas_price`/
  `compute_fee` → `gas × CF × tier × risk`/`compute_fee_v3`; `apply_premium/
  apply_standard` (nonexistent) → `.premium()/.standard()` accessors; WasmKb
  row → `compute_storage_fee`; `update_thresholds` → `update_tier_prices`;
  §8.2.3 dual-domain narration deleted; EstimatedFee → real
  `baseline(circuit_costs, wasm_kb)` signature.
- **`wallet.md`** — §6.4.3 "FeeThreshold_V1 … REMOVED" subsection → current
  "Tier Selection"; §6.4.2 "replaces FeeV1 (removed)" + ↓threshold-prove
  sentence reworded.
- **`mempool.md`** — §8.3 "Threshold Proof Machinery — REMOVED" deleted
  (§8.4 renumbered).
- **`sync-protocol.md`** — "private-fee + FeeThreshold_V1 … unworkable"
  history paragraph deleted.
- **`circuit-versioning.md`** — enum section corrected to the real enum
  (`MintV1=0x01 … FeeV2=0x08`); `FeeV1 = 0x00` block replaced; manifest
  subsection rewritten to reality (`proof_circuit` names a `[[circuits]]`
  store-label entry — need NOT match the in-file circuit name; real fee entry
  quoted); "V1 constants have been removed" → "Only V2 constants exist".
- **`consensus.md`** — MassBalanceFeeV2CallData dual-domain (↓pay-fee +
  ↓threshold-prove) → single-domain ↓pay-fee; "Planned: GasAmount" → applied
  `BlockCharge`.
- **`privacy-model.md`** — "lost their hiding machinery in 2026-09" paragraph
  → current-state (public by design; retained
  `fee_value_commit`/`fee_v2_tx_binding`); link path cleaned.
- **`consensus-coinbase.md`** — §17.1 "FeeV1 — HISTORICAL — REMOVED" section
  (73 lines) deleted; §17.7 FeeV1 constant row + §17.8 five FeeV1 taxonomy
  rows deleted; seven "since 2026-09/b6bf44f79" narrations removed;
  FeeV1 → FeeV2 in scan/spendability text; scan gate list completed with
  `0x08` (FeeV2) and `0x00` marked scan-only (matches `scan.rs`).
- **`consensus/safety.md`** — SPEC-5/encrypted-fee-channel REMOVED paragraph →
  current; F2/F3 findings reworded (F3 → ADDRESSED — README now labels 0x08).
- **`dev/contracts/safety.md`** — "Design Decision — FeeV3 Replaces FeeV2"
  ADR rewritten as current-state "FeeV3 — Public Gas/Fee Model" + Retained
  Proof Machinery (formula → `gas × CF × tier × risk`; dangling fee-spec
  §13.6 citation dropped); Naming section 6 → FeeV2/FeeParamsV3; fee client
  table → `fee.rs`/`ephemeral_signature_secret`.
- **`contract-wasm-type-system.md`** — SUPERSEDED fee-accumulator audit row +
  "fee_get_metadata … removed with FeeV1" sentence deleted; circuit status
  table "fee_collect_v2 removed" → plaintext.
- **Misc one-liners** — `genesis.md` ×2, `uncle_merkle.md` ×2,
  `security-analysis.md` (C2 → `Fee_V2`), `dev/contracts/native_token.md` ×3,
  `dev/contracts/standards.md` (TokenMint_V2 row), `contract/native_token.md`,
  `dwowd_contract_pipeline.md`, `contract_invoke_api.md` (0x00 rows →
  unassigned/InvalidFunction), `dev/testing/heavyweight-spec.md` (manifest
  FYI-only; FeeV2; since-trims), `dev/testing/production-test-standard.md`,
  `hazop-darkleaf-in-contractcall-data.md` (selector citations → current
  validation.rs / lib.rs:100-101 extractor), `red-team-findings.md`,
  `opcodes-status.md` (NT Fee_V2), `tx-lifetime.md`, `spec/contract/deploy/
  deploy.md`, `contract/identity.md` (0x03 unassigned).
- **Upstream comparison docs** — `for-contract-developers.md` +
  `about/differences_from_upstream.md`: "8 functions (FeeV1 through BurnV1),
  6 ZK circuits" corrected against the fork history (`d0c5493ec^`): **9
  functions** (FeeV1=0x00 … BurnV1=0x08), 7 trees, **5 circuits**.
- **Left as records** (not DarkWow-removal narration): `upstream-security-
  findings.md` (quotes current code comments verbatim), `dep/0007.md` (DEP
  archive), `build-resource-hazop.md` (B-guardrail dispositions),
  `sync-red-team-audit.md` (RESOLVED dispositions), dated lesson/result logs
  (`dev/contracts/safety.md` Lesson 17, `test-audit.md`), `[HISTORICAL]`
  SUMMARY labels, `arch/legacy/` tree (real active event-graph layer).

### 20.3 Code-side observations — follow-up status

**Fixed in the code-side sweep (commit 2cb92a008):**

- `native_token/manifest.toml` — fee entrypoint now FeeV2 (code 8,
  `FeeParamsV3` wording); `[[circuits]]` names match the in-file circuit
  names (`Fee_V2`/`Mint_V2`/`Burn_V2`); nonexistent `FeeCollectV2` entry
  removed; `uncle_mint` entry added.
- `src/sdk/src/manifest.rs` + `src/linear/src/opcode_cost.rs` — `2^(k-K_REF)`
  comments replaced with the current `circuit_difficulty` (Σ advice rows)
  wording.
- `bin/dww/src/contract_metadata.rs` — native_token entries corrected (fee
  0x08 + `"Fee_V2"`, transfer/spend require proofs, mint disabled,
  fee_collect/uncle_mint added; "no manifest" comment → FYI wording).
- `bin/dww/src/scan.rs`, `native_token` `lib.rs`/`entrypoint`/`zkbins`/
  `model`/`error.rs` (+ `promissory_note/error.rs` string), README table —
  REMOVED/deprecated narrations → current state.
- Docs following the code: circuit-versioning.md manifest subsection,
  heavyweight-spec MintV1 error name.

**Deliberately NOT changed:**

- `src/barb.rs` — adding `PayFee`/`CollectFees`/`FeeWindowOpen`/
  `FeeWindowEnforce` variants breaks the 1:1 mirror with the Lean4 `Barb`
  inductive and requires a MoC-reviewed snapshot update (per the
  `test_notify_on_barb_set_growth` policy). Needs desktop review + Lean
  proofs sync — recorded as a follow-up, not a hygiene sweep.
- `bin/dwowd/src/lib.rs:166` — `UNVERIFIED(P2-7)` marker (desktop runbook).
- `sync-protocol.md §18.1.1` — one "old rule" history sentence (left by
  HYG-17; doc-side).
- Other contracts' manifests with the same label-vs-circuit naming drift
  (e.g., `bearer_bond` `BurnV2` vs `Burn_V2`) — same fix pattern, needs
  per-contract verification before touching (their manifests ARE read at
  deploy time).

## Constraints & notes

- Edit-only on this VM (chromebook) — no cargo/test-suite runs here; python model
  verification happens on the desktop per the existing runbook.
- Push via SSH to `linear-master` as before; commit style `chore(hygiene): HYG-NN …`.
- `doc/src/spec/concepts.md` (2024-02-09) was checked: short, generic, no pre-fork
  consensus content — leave as-is.
- `doc/src/arch/genesis.md`'s link to `proofs/lean/.../SupplyChain.lean` resolves — fine.
- Out of scope (code-side, already runbook-tracked): UNVERIFIED markers in
  `src/`, `bin/` (incl. `HYG-10-2` in `blockchain.rs`); the vestigial
  `threshold`/`pow_target`/`txs_batch_size` keys still shipped in
  `dwowd_config.toml` (removal is a config change — flagged in HYG-16 #6 for the
  doc; decide separately whether to clean the shipped config).
