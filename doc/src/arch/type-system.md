# DarkWow Type System

This document defines the DarkWow type system. It is the specification
to which all implementation shall conform. It uses SHALL, MUST, SHALL NOT,
MUST NOT per RFC 2119.

## 0. Foundational Calculus

The type system derives from the **ρ-calculus** — the reflective higher-order
π-calculus. The ρ-calculus extends the π-calculus with one property: names are
processes and processes are names. Names can be quoted, inspected as data, and
passed as messages. This reflective property is what makes the calculus suitable
for cryptographic capabilities: a capability IS a name, and that name can be
passed, restricted, and observed.

The primitive operations:

| Operation | Notation | Meaning |
|-----------|----------|---------|
| Inaction | `0` | The stopped process |
| Output | `x!(y)` | Send name `y` on channel `x` |
| Input | `x?(y).P` | Receive name `y` on channel `x`, then behave as `P` |
| Restriction | `νx.P` | Create fresh name `x` with scope `P` |
| Replication | `!P` | Replicate `P` arbitrarily many times |
| Reflection | `quote(x)` | Treat name `x` as data |
| Dereference | `eval(x)` | Treat data `x` as a name |
| Parallel Composition | `P \| Q` | Execute `P` and `Q` concurrently; synchronize on shared names |

In the blockchain context:
- A **channel** is a contract instance (sled tree + WASM entrypoint).
- A **name** is a capability (a secret key whose possession authorizes action).
- **Output** is posting a commitment (placing a name's public face on-chain).
- **Input** is discovering a commitment via AEAD decryption (receiving a name).
- **Restriction** is deriving a per-instance key (scoping a name to a contract).
- **Replication** is the nullifier SMT (a name consumed exactly once; replication
  models the infinite supply of fresh names).

## 1. Definition of a Type

**A type is a behavioral position in a concurrent interaction graph.**

A type `T` constrains three things for any process `P` typed at `T`:

1. **Domain** — what names `P` can hold.
2. **Barbed interface** — what actions `P` can observe and perform.
3. **Scope mobility** — what names `P` can extrude beyond its declared boundary.

```
Γ ⊢ P : T
```

means: in naming context `Γ`, process `P` occupies behavioral position `T`.

### 1.1 Barbs

A **barb** is an observable action. In the ρ-calculus, process `P` exhibits
barb `↓x` if `P` can engage in input or output on channel `x`.

Every type SHALL define the barbs that processes at that type may exhibit.
No type SHALL exhibit a barb that its definition does not declare.

| Barb | Observable Action |
|------|-------------------|
| `↓spend` | Exercises a capability — consumes a resource and authorizes spawning of new resources. Requires possession of the capability's authorization secret. |
| `↓view` | Decrypts an encrypted note addressed to the holder, revealing the capability parameters it contains. |
| `↓nullify` | Publishes evidence that a capability instance has been exercised. Each exercise SHALL produce a unique nullifier. A nullifier appearing on-chain SHALL prevent re-exercise of the same capability instance. |
| `↓commit` | Publishes the public face of a capability as a Commitment. The commitment cryptographically binds the capability parameters while revealing none of them. |
| `↓prove` | Generates a zero-knowledge proof that the holder knows a witness satisfying the capability's predicate language, without revealing the witness. |
| `↓verify` | Verifies a zero-knowledge proof or digital signature. Returns acceptance only if the proof is cryptographically valid against the declared public inputs. |
| `↓dispatch` | Routes a capability exercise to the contract that recognizes it. The contract is identified by its ContractId. |
| `↓gate` | Constrains capability exercise to a specific function. The function is identified by the contract's function code as declared in its manifest. |
| `↓denominate` | Identifies the capability class. Two capabilities with different AssetId values SHALL be distinguishable by verifiers, even if all other parameters are identical. |
| `↓prove-inclusion` | Proves membership of a commitment in a recognized set. In DarkWow: a Merkle proof from the commitment to a known Merkle root. |
| `↓encrypt` | Produces ciphertext that only the holder of the corresponding decryption secret can decrypt. Uses Diffie-Hellman key agreement to derive a shared secret. |
| `↓derive` | Derives a scoped sub-key from an existing secret. The derived key is bound to a specific contract instance and SHALL NOT be usable in other contexts. |
| `↓discover` | Detects capabilities addressed to the holder. In DarkWow: trial AEAD decryption of encrypted notes in block call data. |
| `↓mine` | Produces a valid coinbase commitment. The coinbase is the consensus mechanism that creates the native asset (DRKW). Requires possession of a MiningRecipient. |
| `↓concurrent` | Executes in parallel with sibling processes. Requires no shared mutable state dependency between the processes. |
| `↓merge` | Deterministically combines concurrent state diffs. Two processes with disjoint key sets SHALL produce mergeable state deltas. |
| `↓sync-barrier` | Blocks until a synchronization condition is met. Used to coordinate processes across execution waves. |
| `↓broadcast` | Publishes a message to multiple subscribers simultaneously. The message SHALL be delivered to all active subscribers. |
| `↓rate-limit` | Constrains output rate for backpressure. The process SHALL NOT exceed its declared rate budget. |
| `↓gossip-forward` | Relays an inbound message to a subset of outbound peers. Forwarding SHALL exclude the origin peer. |
| `↓quorum-query` | Queries a threshold of peers and converges on agreement. Agreement requires a supermajority of queried peers. |
| `↓dag-parent` | References prior events in a partial-order data structure. The reference forms a directed acyclic graph edge. |

### 1.2 Bisimulation

Two processes `P` and `Q` are **strongly bisimilar** (`P ∼ Q`) if an observer
cannot distinguish them through interaction. For every action `P` can take,
`Q` can take a matching action leading to bisimilar states, and vice versa.
This extends to concurrency barbs: for every barb `P` exhibits (including
↓concurrent, ↓merge, ↓broadcast, etc.), `Q` MUST exhibit a matching barb.

**Weak bisimulation** (`P ≈ Q`): internal synchronization actions (τ-transitions)
are unobservable. Two process nets that differ only in internal task scheduling
are weak-bisimilar. `P | (a?(x).Q) | a!(v).R ≈ P | Q{v/x} | R` — internal
communication on channel `a` is transparent to observers. The smol executor's
internal task scheduling SHALL be modeled as τ-transitions and MUST NOT affect
observable barb behavior.

**Barbed bisimulation** (`P ≅ Q`): two concurrent processes are equivalent if
their observable concurrent barbs match, even if their internal scheduling
order differs. Two task graphs with different scheduling yield the same sled
overlay if and only if the key sets are disjoint — this is the formal
justification for parallel contract execution. `P | Q ≅ Q | P` (commutativity of
parallel composition). `(P | Q) | R ≅ P | (Q | R)` (associativity of parallel
composition).

## 2. Type Distinction Principle

**Two types SHALL NOT be unified if there exists any context where a process
holding a name of type T₁ exhibits observably different behavior from a process
holding a name of type T₂.**

If a process at type `T₁` exhibits barb `↓x` that no process at type `T₂` can
match, the types MUST remain distinct. The compiler MUST reject any attempt to
use a value of type `T₁` where type `T₂` is expected.

### 2.1 Cryptographic Types Are Nominal

Every cryptographic capability SHALL be a distinct nominal type. The compiler
SHALL NOT accept a `Nullifier` where a `SecretKey` is required. The compiler
SHALL NOT accept `[u8; 32]` where a `Nullifier` is required. The behavioral
positions are provably different under bisimulation:

- `SecretKey` exhibits `↓spend` and `↓derive`. `[u8; 32]` exhibits neither.
- `Nullifier` exhibits `↓nullify`. `[u8; 32]` exhibits no barbs.
- `Commitment` exhibits `↓commit`. `[u8; 32]` exhibits no barbs.
- `PublicKey` exhibits `↓verify` and `↓encrypt`. `pallas::Point` exhibits neither.
- `ContractId` exhibits `↓dispatch`. `[u8; 32]` exhibits no barbs.

### 2.2 Bytes Round-Trip Is Forbidden

No type SHALL be converted to `[u8; 32]` and back across a module boundary.
The intermediate `[u8; 32]` has no behavioral constraints — any process can
produce any 32 bytes. This erases the type distinction and SHALL NOT compile.

The correct path is: construct the typed value directly and pass it across
the boundary as itself. The constructor SHALL validate the input. No `From`
impl SHALL bypass validation.

Conversion to bytes is permitted ONLY at persistence boundaries (sled, SQLite).
The conversion SHALL use `Type::from_bytes()` which SHALL validate. Reading
back from persistence SHALL validate through `Type::from_bytes()`. No code
path SHALL construct a type by directly accessing a `pub` field.

## 3. Generic Types and Capabilities

A generic parameter `T` abstracts over the behavioral position of a name. This
abstraction is permitted ONLY when all three conditions hold:

**(a)** The function's behavior does NOT depend on the specific barbs of `T`.

**(b)** `T` does not cross a restriction boundary (ν-scope). A name created
by restriction SHALL NOT be extruded through a generic interface that erases
its scope.

**(c)** `T` is not a cryptographic capability. Capabilities have distinct
security semantics; a generic interface that accepts any capability erases
the distinction between `↓spend`, `↓nullify`, and `↓prove`.

ANY function that accepts `impl AsRef<[u8]>` and is callable with a
`SecretKey`, `Nullifier`, or `Commitment` SHALL NOT compile. The trait bound
erases the barb. The behavioral position is lost.

## 4. Error Types

Every error variant IS a barb of the system. When a process can fail in ways
that demand different responses from its containing context, those failures
MUST be distinct types.

| Error Barb | Observable By | Context Response |
|------------|---------------|------------------|
| `↓bad-nullifier` | Mempool, Chain | Reject transaction |
| `↓double-spend` | Chain | Block is invalid |
| `↓bad-proof` | Contract VM | Reject call |
| `↓bad-derive` | Wallet | Skip note, do not crash |
| `↓db-fail` | Infrastructure | Fatal — restart |

These barbs SHALL NOT be unified. A `↓double-spend` failure requires
block-level rejection. A `↓bad-derive` failure requires note-level skipping.
Unifying them under a single error type erases the behavioral distinction —
the caller cannot distinguish "consensus failure" from "this note is not mine."

No function SHALL discard an error silently. `unwrap_or_default()` SHALL NOT
appear in any cryptographic path. `.ok()` chains that discard the error reason
SHALL NOT appear in any cryptographic path. Every `Result` SHALL be propagated
to a context that can respond appropriately.

## 5. Authority

**A process SHALL perform action A if and only if it possesses the name for A.**

The function signature SHALL require the capability type as a parameter.
No ambient authority exists. There are no global admin keys, no upgrade
proxies, no `owner` addresses. Authority flows ONLY through explicit name
passing at the type level.

A function that takes no `SecretKey` parameter SHALL NOT sign. A function
that takes no `Nullifier` parameter SHALL NOT check replay. A function whose
signature accepts `[u8; 32]` instead of `OwnedSecretKey` SHALL NOT authorize
mining — the compiler SHALL reject it because `[u8; 32]` is not a capability.

## 6. The Capability Engine: Emergent Types from Sound Primitives

The Authorization Inversion Theorem establishes:

> An ACL-based authorization system A(p, r, s) can be inverted to a
> privacy-preserving O-Cap scheme A'(π, r, s) if and only if there exists a
> ZK proof system for the language L_{r,s} = { w : P_{r,s}(w) = 1 } with
> proofs simulatable without knowledge of w.

Under the ρ-calculus, this becomes a type-level requirement:

**The type of a capability IS the predicate language it proves.**

```
CapabilityType(r, s) ≡ L_{r,s}
```

Where `L_{r,s}` is the ZK proof language for predicate `P_{r,s}` over resource
`r` and action `s`. The capability type encodes:

- What must be proven (the predicate `P_{r,s}`).
- What the verifier observes (the barb `↓prove`).
- What is hidden (the witness `w`).

### 6.1 Capability Types Are Emergent

A capability type — "can transfer up to 100 native tokens," "can vote on
proposal X," "can submit a sealed bid to tender Y" — is not a primitive.
It is constructed by composition of primitive types:

```
Capability(can_transfer_100_native_tokens) ≡
    compose(
        Nullifier(↓nullify),
        Commitment(↓commit),
        AssetId(↓denominate),
        FuncId(↓gate),
        ContractId(↓dispatch),
        SecretKey(↓spend, ν-restricted)
    )
```

The wallet, as a capability engine, constructs these emergent types at scan
time: it discovers a commitment via AEAD decryption, resolves the contract
via its manifest, and derives the capability's type from the composition of
the primitives the contract declares. The wallet never stores a generic
`cap_id: String` — it SHALL store a typed composition.

### 6.2 Primitive Soundness Is a Prerequisite

The construction in §6.1 is mathematically sound IF AND ONLY IF every
primitive type preserves its barbs across every module boundary.

If `Nullifier` is unified with `[u8; 32]` at any boundary, the composition
collapses. The wallet cannot determine whether a given 32-byte value is a
`Nullifier` (exhibiting `↓nullify`, preventing replay), a `Commitment` (exhibiting
`↓commit`, the public face of a capability), or an opaque byte buffer (exhibiting no barbs).
All three are behaviorally distinct under bisimulation (§2). Unifying them
under `[u8; 32]` makes all three indistinguishable.

Strict type boundaries are not a preference. They are the minimum viable
foundation for the capability engine. Without them, emergent capability
types cannot be constructed — because the primitive types they compose from
have had their barbs erased.

### 6.3 The Two Modes

The O-Cap model has two realizations:

- **Reference Mode (Agoric):** The capability IS an object reference. The type
  is checked at runtime by the object system.
- **ZK Mode (DarkWow):** The capability IS a secret whose knowledge can be
  proven in zero-knowledge. The type is the ZK circuit that verifies the
  predicate.

Under bisimulation, these are the SAME model. Agoric's `Payment` type and
DarkWow's `NativeTokenTransfer` circuit both exhibit `↓spend`. The difference
is what the barb reveals: Agoric reveals the payment identity, amount, and
brand; DarkWow reveals only the predicate result and nullifier.

The Authorization Inversion Theorem guarantees conversion is bidirectional.
The type system SHALL preserve this: a ZK capability type SHALL be refinable
to a plaintext capability type, and vice versa, by adding or removing the
zero-knowledge wrapper.

## 7. Compiler-Enforced Invariants

Every program that compiles SHALL satisfy these five invariants:

1. **Name possession.** No name shall be used without being received or
   created. Authority is explicit in the type signature.

2. **Type distinction.** No two distinct behavioral positions shall be
   unified under a single type. `Nullifier` SHALL NOT be `[u8; 32]`.
   `SecretKey` SHALL NOT be `AsRef<[u8]>`.

3. **Scope restriction.** No restricted name shall cross its declared
   scope boundary. A `SecretKey` derived for contract instance `A` SHALL NOT
   be usable in contract instance `B`.

4. **Error barb distinguishability.** All error conditions that demand
   different context responses shall be different types. The caller SHALL
   be able to match on which failure occurred.

5. **Authority-through-possession.** Authority to perform cryptographic
   operations SHALL be represented by possession of the corresponding
   cryptographic key type. No ambient authority.

## 8. Type Namespace

Every type in the DarkWow type system, its inner representation, the barbs
it exhibits, its scope, and its construction rules.

### 8.1 Cryptographic Primitive Types (Nominal)

These types are **nominal** — distinguished by their name and behavioral
position, not by their internal representation. Two primitive types with
identical internal representations (`pallas::Base`) SHALL NOT be unified
if their barbs differ.



| Type | Inner | Barbs | Scope | Construction |
|------|-------|-------|-------|-------------|
| `SecretKey` | `pallas::Base` | `↓spend`, `↓derive` | ν-restricted to holder | `from_bytes` (validates), `derive_instance` (binds to contract+instance) |
| `PublicKey` | `pallas::Point` | `↓verify`, `↓encrypt` | Extrudable | `from_secret`, `from_bytes` (rejects identity) |
| `Nullifier` | `pallas::Base` | `↓nullify` | Public | `new(secret, coin_hash)` only. `from_bytes` SHALL reject zero. |
| `Commitment` | `pallas::Base` | `↓commit` | Public | `from_attributes(pk, value, token_id, spend_hook, user_data, blind)` |
| `ContractId` | `pallas::Base` | `↓dispatch` | Public | `derive(deploy_key)` or well-known constant |
| `AssetId` | `pallas::Base` | `↓denominate` | Public | `derive(auth_parent, user_data, blind)` or well-known constant |
| `FuncId` | `pallas::Base` | `↓gate` | Public | `from(contract_id, func_code)` |
| `MerkleNode` | `pallas::Base` | `↓prove-inclusion` | Public | Tree insertion |

### 8.2 Structural Types

| Type | Composition | Barbs |
|------|------------|-------|
| `Transaction` | `{ calls, proofs, signatures, nullifiers: Vec<Nullifier> }` | `↓process` |
| `ContractCall` | `{ contract_id: ContractId, data: Vec<u8> }` | `↓invoke` |
| `CoinbaseTransaction` | `{ proof, public_inputs, coin: Commitment, nullifier: Nullifier, encrypted_note }` | `↓mine` |
| `BlockHeader` | `{ merkle_root, previous, height, ... }` — all merkle roots SHALL be `blake3::Hash` | `↓validate-pow` |
| `AeadEncryptedNote` | `{ ciphertext, ephem_public: PublicKey }` | `↓discover` |

A `Transaction`'s `proofs` and `signatures` are load-bearing: they SHALL be carried
end-to-end (broadcast → mempool → block) and verified at both mempool admission and block
acceptance ([mempool.md](mempool.md)). Stripping them erases the `↓prove`/`↓verify` barbs
(§2.2) and defeats the authority model (§5). Transaction construction — the exercise of a
held capability — is specified in [wallet.md §6](wallet.md).

### 8.3 Authority Types

| Type | Inner | Barbs | Construction |
|------|-------|-------|-------------|
| `OwnedSecretKey` | `SecretKey` | `↓spend` (only if declared) | `from_declared_bytes`. No `::random()`. No `From<SecretKey>`. |
| `MiningRecipient` | `PublicKey` + `OwnedSecretKey` | `↓mine` | `from_account`. No `From<PublicKey>`. |
| `AccountManager` | `Vec<Account>` | `↓identity` | `open(keys_path, network, profile)` |

### 8.4 Non-Unifiable Pairs

These pairs SHALL NOT be unified under any generic interface, trait bound,
`From` impl, `Deref` impl, or type alias. The compiler SHALL reject any
code that treats the left type as the right type.

| Type | SHALL NOT be treated as | Reason |
|------|------------------------|--------|
| `Nullifier` | `[u8; 32]` | `↓nullify` ≠ no barbs |
| `Nullifier` | `IntentNullifier` | Different predicate languages |
| `Commitment` | `[u8; 32]` | `↓commit` ≠ no barbs |
| `SecretKey` | `[u8; 32]` | `↓spend`, `↓derive` ≠ no barbs |
| `SecretKey` | `pallas::Base` | One barbs, one does not |
| `PublicKey` | `pallas::Point` | One validates identity, one does not |
| `ContractId` | `[u8; 32]` | `↓dispatch` ≠ no barbs |
| `FuncId` | `pallas::Base` | `↓gate` ≠ no barbs |
| `AssetId` | `pallas::Base` | `↓denominate` ≠ no barbs |
| `OwnedSecretKey` | `SecretKey` | `↓spend` requires declaration; `SecretKey` may be random |

### 8.5 Shared Derives

Every newtype over `pallas::Base` in §8.1 SHALL derive:

```
Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable
```

`ContractId` and `MerkleNode` SHALL additionally derive `Ord, PartialOrd`.
`Nullifier` SHALL additionally derive `Ord, PartialOrd`.

No type in §8.1 SHALL derive `Hash`, `Default`, or `From<pallas::Base>`.
The `From<pallas::Base>` impl erases the type distinction — any field element
could become any capability. Construction SHALL use named constructors that
enforce validation (zero-rejection, canonical encoding, identity rejection).

Serialization for chain persistence (serde `Serialize`/`Deserialize`) SHALL
be implemented manually via `to_bytes()`/`from_bytes()` for each type. No
type SHALL derive serde directly — `pallas::Base` does not implement serde.

## 9. Concurrent Execution Model

The ρ-calculus primitives in Section 0 define both authorization semantics
(what capabilities each process holds) and execution semantics (how processes
execute in parallel). This section defines the latter — the mapping from
ρ-calculus concurrent processes to Rust async tasks on the `smol` executor.

### 9.1 Process-to-Task Mapping

Every ρ-calculus process maps to a `smol::Task<T>` spawned on `ExecutorPtr`:

| ρ-Calculus Construct | Rust Implementation | Location |
|---|---|---|
| Process `P` | `smol::Task<T>` — a spawned future | `src/concurrency/mod.rs:45` |
| Channel `x` | `smol::channel::Sender<T>` / `Receiver<T>` | `src/net/channel.rs` |
| `P \| Q` | `JoinSet::spawn(P); JoinSet::spawn(Q); JoinSet::join_all()` | `src/concurrency/join_set.rs` |
| `νx.P` (restriction) | Rust module scope + `Send` bound — `x` cannot escape `P`'s type boundary | Compile-time |
| `!P` (replication) | `StoppableTask` — repl until stopped | `src/concurrency/stoppable_task.rs` |
| `↓sync-barrier` | `CondVar::wait()` / `CondVar::notify()` | `src/concurrency/condvar.rs` |
| `↓broadcast` | `Publisher<T>::notify()` → all `Subscription<T>` receivers | `src/concurrency/publisher.rs` |
| `↓gossip-forward` | `p2p.broadcast_with_exclude(msg, origin_peer)` | `bin/dwowd/src/proto/linear_broadcast.rs` |
| `↓rate-limit` | Linear sleep proportional to `count - RATELIMIT_MIN_COUNT` | `src/event_graph/proto.rs:610` |
| `↓quorum-query` | `consideration_threshold = communicated_peers * 2 / 3` | `src/event_graph/mod.rs:307` |
| `↓dag-parent` | `Event.parents: [blake3::Hash; N_EVENT_PARENTS]` | `src/event_graph/event.rs:44` |
| Temporal scoping | `timeout(Duration, future)` / `sleep(Duration)` | `src/concurrency/timeout.rs:43` |

### 9.2 Parallel Execution Safety

Transaction calls within a block SHALL execute in parallel (`P_1 | P_2 | ... | P_n`)
when their key sets are pairwise disjoint. The duplicate-key check at
`src/linear/src/execution.rs:398-405` (`written_keys.insert(key)`) is the
bisimulation witness: if a key collision is detected, the parallel composition
is NOT bisimilar to sequential execution, and the block SHALL be rejected.

```
theorem parallelMerge_correctness (calls : List CallJob)
    (h_disjoint : pairwise_disjoint_keys calls) :
    parallel_execute(calls) ≈ sequential_execute(calls)
```

Parallel execution is weak-bisimilar (`≈`) to sequential execution because
internal task scheduling (τ-transitions) may differ, but observable state
diff outputs are identical when keys are disjoint.

### 9.3 Block Production Concurrency

Block production SHALL be modeled as concurrent mining with deterministic resolution:

```
BlockProduction =
  νcompeting_blocks.(νconnect_lock.(
    M!(canonical_header, canonical_txs)                // canonical miner
    | U_1!(competing_header_1, competing_txs_1)         // competing miner 1
    | U_n!(competing_header_n, competing_txs_n)         // competing miner n
    | C?(all_blocks).resolve!(tip, uncles)              // consensus observer
  ))
```

Where `resolve!(tip, uncles)` implements:
1. First-seen-wins: the first block at a given height becomes canonical
2. Competing blocks become uncles: `competing_blocks.lock().insert(height, block)`
3. Chain reorganization: `try_reorg_from_competing()`

Mapped to: `CChainState` at `src/linear/src/chain_state.rs:64`,
`competing_blocks: Mutex<HashMap<u64, Vec<Block>>>` at line 105,
`try_reorg_from_competing()` at line 982.

### 9.4 ExecutionSchedule — Dependency Analysis

Before parallel execution, the SHALL-analyze step computes an `ExecutionSchedule`
from the key set of each call:

```
ExecutionSchedule =
  νkey_sets.(
    analyze_keys!(jobs, key_sets)
    | build_waves!(key_sets, waves)   // calls with disjoint key sets form one wave
    | for wave in waves:
        parallel_execute!(wave)       // all calls in a wave execute concurrently
        | merge_wave!(wave)           // barrier before next wave
  )
```

Calls with intersecting key sets SHALL execute in dependency order across
sequential waves. Calls with disjoint key sets SHALL execute concurrently
within a single wave. The schedule SHALL be deterministic: same block,
same key sets, same wave partition.

### 9.5 Scaling — Emergent-Topology Sharding

The scaling model at `doc/src/arch/consensus/scaling.md` formalizes as:

```
ShardedSystem =
  νcanonical_chain.(
    C!(settlement)                                          // canonical chain = settlement layer
    | S_1!(state_root_1, txs_1, uncle_proof_1)              // shard 1 = uncle block
    | S_2!(state_root_2, txs_2, uncle_proof_2)              // shard 2 = uncle block
    | CrossShardProof?(import_A_B).settle!(batch)           // cross-shard settlement
  )
```

Where `S_i` is an uncle block extended with a `state_root` field, and
`CrossShardProof` is a ZK proof that Shard A's state transition depends
on Shard B's state at a known root. This is emergent: the network's latency
graph determines which miners form shards. No protocol-level assignment needed.

## 10. P2P Network as Replicated Process Nets

The P2P network SHALL be formalized as a collection of replicated processes
communicating through typed channels. DarkWow has two distinct P2P paths
sharing a common transport layer.

### 10.1 Three-Tier Feature Gate as Process Hierarchy

The three-tier feature gate at `Cargo.toml` defines a process hierarchy:

```
net-wallet ⊂ net-node ⊂ net-full

ProcessNet(wallet) =
  νtransport.(νchannel.(νsession.(
    ProtocolAddress | ProtocolVersion    // address exchange + handshake
  )))

ProcessNet(node) = ProcessNet(wallet) |
  RefineSession                           // peer refinement (greylist/whitelist)

ProcessNet(full) = ProcessNet(node) |
  ProtocolSeed | SeedSyncSession | BanPolicy |
  TransportTor | TransportI2p | TransportQuic  // additional transports
```

### 10.2 Blockchain Path — Structured Gossip

The blockchain P2P path (`net-node` tier) SHALL replace flood broadcast with
structured fan-out gossip:

```
GossipStructured(b) =
  νfan_out.(
    broadcaster?(b).
    fan_out_selector!(peers, log₂(N)).     // select k = log₂(N) peers
    (for p in fan_out: p!(b)).              // send to selected peers
    fan_out?(acks).                         // wait for k acknowledgments
    GossipStructured(next_b)
  )
```

Fan-out factor `k = log₂(N)` produces O(log N) propagation rounds and
O(k·N) total messages — optimal for epidemic dissemination. This replaces
the current flood broadcast (`p2p.broadcast(&msg)` — send-side fan-out at
`linear_broadcast.rs:206-256`, receive-side relay still flood at `:385`)
which produces O(N²) traffic per block on the receive side.

### 10.3 Event Graph Path — DAG Sync

The event graph DAG sync SHALL be formalized as a replicated process:

```
ProtocolEventGraph =
  handle_event_put      // receive + validate + recursive-fetch incoming events
  | handle_event_req     // serve parent-event requests from peers
  | handle_tip_req       // serve tip-set queries from syncing peers
  | broadcast_rate_limiter  // rate-limited relay of inbound events to other peers
```

These four concurrent tasks correspond to the `ProtocolJobsManager::spawn()`
calls at `src/event_graph/proto.rs:161-164`, each running as an independent
`smol::Task`. The quarantine boundary — event graph sled overlay MUST NOT touch
blockchain execution sled trees — SHALL be enforced as a restriction:

```
νquarantine.(
  νblockchain_sled.( blockchain_processes(blockchain_sled) )
| νeventgraph_sled.( eventgraph_processes(eventgraph_sled) )
)
```

The two sled trees are separate restricted names. No process in the blockchain
scope holds a reference to `eventgraph_sled`, and no process in the event graph
scope holds a reference to `blockchain_sled`. The compiler enforces this through
the `event-graph` feature gate at `src/lib.rs:33-39`.

### 10.4 Bridging — Shared Channels with Typed Barbs

The two paths SHALL communicate through typed channels with barb-carried
type safety:

```
bridge_chain_evg : Channel<BridgeMessage>
  exhibits { ↓commit, ↓verify }            // blockchain barbs

bridge_evg_chain : Channel<StateProof>
  exhibits { ↓broadcast, ↓dag-parent }      // event-graph barbs

sync_barrier : Channel<()>
  exhibits { ↓sync-barrier }                // both paths can wait/notify
```

The quarantine boundary SHALL be enforced at compile time: messages carrying
blockchain barbs (↓spend, ↓nullify, ↓commit) SHALL NOT be routable through
the event graph channel. The `BarbWitness` trait at `src/net/barb_trait.rs`
provides the static check.

Event graph as blockchain P2P substrate: blockchain events SHALL be wrapped
in event content with marker byte `0x42` ('B' for blockchain) and routed
through DAG sync instead of flood broadcast. The event graph sled tree
(`dag`) remains quarantined from blockchain sled trees (`contracts`, `blocks`,
`coins`, `nullifiers`).

### Implementation

The barb system is implemented across three modules:

**BarbId enum** (`src/net/barb_trait.rs`): 22 observable actions — 14
authorization barbs (Spend, View, Nullify, Commit, Prove, Verify, Dispatch,
Gate, Denominate, ProveInclusion, Encrypt, Derive, Discover, Mine) and 8
concurrency barbs (Concurrent, Merge, SyncBarrier, Broadcast, RateLimit,
GossipForward, QuorumQuery, DagParent). Classification predicates:
`is_blockchain_barb()`, `is_event_graph_barb()`, `is_concurrency_barb()`.

**ExhibitsBarb trait** (`src/net/barb_trait.rs`): Protocol handlers implement
this marker trait to declare their barb set at compile time. `bridge_safe
::<Source, Dest>()` provides the static quarantine check — blockchain barbs
(↓spend, ↓nullify, ↓commit) SHALL NOT cross to event-graph channels.

**BridgeChannel** (`src/net/bridge_channel.rs`): Typed channel with
`BarbWitness<B>` phantom type parameter. `BridgeChannel<T, B>::pair()` creates
a `BridgeSender`/`BridgeReceiver` pair. The `B` parameter statically enforces
that a channel declared for blockchain messages cannot receive from an
event-graph process.

**BlockchainEvent bridge** (`src/event_graph/blockchain_bridge.rs`): Wraps
blockchain messages in event graph content. `wrap_blockchain_event(data)`
prepends marker `0x42`. `is_blockchain_event(content)` checks the marker
with a single byte comparison (zero allocation). `unwrap_blockchain_event()`
extracts the payload.

**Quarantine enforcement** operates at three layers:
1. **Feature gate** (`Cargo.toml` + `src/lib.rs:33-39`): `event-graph` feature
   independently enable/disable, sled-overlay quarantined behind it
2. **Compile-time**: `BarbWitness<B>` phantom type + `bridge_safe()` prevent
   blockchain barbs from crossing to event-graph channels
3. **Runtime**: Separate sled trees — blockchain (`contracts`, `blocks`,
   `coins`, `nullifiers`) vs event graph (`dag`)

## 11. Verified Properties

The type system defined in this document is formalized in the Lean4 calculus
of constructions at `proofs/lean/src/DarkFi/Capability/`. The following
theorems are proved or stated with explicit verification status.

### 10.1 Pareto-Efficiency of the Primitive Type Namespace

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Pareto.lean`

`primitiveTypesAreParetoEfficient`: All 12 primitive types have pairwise
distinct barb sets. No type distinction can be removed without losing
behavioral information. Proof: `dec_trivial` over the finite list of
`Finset Barb` values.

15 named pair-distinction theorems provide human-readable cross-references
for each pair in §8.1 and §8.3 (e.g., `secretKey_distinct_from_nullifier`,
`ownedSecretKey_distinct_from_miningRecipient`).

`barbEqualityImpliesTypeEquality`: If two primitive types have identical
barb sets, they are the same type. This is the contrapositive of
pareto-efficiency — no accidental unification is possible.

### 10.2 Non-Unifiable Pairs

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Distinction.lean`

All 10 pairs in §8.4 are proved distinct (`native_decide`). The conjunction
`allUnifiablePairsProved` bundles them for single-reference verification:
Nullifier ≠ [u8; 32], Commitment ≠ [u8; 32], SecretKey ≠ [u8; 32], ContractId ≠
[u8; 32], PublicKey ≠ pallas::Point, SecretKey ≠ pallas::Base, FuncId ≠
pallas::Base, AssetId ≠ pallas::Base, Nullifier ≠ IntentNullifier,
OwnedSecretKey ≠ SecretKey.

### 10.3 Barb Preservation Under Composition

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Composition.lean`

`barbPreservation`: If a primitive type `p` is in the composition list, then
every barb of `p` is in the composed barb set. Proof: structural induction
on the primitive list. This guarantees that composing capability types does
not erase barbs — the fundamental requirement for emergent type construction.

### 10.4 Authorization Inversion (Type-Level)

**Status:** PROVED (type-level). `proofs/lean/src/DarkFi/Capability/Inversion.lean`

`authorizationInversion_TypeLevel`: For every resource `r` and action `s`,
there exists a capability type `CapabilityType r s` iff there exists a list
of primitives whose composition covers `r.requiredBarbs`. Proof: iff
construction (both directions).

The ZK soundness bridge is stated as `circuitSoundnessBridge`: if a circuit
exists for `(r, s)` whose `constrain_instance` calls cover the required
barbs, then the capability type is inhabited. This is an axiom referencing
the manual circuit audit in `proofs/lean/src/DarkFi/Circuits/` (120 circuits,
all `constrain_instance` calls verified for instance-derivation binding).

`capabilityPredicateBypass_prevention`: A capability requiring `↓prove`
MUST have that barb covered by its composition. This closes HAZOP Pattern 4
("capability predicate result is free witness; provenance unverified").

### 10.5 Wallet Type Construction Soundness and Completeness

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Wallet.lean`

`walletConstruct_sound`: If `walletConstruct` returns a capability type, the
required barbs are covered by the composed primitives.

`walletConstruct_complete`: If a `CapabilityType` exists for primitives `p`
and resource `r`, then `walletConstruct p r` returns `some` (not `none`).

`walletConstruct_preservesPrimitives`: The primitives returned are exactly
the primitives passed in — no loss, no modification.

Three concrete constructibility proofs verify that native token transfer,
DAO vote, and tender bid capability types are constructible from their
respective primitive lists.

### 10.6 Full ZK Proof System Model

**Status:** FUTURE WORK. Not yet formalized.

The type-level Authorization Inversion is proved. The full ZK proof system
model (Halo2 constraint semantics, polynomial commitments, Fiat-Shamir
transform) in Lean4 is future work. When complete, `circuitSoundnessBridge`
will be replaced with a proved theorem referencing the Halo2 formalization.

## 12. References

- Meredith, L.G. and Radestock, M. (2005). "A Reflective Higher-Order Calculus."
  *Electronic Notes in Theoretical Computer Science*, 141(5), 49-67.
- Milner, R. (1999). *Communicating and Mobile Systems: the π-Calculus.*
  Cambridge University Press.
- Miller, M.S. (2006). *Robust Composition: Towards a Unified Approach to Access
  Control and Concurrency Control.* PhD dissertation, Johns Hopkins University.
- "The Zero-Knowledge Authorization Inversion Theorem" —
  [technologytruth.substack.com/p/the-zero-knowledge-authorization](https://technologytruth.substack.com/p/the-zero-knowledge-authorization)
- Sangiorgi, D. and Walker, D. (2001). *The π-Calculus: A Theory of Mobile
  Processes.* Cambridge University Press.
- Bradner, S. (1997). "Key words for use in RFCs to Indicate Requirement
  Levels." RFC 2119.

## 13. Design Lesson: Contracts Are Instances, Not Special Cases

A contract name — "Box", "Purse", "Escrow" — is a human-readable label for a
specific barb composition. It is NOT a special code path.

**Example: Box.** Box is the ZK-native o-cap delegation primitive. But from the
calculus of constructions perspective, "Box" is just:

```
boxCapType = compose([SecretKey, Nullifier, ContractId, FuncId, MerkleNode])
```

Five primitives. Five barbs. The wallet's generic `wallet_construct` function
handles this without any Box-specific branches. The contract name documents
the *intent* (linear delegation, per Mark Miller's o-cap model), but the *type*
is fully determined by the primitives.

**Anti-pattern:** Creating a bespoke scan path, client module, or wallet branch
for a specific contract. This breaks the calculus — the whole point is that
`wallet_construct` is a pure function of primitives and required barbs, not
contract names.

**Correct pattern:** When adding a new contract:
1. Define its barb composition (which primitives, which required barbs)
2. Verify through the generic `wallet_construct` that the composition is valid
3. If `wallet_construct` returns `None`, the primitives don't cover the barbs —
   fix the composition, NOT the wallet

The only contract with a bespoke wallet path is NativeToken, because it is
consensus-critical (block rewards, fee payment, supply audit). Every other
contract — genesis or user-deployed — must work through the generic machinery.
