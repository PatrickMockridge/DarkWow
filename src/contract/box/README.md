# Box — ZK Capability Container (L1)

## The Capability

Box is the capability to **delegate** — the ZK-native equivalent of Agoric's
Invitation. It holds an arbitrary capability and transfers it via linear
consumption. In L1, the Box is not a persistent mutable container: each
operation is a **consume+create** — the old box state is nullified and a new
state leaf is appended to the Merkle tree. The `box_id` binds state transitions
in the ZK witness but is never exposed as a public input.

**Trust tier:** ecosystem infrastructure (genesis counter 9). Not
consensus-critical.

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `initialize` | — | Genesis initialization (seeds trees, stores circuits) |
| `0x01` | `put` | `Put` | Place a capability into a Box — consumes old state, creates a new Merkle leaf |
| `0x02` | `take` | `Take` | Take a capability from a Box — consumes current state via nullifier |

## Barbs

Barb names are declared in `manifest.toml [[actions]].required_barbs`; the
mechanisms below are the exact `constrain_instance`/`constrain_equal_base`
constraints in `proof/put.zk` and `proof/take.zk`.

### `put`

| Barb | Mechanism |
|------|-----------|
| `↓spend` + `↓nullify` | Circuit constrains `nullifier == poseidon_hash(1, owner_secret, box_id, old_state_nonce)` — proves the caller holds `owner_secret` and emits the consuming nullifier (`DOMAIN_NULLIFIER = witness_base(1)`) |
| `↓prove-inclusion` | Circuit constrains `merkle_root(leaf_pos, path, old_leaf) == expected_root` where `old_leaf = poseidon_hash(5, box_id, old_contents_commit, old_state_nonce)` (`DOMAIN_MERKLE_LEAF = witness_base(5)`) |
| `↓commit` | Circuit constrains `new_leaf == poseidon_hash(5, box_id, new_contents_commit, new_state_nonce)`; Apply appends `new_leaf` via `merkle_add` and marks the nullifier spent |

### `take`

| Barb | Mechanism |
|------|-----------|
| `↓spend` + `↓nullify` | `nullifier == poseidon_hash(1, owner_secret, box_id, state_nonce)` |
| `↓prove-inclusion` | `merkle_root(leaf_pos, path, box_leaf) == expected_root` where `box_leaf = poseidon_hash(5, box_id, contents_commit, state_nonce)` |
| `↓commit` | Terminal consumption — Apply adds a block-level anchor (`merkle_anchor_add`) and marks the nullifier spent; no new leaf |

Both circuits also bind the transaction: `tx_binding == poseidon_hash(3, tx_commitment, tx_nonce)` (`DOMAIN_TX_BINDING = witness_base(3)`).

## The Four-Component Flow

Every operation follows the L1 architectural pattern
(`contract-wasm-type-system.md` §A, [safety.md Lesson 22]):

1. **Circuit** (`put.zk` / `take.zk`) — computes cryptographic values and
   constrains them equal to caller-provided witnesses via `constrain_equal_base`;
   the `constrain_instance` values are the public inputs.
2. **Params** (`PutParams` / `TakeParams`) — the caller pre-computes every
   `constrain_instance` field (nullifier, `expected_root`, `new_leaf`,
   `tx_binding`) with matching domain constants.
3. **Metadata** (`get_metadata`) — pure echo; extracts `nullifier`,
   `expected_root`, `new_leaf` (Put only), `tx_binding`, `tx_nonce` from params.
   No computation.
4. **Exec** (`process_instruction`) — validates the nullifier is unspent
   (`db_contains_key`) and the `expected_root` exists in `box_roots` (skipped on
   the first, EMPTY-root operation). **Apply** (`process_update`) — writes state
   only: `merkle_add` (Put) / `merkle_anchor_add` (Take) + `db_mark_spent`.

Exec validates; Apply writes. Exec never writes state; Apply never reads it.

## Data Model

```
nullifier  = poseidon_hash(1, owner_secret, box_id, state_nonce)      # DOMAIN_NULLIFIER
box_leaf   = poseidon_hash(5, box_id, contents_commit, state_nonce)   # DOMAIN_MERKLE_LEAF
tx_binding = poseidon_hash(3, tx_commitment, tx_nonce)                # DOMAIN_TX_BINDING
```

## State Trees

| Tree | Purpose |
|------|---------|
| `nullifiers` | Spent nullifier records |
| `box_roots` | Historical Merkle roots for inclusion proofs |
| `info` | Merkle tree state and root pointers |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `box_capability` | `0` | `SecretKey`, `Commitment`, `Nullifier`, `MerkleNode`, `ContractId`, `FuncId` | `{ commitment: pallas_base, state_nonce: pallas_base }` |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `put` | — | — | `box_capability` | `Spend, Nullify, ProveInclusion, Commit, Dispatch, Gate` |
| `take` | — | `box_capability` | — | `Spend, Nullify, ProveInclusion, Commit, Dispatch, Gate` |

## Authorization

`put` **produces** a `box_capability` (a fresh name via `νx`); `take` **consumes**
it (`x?(y).nullify!(y)`). Authority is the `owner_secret` embedded in the
nullifier — the holder who can produce a valid nullifier for `box_id` holds the
delegation. The capability is linear: one `take` nullifies the box, so it can be
exercised exactly once.

## References

- [Box Specification](../../../doc/src/contract/box.md)
- [Contract Manifest](../../../doc/src/arch/manifest.md) — manifest schema
- [Contract WASM Type System](../../../doc/src/arch/contract-wasm-type-system.md) — Part A: barbs, four-component flow
- [Type System](../../../doc/src/arch/type-system.md) — ρ-calculus, o-cap model
- [Privacy Model](../../../doc/src/arch/privacy.md) — L1 consume+create
- [Safety — Lesson 22](../../../doc/src/dev/contracts/safety.md)
- Source: `src/contract/box/`
