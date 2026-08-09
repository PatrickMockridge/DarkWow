# Mempool: The Pending-Transaction Pool

This document specifies the DarkWow mempool — the **pending-transaction pool**. It is the
node-side counterpart to the wallet's provisional state ([wallet.md §6.5](wallet.md)): the
formally-delimited "in-between" where a transaction lives after it is broadcast and before
it is confirmed in a block. It SHALL be read together with the
[Type System Specification](type-system.md) (error barbs §4, authority §5, the
`Transaction` type §8.2) and [Wallet Architecture](wallet.md) (the write path, §6). It
uses SHALL, MUST, SHALL NOT per RFC 2119.

## 0. Foundation: The In-Between

A transaction has three states of existence with respect to the chain:

1. **Constructed** — held by the wallet, not yet broadcast ([wallet.md §6](wallet.md)).
2. **Pending** — broadcast and admitted to the mempool, not yet in a block. *This document.*
3. **Confirmed** — included in an accepted block, discoverable by scan
   ([wallet.md §2](wallet.md)).

The mempool is the pool of Pending transactions. It exists so that miners can select
transactions to include in a block, and so that wallets can observe that a broadcast
transaction has propagated. Introducing this pool is the point at which node and wallet
state stop being a pure function of the confirmed chain alone; the specification below is
what keeps that in-between state sound.

## 1. The Pool Is a Set of Verified Pending Transactions

**Admission SHALL be a total function** `admit(tx) → Admitted | Rejected(barb)`. A
transaction is admitted if and only if it passes **every** admission check; otherwise it
is rejected with a typed error barb ([type-system.md §4](type-system.md)).

Admission checks (all REQUIRED, matching `Mempool::add()` at
`crates/dwow-mempool/src/lib.rs`):

- **Non-empty.** The transaction SHALL have at least one contract call or input.
  An empty transaction SHALL be rejected.
- **Size.** The serialized transaction SHALL NOT exceed `max_tx_size`.
- **`↓bad-nullifier` / on-chain nullifier check.** Every nullifier in
  `Transaction.nullifiers` SHALL be checked against the confirmed nullifier set
  (`cs.has_nullifier(n)` at `lib.rs:298-307`). Nullifiers already spent on-chain
  SHALL be rejected at admission.
- **`↓bad-nullifier` / in-pool nullifier dedup.** No two pending transactions
  SHALL share a nullifier (`lib.rs:288-295`).
- **fee.** The transaction SHALL carry a valid `FeeThreshold_V1` proof (§5.2).
  The fee is denominated in DRKW and embedded as a Pedersen commitment in
  FeeV2 call data (`0x08`). Coinbase transactions (PoWRewardV1, function
  `0x05`) are exempt from the fee requirement.
- **Dedup.** The transaction's hash SHALL NOT already be in the pool.
- **Eviction.** If the pool is at capacity, the lowest fee-rate entry SHALL be evicted
  before inserting a higher fee-rate transaction. Stale entries (older than
  `max_age_secs` = 12,000 (100 blocks at 120s block time, matching
  [wallet.md §6.5](wallet.md) `MEMPOOL_WINDOW`)) SHALL be evicted before each insertion.

> **Note:** Full ZK proof verification and signature verification occur at the
> block acceptance layer (`bin/dwowd/src/block_acceptor.rs:116`,
> `bin/dwowd/src/proto/protocol_tx.rs:133-144`), not in the mempool crate.
> The mempool performs structural, economic, and nullifier checks at admission;
> cryptographic verification is deferred to block acceptance to avoid redundant
> work — a proof valid at admission might become invalid by the time the block is
> mined. The invariant is maintained: no unverified transaction can enter a block
> because block acceptance rejects it before the block is connected.

The pool SHALL carry the **full** transaction — `contract_calls`, `witness`
(containing ZK proofs, signatures, and tx_commitment as an opaque bundle), and
`nullifiers` ([type-system.md §8.2](type-system.md)). The witness SHALL NOT be
stripped on the path from broadcast to the pool; a pool entry that cannot be
re-verified is not a valid pool entry.

> **Invariant (Authenticated Pool).** The mempool SHALL NOT hold a transaction with
> nullifiers already spent on-chain. Equivalently: a transaction whose nullifiers
> appear in the confirmed nullifier set SHALL never occupy the pool, be selected by
> a miner, or be accepted into a block. Full cryptographic verification (ZK proof
> and signature) occurs at block acceptance time (`bin/dwowd/src/block_acceptor.rs`),
> not at mempool admission — an unverified transaction cannot enter a block because
> block acceptance rejects it before connecting.

This invariant is the positive statement of the authority model: **authentication is the
authority mechanism, checked before propagation.** A pool that admitted unverified
transactions would let a party move value it cannot prove it holds — precisely the failure
the invariant forbids.

## 2. Dedup and Consistency

- **Nullifier uniqueness across the pool.** No two pending transactions SHALL share a
  nullifier. A transaction whose nullifier is already claimed by a pending transaction
  SHALL be rejected (`↓double-spend`).
- **Nullifier uniqueness against the confirmed set.** A transaction whose nullifier is
  already spent on-chain SHALL be rejected. Admission SHALL consult the confirmed nullifier
  set, not only the in-pool set — a nullifier is a name consumed exactly once
  ([type-system.md §0](type-system.md); the replication/nullifier model).
- **Monotonic removal on inclusion.** When a block is accepted, every transaction it
  includes SHALL be removed from the pool. A node that mines its own block SHALL remove the
  included transactions from its own pool on success — not only upon receiving a peer's
  block. A pool that re-served an already-mined transaction would produce a block the
  contract layer rejects as a double-spend, halting production; the removal rule forbids
  this (a liveness requirement).
- **Staleness eviction.** A transaction that remains pending beyond a bounded lifetime SHALL
  be evictable, releasing the wallet's reservation ([wallet.md §6.5](wallet.md),
  `Dropped → Unspent`). Eviction is a liveness rule; it SHALL NOT be the primary
  double-spend guard — that is nullifier uniqueness, above.

## 3. Observability

The pool SHALL expose a query interface so that both miners and wallets can observe its
contents:

- **Miners** select from the pool to assemble a block; selection SHALL be by fee priority
  and SHALL NOT admit an unverified transaction (§1).
- **Wallets** observe the pool to advance a broadcast transaction's status from `Broadcast`
  to `Pending`, and to detect `Dropped` ([wallet.md §6.5](wallet.md)). The wallet's
  provisional state is reconcilable **only because** the pool is observable: without a
  query/subscription contract, a wallet cannot distinguish "propagated and pending" from
  "lost."

This query interface is the formal basis for "mempool visibility to miners and the wallet."
It exposes pending-transaction identity and status; it SHALL NOT expose witnesses or private
note contents — those remain AEAD-encrypted, discoverable only by the holder
([wallet.md §2](wallet.md)).

## 4. Relationship to Consensus

Mempool admission (§1) and block acceptance are the **two** verification points, and they
verify the **same** transaction:

- **Admission** verifies before propagation, so an invalid transaction never spreads.
- **Block acceptance** re-verifies at inclusion, so a node that syncs a block it did not
  admit — including historical blocks — independently validates every transaction. Because
  the block persists the full transaction (proofs included), a syncing node has exactly what
  it needs to re-verify; block acceptance SHALL perform this verification and SHALL NOT rely
  on the mempool having done so.

This two-point discipline (verify on admission and on accept) is what makes the
Authenticated-Pool invariant (§1) hold network-wide rather than node-locally.

## 5. Two-Tier Threshold Admission

FeeV2 transactions carry a hidden fee behind a Pedersen commitment. The mempool
cannot sort by fee-per-byte directly — it uses FeeThreshold_V1 ZK proofs to gate
admission without learning individual fee amounts. Specification:
[fee-spec.md §5.5](consensus/fee-spec.md).

### 5.1 Architecture

| Tier | Proof Required | Ordering | Purpose |
|------|---------------|----------|---------|
| Premium | `fee >= premium_threshold` | FIFO (arrival order) | Urgent transactions |
| General | `fee >= general_threshold` | FIFO after premium exhausted | Normal transactions |
| Rejected | — | — | Fee below general threshold |

### 5.2 Admission Algorithm

```
admit(tx):
  // Extract fee commitment and threshold proof from call data
  fee_commit = extract_fee_commitment(tx)
  if fee_commit is None → REJECT (not a FeeV2 transaction)

  // Verify against premium tier
  if verify_threshold_proof(tx, premium_threshold):
    admit_to_premium_queue(tx)
    return ADMITTED

  // Verify against general tier
  if verify_threshold_proof(tx, general_threshold):
    admit_to_general_queue(tx)
    return ADMITTED

  // Fee below all thresholds
  REJECT ↓bad-threshold-proof
```

### 5.3 Block Selection

`select_for_block(max_gas, max_txs)`:
1. Drain premium queue in FIFO order until `max_gas` or `max_txs` reached.
2. Drain general queue in FIFO order until limits reached.
3. Return selected transactions. Selection is non-destructive — call
   `mark_mined` after block acceptance to remove confirmed transactions.

### 5.4 Threshold Values

`PREMIUM_THRESHOLD` and `GENERAL_THRESHOLD` are consensus-critical values
derived from `compute_fee()` at each fee window boundary (see
[fee-spec.md §12](consensus/fee-spec.md)). The genesis block defines initial
values; thereafter the PID-controlled CongestionFactor governs adjustments.
Miners signal updated thresholds in the `fee_window_flags` field of each
block header at the final block of the fee window.

## 6. Threshold Discovery

Current threshold values are published in the `fee_window_flags` field of
each block header at the final block of the fee window (see
[fee-spec.md §12.6](consensus/fee-spec.md)). Wallets and other nodes
discover thresholds by reading the latest block header — no separate P2P
announcement protocol is required. Block headers are already validated
during chain sync; `fee_window_flags` are part of the canonical header.

### 6.1 FeeWindowFlags Format

`fee_window_flags` is a `u16` encoding CF direction for both circuit and
WASM dimensions:

| Bits | Field | Values |
|------|-------|--------|
| 0:2 | CIRCUIT_CF direction | 0b000=hold, 0b001=+10%, 0b010=−10% |
| 3:5 | WASM_CF direction | (same encoding) |
| 6:15 | Reserved | Must be zero |

The wallet replays fee window history from genesis (deterministic per I1)
to maintain the current absolute CF values. The flags provide the direction;
chain replay provides the magnitude.

## 7. Fee Structure

The fee model has exactly two components: storage (WASM) and computation
(ZK circuits). Sled writes are deterministic — they are not priced as a
separate resource dimension. The two-component formula (see
[fee-spec.md §12](consensus/fee-spec.md)) is:

```
fee = ((wasm_kB × BASELINE_STORAGE × WASM_CF) + (Σ opcode_difficulty × CIRCUIT_CF)) / SCALE
```

### 7.1 Components

| Component | Meaning | Source |
|-----------|---------|--------|
| `wasm_kB` | WASM bytecode size in kB (min 1 for all transactions) | Wallet computes from bytecode |
| `BASELINE_STORAGE` | Per-kB storage cost constant = 1,000,000 (0.01 DRKW at CF=1.0) | Consensus constant |
| `WASM_CF` | Congestion factor for WASM storage (premium or standard tier) | Fee window PID controller |
| `Σ opcode_difficulty` | Sum of k-scaled opcode difficulties for all circuits | Manifest `[[cost_profiles]]` |
| `CIRCUIT_CF` | Congestion factor for circuit execution (premium or standard tier) | Fee window PID controller |
| `SCALE` | Fixed-point scale = 1,000,000 | Consensus constant |

### 7.2 Congestion Factors

`WASM_CF` and `CIRCUIT_CF` are independent congestion factors, each with
`premium` and `standard` tier values, governed by a dual PID controller
(see [fee-spec.md §12.7](consensus/fee-spec.md)). At zero congestion,
both equal `SCALE` (1,000,000). Congestion increases the factors above
`SCALE` via a log₂ formula based on mempool queue depth. Each factor is
constrained to ±10% change per 20-block window.

Miners signal CF direction in the `fee_window_flags` field of each
block header at the final block of the fee window (see
[fee-spec.md §12.6](consensus/fee-spec.md)).

### 7.3 Threshold Relationship

The fee determines which tier to target:
```
if fee >= premium_threshold:
    build FeeThreshold_V1 proof against premium_threshold
elif fee >= general_threshold:
    build FeeThreshold_V1 proof against general_threshold
else:
    fee too low — transaction will not be admitted
```

The actual fee paid MAY exceed the threshold — the proof only guarantees
the lower bound. The wallet MAY offer a higher fee for faster inclusion.
Tier is determined by the proof: a transaction is premium if its
FeeThreshold_V1 proof verifies against the premium threshold; general if
it verifies against the general threshold.

## 8. FeeSignallingExtractor Trait `[domain: fee_signalling]`

The mempool delegates fee extraction and threshold verification to a
per-contract extractor. The `FeeSignallingExtractor` trait is defined in
`crates/dwow-mempool/src/lib.rs`.

### 8.1 Interface

```
trait FeeSignallingExtractor {
    /// Extract the Pedersen commitment to the fee from a transaction.
    /// Returns None if the transaction does not carry a fee commitment
    /// (e.g., non-fee calls, coinbase transactions).
    fn extract_fee_commitment(&self, tx: &Transaction) -> Option<FeeCommitment>;

    /// Verify the FeeThreshold_V1 proof embedded in the transaction
    /// against the given threshold. Returns true iff the proof is
    /// cryptographically valid AND proves fee >= threshold.
    fn verify_threshold_proof(&self, tx: &Transaction, threshold: u64) -> bool;
}
```

Both methods are MANDATORY. `FeeCommitment` wraps `pallas::Point`.

### 8.2 Integration Points

- **Admission** (§5.2): `verify_threshold_proof` gates tier assignment.
- **Block selection** (§5.3): Txs without valid proofs are excluded from
  `select_for_block`.
- **Daemon** (`bin/dwowd/src/lib.rs`): `NativeTokenFeeSignallingExtractor` implements
  both methods, parsing `FeeParamsV2` from call data.

### 8.3 Proof Verification

The `FeeThreshold_V1` circuit has 2 public inputs: `threshold` and `tx_binding`.
Verification SHALL:
1. Deserialize the proof bytes from `FeeParamsV2.threshold_proof`.
2. Verify the proof against public inputs `(threshold, tx_binding)`.
3. Check that `tx_binding == poseidon(DOMAIN_TX_BINDING, tx_commitment, threshold)`
   — the threshold in the binding MUST match the tier being verified. This
   prevents a proof built for the premium tier from being replayed against
   the general tier.

### 8.4 Verification WASM Widget

FeeThreshold_V1 proofs are verified using a **verification WASM widget** — a
minimal WASM module that wraps the `fee_threshold_v1.zk` circuit. This is NOT
a contract (no state, no `accept_block`, no `__entrypoint`). It is a portable
verification module shared by the mempool and miners.

**Architecture.** Two WASM widgets are built from the same zkas circuit — a
proving widget (wallet-side, wallet.md §6.4.3) and a verification widget
(mempool/miner-side). The architecture diagram is at fee-spec.md §0.

**Verification widget crate.** The verification WASM widget is a minimal cdylib
crate at `src/contract/native_token/verify_fee_threshold/`. It is NOT a
contract — it has noop `exec`/`apply` and exists solely to provide public
inputs for `verify_zkp()` via `__metadata`.

```
src/contract/native_token/verify_fee_threshold/
├── Cargo.toml     # cdylib, depends on dwow-sdk (wasm feature)
└── src/lib.rs     # define_contract! with noop exec/apply, metadata returns public inputs
```

- Crate type: `cdylib` (compiles to `verify_fee_threshold.wasm`)
- Embeds `fee_threshold_v1.zk.bin` via `include_bytes!` (byte-identical to
  proving widget)
- `__initialize`: registers `.zk.bin` via `wasm::db::zkas_db_set`
- `__metadata`: decodes `FeeParamsV2` from call data, returns
  `[(FeeThreshold_V1, [threshold, tx_binding])]`
- Deployed at genesis, cached in the contracts sled tree
- Mempool loads from sled tree; miners load the same module for independent
  re-verification

**Verification flow:**
1. Load the verification WASM widget (deployed at genesis, cached in the
   contracts sled tree).
2. Call `__metadata` on the widget with the FeeV2 call data → returns
   `[(FeeThreshold_V1, [threshold, tx_binding])]`.
3. Load the `fee_threshold_v1.zk.bin` from the contracts sled tree (registered
   by `__initialize`).
4. Call `verify_zkp(threshold_proof, zkbin, [threshold, tx_binding])` via the
   native ZK stack.
5. Return `true` iff cryptographic verification succeeds.

**The mempool SHALL NOT trust the plain `params.threshold` u64 field.** That
field is user-supplied. Only cryptographic verification of the ZK proof
constitutes a gate. The current `NativeTokenFeeSignallingExtractor` stub that
compares `params.threshold.get() == threshold` without calling `verify_zkp()`
is NOT a valid gate — it SHALL be replaced with the WASM widget path above.

**Miner re-verification.** Miners SHALL independently load the same
verification WASM widget and re-verify threshold proofs before including
transactions in a block. This closes the trust gap — the miner does not
blindly trust the mempool's word that a proof verified. The miner performs
the identical 5-step verification flow described above.

### 8.5 References

- The two-widget architecture: [fee-spec.md §0](consensus/fee-spec.md)
- Proving widget spec: [wallet.md §6.4.3](wallet.md)
- Circuit definition: [fee-spec.md §5.5](consensus/fee-spec.md)
- FeeSignallingExtractor trait: [fee-spec.md §7.2](consensus/fee-spec.md)

## 9. References

- **[Wallet Architecture](wallet.md)** — The write path (§6) and provisional state (§6.5).
  FeeV2 fee payment at §6.4.2, FeeThreshold_V1 threshold proof and proving widget at §6.4.3.
- **[Type System Specification](type-system.md)** — Error barbs (§4), authority (§5), the
  `Transaction` type and metadata ABI (§8.2).
- **[Fee Payment Specification](consensus/fee-spec.md)** — FeeV2 circuits (§5),
  commitment accumulation (§5.6), FeeCollectV1 verification (§4.2).
- **[O-Cap: Emergent Types](ocap.md)** — The Exercise / Verify lifecycle (§6).
- **[Genesis Contracts](genesis.md)** — NativeToken (fee payment) and the coinbase.
