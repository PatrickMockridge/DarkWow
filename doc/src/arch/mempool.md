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
- **fee.** The transaction SHALL pay at least the minimum fee, denominated in DRKW and
  itself a verified NativeToken call — the fee is not exempt from proof verification.
  Coinbase transactions (PoWRewardV1, function `0x05`) are exempt from the fee minimum.
- **Dedup.** The transaction's hash SHALL NOT already be in the pool.
- **Eviction.** If the pool is at capacity, the lowest fee-rate entry SHALL be evicted
  before inserting a higher fee-rate transaction. Stale entries (older than
  `max_age_secs`) SHALL be evicted before each insertion.

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

## 5. References

- **[Wallet Architecture](wallet.md)** — The write path (§6) and provisional state (§6.5).
- **[Type System Specification](type-system.md)** — Error barbs (§4), authority (§5), the
  `Transaction` type and metadata ABI (§8.2).
- **[O-Cap: Emergent Types](ocap.md)** — The Exercise / Verify lifecycle (§6).
- **[Genesis Contracts](genesis.md)** — NativeToken (fee payment) and the coinbase.
