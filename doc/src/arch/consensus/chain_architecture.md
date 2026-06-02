# Chain Architecture — Implementation

> Current as of the CChainState refactor (May 2026). For the theoretical
> uncle-merkle consensus design, see [linear_blockchain.md](linear_blockchain.md).

## Production Patterns

The architecture follows tried-and-tested patterns from production blockchains:

| Pattern | Source | Where Used |
|---------|--------|------------|
| Single chain state | Bitcoin Core `CChainState` | `src/linear/src/chain_state.rs` |
| Two-stage PoW | Bitcoin Core `CheckBlockHeader` / `ContextualCheckBlockHeader` | `src/linear/src/validation.rs` |
| IBD-derived sync | Bitcoin Core `IsInitialBlockDownload` | sync task (pending Phase 3) |
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
    store: Arc<LinearStore>,        // sled persistence (6 trees)
    consensus: Mutex<PoWConsensus>, // difficulty adjustment
    height: AtomicU64,              // O(1) cached tip height
    vm_cache: Mutex<HashMap>,       // RandomX VM pool
    coin_set: Mutex<HashMap>,       // coin commitments → height
    nullifier_set: Mutex<HashSet>,  // spent nullifiers
    peer_best_height: AtomicU64,    // best peer-reported height
}
```

Single insertion path: `connect_block()`. Used by genesis, sync, broadcast,
miner RPC, stratum, and merge mining. Replaces the old dual-instance pattern
where `src/linear/src/blockchain.rs` and `bin/dwowd/src/blockchain.rs` held
independent caches that diverged.

### MiningState — Block Production

Extracted from the DwowNode god object. Single concern: everything related to
producing blocks via mining.

```
MiningState {
    last_block_time: AtomicU64,
    linear_zk: Mutex<Option<LinearPowRewardZk>>,
    current_linear_template: Mutex<Option<LinearBlockTemplate>>,
    linear_stratum_publisher: Mutex<Option<...>>,
    linear_recipient_config: Mutex<Option<...>>,
    linear_submit_lock: Mutex<()>,
    linear_genesis_hash: Mutex<Option<HeaderHash>>,
    mm_jobs: Mutex<HashMap<String, ()>>,
    mm_jobs_submitted: Mutex<HashSet<String>>,
    sync_complete: AtomicBool,
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
pub fn get_next_work_required(&self, height: u64) -> u32 {
    if height <= 1 {
        u32::MAX  // genesis: any hash valid
    } else {
        self.target.load(Ordering::Relaxed)
    }
}
```

This prevents self-declared-target attacks: a peer cannot mine with
`target = u32::MAX` at height 100 and have it accepted.

## Built-in Miner

The node mines internally — no external bash script, no raw TCP connection.
Like Bitcoin Core's `-gen` flag and Geth's `--mine` flag.

```rust
// bin/dwowd/src/lib.rs
async fn miner_task(node: DwowNodePtr, db_path: PathBuf) -> Result<()> {
    // 1. Read mining address from persisted file
    // 2. Wait for sync_complete
    // 3. Loop:
    //    a. Get latest block, compute next height
    //    b. Get consensus target
    //    c. Build coinbase + mempool transactions
    //    d. Mine nonce (RandomX)
    //    e. Apply block (via CChainState::apply_block)
    //    f. Broadcast to peers
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
