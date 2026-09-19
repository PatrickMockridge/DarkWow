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
- **fee.** The transaction SHALL carry a plaintext fee meeting the tier price
  (§5.2). The fee is denominated in DRKW and rides in the clear in FeeV3 call
  data (`0x08`, `FeeParamsV3`). Coinbase transactions (PoWRewardV1, function
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

## 5. Three-Tier Plaintext Admission

FeeV3 transactions carry the fee in the clear (`FeeParamsV3`). The mempool
sorts by plain comparison against tier prices. Specification:
[fee-spec.md §12.8.1](consensus/fee-spec.md).

### 5.1 Architecture

| Tier | Admission | Ordering | Purpose |
|------|-----------|----------|---------|
| High (4x) | `fee >= price_high` | FIFO (arrival order) | Urgent transactions |
| Medium (2x) | `price_medium <= fee < price_high` | FIFO after high exhausted | Normal transactions |
| Low (1x) | `price_low <= fee < price_medium` | FIFO after medium exhausted | Best-effort transactions |
| Rejected | — | — | Fee below the low tier price |

### 5.2 Admission Algorithm

```
admit(tx):
  // Extract plaintext fee from call data
  fee = extract_fee(tx)
  if fee is None → REJECT (not a FeeV3 transaction)

  // Plain comparison — no ZK threshold proof
  if fee >= price_high:
    admit_to_high_queue(tx)
    return ADMITTED
  if fee >= price_medium:
    admit_to_medium_queue(tx)
    return ADMITTED
  if fee >= price_low:
    admit_to_low_queue(tx)
    return ADMITTED

  // Fee below all tier prices
  REJECT ↓bad-fee
```

### 5.3 Block Selection

`select_for_block(max_gas, max_txs)`:
1. Drain high queue in FIFO order until `max_gas` or `max_txs` reached.
2. Drain medium queue in FIFO order until limits reached.
3. Drain low queue in FIFO order until limits reached.
4. Return selected transactions. Selection is non-destructive — call
   `mark_mined` after block acceptance to remove confirmed transactions.

### 5.4 Tier Prices

`price_high`, `price_medium`, and `price_low` default to 4×/2×/1×
`CongestionFactor::SCALE` (`MempoolConfig::default()`,
`crates/dwow-mempool/src/lib.rs`) and are runtime-updatable via
`update_tier_prices()` (see [fee-spec.md §12](consensus/fee-spec.md)).
Miners signal congestion direction in the `fee_window_flags` field of each
block header at the final block of the fee window.

## 6. Fee Window Discovery

Congestion direction is published in the `fee_window_flags` field of
each block header at the final block of the fee window (see
[fee-spec.md §12.6](consensus/fee-spec.md)). Wallets and other nodes
discover it by reading the latest block header — no separate P2P
announcement protocol is required. Block headers are already validated
during chain sync; `fee_window_flags` are part of the canonical header.

### 6.1 FeeWindowFlags Format

`fee_window_flags` is a `u16` encoding CF direction for both circuit and
WASM dimensions. The canonical bit layout is defined in
[fee-spec.md §12.6](consensus/fee-spec.md):

| Byte | Bits | Field | Values |
|------|------|-------|--------|
| Byte 0 | 0 | FEE_WINDOW_ACTIVE | 0=legacy, 1=active |
| Byte 0 | 1:3 | Reserved | Must be zero |
| Byte 0 | 4:7 | CIRCUIT_CF direction | 0b0000=hold, 0b0001=+10%, 0b0010=-10% |
| Byte 1 | 8 | FEE_WINDOW_ACTIVE | 0=legacy, 1=active |
| Byte 1 | 9:11 | Reserved | Must be zero |
| Byte 1 | 12:15 | WASM_CF direction | 0b0000=hold, 0b0001=+10%, 0b0010=-10% |

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

### 7.3 Tier Selection

The wallet declares a tier in the call data (`FeeParamsV3.tier`); the
mempool routes by plain comparison:
```
if fee >= price_high:
    admit to the high queue
elif fee >= price_medium:
    admit to the medium queue
elif fee >= price_low:
    admit to the low queue
else:
    fee too low — transaction will not be admitted
```

The actual fee paid MAY exceed the tier price. The wallet MAY offer a
higher fee for faster inclusion. Tier assignment is determined by the
plain comparison at admission.

## 8. FeeSignallingExtractor Trait `[domain: fee_signalling]`

The mempool delegates fee and tier extraction to a per-contract extractor.
The `FeeSignallingExtractor` trait is defined in
`crates/dwow-mempool/src/lib.rs`.

### 8.1 Interface

```
trait FeeSignallingExtractor {
    /// Extract the plaintext fee amount from call data.
    fn extract_fee(&self, tx: &Transaction) -> FeeAmount;

    /// Read the three-tier priority selector (1=low, 2=medium, 4=high).
    fn extract_tier(&self, tx: &Transaction) -> FeeTier;

    /// Declare the block capacity charge for block packing.
    fn declare_charge(&self, tx: &Transaction) -> BlockCharge;
}
```

`FeeAmount` and `BlockCharge` are distinct domain types (type-system.md
§2.3.1) so gas arithmetic cannot mix with fee or supply accounting.

### 8.2 Integration Points

- **Admission** (§5.2): plain `fee >= tier_price` comparison gates tier
  assignment — no ZK proof.
- **Block selection** (§5.3): transactions with fee below the low tier price
  are excluded.
- **Daemon** (`bin/dwowd/src/lib.rs`): `NativeTokenFeeSignallingExtractor`
  implements the trait, parsing `FeeParamsV3` from call data.

Nothing needs cryptographic verification at admission — the fee is plaintext,
and the comparison in §5.2 is the entire gate.

### 8.3 References

- FeeSignallingExtractor trait: [fee-spec.md §7.2](consensus/fee-spec.md)
- Three-tier admission: [fee-spec.md §12.8.1](consensus/fee-spec.md)

## 9. References

- **[Wallet Architecture](wallet.md)** — The write path (§6) and provisional state (§6.5).
  FeeV3 fee payment (plaintext `FeeParamsV3`) at §6.4.2.
- **[Type System Specification](type-system.md)** — Error barbs (§4), authority (§5), the
  `Transaction` type and metadata ABI (§8.2).
- **[Fee Payment Specification](consensus/fee-spec.md)** — FeeV3 circuits (§5),
  commitment accumulation (§5.6), FeeCollectV1 verification (§4.2).
- **[O-Cap: Emergent Types](ocap.md)** — The Exercise / Verify lifecycle (§6).
- **[Genesis Contracts](genesis.md)** — NativeToken (fee payment) and the coinbase.
