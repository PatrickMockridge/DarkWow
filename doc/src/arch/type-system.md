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
- **Replication** is the nullifier marker set (a name consumed exactly once;
  replication models the infinite supply of fresh names). Nullifiers are stored
  as flat sled markers via `db_mark_spent`/`db_contains_key` per
  [contract-wasm-standards-best-practices.md §9](contract-wasm-standards-best-practices.md),
  not in a Sparse Merkle Tree.

### 0.1 The Representation Faithfulness Law

The storage of a spent nullifier is governed by a single invariant, named here
for the first time (previously it existed only as the two scattered rules in
§9.1 and §C.3.5 below):

> **Representation Faithfulness (Distinguished Witness).** A decidable, monotone
> observation (a barb) is faithfully encoded into a store iff its witness is a
> **distinguished element** — a value disjoint from the canonical "absent"
> element of the representation's carrier. The degenerate element (the empty
> byte string `ε`, or the zero field element `0`) is the identity of the
> representation monoid and the canonical witness of *no observation* (`∅`
> barbs); it can never witness a positive barb.

Consequences:

1. **The spent marker must be non-empty.** Writing `db_set(key, &[])` encodes
   "spent" with `ε`, which the read (`db_contains_key`/`db_get`) treats as
   *absent* — the `↓nullify` barb collapses into the null barb, so the marker
   silently fails to mark. The faithful witness is the non-empty marker `&[1]`
   ([contract-wasm-standards-best-practices.md §9](contract-wasm-standards-best-practices.md)).
2. **The nullifier must be non-zero.** `Nullifier::from_bytes` SHALL reject the
   zero element: a zero nullifier is the identity of the field, semantically
   equivalent to "no nullifier"
   ([contract-wasm-type-system.md §C.3.5](contract-wasm-type-system.md)).

The law is a corollary of Type Distinction (§2) and the barb monoid
([composition.md §1.2](composition.md)): the degenerate element inhabits
`rawBytes` (`∅` barbs), which SHALL NOT be unified with `Nullifier` (`↓nullify`);
a faithful witness must therefore be a non-degenerate representative.

The law is mechanized in Lean4 as
`proofs/lean/src/DarkFi/Combinatorial/NullifierStorage.lean` — the theorems
`markSpent_faithful` (marking is `recover s ∪ {n}`), `markEmpty_not_spent` and
`markEmpty_never_adds` (the empty marker is the defect), `markSpent_sound`
(decidable spent), `markSpent_monotone`, `markSpent_idempotent` (replay
rejection), and `faithful_iff_nonempty` (a marker is faithful iff non-empty).

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

| Barb | Domain | Observable Action |
|------|--------|-------------------|
| `↓spend` | consensus | Exercises a capability — consumes a resource and authorizes spawning of new resources. Requires possession of the capability's authorization secret. |
| `↓view` | wallet | Decrypts an encrypted note addressed to the holder, revealing the capability parameters it contains. |
| `↓nullify` | consensus | Publishes evidence that a capability instance has been exercised. Each exercise SHALL produce a unique nullifier. A nullifier appearing on-chain SHALL prevent re-exercise of the same capability instance. |
| `↓commit` | consensus | Publishes the public face of a capability as a Commitment. The commitment cryptographically binds the capability parameters while revealing none of them. |
| `↓prove` | consensus | Generates a zero-knowledge proof that the holder knows a witness satisfying the capability's predicate language, without revealing the witness. |
| `↓verify` | consensus | Verifies a zero-knowledge proof or digital signature. Returns acceptance only if the proof is cryptographically valid against the declared public inputs. |
| `↓dispatch` | consensus | Routes a capability exercise to the contract that recognizes it. The contract is identified by its ContractId. |
| `↓gate` | dispatch | Constrains capability exercise to a specific function. The function is identified by the contract's function code as declared in its manifest. |
| `↓denominate` | consensus | Identifies the capability class. Two capabilities with different AssetId values SHALL be distinguishable by verifiers, even if all other parameters are identical. |
| `↓prove-inclusion` | consensus | Proves membership of a commitment in a recognized set. In DarkWow: a Merkle proof from the commitment to a known Merkle root. |
| `↓encrypt` | wallet | Produces ciphertext that only the holder of the corresponding decryption secret can decrypt. Uses Diffie-Hellman key agreement to derive a shared secret. |
| `↓derive` | wallet | Derives a scoped sub-key from an existing secret. The derived key is bound to a specific contract instance and SHALL NOT be usable in other contexts. |
| `↓discover` | wallet | Detects capabilities addressed to the holder. In DarkWow: trial AEAD decryption of encrypted notes in block call data. |
| `↓mine` | mass_balance | Produces a valid coinbase commitment. The coinbase is the consensus mechanism that creates the native asset (DRKW). Requires possession of a MiningRecipient. |
| `↓concurrent` | execution | Executes in parallel with sibling processes. Requires no shared mutable state dependency between the processes. |
| `↓merge` | execution | Deterministically combines concurrent state diffs. Two processes with disjoint key sets SHALL produce mergeable state deltas. |
| `↓sync-barrier` | execution | Blocks until a synchronization condition is met. Used to coordinate processes across execution waves. |
| `↓broadcast` | Publishes a message to multiple subscribers simultaneously. The message SHALL be delivered to all active subscribers. |
| `↓rate-limit` | Constrains output rate for backpressure. The process SHALL NOT exceed its declared rate budget. |
| `↓gossip-forward` | Relays an inbound message to a subset of outbound peers. Forwarding SHALL exclude the origin peer. |
| `↓quorum-query` | Queries a threshold of peers and converges on agreement. Agreement requires a supermajority of queried peers. |
| `↓dag-parent` | References prior events in a partial-order data structure. The reference forms a directed acyclic graph edge. |
| `↓pay-fee` | mass_balance | Exercises FeeV2 — exercises a capability via nullifier, splits value into change + fee. Fee commitment accumulated into `fee_commit_accumulator`. See [fee-spec.md §5.6](consensus/fee-spec.md). |
| `↓collect-fees` | mass_balance | Exercises FeeCollectV1 — verifies PedersenCommit(total, blind) == fee_commit_accumulator, creates the fee commitment to the miner, resets accumulator and fees_db. See [fee-spec.md §4.2](consensus/fee-spec.md). |
| `↓threshold-prove` | fee_signalling | Proves hidden fee meets public threshold — gates mempool tier admission. Uses FeeThreshold_V1 circuit. See [mempool.md §5](mempool.md). |
| `↓bad-fee-amount` | mass_balance | input.value <= fee — rejected at FeeV2CallBuilder::build(). |
| `↓bad-threshold-proof` | fee_signalling | FeeThreshold_V1 verification fails — transaction rejected from mempool. See [mempool.md §5.2](mempool.md). |
| `↓bad-merkle-root` | mass_balance | Merkle root not found in commitment_roots_db — rejected at fee_v2 exec. |
| `↓zero-claim` | mass_balance | FeeCollectV1 total_fees == 0 — rejected as replay attack. |
| `↓bad-claim` | mass_balance | FeeCollectV1 PedersenCommit(total, blind) != fee_commit_accumulator — claimed amount mismatch against commitment sum. See [fee-spec.md §4.2](consensus/fee-spec.md). |
| `↓acc-init` | mass_balance | Writes Identity to fee_commit_accumulator at contract deployment. See [fee-spec.md §5.6.2.1](consensus/fee-spec.md). |
| `↓acc-read` | mass_balance | Reads fee_commit_accumulator from sled, decodes as AccumulatorPoint. See [fee-spec.md §5.6.2.1](consensus/fee-spec.md). |
| `↓acc-add` | mass_balance | Adds fee_value_commit to accumulator via Pedersen homomorphic addition. See [fee-spec.md §5.6.2.1](consensus/fee-spec.md). |
| `↓acc-verify` | mass_balance | Verifies PedersenCommit(total, blind) == accumulator. See [fee-spec.md §5.6.2.1](consensus/fee-spec.md). |
| `↓acc-reset` | mass_balance | Overwrites accumulator to Identity at block boundaries. See [fee-spec.md §5.6.2.1](consensus/fee-spec.md). |
| `↓bad-accumulator` | mass_balance | Accumulator decode failed: wrong size or invalid point. See [fee-spec.md §5.6.2.1](consensus/fee-spec.md). |
| `↓fee-window-open` | fee_signalling | Window boundary at `height ≡ 0 (mod N)`, height > 0. CF_premium and CF_standard recomputed from mempool queue depths. Fires exactly once per window boundary — the trigger for all subsequent window-transition actions. See [fee-spec.md §12.2](consensus/fee-spec.md). |
| `↓fee-window-advertise` | fee_signalling | Miner sets `fee_window_flags` in BlockHeader at the final block of a fee window. Encodes CF direction (hold/+10%/-10%) into the 4-bit congestion_multiplier field for wallet threshold discovery. See [fee-spec.md §12.6](consensus/fee-spec.md). |
| `↓fee-window-enforce` | fee_signalling | Mempool applies window thresholds to new transaction arrivals. Tx admitted to premium/general tier or rejected per fee-spec.md §12.8.1. FCFS within tier. Thresholds read via `AtomicU64::Acquire` on the mempool hot path. See [fee-spec.md §12.8](consensus/fee-spec.md). |
| `↓fee-window-discover` | fee_signalling | Wallet reads `fee_window_flags` from latest block header, decodes active thresholds via `WindowSignalling::decode_next_premium()`, constructs FeeThreshold_V1 proof against the correct threshold. If the window boundary passes before mining, wallet SHALL re-query and re-prove. See [fee-spec.md §12.9](consensus/fee-spec.md). |

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

### 1.3 Composite Barb Sets — The Wallet

The top-level wallet process (`Dww`) SHALL implement `ExhibitsBarb` with barb
set `[Discover, Spend, Verify, Encrypt, Decrypt, Derive, Broadcast,
SyncBarrier, Gate, Denominate]`. Each barb corresponds to an observable action
the wallet process may exhibit:

| Barb | Observable Action |
|------|-------------------|
| `↓discover` | Scans blocks for capabilities (Path 1 coinbase + Path 2 manifest) |
| `↓spend` | Constructs and broadcasts a transaction exercising a capability |
| `↓verify` | Validates chain continuity on synced blocks (previous-hash link) |
| `↓encrypt` | Produces AEAD-encrypted notes for contract calls |
| `↓decrypt` | Trial-decrypts AEAD notes to discover held capabilities |
| `↓derive` | Derives per-contract instance keys from declared identity |
| `↓broadcast` | Publishes transactions to the P2P network |
| `↓sync-barrier` | Synchronizes chain state from peers via GetTip/GetBlocks |
| `↓gate` | Routes contract calls to the correct ContractId + function selector |
| `↓denominate` | Identifies capability asset class (AssetId) |

This is the composite barb set of the capability type construction engine
(wallet.md §0). Every wallet operation SHALL exhibit at least one of these
barbs. An operation that exhibits none is an unobservable τ-transition and
SHALL NOT have side effects.

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

### 2.3 Consensus Numeric Domains Are Nominal

The distinction principle applies to consensus scalars exactly as it applies
to `pallas::Base` capabilities. A block height and a reward amount are both
representable as `u64`, but a process holding a height exhibits observably
different behavior (chain position, maturity, key derivation) from a process
holding an amount (value conservation, supply accounting). They SHALL NOT
unify.

- Block heights SHALL be the nominal type `BlockHeight` (inner `u64`)
  end-to-end: SDK, WASM host interface, contract models, chain store,
  daemon, and wallet. There SHALL be exactly one height domain — a parallel
  `u32` height domain SHALL NOT exist.
- A bare `as` cast on any consensus quantity (height, amount, supply)
  SHALL NOT pass review. Width conversions at FFI edges (the WASM import
  ABI is `i64`) SHALL use `try_from` with an explicit error path.
- The canonical byte encoding of a height SHALL be
  `BlockHeight::to_le_bytes() -> [u8; 8]`. Every hash preimage, key
  derivation, and sled key that includes a height SHALL use it.
- Persistence boundaries lift heights via the validating constructor
  (`BlockHeight::new` / `from_le_bytes`), per §2.2. No code path SHALL
  construct a height by directly accessing a `pub` field.

### 2.3.1b Nominal Type Methods (Documented 2026-08-07)

Each nominal consensus type follows the §8.5 pattern (`new(u64)`, `.get()`,
`to_le_bytes`/`from_le_bytes`, manual serde). Domain-specific methods beyond
the boilerplate are documented here. These methods SHALL be used in preference
to raw `.get()` extraction wherever the semantics match.

**BlockHeight** (`u64`): `succ() -> BlockHeight`, `pred() -> Option<BlockHeight>`,
`checked_sub(u64) -> Option<BlockHeight>`, `saturating_sub(u64) -> BlockHeight`,
`saturating_sub_blocks(u64) -> BlockHeight`, `is_zero() -> bool`,
`is_genesis() -> bool`, `to_field_element() -> pallas::Base`,
`from_sqlite_i64(i64) -> Option<BlockHeight>`, `to_sqlite_i64() -> Option<i64>`,
`to_sqlite_i64_saturating() -> i64`, `GENESIS: BlockHeight = new(1)`.

**BlockTarget** (`u32`): `reached(hash_u32: u32) -> bool`,
`hash_is_valid(hash_u32: u32) -> bool`, `adjust(scale, adjustment, min, max) -> BlockTarget`,
`chain_work() -> u128`, `difficulty() -> u64`, `MAX: BlockTarget = new(u32::MAX)`.

**BlockReward** (`u64`): `split_for_uncle(depth) -> BlockReward`,
`ZERO: BlockReward = new(0)`. Cross-domain conversion:
`impl From<BlockReward> for SupplyAmount` — reward becomes part of cumulative supply.

**SupplyAmount** (`u64`): `saturating_add(SupplyAmount) -> SupplyAmount`,
`ZERO: SupplyAmount = new(0)`.

**FeeAmount** (`u64`): `checked_add(FeeAmount) -> Option<FeeAmount>`,
`checked_sub(FeeAmount) -> Option<FeeAmount>`, `ZERO: FeeAmount = new(0)`.

**MoneroBlockHeight** (`u64`): Distinguished from `BlockHeight` — the two chains
advance independently. `serde_default()` for backward-compatible deserialization.

**BlockVersion** (`u8`): `CURRENT: BlockVersion = new(1)`. Included in hash
preimages to bind block identity to the protocol version.

### 2.3.2 No `unwrap()` on P2P Critical Path

`unwrap()` SHALL NOT appear on the P2P critical path. `Weak::upgrade().unwrap()`
in session lifecycles and `UNIX_EPOCH.elapsed().unwrap()` in channel dispatch
convert infrastructure failures into panics. Per the ρ-calculus, a process that
encounters a dead parent SHALL transition to 0 (inaction) with a logged error,
not crash the runtime.

`Weak::upgrade()` failures SHALL produce `ChannelStopped` or `SessionStopped`
errors. `UNIX_EPOCH.elapsed()` SHALL use `.unwrap_or(Duration::ZERO)` with
explicit comment explaining that a pre-epoch system clock is a fatal
configuration error detectable at the observable barb level.

This extends §2.3's prohibition against `unwrap_or(0)` and bare casts to the
signal path: processes communicate via typed channels, and a process death
signal is semantically distinct from a successful process completion.
Conflating them via `unwrap()` erases the `↓process-death` barb.

**Planned newtypes** (Change 4 of consensus type-enforcement plan,
`src/sdk/src/blockchain.rs`):
- `BlockReward(u64)` — coinbase reward amounts. Distinguished from
  `BlockHeight` so `expected_reward(height)` is a type error.
- `BlockTarget(u32)` — PoW target. Distinguished from bare `u32` so
  `target < reward` is a type error.
- `BlockCharge(u64)` — Declarative block capacity charge (potential energy,
  not measured work). Distinguished from `FeeAmount` and `BlockReward` so
  charge arithmetic cannot mix with fee or supply accounting. See
  fee-spec.md §12.4.3.

Each SHALL follow the `BlockHeight` pattern: `#[repr(transparent)]`,
named constructors, no `From<u64>`, no `Add`/`Sub` operators, manual
serde as plain number, `dwow_serial` transparent encoding. Consensus
outcome types (§4.1) extend the distinction principle to non-numeric
domains — the same logic that separates `BlockHeight` from `BlockReward`
also separates `CanonicalExtension` from `CompetingStored`.

### 2.3.1 Additional Consensus Domains (extends Change 4)

The original Change 4 identified three domains (block reward, PoW target,
gas amount). The 2026-07-22 audit identified four additional consensus
domains currently crossing module boundaries as raw `u64`. These SHALL
be lifted to nominal newtypes following the identical pattern:

**SupplyAmount(u64)** — cumulative and per-block supply in base units.
Distinguished from `BlockReward` (what is minted per block) because the
cumulative supply is an audit anchor, not a per-block quantity. Supply
audit computations (`total_supply + coinbase_reward`) MUST NOT silently
accept `total_supply + block_height`. The compiler SHALL reject cross-domain
arithmetic.

**FeeAmount(u64)** — transaction fees in base units. Distinguished from
`BlockReward` (minted supply) and `GasAmount` (computation measure) because
`fee = gas * gas_price` is a distinct economic domain. A function returning
`compute_fee(gas) -> u64` allows callers to add the result to a block reward
or supply without the compiler noticing. `compute_fee(gas) -> FeeAmount`
makes the domain visible at every call site.

**BlockTimestamp(u64)** — wall-clock time in seconds since UNIX epoch.
Distinguished from `BlockHeight` (logical chain position) because the
difficulty adjustment algorithm feeds timestamps into a sliding window.
Confusing a height for a timestamp biases the target adjustment. The
`MiningState` struct fields `last_block_time` and `template_height` are
both `AtomicU64` — the compiler SHALL reject transposition.

**MoneroBlockHeight(u64)** — a block height on the Monero blockchain.
Distinguished from our `BlockHeight` because the two chains advance
independently. The `BlockHeader.anchor_monero_height` field gates
merge-mining finality — confusing our height for the Monero anchor
height silently breaks the finality guarantee.

**CfValue(u32)** — congestion factor fixed-point value with SCALE = 1_000_000.
Distinguished from `BlockTarget(u32)` (PoW difficulty) because a CF multiplier
applies to fee admission thresholds, not to proof-of-work verification. The
`CongestionFactor` compound type encapsulates two `CfValue` components
(premium and standard). Direct `CfValue` extraction SHALL use `.premium()`
and `.standard()` accessors; fee arithmetic SHALL route through
`apply_premium(FeeAmount) -> FeeAmount` and `apply_standard(FeeAmount) -> FeeAmount`.

**RiskFactor(u64)** — execution risk multiplier in `RISK_FACTOR_SCALE` units
(100_000 = 1.0×). Distinguished from `FeeAmount(u64)` because a risk factor
is a dimensionless multiplier applied to circuit difficulty, not a payment
amount. Multiplying a risk factor by a fee SHALL produce a `FeeAmount`, not
a bare `u64`.

**WasmKb(u64)** — WASM deploy size in kilobytes. Distinguished from
`FeeAmount(u64)` because storage size and payment amount inhabit distinct
economic domains. The `compute_fee(WasmKb, CircuitDifficulty, ...)` function
takes these as distinct typed parameters; `compute_fee(fee_amount, fee_amount, ...)`
SHALL NOT compile.

**ThresholdAmount(u64)** — mempool admission threshold. Distinguished from
`FeeAmount(u64)` because a threshold gates admission (a policy parameter)
while a fee is paid (an economic value transfer). The mempool's
`verify_threshold_proof(tx, ThresholdAmount)` SHALL NOT accept a `FeeAmount`
without explicit conversion.

**EstimatedFee(FeeAmount)** — a fee value that is an ESTIMATE, not a
cryptographically verified amount. Distinguished from `FeeAmount` because
an estimate SHALL NOT participate in consensus-critical computation (block
hash, state root, FeeCollectV1 accumulator). The single constructor is
`EstimatedFee::baseline()` — computed from `compute_fee()` at the current
chain-synced congestion factors. This type explicitly SHALL NOT implement
`Copy` — every use site must acknowledge the estimate's uncertainty. An
`EstimatedFee` SHALL be converted to `FeeAmount` only through an explicit
acknowledgment method `.acknowledge_estimate() -> FeeAmount`; code audit
SHALL flag every call to this method.

Each SHALL derive `Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd,
SerialEncodable, SerialDecodable`. Each SHALL implement manual serde as a
plain JSON number (byte-identical wire format). None SHALL implement
`From<u64>`, `Default`, `Hash`, or `Add`/`Sub` operators. Construction
is `new(u64)` at domain entry points; extraction is `.get() -> u64` at
display/persistence boundaries only (§8.5).

Exception: `EstimatedFee` SHALL NOT implement `Copy`. It wraps `FeeAmount`,
not `u64`, and its marker semantics require explicit acknowledgment at each
use site. Its single constructor `EstimatedFee::baseline()` derives the
value from the current chain-synced congestion factors per `fee-spec.md` §13
SPEC-2; no `new(u64)` constructor exists.

### 2.3.3 AtomicU64 Dispensation for Lock-Free Hot-Path Thresholds

Consensus thresholds stored in `AtomicU64` for lock-free mempool admission
SHALL use the underlying `u64` at the storage layer. This is a Rust language
limitation — `AtomicU64` wraps a primitive integer and cannot wrap a
`#[repr(transparent)]` newtype. The following compensating controls SHALL apply:

1. **Single conversion boundary.** The conversion to/from the nominal type
   (`FeeAmount` or `ThresholdAmount`) SHALL occur at exactly one code location:
   the `update_thresholds` method (write) and the threshold accessor method
   (read). No other code path SHALL extract the raw `u64` from the `AtomicU64`.

2. **Private storage.** The `AtomicU64` field SHALL be private to the struct
   that owns it. External code SHALL interact with it exclusively through
   the typed accessor methods.

3. **No arithmetic on the raw value.** The extracted `u64` SHALL be immediately
   wrapped in the nominal type before any arithmetic or comparison. Threshold
   comparison logic (`fee >= threshold`) SHALL operate on `FeeAmount` values,
   not on bare `u64`.

4. **Documented dispensation.** Every `AtomicU64` field storing a consensus
   threshold SHALL carry a doc comment citing this section (§2.3.3) and
   explaining why `AtomicU64` is used (lock-free hot-path reads).

This dispensation applies to mempool admission thresholds, congestion factor
storage, and any future consensus domain requiring lock-free atomic access.
It does NOT apply to persistence (sled), wire format, or configuration —
those paths SHALL use the nominal type directly.

### 2.3.4 Compile-Time Enforcement of No Raw Unwrap

The prohibition on raw `.unwrap()`/`.expect()` is enforced at compile time,
not by convention. Every production crate SHALL carry, at its crate root
(`lib.rs`/`main.rs`, immediately after the license header):

```rust
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]
```

`#[cfg(test)]` code is exempt (test fixtures/assertions legitimately unwrap).

A panic-capable unwrap that is *provably safe* (locally-provable invariant,
compile-time constant, FFI/byte-encode boundary, or length-checked
`.try_into()`) is a **documented dispensation**, not a raw call:

```rust
#[expect(clippy::unwrap_used, reason = "type-system.md §2.3 — len checked above")]
```

`#[expect]` (Rust ≥ 1.81) documents *and* warns if the unwrap is later removed,
so the annotation cannot go stale. A raw `.unwrap()`/`.expect()` that CAN panic
on untrusted/boundary input SHALL be fixed (`?`, `if let`, typed error,
`copy_from_slice`) — never annotated away.

Known enforcement limitations (do not rely on the lint to catch these):

- `#[expect]`/`#[allow]` may only be placed on a `let` statement or item, not on
  an assignment or tail expression (E0658). Restructure into a `let` binding.
- clippy does NOT flag `.unwrap()`/`.expect()` on `subtle::CtOption` (the lint
  targets std `Option`/`Result` only), nor inside `macro_rules!` expansions.
  Audit these manually.

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

### 4.1 Consensus Outcome Types

The distinction principle (§2) applies to consensus outcomes exactly as it
applies to error barbs and numeric domains. A function that can produce
semantically different consensus outcomes SHALL return a nominal enum —
`Result<()>` SHALL NOT be used where the `Ok` variant collapses multiple
states.

**Applied:** `BlockConnectOutcome` at `src/linear/src/chain_state.rs`.
`connect_block()` previously returned `Result<()>` for three fundamentally
different outcomes:

| Outcome | Old Return | New Return | Caller Obligation |
|---------|-----------|-----------|-------------------|
| Block extended canonical chain | `Ok(())` | `Ok(CanonicalExtension{new_height})` | `mark_mined` permitted |
| Block stored as competing | `Ok(())` | `Ok(CompetingStored)` | mempool unchanged |
| Uncle chain extended | `Ok(())` | `Ok(UncleExtended)` | mempool unchanged |

The type system enforces that callers match all three variants. A caller
that previously called `mp.mark_mined()` on any `Ok(())` — removing mempool
transactions for a block that was silently demoted to competing — is now a
compile-time error. The `mark_mined` call SHALL appear only in the
`CanonicalExtension` arm.

This prevents HAZID H-H7 (mempool transaction loss on non-canonical blocks)
and H-H8 (competing block permanent loss) at the type level — no runtime
check, no convention, no code review can override the compiler's exhaustive
match enforcement.

**Planned:** `ConsensusPhase` enum (Change 5) extends this pattern to error
dispatch. Each of the 8 validation phases (0-7 per `consensus.md`) maps to a
specific `BarbId`:

| Phase | Barb | Recovery Strategy |
|-------|------|-------------------|
| 0 (Structural) | `↓bad-proof` | Reject block |
| 1 (PoW) | `↓bad-proof` | Reject block |
| 2 (Continuity) | `↓bad-proof` | Reject block |
| 3 (Nullifier + ZK) | `↓bad-nullifier` | Reject block, ban peer |
| 4 (WASM execution) | `↓bad-proof` | Reject block |
| 5 (Transactions) | `↓bad-proof` | Reject block |
| 6 (Nullifier set) | `↓bad-nullifier` | Reject block |
| 7 (Atomic commit) | `↓db-fail` | Fatal — restart node |

`LinearError::error_barb()` SHALL return `BarbId`, not `&str`. Callers
match on `err.phase()` for recovery strategy — "ban peer" vs "restart node"
vs "skip block" — without string matching.

### 4.2 Error Propagation Audit Requirements

The 2026-07-22 audit identified systematic error suppression patterns across
the wallet and sync codebase. In response, the following audit requirements
are codified as specification rules:

**§4.2.1 `let _` prohibition.** `let _ = fallible_call()` SHALL NOT appear in
any production code path. The `let _` pattern discards the Result without
inspecting it — the error is invisible to the compiler, the logger, and the
operator. Every Result SHALL be either:

- Propagated via `?` to the caller,
- Matched explicitly (`match` / `if let Err(e)` with a log at `warn!` or
  `error!` level), or
- Suppressed with `#[allow(unused_results)]` AND a comment explaining why
  the error is intentionally ignored (e.g., "best-effort migration ALTER
  TABLE — table may already exist").

**§4.2.2 `.ok()` prohibition.** `.ok()` SHALL NOT appear in any cryptographic
path or consensus path. `.ok()` converts `Result<T, E>` to `Option<T>`,
discarding the error reason. This conflates "operation succeeded" with
"operation failed for an unknown reason." In cryptographic paths (key
derivation, AEAD decryption, commitment verification, nullifier matching)
the reason for failure IS the signal.

**§4.2.3 `unwrap_or` audit.** `unwrap_or(default)` on a fallible operation
SHALL only appear when the default value is semantically correct for ALL
failure modes of the operation. `chain_height().unwrap_or(0)` is prohibited
because "database corrupted" is NOT semantically equivalent to "chain is
empty." The compiler SHALL enforce this where the return type is a nominal
newtype lacking `From<u64>` and `Default` — `unwrap_or(0)` becomes a type
error because `0` cannot coerce to the newtype.

**§4.2.4 typed diagnostic counters.** Every pure function that processes data
through a pipeline of fallible stages SHALL return structured diagnostics
counting attempts and successes at each stage (§Z: Diagnostic Transparency).
Callers SHALL be able to distinguish "no results found" from "all attempts
failed" by inspecting the diagnostics counters.

### 4.3 Seed Error Vocabulary

Seed error codes crossing the P2P wire SHALL be a `#[repr(u32)]` enum
`SeedErrorCode`, not raw `u32` constants. Every error variant IS a barb
(§4). A `match` on a `SeedErrorCode` is exhaustive; a range check on a
`u32` is not.

```
SeedErrorCode SHALL implement ExhibitsBarb with barb set [Gate].
```

Variants:
- `BadRequest = 400` — client error, request malformed
- `VersionMismatch = 401` — protocol version incompatible
- `Forbidden = 403` — peer not authorized
- `UnknownMessage = 404` — unrecognized message type
- `NoMatchingTransports = 406` — no compatible transport
- `RateLimited = 429` — sender exceeded rate budget
- `Internal = 500` — server error, retry may succeed
- `HostlistEmpty = 503` — no peers to share
- `UpstreamTimeout = 504` — upstream seed unreachable

The classification functions `seed_error_is_client_error()` and
`seed_error_is_server_error()` SHALL operate on the enum, not on raw `u32`.

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

### 5.1 Authority Marker Types

A bare `bool` SHALL NOT gate consensus-critical paths. A `bool` carries no
proof of key possession, no type-level distinction from any other `bool`,
and no compiler enforcement. Consensus authority SHALL be represented by
nominal marker types constructible only through proof of capability
possession.

**Applied:** `GenesisAuthority` at `bin/dwowd/src/task/consensus_linear.rs`
(Change 3). Previously `ConsensusInitTaskConfig.genesis_authority: bool` —
any misconfigured node with a typo'd TOML could claim genesis authority. The
replacement is a zero-sized marker type:

```rust
pub struct GenesisAuthority { _private: () }
impl GenesisAuthority {
    pub fn from_key(secret: &OwnedSecretKey) -> Option<Self> { ... }
}
impl ExhibitsBarb for GenesisAuthority {
    fn exhibited_barbs() -> &'static [BarbId] { &[BarbId::Mine] }
}
```

The `ConsensusInitTaskConfig.genesis_authority` field changes from `bool` to
`Option<GenesisAuthority>`. The "mine without peers" path in the sync state
machine requires `Some(authority)` — a node that lost its genesis key cannot
accidentally claim authority. The `ExhibitsBarb` impl witnesses the `↓mine`
barb at compile time: only processes possessing `GenesisAuthority` can
exhibit mining behavior.

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

**Multi-capability composition.** When an action involves multiple capabilities
— producing one type while consuming a different type — the capability type is
the composition of the UNION of primitives from all involved capabilities:

```
Capability(withdraw_from_purse) ≡
    compose(
        SecretKey(↓spend, ν-restricted),      // purse_balance ∩ purse_withdrawal
        Nullifier(↓nullify),                   // purse_withdrawal only
        Commitment(↓commit),                   // purse_balance only
        AssetId(↓denominate),                  // both
        FuncId(↓gate),                         // both
        ContractId(↓dispatch)                  // both
    )
```

This follows from the ρ-calculus: `x?(old).νnew.(y!(new) | Q)` — both `old`
(consumed) and `new` (produced) are in scope during the action. The composed
type must carry the barbs of both names because the action exercises authority
over both simultaneously. When the produced and consumed capabilities are the
same type, the union is identity and a single-capability composition suffices.

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

6. **Consensus outcome distinction.** Every function that can produce
   semantically different consensus outcomes SHALL return a nominal enum
   (§4.1). `Result<()>` SHALL NOT be used where the `Ok` variant collapses
   multiple states — canonical extension, competing block stored, and
   uncle chain extension are three distinct types of outcome. The compiler
   SHALL enforce that every caller handles all three.

7. **State machine transition validity.** Consensus state machines crossing
   module boundaries SHALL be typed enums with explicit variants (§9.3).
   Raw integer constants (`pub const X: u8 = N`) with manual
   `AtomicU8::load`/`store` SHALL NOT implement a distributed state machine.
   The compiler SHALL reject comparisons between states from different
   domains — `SyncState` and `BlockConnectOutcome` are distinct types with
   no implicit conversion.

## 8. Type Namespace

Every type in the DarkWow type system, its inner representation, the barbs
it exhibits, its scope, and its construction rules.

### 8.1 Cryptographic Primitive Types (Nominal)

These types are **nominal** — distinguished by their name and behavioral
position, not by their internal representation. Two primitive types with
identical internal representations (`pallas::Base`) SHALL NOT be unified
if their barbs differ.

> **There is no value-unit primitive, and there are no coins.** The value carriers
> are the **native token (DRKW)** and **capabilities** (promissory notes, box, purse).
> `Commitment` (↓commit) is the commitment face of a capability; the spending key is
> `SecretKey` (↓spend). The **only** "coin" is the **coinbase** (the `PoWRewardV1` mint);
> the term has no other meaning in this system.



| Type | Inner | Barbs | Scope | Construction |
|------|-------|-------|-------|-------------|
| `SecretKey` | `pallas::Base` | `↓spend`, `↓derive` | ν-restricted to holder | `from_bytes` (validates), `derive_instance` (binds to contract+instance) |
| `PublicKey` | `pallas::Point` | `↓verify`, `↓encrypt` | Extrudable | `from_secret`, `from_bytes` (rejects identity) |
| `Nullifier` | `pallas::Base` | `↓nullify` | Public | `new(secret, commitment)` only. `from_bytes` SHALL reject zero. |
| `Commitment` | `pallas::Base` | `↓commit` | Public | `from_attributes(pk, value, asset_id, spend_hook, user_data, blind)` |
| `ContractId` | `pallas::Base` | `↓dispatch` | Public | `derive(deploy_key)` or well-known constant |
| `AssetId` | `pallas::Base` | `↓denominate` | Public | `derive(auth_parent, user_data, blind)` or well-known constant |
| `FuncId` | `pallas::Base` | `↓gate` | Public | `from(contract_id, func_code)` |
| `MerkleNode` | `pallas::Base` | `↓prove-inclusion` | Public | Tree insertion |
| `AccumulatorPoint` | `pallas::Point` | `↓acc-read`, `↓acc-add`, `↓acc-verify`, `↓acc-reset` | Block-scoped | `identity()`, `decode([u8; 32])`, `add_commitment(Point)`. Spec: [fee-spec.md §5.6.2.1](consensus/fee-spec.md). SHALL NOT implement `Default`, `Copy`, `Sub`, `From<pallas::Point>` or `From<[u8; 32]>`. |
| `BlockHeight` | `u64` | `↓chain-position` | Public | `new(u64)`; `0` = pre-genesis sentinel, `1` = genesis. `from_le_bytes([u8; 8])` at persistence boundaries only. |
| `BlockVersion` | `u8` | `↓gate-version` | Public | `new(u8)`; `CURRENT = BlockVersion(1)`. Controls soft-fork signaling at the wire protocol level. Included in hash preimages to bind block identity to the protocol version. |

`BlockHeight` and `BlockVersion` are the non-`pallas::Base` primitives in
this table: nominal consensus scalars (§2.3). Their nominality guards the
numeric domain (height ≠ amount ≠ supply; version ≠ function selector),
not cryptographic secrets.

### 8.2 Structural Types

Structural types are organized by architectural domain per [fee-spec.md §0](consensus/fee-spec.md).
The domain column communicates whether a type participates in the consensus-critical
Pedersen mass balance proof (verified during `accept_block`) or the fee_signalling
coordination protocol (verified at mempool admission).

#### 8.2.1 Core Blockchain Types

| Type | Composition | Barbs | Domain |
|------|------------|-------|--------|
| `Transaction` | `{ version: u8, inputs: Vec<TxInput>, outputs: Vec<TxOutput>, contract_calls: Vec<ContractCall>, lock_time: u64, nullifiers: Vec<Nullifier>, witness: Vec<u8> }` | `↓process` | consensus |
| `TxInput` | `{ previous_output: blake3::Hash, script: Vec<u8>, sequence: u32 }` | — | consensus |
| `TxOutput` | `{ value: u64, script: Vec<u8> }` | — | consensus |
| `ContractCall` | `{ contract_id: ContractId, data: Vec<u8> }` | `↓invoke` | dispatch |
| `CoinbaseTransaction` | `{ proof: Vec<u8>, public_inputs: ZkPublicInputs<9>, commitment: Commitment, value_commit_x: PedersenCoordinate, value_commit_y: PedersenCoordinate, token_commit: TokenCommitment, nullifier: Nullifier, new_cumulative_x: PedersenCoordinate, new_cumulative_y: PedersenCoordinate, encrypted_note: Vec<u8> }` | `↓mine` | mass_balance |
| `Commitment` | `pallas::Base` — `C = poseidon_hash([pk.x, pk.y, value, asset_id, ...])` | `↓commit` | consensus |
| `TokenCommitment` | `pallas::Base` — `poseidon_hash([DRK_POSEIDON_DOMAIN_TOKEN_COMMIT, asset_id, token_blind])` | `↓denominate` | consensus |
| `PedersenCoordinate` | `pallas::Base` — one coordinate of a Pedersen value commitment | — | mass_balance |
| `ZkPublicInputs<N>` | `[[u8; 32]; N]` — N circuit-specific elements exposed to the verifier | `↓verify` | consensus |
| `BlockHeader` | `{ merkle_root, previous, height, ... }` — all merkle roots SHALL be `blake3::Hash` | `↓validate-pow` | consensus |
| `AeadEncryptedNote` | `{ ciphertext, ephem_public: PublicKey }` | `↓discover` | wallet |

#### 8.2.2 Mass Balance Types (consensus-critical)

These types participate in the Pedersen mass balance proof verified during
`accept_block`. Their `MassBalance` prefix communicates domain membership:
editing code that references these types means editing consensus-critical
block verification.

| Type | Composition | Barbs | Domain |
|------|------------|-------|--------|
| `MassBalanceCoinbaseV1CallData` | `MassBalanceCoinbaseV1Selector` (zero-sized) + `PoWRewardParamsV1` | `↓gate`, `↓mine` | mass_balance |
| `MassBalanceCoinbaseV1Selector` | Zero-sized witness type — hardcodes selector byte `0x05` | `↓gate` | mass_balance |
| `MassBalanceFeeCollectV1CallData` | `MassBalanceFeeCollectV1Selector` + `FeeCollectParamsV1` | `↓gate`, `↓collect-fees` | mass_balance |
| `MassBalanceFeeCollectV1Selector` | Zero-sized witness type — hardcodes selector byte `0x06` | `↓gate` | mass_balance |

#### 8.2.3 Dual-Domain Type (mass_balance + fee_signalling)

`MassBalanceFeeV2CallData` is the only dual-domain type. It carries both
`↓pay-fee` [mass_balance] (Pedersen value conservation, verified during
`accept_block`) and `↓threshold-prove` [fee_signalling] (fee ≥ threshold,
verified at mempool admission).

| Type | Composition | Barbs | Domain |
|------|------------|-------|--------|
| `MassBalanceFeeV2CallData` | `MassBalanceFeeV2Selector` (zero-sized) + `FeeParamsV2` | `↓gate`, `↓pay-fee`, `↓threshold-prove` | mass_balance + fee_signalling |
| `MassBalanceFeeV2Selector` | Zero-sized witness type — hardcodes selector byte `0x08` | `↓gate` | dispatch |

The `MassBalanceFeeV2Selector` is a zero-sized witness: it SHALL be constructible only via
`MassBalanceFeeV2Selector::new()` which hardcodes `0x08`. No `From<u8>` impl exists. Its
sole purpose is to witness the `↓gate` barb at the type level — the selector byte
is guaranteed by construction, never recovered from `data[0]` at runtime.

`MassBalanceFeeV2CallData` carries its own barbs (`↓gate`, `↓pay-fee`, `↓threshold-prove`).
A process receiving a `MassBalanceFeeV2CallData` observes these barbs on the name; it does
NOT inspect `data[0]` to determine the fee function. The `from_bytes()` constructor
is the single absorber boundary (§10.5) where raw bytes are re-lifted to the
nominal type.

`ContractCall` SHALL provide typed accessors that return `Option<MassBalanceFeeV2CallData>`,
`Option<MassBalanceCoinbaseV1CallData>`, and `Option<MassBalanceFeeCollectV1CallData>`. The `Option`
return forces the consumer to handle both `Some` (typed, barb-carrying) and
`None` (opaque bytes) — the compiler SHALL enforce this exhaustiveness.

A `Transaction`'s `witness` carries the ZK proofs and signatures as an opaque,
dwow_serial-encoded bundle. The witness SHALL be carried end-to-end (broadcast →
mempool → block) and verified at both mempool admission and block acceptance
([mempool.md](mempool.md)). The witness is EXCLUDED from the transaction hash —
block identity commits to transaction semantics (version, inputs, outputs,
contract_calls, lock_time, nullifiers), never to interchangeable witness bytes.
Stripping the witness erases the `↓prove`/`↓verify` barbs (§2.2) and defeats the
authority model (§5).

The `nullifiers` field carries pre-extracted nullifiers for the mempool's
double-spend detection ([mempool.md §2](mempool.md)). When empty, it SHALL be
omitted from the serialized form to preserve hash determinism across code versions.

### 8.2.4 BlockHash — Nominal Hash on the Wire

Block hashes crossing the P2P wire SHALL be the nominal type `BlockHash`, never
bare `String`. The `Tip.hash` field and `Tip.genesis_hash` field SHALL be
`BlockHash`. The wallet's `local_genesis_hash` SHALL be `Option<BlockHash>`.

```rust
#[repr(transparent)]
pub struct BlockHash(blake3::Hash);

impl ExhibitsBarb for BlockHash {
    fn exhibited_barbs() -> &'static [Barb] {
        &[Barb::Verify]
    }
}
```

`BlockHash` exhibits `↓verify` — a process holding a `BlockHash` can prove
it knows a specific chain position. A bare `String` exhibits no cryptographic
barbs. The reorg detection process at `sync_task.rs` SHALL compare `BlockHash`
values, not `String` values. The P2P absorber boundary (§10.5) SHALL enforce
this: `Tip` arriving on the wire SHALL deserialize `hash` as `[u8; 32]` (the
canonical blake3 output) and the `eval` step SHALL construct `BlockHash` via
`from_bytes`, rejecting invalid lengths.

Transaction construction — the exercise of a held capability — is specified in
[wallet.md §6](wallet.md). The Rust implementation is at
`src/linear/src/transaction.rs`.

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
| `MassBalanceFeeV2CallData` | `Vec<u8>` | `↓gate`, `↓pay-fee` [mass_balance], `↓threshold-prove` [fee_signalling] ≠ no barbs |
| `MassBalanceCoinbaseV1CallData` | `Vec<u8>` | `↓gate`, `↓mine` [mass_balance] ≠ no barbs |
| `MassBalanceFeeCollectV1CallData` | `Vec<u8>` | `↓gate`, `↓collect-fees` [mass_balance] ≠ no barbs |

### 8.5 Shared Derives

Every newtype over `pallas::Base` in §8.1 SHALL derive:

```
Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable
```

`ContractId` and `MerkleNode` SHALL additionally derive `Ord, PartialOrd`.
`Nullifier` SHALL additionally derive `Ord, PartialOrd`.

`BlockHeight` (inner `u64`, §2.3) SHALL derive
`Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, SerialEncodable,
SerialDecodable` — `Ord` per the `ContractId`/`Nullifier` precedent (map
keys, comparisons). Its dwow-serial encoding is transparent (the inner
`u64`), so the wire format of every structure that carries a height is
unchanged by the newtype. Its serde encoding SHALL be implemented manually
as a plain JSON number. `BlockHeight` SHALL NOT derive `Hash`, `Default`,
or implement `From<u64>` — construction is `new(u64)` at domain entry
points and `from_le_bytes` at persistence boundaries. It SHALL NOT
implement `Add`/`Sub` operators or `Step`; height arithmetic SHALL use the
named methods (`succ`, `pred`, `checked_sub`, `saturating_sub`) so intent
is explicit, and range iteration constructs `BlockHeight::new(h)` from a
`u64` loop variable.

No type in §8.1 SHALL derive `Hash`, `Default`, or `From<pallas::Base>`.
The `From<pallas::Base>` impl erases the type distinction — any field element
could become any capability. Construction SHALL use named constructors that
enforce validation (zero-rejection, canonical encoding, identity rejection).

Serialization for chain persistence (serde `Serialize`/`Deserialize`) SHALL
be implemented manually via `to_bytes()`/`from_bytes()` for each type. No
type SHALL derive serde directly — `pallas::Base` does not implement serde.

### 8.6 Wire-Layer Defense Requirements

The P2P message dispatcher provides two independent defense layers at the
wire boundary: `MAX_BYTES` (bounds check before deserialization) and
`METERING_SCORE` (contribution to global rate limiting). A message type
that sets both to zero has NO wire-layer protection — the payload size is
unchecked and the message does not count toward rate limits.

**§8.6.1 Layered defense.** Every P2P message type SHALL have at least one
active wire-layer defense. `METERING_SCORE=0` (bypasses metering) is permitted
ONLY when `MAX_BYTES > 0` (the dispatcher enforces a size bound).
`MAX_BYTES=0` AND `METERING_SCORE=0` simultaneously SHALL NOT appear on any
production message type.

**§8.6.2 Per-type bounds.** `MAX_BYTES` SHALL be set to a value that
accommodates the largest valid payload of that message type plus JSON
encoding overhead. A generous bound (e.g., 10MB for event batches, 2MB
for file chunks) is acceptable; zero (unlimited) is not — metering is not
a substitute for size bounds. The bound is a defense-in-depth measure —
application-level validation catches oversized payloads, but the wire layer
SHALL reject them before allocation. The `Blocks` sync
message type SHALL declare a generous but finite `MAX_BYTES` proportional
to the maximum block size plus encoding overhead.

**§8.6.3 Channel identity.** Each P2P channel SHALL carry exactly one
message type. Channel name strings SHALL be sufficiently distinct that
developer confusion between channels is unlikely. A listener subscribing
to a channel SHALL be able to exhaustively handle every message that can
arrive on that channel — the type system SHALL guarantee this by making
the channel type-parametric.

### 8.6.4 ChannelId — Nominal P2P Channel Identity

`ChannelId(u32)` SHALL be the nominal type for P2P channel identities. A P2P
channel SHALL be identified by a `ChannelId`, never by a bare `u32`. The
`ChannelInfo.id: u32` field SHALL be `ChannelId`. The `ProtocolGenericHandler`
sender/receiver routing keyed on `u32` SHALL be keyed on `ChannelId`.

```rust
#[repr(transparent)]
pub struct ChannelId(u32);

impl ExhibitsBarb for ChannelId {
    fn exhibited_barbs() -> &'static [Barb] {
        &[Barb::Gate, Barb::GossipForward]
    }
}
```

A bare `u32` channel identity exhibits no barbs. Per §0: "Every type SHALL
define the barbs that processes at that type may exhibit." A channel identity,
as the name on which synchronization occurs, is the most fundamental ρ-calculus
name in the P2P system.

### 8.6.5 SessionSelector — Nominal Session Topology

`SessionSelector` SHALL be a `#[repr(u32)]` enum replacing the bare
`SessionBitFlag = u32` type alias. Variants SHALL be: `Inbound = 0b000001`,
`Outbound = 0b000010`, `Manual = 0b000100`, `Seed = 0b001000`,
`Refine = 0b010000`, `Direct = 0b100000`.

```rust
#[repr(u32)]
pub enum SessionSelector {
    Inbound  = 0b000001,
    Outbound = 0b000010,
    Manual   = 0b000100,
    Seed     = 0b001000,
    Refine   = 0b010000,
    Direct   = 0b100000,
}

impl ExhibitsBarb for SessionSelector {
    fn exhibited_barbs() -> &'static [Barb] {
        &[Barb::Gate]
    }
}
```

The `Session::type_id()` method SHALL return `SessionSelector`. The
`ProtocolRegistry::attach()` and `register()` methods SHALL accept
`SessionSelector`. Bitmask operations on bare `u32` SHALL NOT appear
outside the `SessionSelector` impl. This follows the `SyncState` pattern
(§7.7) which replaced raw `u8` constants with a `#[repr(u8)]` enum.

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

The three process roles map to the three variants of `BlockConnectOutcome`
(§4.1, `src/linear/src/chain_state.rs`):

| Process | Outcome Variant | Effect |
|---------|----------------|--------|
| M (canonical miner) | `CanonicalExtension{new_height}` | Height advances, `mark_mined` permitted |
| U_i (competing miner) | `CompetingStored` | Block stored as uncle candidate, height unchanged |
| C (consensus observer) | `UncleExtended` | Uncle chain extended, height unchanged |

The `connect_block` return type enforces this at compile time: callers that
previously collapsed all three roles into `Ok(())` and called `mark_mined`
unconditionally are now a type error (invariant 6, §7).

**Sync State Machine.** The miner SHALL check `SyncState::CaughtUp` before
producing blocks. The sync task SHALL set `SyncState::Syncing` during active
download. The four-state machine is modeled as:

```
SyncMachine =
  Initial.sync_start.Syncing
  | Syncing.caught_up.CaughtUp
  | Syncing.detected_behind.Behind
  | CaughtUp.peers_ahead.Syncing
  | CaughtUp.detected_behind.Behind
  | Behind.retry_sync.Syncing
```

Previously implemented as four raw `u8` constants (`SYNC_INITIAL: u8 = 0`
through `SYNC_BEHIND: u8 = 3`) with manual `AtomicU8::load`/`store` across
5 files. Replaced by `SyncState` enum at `bin/dwowd/src/lib.rs` with
`#[repr(u8)]` and a single `SyncState::load(&AtomicU8)` accessor (Change 2).

Mapped to: `CChainState` at `src/linear/src/chain_state.rs:64`,
`competing_blocks: Mutex<HashMap<u64, Vec<Block>>>` at line 105,
`try_reorg_from_competing()` at line 982.
`SyncState` at `bin/dwowd/src/lib.rs`,
`BlockConnectOutcome` at `src/linear/src/chain_state.rs`.

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
O(k·N) total messages — optimal for epidemic dissemination. The send side
is implemented at `linear_broadcast.rs:206-256`. The receive-side relay
(`linear_broadcast.rs:394`) intentionally floods to all peers: height-gap
rejection (`C4` fix) dampens amplification (peers ahead of the block
silently skip it), and relay nodes amplify propagation for the remaining
subset with fewer hops than the initial fan-out. This is a deliberate
deviation from the SHALL above; future analysis may tighten the relay
fan-out to a structured subset.

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
blockchain barbs (↓spend, ↓nullify, ↓commit, ↓mine) SHALL NOT be routable
through the event graph channel. The `BarbWitness` trait at
`src/net/barb_trait.rs` provides the static check.

Event graph as blockchain P2P substrate: blockchain events SHALL be wrapped
in event content with marker byte `0x42` ('B' for blockchain) and routed
through DAG sync instead of flood broadcast. The event graph sled tree
(`dag`) remains quarantined from blockchain sled trees (`contracts`, `blocks`,
`commitments`, `nullifiers`).

### Implementation

**Status at revision 2938de1549:** the barb-typed layer has its first
production implementors. `BlockConnectOutcome` (`src/linear/src/chain_state.rs`)
carries `&[Commit, Verify, SyncBarrier]` — the three consensus outcome
variants declare their observable barbs at the type level. `SyncState`
(`bin/dwowd/src/lib.rs`) replaces raw `u8` constants with a `#[repr(u8)]`
enum. `GenesisAuthority` (`bin/dwowd/src/task/consensus_linear.rs`, Change 3
planned) replaces bare `bool` with a zero-sized marker type implementing
`ExhibitsBarb { &[Mine] }`.

The event-graph DAG path (dag_absorber.rs, §10.3) has been removed from
dwowd — block propagation uses flood broadcast via `net-node`/`net-wallet`
profiles which are architecturally gated from event-graph. The
`BridgeChannel` type-translation mechanism remains as future-use vocabulary.

The barb system is implemented across three modules:

**BarbId enum** (`src/barb.rs`): 22 observable actions — 14
authorization barbs (Spend, View, Nullify, Commit, Prove, Verify, Dispatch,
Gate, Denominate, ProveInclusion, Encrypt, Derive, Discover, Mine) and 8
concurrency barbs (Concurrent, Merge, SyncBarrier, Broadcast, RateLimit,
GossipForward, QuorumQuery, DagParent). Classification predicates:
`is_blockchain_barb()`, `is_event_graph_barb()`, `is_concurrency_barb()`.

**ExhibitsBarb trait** (`src/barb.rs`): Types declare their barb set at
compile time. First production implementors: `BlockConnectOutcome` and
`GenesisAuthority`. `bridge_safe::<Source, Dest>()` provides the static
quarantine check — blockchain barbs (↓spend, ↓nullify, ↓commit, ↓mine)
SHALL NOT cross to event-graph channels. Planned: wire `ExhibitsBarb` to
`LinearBlockchainProtocol`, `LinearSyncProtocol`, and `ProtocolTx` (Change 8).

**BridgeChannel** (`src/net/bridge_channel.rs`): Typed channel with
`BarbWitness<B>` phantom type parameter. `BridgeChannel<T, B>::pair()` creates
a `BridgeSender`/`BridgeReceiver` pair. The `B` parameter statically enforces
that a channel declared for blockchain messages cannot receive from an
event-graph process. Currently vocabulary only — zero production channels
use the typed bridge (the production P2P operates as a bytes-level absorber
with runtime enforcement per §10.5).

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
   `commitments`, `nullifiers`) vs event graph (`dag`)

### 10.5 Channel Boundaries as Barb Absorbers

P2P is one instance of a general principle — every channel with an in and an
out, a send and a receive per the ρ-calculus (§0), acts as a shock absorber
where the exponential cost of compositionally verifying every pair of
process types against the 22-dimensional barb space collapses to a per-channel
obligation. The collapse is purchased in exchange for four runtime obligations
at the boundary.

**Definition.** A *boundary* is a `quote(x)`/`eval(x)` edge (§0, §2.2): a
value carrying barbs is serialized to bytes (which have no behavioral
constraints — "any process can produce any 32 bytes"), transmitted across a
wire, and re-lifted at the receiving side through a validating constructor.
The serialization step is `Channel::send_message`
(`src/net/channel.rs:333-367`); the re-lift is `main_receive_loop`
dispatch (`channel.rs:491-613`), keyed on a runtime command name — the
runtime representation of the ρ-calculus channel name.

**The shock absorber claim (complexity collapse).** In an open network,
parallel composition `P₁ | … | Pₙ` synchronizes on named channels. Without
channel-typing, verifying the composite requires reasoning about all pairwise
interactions against all reachable barb combinations — O(n²) pairs against a
2^22 space. A channel that declares a barb set S ⊆ B factors this: each
process is checked once against each channel it uses (does its point lie
inside S?), and channels are checked once against the quarantine predicate
(`bridge_safe`). Obligation drops to Σ|Sₖ| — linear in channels, bounded per
channel. This is exactly the design embodied by the `bridge_chain_evg:
Channel<BridgeMessage> exhibits {↓commit, ↓verify}` declaration and by every
`impl_p2p_message!`'s `MAX_BYTES` and metering constants — a declared budget
vector per channel.

**The four runtime obligations.** Every byte erasure at a boundary creates
obligations that the compiler alone cannot discharge, because the sender is a
remote process whose code the local compiler cannot see:

1. **Re-lift validation.** The receiving side SHALL validate every byte
   sequence through the named constructor of the expected type
   (`from_bytes` rejecting zero/identity; `from_le_bytes` for consensus
   numeric domains (§2.3); `try_from` for width conversions at FFI edges).
   Untrusted bytes SHALL NOT be treated as a typed value without passing
   through the constructor's validation.
2. **Violator exclusion.** A process that persistently sends invalid messages
   (undeclared channels, oversize payloads, decoding failures) SHALL be
   excluded from further communication. `ban()` → `HostColor::Black` at
   `src/net/channel.rs:616-669` is the primary mechanism; the `hosts.rs`
   quarantine gates (`move_host`, `filter_addresses`) provide perimeter
   enforcement.
3. **Rate discipline.** Every channel SHALL declare a metering budget, and
   the receiving side SHALL enforce it to prevent unbounded resource
   consumption. `MeteringQueue` at `src/net/metering.rs` processes
   `MeteringScore` per message. The `↓rate-limit` barb SHALL be enforced
   mechanically at the boundary — the remote peer's behavior determines
   compliance; the local side detects and responds.
4. **Budget declaration.** Every channel SHALL declare maximum message
   sizes (`MAX_BYTES`) and metering configuration at message-definition
   time. The declaration is a compile-time constant; the enforcement is
   runtime at the boundary — `MAX_COMMAND_LENGTH = 255` and per-message
   `MAX_BYTES` budget.

**Static/dynamic split.** The properties in §11.1–11.5 (pareto-efficiency,
non-unifiability, barb preservation under composition, authorization
inversion, wallet construction soundness) are discharged by the compiler or
the Lean proof assistant — they are statements about the interior of a
process net. The four obligations above are statements about the *boundary*
and are discharged by runtime enforcement. A test that witnesses a boundary
obligation is therefore a type-system test — it verifies that the declared
barb set actually holds against an adversary who can send arbitrary bytes,
which phantom types cannot prevent. The separation of concerns is NOT
"P2P infrastructure vs blockchain vs type system" but **statically-proven
interior ⊆ absorber boundary ⊆ dynamic residue**.

**Per-boundary test obligations.** Every declared SHALL at a boundary SHALL
have at least one runtime witness test. The boundary families and their
enforcement mechanisms are:

| Boundary | Re-lift validation | Violator exclusion | Rate/budget | Witnessed by |
|----------|--------------------|--------------------|-------------|--------------|
| P2P wire (`channel.rs`) | `from_bytes`/`AsyncDecodable` per message | `ban()` → Black; `hosts` quarantine | `MeteringQueue`; per-message `MAX_BYTES`, `MAX_COMMAND_LENGTH` | `src/net/tests.rs` (command-length, message-length, MissingDispatcher bans; `p2p_test` hostlist) |
| Mempool admission (`zk_verifier.rs`) | `decode_and_reconcile`; nullifier checks; proof-presence structural check | Transaction dropped on admission failure; blacklist-able peer by caller | Gas-limit equivalent per block | `src/linear/src/zk_verifier.rs` tests |
| **FeeV2 call data absorber** (`mass_balance_call_data.rs`) | `MassBalanceFeeV2CallData::from_bytes(&data)` — validates `data[0] == 0x08` AND `FeeParamsV2::decode(&data[1..])` succeeds; returns `Option<MassBalanceFeeV2CallData>`, never inspects `data[0]` at call sites | `Option::None` path skips FeeV2 admission — no false routing of garbage bytes to the fee path | None (type-level only) | Unit tests on `MassBalanceFeeV2CallData::from_bytes` (valid, invalid selector, truncated data, malformed params) |
| Contract entrypoints (`execution.rs`) | `ContractId::from_bytes`; entrypoint data-length gating; auto-validating `deserialize` on typed params | Call failure reverts to checkpoint; canonical failures reject the block | `BLOCK_GAS_LIMIT` | Contract WASM tests (per-contract) |
| Wallet manifest (`manifest.rs`) | Closed vocabularies for parameter types, barbs, primitives — unknown name = parse error, not passthrough | Typed error barbs returned to caller; no fallback | TOML length / field count caps; circuit witness binding depth | SDK manifest tests; Lean `walletConstruct_sound` |
| Persistence (`store.rs`/`walletdb.rs`/`supply_chain.rs`) | `from_le_bytes`/`from_bytes` named constructors; sled key width is canonical 8-byte LE (§2.3) | Write failure returns `Result::Err` — no silent truncation | B-tree key ordering; SQLite `INTEGER` domain | `chain_state.rs` persistence round-trip tests |
| WASM host FFI (`import/*.rs`) | `acl_allow` section-check; `try_from` width conversions at i64 boundary; return-data len `u32::try_from`; buffer size caps | Access denied for unauthorized sections; negative error codes | `subtract_gas` per operation; host-object count cap | Contract tests (indirect via execution) |
| C FFI + JSON-RPC (`ffi.rs`; RPC handlers) | Null-pointer checks; buffer-len caps; `catch_unwind` isolation; `BlockHeight::new` param lift | Error buffer return; JSON error response | Output buffer sizes; `MAX_BLOCK_SIZE` | RPC-level tests; wallet FFI integration |

**Tests SHALL NOT re-verify interior facts.** A test whose failure condition
is "the code failed to compile" (e.g. a type error on `BlockHeight→u64`
mismatch, a barb-set declaration change that the compiler catches) SHALL be
demoted or removed. The compiler IS the test for interior facts. The full
workspace test suite SHALL NOT be run as a gate on a compile-proven type
change — the residual risk after such a change is bounded to the
persistence-boundary lift (re-lift of the new bytes via `from_le_bytes`),
which a targeted boundary witness (§2.3, see `chain_state.rs` persistence
round-trip) covers.

**Enforcement-symmetry rule.** Every enforcement mechanism at a boundary SHALL
be explicitly enabled by the test fixture (not left to feature-driven
defaults). `BanPolicy::Strict` and `p2p_local: true` SHALL be pinned in
network-layer test `Settings` so enforcement does not silently depend on a
build-profile feature flag — the `ban-policy` cfg lattice and `BanPolicy`
default-flip producing a silent no-op for loopback bans is the exhibited
failure of letting enforcement be a lattice of cfg + default.

The barb-layer wiring onto `impl_p2p_message!` (`const BARBS: &'static [BarbId]`)
and production `ExhibitsBarb` implementors makes this boundary measurable:
per-channel barb-set cardinality SHALL be snapshot-tested, and any unreviewed
increase SHALL fail CI — the alarm for an absorber channel drifting toward
the union of both path-sets, at which point the composition cost returns to
exponential.

### 10.5.1 Sender Rate Discipline

Every P2P process that sends messages SHALL declare an outbound rate budget.
The wallet sync loop SHALL self-limit `GetTip` and `GetBlocks` request rates
per peer. The `↓rate-limit` barb SHALL be exhibited on outbound sync channels.
Rate discipline at the sender boundary is the dual of metering at the receiver
boundary (§8.6.1): the receiver enforces a budget for inbound messages; the
sender SHALL NOT exceed a declared budget for outbound messages.

The wallet's sync loop at `sync_task.rs` SHALL track per-peer `GetTip` and
`GetBlocks` send rates. A peer that has received `N` sync requests in the
current rate window SHALL be deferred to the next tick. The rate limit is
a self-imposed constraint, not a network-enforced one — a wallet that
violates its own budget is a bug, and the `↓rate-limit` barb SHALL be
observable in structured diagnostics (§4.2.4) to distinguish "waiting for
peers" from "self-throttled."

### 10.5.2 Frame-Aligned Inbound Stream (Interior)

The inbound P2P path is one ρ-process per channel: `recv_loop ≡ recv_command · (dispatch ⊕ drain)`.
The wire frame is `[magic(4) ‖ cmd_len ‖ command ‖ msg_len ‖ payload]`; `command` is the runtime
representation of the ρ-calculus channel name (§10.5), and `msg_len ‖ payload` is the message body.

- `recv_command : Stream → (Command × Stream)` reads the header (`magic + cmd_len + command`) at
  `src/net/channel.rs:379-434`.
- `dispatch : Command → Stream → (Message × Stream)` reads `msg_len + payload` through a registered
  dispatcher (`trigger`, `src/net/message_publisher.rs:261-303`).
- `drain : Stream → Stream` reads `msg_len + payload` and discards it (no dispatcher registered).

**Invariant (frame alignment).** `recv_command` and `(dispatch ⊕ drain)` are the two halves of the
*same* frame read. They SHALL be composed so the receive loop consumes exactly one whole frame per
step — the stream is at a frame boundary at the top of every iteration, and a frame is either
dispatched or drained, never left half-read. A `MissingDispatcher` return that does not read
`msg_len ‖ payload` violates this: the next `recv_command` misreads the leftover payload bytes as the
`magic` header (`Magic bytes mismatch` → `Malformed packet` → teardown/reconnect) — the exact defect
that broke the Docker wallet receive path.

**Interior, not a boundary obligation.** Unlike the four §10.5 obligations (re-lift / ban / rate /
budget — runtime, because the sender is a remote process the local compiler cannot see), frame
alignment is the local receive loop's own discipline: it depends only on the local read of the stream,
so it is provable in the calculus of constructions and enforceable at compile time. The Lean proof is
`DarkFi.Net.Framing` (`proofs/lean/src/DarkFi/Net/Framing.lean`): `dispatchOrDrain_total` (every frame
is dispatched or drained) and `recvLoop_frame_aligned` (the loop consumes one whole frame per step).
The Rust receive loop SHALL therefore be a total `dispatch ⊕ drain` fold — a `read_frame` that returns
either a dispatched message or a drained frame, so a caller cannot hold a half-read stream.

## 11. Verified Properties

The type system defined in this document is formalized in the Lean4 calculus
of constructions at `proofs/lean/src/DarkFi/Capability/`. The following
theorems are proved or stated with explicit verification status.

### 11.1 Pareto-Efficiency of the Primitive Type Namespace

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Pareto.lean`

`primitiveTypesAreParetoEfficient`: All 17 primitive types have pairwise
distinct barb sets. No type distinction can be removed without losing
behavioral information. Proof: `dec_trivial` over the finite list of
`Finset Barb` values.

15 named pair-distinction theorems provide human-readable cross-references
for each pair in §8.1 and §8.3 (e.g., `secretKey_distinct_from_nullifier`,
`ownedSecretKey_distinct_from_miningRecipient`).

`barbEqualityImpliesTypeEquality`: If two primitive types have identical
barb sets, they are the same type. This is the contrapositive of
pareto-efficiency — no accidental unification is possible.

### 11.2 Non-Unifiable Pairs

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Distinction.lean`

All 10 pairs in §8.4 are proved distinct (`native_decide`). The conjunction
`allUnifiablePairsProved` bundles them for single-reference verification:
Nullifier ≠ [u8; 32], Commitment ≠ [u8; 32], SecretKey ≠ [u8; 32], ContractId ≠
[u8; 32], PublicKey ≠ pallas::Point, SecretKey ≠ pallas::Base, FuncId ≠
pallas::Base, AssetId ≠ pallas::Base, Nullifier ≠ IntentNullifier,
OwnedSecretKey ≠ SecretKey.

### 11.3 Barb Preservation Under Composition

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Composition.lean`

`barbPreservation`: If a primitive type `p` is in the composition list, then
every barb of `p` is in the composed barb set. Proof: structural induction
on the primitive list. This guarantees that composing capability types does
not erase barbs — the fundamental requirement for emergent type construction.

### 11.4 Authorization Inversion (Type-Level)

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

`verifierLearnsOnlyRequiredBarbs` (`verifierMinimumDisclosure`): The verifier
learns NO barbs beyond those explicitly declared as required. This is the
privacy property of the Authorization Inversion — the ZK proof reveals only
the predicate result, not the witness.

### 11.5 Wallet Type Construction Soundness and Completeness

**Status:** PROVED. `proofs/lean/src/DarkFi/Capability/Wallet.lean`

`walletConstruct_sound`: If `walletConstruct` returns a capability type, the
required barbs are covered by the composed primitives.

`walletConstruct_complete`: If a `CapabilityType` exists for primitives `p`
and resource `r`, then `walletConstruct p r` returns `some` (not `none`).

`walletConstruct_preservesPrimitives`: The primitives returned are exactly
the primitives passed in — no loss, no modification.

`walletConstruct_deterministic`: Same primitives + same resource → same
TypedCapability every time (deterministic pure function).

`walletConstruct_idempotent`: Constructing twice with identical arguments
produces identical results.

`walletConstruct_rejects_emptyPrimitives`: An empty primitive list always
returns `none` — soundness gate closed when no primitives are provided.

Four concrete constructibility proofs verify that native token transfer,
DAO vote, tender bid, and coinbase claim capability types are constructible
from their respective primitive lists (`nativeTokenTransferExists`,
`daoVoteExists`, `tenderBidExists`, `coinbaseClaim_constructible`).

### 11.6 Full ZK Proof System Model

**Status:** FUTURE WORK. Not yet formalized.

The type-level Authorization Inversion is proved. The full ZK proof system
model (Halo2 constraint semantics, polynomial commitments, Fiat-Shamir
transform) in Lean4 is future work. When complete, `circuitSoundnessBridge`
will be replaced with a proved theorem referencing the Halo2 formalization.

### 11.7 Frame-Aligned Inbound Stream

**Status:** PROVED. `proofs/lean/src/DarkFi/Net/Framing.lean`

`dispatchOrDrain_total`: every inbound frame has a defined outcome — it is
either dispatched (a registered dispatcher decodes its payload) or drained (no
dispatcher; the payload is discarded). There is no third, half-consumed state.

`recvLoop_frame_aligned`: the receive loop consumes exactly one whole frame per
step, so its output is the `filterMap` of `dispatchOrDrain` over the frame
stream — no interleaving, reordering, or partial consumption. This is the
compile-time guarantee that the local receive loop never leaves `msg_len ‖
payload` unread, which is the defect that caused the `Magic bytes mismatch`
desync (§10.5.2).

### 11.8 Receive Decrypt Soundness + Fee-Window Boundary

**Status:** PROVED. `proofs/lean/src/DarkFi/Net/Receive.lean`,
`proofs/lean/src/DarkFi/Fee/Window.lean`

`decrypt_sound` (`↓discover`, wallet.md §2.1): a note decrypts to a capability
only when the trial key is the note's recipient key — a wrong-key wallet
discovers nothing. This is the invariant the transfer receive path preserves;
the Docker transfer-receive divergence must therefore be a key/address or
sync-ordering defect, not a decrypt-logic defect.

`window_boundary_emission` (`pre_boundary_no_emission`, fee-spec.md §12): a node
at height < `WINDOW` has not yet emitted a fee-window boundary log. That
pre-boundary state is legal, so the multi-node consensus check treats it as
"not yet reached" (warn + continue), never an abort.

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
