# Transaction Lifetime

> **Normative language (SHALL/MUST/SHOULD) per RFC 2119.**

This document specifies the lifetime of a DarkWow transaction — from
construction in the wallet through broadcast, mempool admission, block
inclusion, consensus validation, and confirmation. It SHALL be read together
with the [Type System Specification](../type-system.md) (Transaction type §8.2,
error barbs §4, authority §5), [Wallet Architecture](../wallet.md) (write path
§6, provisional state §6.5), [Mempool](../mempool.md), and [O-Cap Model](../ocap.md)
(Exercise/Verify lifecycle §6).

## 1. Transaction Definition

A transaction `T` is an ordered sequence of contract calls:

$$T = [C_1, \ldots, C_n]$$

where each `C_i` is a `ContractCall` ([type-system.md §8.2](../type-system.md))
with fields `{ contract_id: ContractId, data: Vec<u8> }`. The `data` field SHALL
carry a function selector byte followed by serialized call parameters.

Every contract call exercises a held capability. The caller SHALL possess the
capability's authorization secret before constructing the call. The call SHALL
include a nullifier — evidence that the capability has been exercised — and a
ZK proof that the caller knows the witness without revealing it
([ocap.md §6](../ocap.md)).

Transactions SHALL be atomic: either all calls succeed or none do. A
transaction that partially executes SHALL be rejected by the consensus layer.

## 2. Transaction Structure

The `Transaction` type is defined in [type-system.md §8.2](../type-system.md):

```
Transaction {
    version: u8,
    inputs: Vec<TxInput>,
    outputs: Vec<TxOutput>,
    contract_calls: Vec<ContractCall>,
    lock_time: u64,
    nullifiers: Vec<Nullifier>,
    witness: Vec<u8>,
}
```

- **`contract_calls`** SHALL be non-empty for a valid transaction. The coinbase
  transaction places its `PoWRewardV1` call at `transactions[0].contract_calls[0]`.
- **`nullifiers`** carry pre-extracted nullifiers for the mempool's
  double-spend detection ([mempool.md §2](../mempool.md)). Each nullifier SHALL
  be a typed `Nullifier` (poseidon_hash of the authorization secret and
  commitment), never raw bytes.
- **`witness`** carries the ZK proofs and signatures as an opaque,
  dwow_serial-encoded bundle. The witness SHALL be verified at both mempool
  admission and block acceptance. It is EXCLUDED from the transaction hash —
  block identity commits to transaction semantics, never to interchangeable
  witness bytes.
- **`inputs`/`outputs`** provide a Bitcoin-compatible structure for value
  transfer tracking at the chain level.
- **`lock_time`** constrains the earliest block height or timestamp at which
  the transaction MAY be included.

## 3. Transaction States

A transaction has three states of existence with respect to the chain
([mempool.md §0](../mempool.md)):

1. **Constructed** — held by the wallet, not yet broadcast
   ([wallet.md §6](../wallet.md)).
2. **Pending** — broadcast and admitted to the mempool, not yet in a block.
3. **Confirmed** — included in an accepted block, discoverable by scan
   ([wallet.md §2](../wallet.md)).

## 4. Construction — The Write Path

The wallet constructs transactions as a pure function of its inputs
([wallet.md §6](../wallet.md)):

```
build_transaction(capabilities, action) → Transaction
```

Steps:
1. **Capability selection** — the wallet selects held capabilities whose
   barbs cover the required action. Selection SHALL NOT select the same
   capability twice (one nullifier per exercise).
2. **ZK proof generation** — the wallet proves knowledge of each capability's
   authorization secret via the circuit declared in the capability's manifest.
3. **Nullifier computation** — `nf = poseidon_hash(secret_inner, commitment)`.
   Each exercised capability SHALL produce exactly one nullifier.
4. **Witness assembly** — proofs, signatures, and tx_commitment are bundled
   into the opaque `witness` field.
5. **Fee attachment** — a separate DRKW capability is selected for the fee
   and exercised via `FeeV1`. The fee capability SHALL be distinct from the
   primary input capability.

## 5. Broadcast → Mempool → Block

1. **Broadcast.** The wallet publishes the transaction via P2P gossip
   ([wallet.md §6.5](../wallet.md)). The transaction SHALL carry its full
   witness — proofs and signatures SHALL NOT be stripped before broadcast.

2. **Mempool admission.** The receiving node SHALL verify:
   - Transaction is non-empty (has contract calls or inputs)
   - Size does not exceed `max_tx_size`
   - Fee meets the minimum (unless coinbase)
   - Nullifiers are not already in the pool or the confirmed nullifier set
   - Duplicate transaction hash is not already in the pool
   - Witness verification passes (L2 admission gate)
   ([mempool.md §1](../mempool.md))

3. **Block inclusion.** A miner selects transactions from the mempool in
   fee-descending order, respecting gas and size limits. The coinbase
   transaction SHALL be placed at index 0. Selected transactions are NOT
   removed from the mempool until block acceptance.

4. **Block acceptance.** The block validator SHALL re-verify every
   transaction's witness. This is the second verification point — a node
   that syncs a block it did not admit independently validates every
   transaction ([mempool.md §4](../mempool.md)).

## 6. Capability Exercise — The Nullifier Model

The nullifier model replaces the UTXO/coin model. A capability is exercised
by publishing a nullifier:

```
Commitment → ZK Proof (witness hidden) → Nullifier (public evidence)
```

The nullifier SHALL be unique across the entire chain history. The nullifier
SMT (Sparse Merkle Tree) enforces this: any duplicate nullifier SHALL cause
block rejection. The wallet detects its own spent capabilities during scan by
matching nullifiers against its held commitments.

This is the same pattern for every capability exercise: coinbase claim
(`PoWRewardV1`), fee payment (`FeeV1`), value transfer (`TransferV1`),
burn (`BurnV1`), and spend (`SpendV1`). The contract differs; the pattern is
identical.

## 7. Value Conservation

Value is conserved across transaction calls via Pedersen commitment
homomorphism:

$$\sum \text{outputs} + \sum \text{burns} + \sum \text{fees} = \sum \text{inputs}$$

The per-block mass balance check ([consensus-coinbase.md §2.8](../consensus-coinbase.md))
verifies this equation for every block. A transaction that creates or destroys
value SHALL cause block rejection.

## 8. Atomic Execution

All contract calls in a transaction SHALL execute atomically. Each call runs
with its own `TxBackend` — an isolated in-memory state buffer. If any call
fails, all state changes from all calls are discarded. The transaction-level
`SledTreeOverlay` enforces this: `checkpoint()` before execution,
`revert_to_checkpoint()` on failure.

## 9. References

- **[Type System Specification](../type-system.md)** — Transaction type §8.2, error barbs §4, authority §5.
- **[Wallet Architecture](../wallet.md)** — Write path §6, provisional state §6.5.
- **[Mempool](../mempool.md)** — Admission, dedup, observability.
- **[O-Cap Model](../ocap.md)** — Exercise/Verify lifecycle §6.
- **[Consensus & Coinbase](../consensus-coinbase.md)** — Block production, coinbase nullifier claim.
- **[Consensus](../consensus/consensus.md)** — 7-phase block validation.
