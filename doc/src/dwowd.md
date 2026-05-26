# DarkWow Daemon (dwowd)

`dwowd` is the DarkWow full node daemon — the binary that validates the
blockchain, processes transactions, and provides JSON-RPC and stratum interfaces
for wallets and miners.

## Architecture

### Component Map

```
DwowNode (top-level node state)
├── LinearBlockchain (sled-backed, height-indexed, pure coordinator)
│   ├── LinearStore (sled tree)
│   ├── PoWConsensus (target adjustment, proof verification)
│   ├── FinalityConfig (anchoring mode + Caribina)
│   ├── coin_set / nullifier_set (double-mint / double-spend protection)
│   └── RandomX VM cache (keyed per-block)
├── TxBackend (per-transaction state access — overlay + store, never LinearBlockchain)
├── Mempool (Vec<Transaction> behind Arc<Mutex>)
├── DwowMinersRegistry (stratum server + mm_rpc server)
├── P2P handler (linear_sync + linear_broadcast)
├── RPC connection trackers (main + management)
├── JSON-RPC subscribers (blocks, txs, proposals, dnet)
├── ZK materials cache (lazy-initialized Mint_V1 PK)
├── Block template cache (current mining round)
└── Stratum publisher (push job notifications)
```

`Dwowd` wraps `DwowNode` with four `StoppableTask` handles: `dnet_task`,
`rpc_task`, `management_rpc_task`, and `consensus_task`.

### Two-Layer LinearBlockchain

The daemon uses **two** LinearBlockchain instances:

| Layer | Type | Purpose |
|-------|------|---------|
| P2P layer | `dwow_chain::LinearBlockchain` | Block storage, serialization, P2P sync (pure library, no WASM) |
| Daemon layer | `crate::LinearBlockchain` | Wraps the P2P store. Adds PoW consensus, WASM runtime, ZK verification, coin/nullifier tracking |

The P2P layer is initialized first (`init_linear` at [lib.rs:212](../bin/dwowd/src/lib.rs#L212)).
Its sled `LinearStore` is then shared with the daemon-layer wrapper. The daemon
layer adds consensus (`PoWConsensus` with `Mutex`), RandomX VM management,
contract execution via the WASM runtime, and state rehydration from stored
blocks.

### State Atomicity

Block execution in `apply_block_with_uncles()` is **atomic**: all contract
state changes within a block either commit together or not at all. This is
achieved via the [`sled-overlay`](https://docs.rs/sled-overlay/latest/sled_overlay/)
crate — the same atomicity mechanism used in upstream DarkFi's fork-aware
consensus.

**How it works:**

1. **`SledTreeOverlay`** wraps the contracts sled tree. All `db_insert`,
   `db_get`, `db_remove`, `db_contains_key`, `contract_lookup`, `contract_init`,
   `contract_insert_bincode`, and `contract_get_bincode` calls go through an
   in-memory BTreeMap — nothing touches sled during execution.

2. **`TxBackend`** implements `RuntimeBackend` and routes state operations
   through the overlay. It holds only `Arc<LinearStore>` (for read-only
   contract data lookups) — the `LinearBlockchain` coordinator is never in
   the execution path. Chain queries (`last_block_height`, `get_tx`, etc.)
   go through the store.

3. **Per-call checkpoint/rollback**: Before each contract call,
   `overlay.checkpoint()` snapshots the current state. If `metadata()`,
   `exec()`, or `apply()` fails, `revert_to_checkpoint()` rolls back only that
   call's writes — prior successful calls in the same block are preserved.

4. **Atomic commit**: When all calls succeed, `overlay.aggregate()` produces a
   `sled::Batch` and `apply_batch()` writes it to the contracts tree as a
   single atomic operation. If the process crashes mid-block, nothing has
   been written to sled.

```
apply_block_with_uncles()
 ├── Create base SledTreeOverlay on contracts tree
 ├── For each call (block + uncles, executed sequentially):
 │    ├── Clone overlay from base → TxBackend { overlay, store }
 │    ├── overlay.checkpoint()
 │    ├── Runtime::new(wasm, backend, ...)
 │    ├── runtime.metadata(&call_data)   ← reads/writes go to overlay
 │    ├── runtime.exec(&call_data)       ← ZK verification, circuit logic
 │    ├── runtime.apply(&[])             ← state changes staged in overlay
 │    ├── On failure: revert_to_checkpoint() — only this call undone
 │    └── On success: compute diff, keep state in overlay
 ├── Merge diffs deterministically (sort by tx hash, canonical-first)
 ├── Canonical diffs applied, then uncle diffs (subtract canonical total)
 ├── overlay.aggregate() → sled::Batch
 ├── contracts_tree.apply_batch(batch)  ← single atomic sled write
 ├── insert_validated_block(block)
 └── store uncles
```

### WASM Runtime

Contract execution uses the [`wasmer`](https://wasmer.io/) WebAssembly runtime
with the **Singlepass** compiler backend by default. Singlepass provides fast
compilation and predictable stack usage, matching upstream DarkFi's proven
configuration.

**Cranelift is available as an opt-in performance enhancer** via the
`cranelift-compiler` Cargo feature flag. Cranelift provides 3-10× faster WASM
execution but comes with deeper native stack frames and larger memory
requirements. It is not the default because stability and determinism take
precedence over raw execution speed. When wasmer's concurrency model matures,
Cranelift will be re-evaluated alongside other performance approaches:

- **Batch compilation**: Dedicated compilation threads with compiled artifact
  caching (requires wasmer Engine-per-thread safety)
- **Thread pools**: WASM instances in thread pools with Engine-per-thread
  isolation
- **Alternative runtimes**: wasmtime or other WASM runtimes with better
  concurrency support

| Aspect | Detail |
|--------|--------|
| Compiler | Singlepass (default); Cranelift via `cranelift-compiler` feature |
| Module cache | None — always recompile (avoids cross-Engine corruption) |
| Stack requirement | Default OS stack (8 MB) — no special configuration needed |
| Block gas limit | `BLOCK_GAS_LIMIT = 100_000_000_000` (250× per-call `GAS_LIMIT` of 400M) |
| Max calls per block | `MAX_CALLS_PER_BLOCK = 10` — enforced at template generation |

The `cranelift-compiler` feature flag is available as a compile-time opt-in:
```toml
# Default: Singlepass (stable, predictable stacks)
wasmer = { version = "6.1.0", features = ["singlepass"] }
wasmer-compiler-singlepass = { version = "6.1.0" }

# Opt-in: Cranelift (3-10× faster, deeper stacks)
wasmer-compiler-cranelift = { version = "6.1.0", optional = true }
```

## Startup Sequence

```
main()
 ├── Parse CLI args + TOML config (network, finality mode, RPC settings)
 ├── Open/create sled database
 ├── Build P2P settings from config
 └── Dwowd::init_linear(network, sled_db, db_path, net_settings, ex, finality_config)
      ├── Create dwow_chain::LinearBlockchain (P2P layer) with FinalityConfig
      ├── Create daemon LinearBlockchain wrapper with PoWConfig
      ├── Deploy genesis contracts (Deployooor, NativeToken) from embedded WASM
      ├── Mine genesis block at height 1 (target=u32::MAX, instant pass)
      ├── Auto-generate mining keypair if none exists (persisted to disk)
      ├── Initialize P2P handler (linear_sync + linear_broadcast protocols)
      ├── Initialize miners registry (stratum + mm_rpc listeners)
      ├── Initialize JSON-RPC subscribers (blocks, txs, proposals, dnet)
      └── Return Dwowd with node + 4 StoppableTask handles

main() continues:
 └── Dwowd::start(ex, rpc_settings, management_rpc, stratum_rpc, mm_rpc, config)
      ├── Spawn dnet subscriber task (forwards P2P events to JSON-RPC subscribers)
      ├── Spawn main JSON-RPC server (DefaultRpcHandler)
      ├── Spawn management JSON-RPC server (ManagementRpcHandler)
      ├── Start miners registry (stratum + mm_rpc listeners)
      ├── Start P2P network (listener + dialer + sync + broadcast handlers)
      ├── Spawn consensus task (placeholder — mining is RPC-driven)
      └── Return Ok(())

main() blocks on signal handler (SIGINT/SIGTERM), then calls Dwowd::stop()
```

### ZK Material Initialization

ZK proving keys (for privacy-preserving coinbase transactions) are **lazy-loaded**.
The `LinearPowRewardZk` struct (Mint_V1 circuit + proving key) is created on the
first stratum login or RPC mine call, not at startup. This avoids blocking daemon
startup on expensive cryptographic setup.

### Mining Keypair

If no mining keypair exists at startup, `dwowd` generates one automatically:
- `{db_path}/mining_address` — wallet address string (for xmrig config)
- `{db_path}/mining_secret` — hex-encoded secret key

These are persisted so the same address is used across restarts.

## Task Graph

All concurrent tasks spawned by `Dwowd::start()`:

| Task | Purpose | Runtime |
|------|---------|---------|
| **dnet subscriber** | Forwards P2P events to `dnet.subscribe_events` JSON-RPC subscriber | Loop forever |
| **main JSON-RPC** | Serves `DefaultRpcHandler` — blockchain, contract, tx, stratum methods | `listen_and_serve` |
| **management JSON-RPC** | Serves `ManagementRpcHandler` — shutdown, status | `listen_and_serve` |
| **stratum listener** | TCP server for xmrig-compatible stratum protocol | Accept loop |
| **mm_rpc listener** | TCP server for merge mining (Caribina/p2pool bridge) | Accept loop |
| **P2P listener** | libp2p transport listener (inbound connections) | libp2p event loop |
| **P2P dialer** | Outbound connections to seed nodes | libp2p event loop |
| **linear sync** | Block download from peers (GetBlocks/Blocks/GetBlock) | Periodic + on-connect |
| **linear broadcast** | One-hop block relay to peers on new block | On-block-insert |
| **consensus** | Placeholder — sleeps forever, mining is entirely RPC-driven | `pending().await` |
| **miner registry tick** | Periodic miner registration maintenance | Interval loop |

### Key Architectural Point

There is **no background mining loop**. Mining is triggered exclusively by
external RPC calls:

- **`miner.mine_linear`** — Dev RPC: mines one block on the local RandomX VM
- **Stratum `submit`** — External xmrig submits a solved nonce via TCP

The "consensus task" is a no-op placeholder. All chain progression is
externally driven.

## RPC Layer

### Main JSON-RPC Server

Default port: `tcp://127.0.0.1:31345`. Methods:

| Namespace | Methods | Purpose |
|-----------|---------|---------|
| `blockchain` | `get_tip`, `get_block`, `get_block_linear`, `subscribe_blocks`, `subscribe_txs` | Chain queries + subscriptions |
| `contract` | `submit_transaction` | Submit contract calls (validated + added to mempool) |
| `tx` | `submit` | Submit raw transactions |
| `miner` | `mine_linear`, `get_mining_address`, `get_block_template` | Dev mining (local RandomX) |

### Stratum Server

Default port: `tcp://127.0.0.1:31347`. xmrig-compatible TCP protocol.

Methods: `login` (subscribe + get job), `submit` (submit solved nonce).

Full specification at [Stratum Protocol](arch/consensus/stratum.md).

### Management JSON-RPC Server

Internal management interface. Methods: `shutdown`, `status`.

### Merge Mining RPC (mm_rpc)

Default port: `tcp://127.0.0.1:31348`. Raw TCP JSON-RPC for p2pool integration.

Methods: `merge_mining_get_chain_id`, `merge_mining_get_aux_block`,
`merge_mining_submit_solution`.

## P2P Layer

libp2p-based with two custom protocols:

### Linear Sync Protocol

Request/response protocol for block download. Messages:
- `GetBlocks` (start_height, count) → `Blocks` (up to 20 blocks)
- `GetBlock` (height) → `BlockResponse` (full block with transactions)
- `GetTip` → `Tip` (current height + hash)

Sync is triggered on peer connect and periodically. Max 20 blocks per response
(`LINEAR_SYNC_BATCH`).

### Linear Broadcast Protocol

One-way block propagation. When a block is mined and inserted, it is broadcast
to all connected peers via `BlockBroadcast` message. Receivers validate and
insert. **No rebroadcast** — each block propagates exactly one hop from the miner.

## Native Contracts

`dwowd` ships with two genesis contracts deployed at block 1:

- **Deployooor** (`DEPLOYOOOR_CONTRACT_ID`) — WASM contract deployment via
  `DeployV1` calls. All user contracts are deployed through Deployooor.
- **NativeToken** (`NATIVE_TOKEN_CONTRACT_ID`) — Consensus token handling
  network fees (`FeeV1`), block rewards (`PoWRewardV1`), and burns (`BurnV1`).

Both are embedded in the binary via `include_bytes!` and deployed during
`init_linear()`.

## Configuration

Config lives at `~/.config/dwow/dwowd_config.toml`. Example for darkwow-testnet:

```toml
network = "darkwow-testnet"

[network_config."darkwow-testnet"]
database = "~/.local/share/dwow/dwowd/darkwow-testnet"
threshold = 3
recipient = "YOUR_WALLET_ADDRESS"

[network_config."darkwow-testnet".pow]
target_block_time = 120       # seconds between blocks
initial_target = 16777215     # 0x00FFFFFF (~1/256 hashes pass)
min_target = 1                # hardest possible
max_target = 4294967295       # u32::MAX, easiest possible
min_block_interval = 10       # minimum seconds between blocks

[network_config."darkwow-testnet".rpc]
rpc_listen = "tcp://127.0.0.1:31345"

[network_config."darkwow-testnet".stratum_rpc]
rpc_listen = "tcp://127.0.0.1:31347"

[network_config."darkwow-testnet".finality]
mode = "always"               # "always" | "native" | "signaled"
caribina_enabled = true

[network_config."darkwow-testnet".net]
localnet = false
inbound = ["tcp+tls://0.0.0.0:31342"]
seeds = ["tcp+tls://seed.darkwow.org:31340"]
allowed_transports = ["tcp+tls"]
outbound_connections = 8
```

### CLI Flags

| Flag | Description |
|------|-------------|
| `-c` / `--config` | Configuration file path |
| `-n` / `--network` | Network name (`linear-testnet`, `darkwow-testnet`, `dwow-devnet`) |
| `-l` / `--log` | Log file path |
| `-v` / `--verbose` | Increase verbosity (`-vvv` supported) |
| `--finality-mode` | Override finality mode: `native`, `always`, `signaled` |
| `--finality-disable-caribina` | Disable Caribina Arweave anchoring |
| `--finality-enable-monero` | Enable Monero p2pool anchoring (default: false) |
| `--monero-min-confirmations` | Monero confirmations before finality (default: 3) |
| `--monerod-rpc-url` | monerod JSON-RPC URL for anchor verification |

## Shutdown Sequence

```
Dwowd::stop()
 ├── Stop dnet subscriber task
 ├── Stop main JSON-RPC server
 ├── Stop management JSON-RPC server
 ├── Stop miners registry (stratum + mm_rpc)
 ├── Stop P2P handler
 ├── Stop consensus task
 └── Flush sled database
```

## Source Layout

| Path | Purpose |
|------|---------|
| `bin/dwowd/src/main.rs` | CLI parsing, config, entrypoint |
| `bin/dwowd/src/lib.rs` | DwowNode, Dwowd, init_linear, start, stop |
| `bin/dwowd/src/blockchain.rs` | Daemon LinearBlockchain wrapper, TxBackend, atomic apply |
| `bin/dwowd/src/mempool.rs` | Mempool (`Vec<Transaction>`) |
| `bin/dwowd/src/rpc/stratum.rs` | Stratum protocol (login, submit) |
| `bin/dwowd/src/rpc/miner.rs` | Dev mining RPC (mine_linear) |
| `bin/dwowd/src/registry/model.rs` | Block template generation, ZK coinbase |
| `bin/dwowd/src/proto/linear_sync.rs` | P2P sync protocol |
| `bin/dwowd/src/proto/linear_broadcast.rs` | P2P block broadcast |
| `bin/dwowd/src/task/consensus_linear.rs` | Consensus task (placeholder) |
| `src/runtime/vm_runtime.rs` | WASM VM runtime (Singlepass compiler, no module cache) |
| `src/runtime/import/db.rs` | WASM host functions: db_set, db_get, db_remove, db_contains_key |
| `src/linear/src/block.rs` | BlockHeader, Block, UncleBlock, mining blob |
| `src/linear/src/consensus.rs` | PoWConsensus (target adjustment, proof check) |
| `src/linear/src/finality.rs` | FinalityConfig, three modes |
| `src/linear/src/blockchain.rs` | Core LinearBlockchain (sled-backed) |
| `src/linear/src/store.rs` | LinearStore (sled trees: blocks, txs, contracts, uncles) |
| `src/sdk/src/blockchain.rs` | Emission schedule, expected_reward() |

## Related Documentation

- [Consensus](arch/consensus/consensus.md) — PoW consensus and block production
- [Stratum Protocol](arch/consensus/stratum.md) — xmrig-compatible mining protocol
- [Linear Blockchain](arch/consensus/linear_blockchain.md) — Uncle Merkle, PoW, WASM
- [Mining Tokenomics](arch/mining-tokenomics.md) — Emission schedule, rewards, merge mining
- [Contract Deployment Pipeline](arch/dwowd_contract_pipeline.md) — WASM deployment flow
- [Testing Overview](dev/testing/overview.md) — Four-level testing taxonomy
