# Chain Architecture — Implementation

> For the theoretical uncle-merkle consensus design, see
> [linear_blockchain.md](linear_blockchain.md).

## Production Patterns

The architecture follows tried-and-tested patterns from production blockchains:

| Pattern | Source | Where Used |
|---------|--------|------------|
| Single chain state | Bitcoin Core `CChainState` | `src/linear/src/chain_state.rs` |
| Two-stage PoW | Bitcoin Core `CheckBlockHeader` / `ContextualCheckBlockHeader` | `src/linear/src/validation.rs` |
| IBD-derived sync | Bitcoin Core `IsInitialBlockDownload` | `bin/dwowd/src/task/consensus_linear.rs` — `consensus_linear_init_task` pull loop + caught-up gate |
| Built-in miner | Bitcoin Core `-gen`, Geth `--mine` | `bin/dwowd/src/lib.rs` `miner_task()` |
| Uncle-merkle proofs | Polkadot BABE/GRANDPA parachain inclusion | `src/linear/src/validation.rs` `check_uncles()` |
| Binary wire protocol | Bitcoin Core `CDataStream` | `dwow_serial` derive macros |

## Architecture Overview

```
                    ┌─────────────────────────────┐
                    │         Dwowd               │
                    │  (daemon lifecycle)          │
                    │  dnet_task, rpc_task,        │
                    │  consensus_task, miner_task  │
                    └──────────┬──────────────────┘
                               │
                    ┌──────────▼──────────────────┐
                    │        DwowNode              │
                    │  chain_state  ──► CChainState │
                    │  mempool                     │
                    │  p2p_handler                 │
                    │  registry                    │
                    │  rpc_state    ──► RpcState   │
                    │  mining_state ──► MiningState│
                    └──────────────────────────────┘
```

### CChainState — Single Authoritative State

One instance per node. No dual caches. No diverged height/target/VM state.

```
CChainState {
    store: Arc<LinearStore>,        // sled persistence (12 trees: blocks, transactions,
                                    //   contracts, uncles, consensus, commitment_set, nullifiers,
                                    //   supply_chain, block_targets, contract_risk,
                                    //   contracts_undo, uncles_by_height)
    supply_chain: CumulativeSupplyChain, // Pedersen chain S_H = S_{H-1} + C_H + TOTAL_SUPPLY
    consensus: Mutex<PoWConsensus>, // difficulty adjustment
    finality_config: FinalityConfig,// Caribina/Monero finality anchors
    fee_window: Option<FeeWindowState>, // adaptive fee-window state (SPEC-4)
    contract_risk_tracker: Mutex<ContractRiskTracker>, // per-contract dynamic risk (FI-RISK-3)
    height: AtomicU64,              // O(1) cached tip height
    vm_cache: Mutex<HashMap>,       // RandomX VM pool (keyed by randomx_key)
    cache_pool: Mutex<HashMap>,     // RandomXCache pool (256 MB, Arc-shared)
    commitment_set: Mutex<BTreeMap>,// Commitment → creation_height
    uncle_commitment_set: Mutex<HashMap>, // uncle Pedersen commitment → creation_height
    nullifier_set: Mutex<BTreeMap>, // claim Nullifier → creation_height
    spent_nullifiers: Mutex<BTreeSet>, // spend nullifiers (double-spend prevention)
    block_anchor_tree: Arc<Mutex<MerkleTree>>, // depth-32 anchor tree (Orchard standard)
    competing_blocks: Mutex<BTreeMap>, // height → Vec<Block> (uncle candidates)
    competing_seen: Mutex<HashSet>,   // blake3 dedup hashes
    connect_lock: Mutex<()>,        // serializes block insertion
    reorg_lock: Mutex<()>,          // serializes disconnect + reconnect
    tip_hash: Mutex<Option<(BlockHeight, blake3::Hash)>>, // cached tip hash (GetTip)
    genesis_hash: OnceLock<blake3::Hash>, // cached genesis hash
}
```

Single insertion path: `connect_block()` returns `BlockConnectOutcome` with
five variants — canonical extension (`CanonicalExtension{new_height}`),
competing block storage (`CompetingStored`), uncle chain extension
(`UncleExtended`), and already-known block (`AlreadyKnown`). Used by genesis,
sync, broadcast, miner RPC, stratum, and merge mining. All callers MUST match
on the outcome — `mark_mined` is only permitted on `CanonicalExtension` (per
`type-system.md` §4.1, §7 invariant 6). Reorg detection is a separate
pre-connect step: `detect_reorg` returns `ReorgSignal::{Heavier{fork_height,
competing_block}, Lighter, None}`.

**Cache restoration on restart:** On node startup, `commitment_set` and
`nullifier_set` are rebuilt from the `commitment_set` and `nullifiers` sled
trees, preventing duplicate commitment/nullifier acceptance after a crash
restart. `uncle_commitment_set` is in-memory only — uncle commitments are
deterministically recomputable from chain data
(`r_i = blake3(uncle_hash ‖ u_i ‖ H) mod p`). The `competing_blocks` and
`competing_seen` caches are NOT restored (in-flight state that is invalidated
by a restart).

### MiningState — Block Production

Extracted from the DwowNode god object. Single concern: everything related to
producing blocks via mining.

```
MiningState {
    last_block_time: LastBlockTime,               // rate-limit timestamp
    current_linear_template: Mutex<Option<LinearBlockTemplate>>,
    template_height: TemplateHeight,              // template staleness check (type-system.md §9.3)
    linear_stratum_publisher: Mutex<Option<...>>,
    linear_recipient_config: Mutex<Option<...>>,
    linear_submit_lock: Mutex<()>,
    linear_genesis_hash: Mutex<Option<HeaderHash>>,
    mm_jobs: Mutex<HashMap<JobId, ()>>,
    mm_jobs_submitted: Mutex<HashSet<JobId>>,
    miner_config: MinerConfig,                    // fee policy, gas limits, tx count
    sync_state: Arc<AtomicU8>,    // SyncState: 2 = CaughtUp (mine), else Behind (pause)
                                  // with SyncState::load() typed accessor
                                  // — type-system.md §9.3, consensus.md Type-Level Enforcement
}
```

### RpcState — Connection Lifecycle

```
RpcState {
    subscribers: HashMap<&str, JsonSubscriber>,
    rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
    management_rpc_connections: Mutex<HashSet<StoppableTaskPtr>>,
}
```

## Two-Stage PoW Validation

Following Bitcoin Core's pattern exactly:

**Stage 1 (stateless):** `hash_u32 <= block.header.target`
— Hash must meet the block header's own declared target.

**Stage 2 (stateful):** `block.header.target == get_next_work_required(height)`
— The declared target must match what consensus rules require for this height.

```rust
// src/linear/src/consensus.rs
pub fn get_next_work_required(
    &self,
    store: &LinearStore,
    height: BlockHeight,
) -> Result<BlockTarget, LinearError>
```

Genesis returns `BlockTarget::MAX`. For height > 1 there is a fast path —
read the cached `target[H-1]` from the `block_targets` sled tree plus the
last `TIMESTAMP_WINDOW` timestamps and run `compute_adjustment` — and a slow
path that walks the chain from genesis recomputing the target from each
block's timestamp. A gap in local history returns `Err(BlockNotFound)`.

This prevents self-declared-target attacks: a peer cannot mine with
`target = u32::MAX` at height 100 and have it accepted.

Validation failures at each stage SHALL produce phase-typed error barbs
(`type-system.md` §4.1, [consensus.md](consensus.md) 7-phase validation):
`↓bad-proof` (ZK/signature/structural — reject block), `↓bad-nullifier`
(duplicate nullifier — reject block), `↓db-fail` (state corruption — fatal,
restart node).

## Built-in Miner

The node mines internally — no external bash script, no raw TCP connection.
Like Bitcoin Core's `-gen` flag and Geth's `--mine` flag.

```rust
// bin/dwowd/src/lib.rs
async fn miner_task(node: DwowNodePtr) -> Result<()> {
    // 1. Resolve the node's declared mining key (MiningRecipient)
    // 2. Wait for sync_state == CaughtUp
    // 3. Loop:
    //    a. Get latest block, compute next height
    //    b. Get consensus target
    //    c. Build coinbase + mempool transactions
    //    d. Mine nonce (RandomX)
    //    e. Apply block (block_acceptor::accept_block)
    //    f. Broadcast to peers (proto::linear_broadcast::broadcast_block)
    //    g. Rate-limit (min_block_interval)
}
```

## Python Dockernet Model

A 1-to-1 Python model of the full dockernet exists at
`contrib/model/dockernet_model.py`. It models two mining nodes producing
blocks continuously with P2P broadcast and fork resolution. Developers can
run it to understand the block production flow without Docker:

```bash
python3 contrib/model/dockernet_model.py
```

The model maps every Rust function 1-to-1:
- `PoWConsensus.get_next_work_required()` / `adjust_target()`
- `check_block_header()` — two-stage PoW
- `Miner.mine()` — nonce iteration
- `CChainState.connect_block()` — validation + commit
- `sync_loop()` — peer tip query + block fetch

### Merge Mining Model

A separate model at `contrib/model/merge_mining_model.py` extends the native
model with Monero merge mining. It models the full merge dockernet:
4 containers — monerod + 3 mining nodes (2 merge-mining, 1 native).

```bash
python3 contrib/model/merge_mining_model.py
```

The merge model traces:
- `MoneroNode` — monerod solo miner at fixed difficulty, ZMQ publish
- `P2PoolSidecar` — p2pool integrated into the node container (no standalone container), stratum jobs, mm_rpc submit
- `XmrigSidecar` — xmrig integrated into the node container, share mining against p2pool stratum
- `MergeMiningNode` — self-contained container (dwowd + p2pool + xmrig)

## Dockernet Profiles

### Native (`--mode native`)

3 containers: lilith (seed) + 2 mining fullnodes. Both nodes mine internally
via the built-in Rust miner task (no bash loop, no external xmrig). The
built-in miner reads the mining address from the persisted file and loops
indefinitely: mine → apply → broadcast → rate-limit.

### Merge (`--mode merge`)

5 containers: lilith + monerod + 3 fullnodes. Two of the fullnodes are
merge-mining (dwowd + p2pool sidecar + xmrig sidecar), one is native-mining
(built-in Rust miner). The monerod mines Monero blocks at fixed difficulty.

Each merge-mining node is self-contained — p2pool and xmrig run as sidecar
processes inside the node container. No standalone p2pool or xmrig containers.
