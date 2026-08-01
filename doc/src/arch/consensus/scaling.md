# Sharding as Natural Topology: The Uncle Merkle Scaling Vision

> **FORWARD-LOOKING**: This document describes a scaling architecture that emerges
> naturally from Uncle Merkle consensus. It is a design vision, not a description of
> current implementation. The primitives it builds on — uncle blocks, merkle proofs,
> pin rewards, RandomX PoW binding, O-Cap ZK proofs — are implemented and working.
> What is described here is where those primitives lead when extended to parallel
> state. Nothing contradicts existing code; everything described is the natural
> next step.
>
> **Scaffolding:** Type scaffolding and function stubs for every concept in this
> document exist behind `#[cfg(feature = "sharding")]` — disabled by default,
> intended as post-mainnet namespace reservation. See `src/sdk/src/crypto/shard.rs`,
> `src/linear/src/shard.rs`, `src/runtime/import/shard.rs`, and
> `src/linear/src/block.rs`.

## The Hidden Topology of Uncle Merkle

Uncle Merkle consensus is usually described as a fork-handling mechanism: competing
blocks that don't win the canonical slot still earn a partial reward, so mining work
is never wasted. That description is correct but incomplete. It misses the deeper
structural property that makes Uncle Merkle a scaling primitive.

Every proof-of-work network has a latency graph. Blocks that originate near the
center of the graph propagate faster, reach more peers, and tend to become canonical.
Blocks that originate near the edge propagate slower and are more likely to be seen
as uncles — still valid, still carrying real transactions, still earning reward, but
sitting one or two levels deep in the uncle merkle tree rather than at the tip.

This is not a flaw. It is a sorting process. The network's own physics — latency,
bandwidth, hashrate distribution — naturally partitions miners into center and
periphery without any protocol-level assignment. No committee election. No stake
weighting. No beacon chain. The topology emerges from the ground.

```
          CANONICAL CHAIN (center — linear time)
    ===================================================
    Block N-2 ─── Block N-1 ─── Block N ─── Block N+1
                     |            |            |
                UncleTree     UncleTree    UncleTree
               /    |    \    /  |  \     /   |   \
             U1    U2    U3  U1  U2  U3  U1   U2   U3
            (d=1) (d=1) (d=1)
             |
            U1a  (d=2)   ← edge — parallel transaction processing
```

The center competes for one thing: producing the next canonical block. This is a
linear-time competition — one winner per height. The edge competes for something
else entirely: processing transactions. This is embarrassingly parallel — many uncle
blocks can exist at the same depth, each carrying a different subset of the
transaction pool, each earning a pin reward proportional to its depth.

This asymmetry is the scaling lever. Most blockchain designs treat the edge as a
problem to be minimized (stale blocks are waste). Uncle Merkle treats it as a
resource to be organized (uncle blocks are parallel throughput). The question is not
how to eliminate uncles. The question is how to give them independent work to do.

## From Uncle Trees to Sharded State

Once you see uncle blocks as a natural edge, the next step follows directly: let
those edge blocks maintain their own state instead of competing for inclusion in the
canonical state.

In the current design, uncle transactions are merged into the canonical chain's
state deterministically — canonical state wins on key conflicts, uncle diffs fill in
the rest. This works for a single shard. But the uncle block already carries
everything needed for independence: its own transaction set, its own RandomX PoW
binding, and a merkle proof anchoring it to the canonical chain. The only missing
piece is a state root — a commitment to the state that results from executing the
uncle's transactions independently rather than merging them into the canonical state.

Once an uncle block carries a state root, it becomes a shard. Not a shard assigned
by a protocol committee. A shard formed naturally by the miners whose blocks happen
to propagate through a particular region of the network.

> **Scaffolding:** `#[cfg(feature = "sharding")]` in `src/linear/src/block.rs`.
> `UncleBlock` and `UncleProof` carry optional `state_root: Option<[u8; 32]>`
> fields. `UncleProof` also carries `shard_id: Option<[u8; 32]>`. All fields
> are `None` (traditional uncle behavior) unless the sharding feature is
> enabled post-mainnet. See `src/sdk/src/crypto/shard.rs` for the shard
> identity types (`ShardId`, `ShardStateRoot`).

```
            CANONICAL CHAIN (settlement layer)
    ===================================================
    Block N-1               Block N               Block N+1
    |                       |                     |
    |--- Shard A            |--- Shard A          |--- Shard A
    |    state_root: 0xa1   |    state_root: 0xa7 |    state_root: 0xaf
    |    UncleProof(d=1)    |    UncleProof(d=1)  |    UncleProof(d=1)
    |                       |                     |
    |--- Shard B            |--- Shard B          |--- Shard B
    |    state_root: 0xb3   |    state_root: 0xb9 |    state_root: 0xbf
    |    UncleProof(d=1)    |    UncleProof(d=1)  |    UncleProof(d=1)
    |                       |                     |
    |                       |--- Shard C          |
    |                       |    state_root: 0xc1 |
    |                       |    UncleProof(d=2)  |
```

The canonical chain no longer executes every transaction. It stores merkle roots of
shard state transitions and verifies their proofs. It becomes a settlement layer —
the same role Ethereum's rollup-centric roadmap envisions for its L1, but achieved
without a separate L2 construction. The uncle block is the rollup block. The shard
miner is the sequencer. The canonical chain is the bridge. Same data structures,
same consensus, same PoW, extended to parallel state.

This is what most sharding designs miss. They impose shards from above — partition
the validator set, assign committees, run separate consensus per shard — and then
spend enormous complexity on the problems that creates: cross-shard communication
protocols, receipt proofs, async coordination, state fragmentation. Uncle Merkle
sidesteps all of this because the shards were never assigned. They emerged. The
canonical chain already knows about every shard's state root because every shard
block is an uncle block, and every uncle block is in the merkle tree.

## ZK State Proofs Between Shards

If the canonical chain stores shard state roots, any shard can prove something about
any other shard's state without that shard being online. The mechanism is already
the central paradigm for DarkWow smart contracts: Object Capability authorization.

At the contract level, O-Cap answers "can you prove you have access to X?" instead
of "who are you?" The pattern is Commitment → ZK Proof → Consume (nullifier). Alice
proves she holds capability X without revealing her identity.

At the shard level, the same pattern applies to state references. Shard A needs to
know that Shard B's state satisfies some predicate — for example, that account X in
Shard B has a balance sufficient to cover a cross-shard transfer. Shard A does not
ask Shard B. It proves it:

1. **Commitment**: Shard B's state root is committed in the canonical chain's uncle
   merkle tree. This is the on-chain commitment — exactly like an O-Cap commitment
   on a contract.

2. **Proof**: Shard A produces a ZK proof that there exists a valid merkle path from
   the claimed state data to Shard B's state root, AND that the state data satisfies
   the required predicate (e.g., `balance >= 100`), AND that Shard B's state root is
   itself committed in the canonical chain's uncle merkle root at block height N.

3. **Consume**: A nullifier prevents the same state proof from being submitted twice.

The canonical chain verifies the ZK proof and the merkle inclusions. It does not
execute the transaction. It does not store Shard B's state. It only needs the uncle
merkle root — which it already has — and the ZK proof — which is compact. This is
the same verification model as a ZK-rollup posting a validity proof to Ethereum L1,
but the L1 is the canonical chain, the rollup is the shard, and they share the same
consensus mechanism.

Without ZK, Shard A would need to publish Shard B's full state on-chain to prove it
knows it — exactly the replication that sharding is meant to avoid. With ZK, Shard A
publishes a proof of constant (or logarithmic) size regardless of how much state is
being referenced. Verification cost scales with the proof circuit, not with the
state size.

> **Scaffolding:** `src/sdk/src/crypto/shard.rs`. The O-Cap pattern is encoded
> as `ShardCommitment` (state root anchored in canonical uncle tree) →
> `CrossShardProof` (ZK proof + nullifier) → `ShardNullifier` (replay prevention).
> `verify_aggregate_proof()` is a `todo!()` stub. The canonical chain's
> `BlockHeader` carries `shard_state_roots_root` and `aggregate_proof_root`
> behind `#[cfg(feature = "sharding")]`.

## Git-Type State Proof Import

The model for how shards share state is not a database replication protocol. It is
git. A git repository does not contain every object from every fork. When you merge
a branch, git identifies the common ancestor, computes what changed, and imports
only the objects reachable from the branch tip that you don't already have.

DarkWow shards operate the same way. Each shard maintains its own state tree. When a
cross-shard transaction in Shard A references state from Shard B, Shard A imports
only three things:

- A merkle proof that Shard B's state root exists in the canonical chain's uncle
  merkle tree at a known height. This is the ancestry check — `git merge-base`.

- A ZK proof that the specific state item (e.g., an account balance, a contract
  storage slot) satisfies the transaction's predicate. This is the diff.

- A nullifier that marks the state reference as consumed. This prevents replay —
  equivalent to git marking a commit as already applied.

```
    Shard A (importing)         Canonical Chain          Shard B (source)
    =================          =================         =================
    State Tree A               Uncle Merkle Root        State Tree B
    ├── contract_a/            ├── ShardA: root_A       ├── contract_b/
    ├── contract_b/ (imported) │   └── proof_A          ├── contract_c/
    │   └── [ZK: valid state]  ├── ShardB: root_B ◄──── state root_B
    └── contract_d/            │   └── proof_B
                               └── ShardC: root_C
                                       │
    Import: proof that B's state ──────┘
    satisfies predicate, anchored
    at root_B in canonical tree
```

Shard A does not need Shard B to be online. It only needs the ZK proof — which can
be gossiped, cached, or retrieved from archival nodes — and the canonical chain's
uncle merkle root — which every full node already stores. State liveness is
completely decoupled from state availability. A shard can go offline for a week, and
cross-shard transactions referencing its state still settle, because the canonical
chain holds the last-committed state root and ZK proofs fill in the gaps.

This is lazy import, not eager replication. Unreferenced state in Shard B is never
imported by Shard A. Over time, frequently-referenced state creates a natural cache
of hot cross-shard references, while cold state remains where it was committed. The
system does not pay to replicate what nobody is using.

> **Scaffolding:** `src/sdk/src/crypto/shard.rs`. `StateImportProof` encodes the
> git-import pattern: `source_shard` → `target_shard`, anchored at a
> `canonical_height` with an `uncle_merkle_proof`, carrying a `zk_predicate_proof`
> against a `PredicateType` (BalanceGte, ContractState). `verify_state_import()`
> is a `todo!()` stub. `SettlementTransaction` aggregates `StateImportProof`s
> across multiple shards with a `ShardNullifier` for replay protection.

## Inter-Shard Settlement on the Canonical Chain

When Shard A's state transition depends on Shard B's state, the combined transaction
is posted to the canonical chain as a single settlement batch. The canonical block
that includes it stores:

- The merkle root of the uncle blocks containing each shard's state transitions
- An aggregate ZK proof that all inter-shard predicates are satisfied
- Merkle paths linking each shard's previous state root to a prior canonical uncle root

The canonical chain verifies the aggregate proof. It does not execute the
transactions. The execution happened in the shards. The canonical chain verifies
that it happened correctly and commits the result.

```
    Block N (Canonical)
    ===================
    uncle_merkle_root ────────────┐
    transactions[]               │
    zk_aggregate_proof ──┐       │
                         │       │
         ┌───────────────┘       │
         ▼                       ▼
    ┌──────────────────┐   ┌──────────────────┐
    │ State proof:     │   │ Uncle Merkle     │
    │ ShardA → ShardB  │   │ Tree             │
    │ predicate: valid  │   │ ├── ShardA root  │
    │ no double-spend   │   │ ├── ShardB root  │
    └──────────────────┘   │ └── ShardC root  │
                           └──────────────────┘
              │
    Canonical chain verifies:
    1. ZK proof is valid
    2. Merkle paths reach uncle_merkle_root
    3. Reward distribution is correct

    State execution: none (executed in shards)
```

This is the rollup pattern, but without the artificial boundary between L1 and L2.
In Ethereum, a ZK-rollup is a separate system with its own sequencer, its own
prover, its own bridge contract, its own security assumptions. In DarkWow, the
rollup infrastructure is the uncle mechanism extended by one field. The uncle block
is the rollup block. The shard miner is the sequencer. The canonical chain is the
bridge. The ZK proof is the validity condition.

The pin reward economics already align the incentives. A shard miner who produces a
valid state transition and gets it referenced in the canonical chain earns a pin
reward — 50% at depth 1, halving each level deeper. A shard miner who produces an
invalid transition gets nothing because verification fails. Shard miners and
canonical miners don't need to trust each other. They only need to agree on the
merkle root and the ZK proof — both of which are mathematically verifiable.

> **Scaffolding:** `src/sdk/src/crypto/shard.rs`. `SettlementBatch` combines
> `ShardStateCommitment`s (per-shard prev/new state roots with merkle proofs),
> `CrossShardProof`s, an optional `AggregateProof`, and `SettlementTransaction`s
> into a single canonical chain submission. `verify_settlement_batch()` is a
> `todo!()` stub. Validation stubs live in `src/linear/src/shard.rs`
> (`verify_shard_block`, `verify_shard_merkle_inclusion`,
> `verify_cross_shard_proofs`). Host function skeletons are at
> `src/runtime/import/shard.rs` (`merkle_shard_proof_add`,
> `settlement_batch_verify`). WASM SDK stubs at
> `src/sdk/src/wasm/merkle.rs`.

## Why This Is Natural Evolution, Not a Bolt-On

Every piece of this architecture already exists in Uncle Merkle consensus. Sharding
is not a new feature. It is the same primitives aimed at parallel state instead of
competing state.

| Uncle Merkle Primitive | Today | With Sharding |
|------------------------|-------|---------------|
| Uncle block | Competing transaction set, earns partial reward | Shard state transition, earns pin reward |
| Uncle merkle root | Commitment to uncle txs in canonical block | Commitment to shard state roots in settlement block |
| Uncle proof | Proves uncle included in merkle tree (with PoW) | Proves shard state transition is valid (with ZK) |
| Pin mechanism | Use-it-or-lose-it reward offer | Economic incentive for shard validity |
| RandomX PoW binding | Prevents fake uncle proofs | Prevents fake shard transition proofs |
| Deterministic merge | Canonical/uncle diffs merged in order | Cross-shard state imports applied in dependency order |
| O-Cap ZK proofs | Contract-level authorization | Shard-level state reference authorization |

What changes is small: uncle blocks gain a `state_root` field, and ZK proof
verification is added to the canonical block verification path alongside the
existing PoW verification. What stays the same is everything else: the data
structures, the merkle tree, the pin reward formula, the difficulty target, the
RandomX binding, the deterministic merge logic. No new consensus mechanism. No new
network protocol. No new validator role.

> **Scaffolding status:** Every primitive in the table above has a corresponding
> type or stub behind `#[cfg(feature = "sharding")]`. The feature flag is
> disabled by default — this is post-mainnet architectural scaffolding, not
> unwired pre-mainnet code. See `src/sdk/src/crypto/shard.rs` (proof types),
> `src/linear/src/shard.rs` (validation stubs),
> `src/runtime/import/shard.rs` (host function skeletons), and
> `src/linear/src/block.rs` (block field additions).

Shards do not need their own consensus because they piggyback on the canonical
chain's consensus via uncle merkle proofs. This avoids the hardest problem in
sharding design — cross-shard consensus — entirely. If a shard goes offline, the
canonical chain continues producing blocks. Other shards continue importing proofs
from the canonical chain (which holds the shard's last-committed state root). When
the shard comes back online, it resumes from its last canonical checkpoint. There is
no liveness dependency between shards.

## Open Questions

These are areas where the design space is understood but the specifics are not yet
settled.

**State proof size vs verification cost.** ZK proofs grow with circuit complexity.
Cross-shard predicate circuits need to be small enough that aggregating proofs from
many shards in a single canonical block remains feasible. Recursive proof
composition (proving the verification of another proof) may be necessary at scale.

**Shard formation dynamics.** The network's latency graph determines shard
membership naturally, but the equilibrium properties — how many shards form, how
stable they are, whether hashrate concentrates in profitable shards — have not been
formally analyzed. The pin reward function (depth-based halving) creates a natural
cost to being far from the center, which should bound shard count, but the exact
equilibrium depends on network topology.

**Data availability for shard state.** The canonical chain commits to shard state
roots (hashes). It does not store the state. If a shard's miners all go offline
permanently, the state behind those hashes is lost. Archival nodes or a separate
data availability sampling layer would be needed for full state recovery. This
parallels Ethereum's DA problem under the rollup-centric roadmap.

**Fee markets across shards.** If Shard A has higher fee volume than Shard B, miners
migrate to Shard A. This is economically rational but could create a concentration
dynamic where one shard dominates transaction processing. Whether pin reward
economics are sufficient to maintain a diverse shard ecosystem is an open question.

## Parallel Contract Execution (Future)

DarkWow's architecture is structurally ready for parallel contract execution
within a single block. The ρ-calculus formalization (see
[Type System §9](../type-system.md#9-concurrent-execution-model)) defines
contract calls as concurrent processes: `P₁ | P₂ | ... | Pₙ` where processes
with disjoint write key sets execute independently and their state diffs
merge deterministically.

**What's in place:**

- **Independent per-call overlays** (`execution.rs:163`): each contract call
  executes against a clone of the base sled overlay (`Arc<Mutex<SledTreeOverlay>>`),
  so calls do not share mutable state during execution.
- **JoinSet<T>** (`src/concurrency/join_set.rs`): concurrent task collection
  for spawning parallel work and joining results — the Rust mapping of
  ρ-calculus `P | Q | merge!`.
- **ExecutionSchedule** (`src/linear/src/schedule.rs`): key-set-based
  dependency analysis that partitions calls into parallel waves. The merge
  phase already collects per-call write keys and logs a parallelism score
  (`info!` level: wave count, max parallelism, fully_parallel flag).
- **Duplicate-key detection** (`execution.rs:398-405`): the existing
  `written_keys.insert(key)` check is the bisimulation witness — if a
  key collision is detected at merge time, the block is rejected. This
  is exactly the safety condition for parallel execution: calls with
  disjoint keys are bisimilar to sequential execution.

**The gate: wasmer Runtime concurrency.** The `wasmer::Runtime` type
handles WASM compilation, caching, and execution. Its internal state
(compiler, code cache, instance pool) may not be safe for concurrent
access from multiple smol tasks. Each call already creates a fresh
`Runtime::new()` instance (`execution.rs:232`), so the per-call state
is independent — the question is whether the underlying compiler
internals are thread-safe.

**Path to enablement:**

1. Confirm wasmer `Runtime` is `Send + Sync` for independent instances
2. Add `#[cfg(feature = "parallel-execution")]` feature gate in `Cargo.toml`
3. Replace the sequential `for job in jobs` loop with `JoinSet::spawn` +
   `JoinSet::join_all`
4. Verify deterministic consensus: same block, parallel or sequential,
   identical state root

The ExecutionSchedule diagnostic already logs per-block parallelism
potential. When wasmer confirms concurrent Runtime safety, the code
change is mechanical — the architecture, formal verification, and
diagnostic infrastructure are complete.

Known wasmer concurrency issues (as of July 2026):
- [wasmer#4158](https://github.com/wasmerio/wasmer/issues/4158) — Send+Sync soundness bug
- [wasmer#1630](https://github.com/wasmerio/wasmer/issues/1630) — host_state shared across threads (UB)
- [wasmer#1632](https://github.com/wasmerio/wasmer/issues/1632) — memory_grow unsynchronized write (data race)

Track concurrency status:
[wasmer GitHub Issues](https://github.com/wasmerio/wasmer/issues)
(search: "thread safety" / "Send Sync" / "concurrent Runtime")

## Comparison

| Aspect | ETH2 Sharding | Polkadot Parachains | Cosmos Zones | DarkWow Uncle-Merkle Sharding |
|--------|--------------|---------------------|--------------|-------------------------------|
| Shard assignment | Committee (random sampling) | Slot auction | Sovereign chain | Emergent (latency + hashrate) |
| Cross-shard comms | Async message passing | XCMP channels | IBC relayers | ZK proofs via canonical chain |
| Consensus per shard | Separate (beacon finalizes) | Separate (relay finalizes) | Separate (Tendermint) | None (canonical via uncle proofs) |
| Settlement layer | Beacon chain | Relay chain | Hub chain | Canonical chain |
| Trust model | 2/3 committee honest | Collator honest | Zone validator honest | PoW + ZK proof validity |
| Complexity | High (DA sampling, custody game) | Medium (auctions, collators) | Medium (IBC, light clients) | Low (extends existing uncle mechanism) |

## See Also

- [Uncle Merkle Consensus](uncle_merkle.md) — The consensus mechanism this scaling vision extends
- [Consensus](consensus.md) — Current consensus specification and finality layers
- [O-Cap & Composable Privacy](../ocap.md) — Object capability authorization, the model for inter-shard state proofs
- [Consensus & Coinbase](../consensus-coinbase.md) — Reward schedule and pin economics
- [Linear Blockchain](linear_blockchain.md) — Current linear chain architecture and uncle block construction
