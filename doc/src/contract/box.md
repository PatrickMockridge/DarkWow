# Box — ZK Capability Container (L1)

## The Primitive Capability

Box is the capability to delegate — the ZK-native equivalent of Agoric's
Invitation. It holds an arbitrary capability and transfers it via linear
consumption. In L1, the Box is not a persistent mutable container. Each
operation is a consume+create: the old box state is nullified and a new
state leaf is appended to the Merkle tree. The box_id binds state transitions
in the ZK witness but is never exposed as a public input.

## Operations

| Operation | Opcode | Circuit | What It Proves |
|-----------|--------|---------|---------------|
| `Initialize` | 0x00 | — | Genesis initialization |
| `Put` | 0x01 | `put.zk` | Poseidon ownership proof. Nullifier consumes old state. Merkle inclusion of old state. New state commitment appended. |
| `Take` | 0x02 | `take.zk` | Same as Put. Nullifier consumes current state. |

## Barbs

### Put
| Barb | Mechanism |
|------|-----------|
| `↓spend` | Circuit constrains `owner_pub == poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)` |
| `↓nullify` | Circuit constrains `nullifier == poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, old_state_nonce)` |
| `↓prove-inclusion` | Circuit constrains `merkle_root(leaf_pos, path, leaf) == expected_root` where `leaf = poseidon_hash(DOMAIN_SIGNATURE_SECRET, box_id, old_contents_commit, old_state_nonce)` |
| `↓commit` | Apply appends new leaf to Merkle tree, marks nullifier in DB |

### Take
Same barbs as Put.

## The Four-Component Flow

Every operation follows the same architectural pattern:

1. **Circuit** (`put.zk` / `take.zk`): All `constrain_instance` values are
   caller-provided witnesses. The circuit computes cryptographic values and
   constrains them equal to the witnesses via `constrain_equal_base`.

2. **Params** (`PutParams` / `TakeParams`): Every `constrain_instance` position
   maps to a field. The caller pre-computes all circuit-derived values
   (nullifier, expected_root, tx_binding) with matching domain constants.

3. **Metadata** (`get_metadata`): Pure echo — reads `params.field` directly.
   No domain constants, no poseidon_hash, no computation.

4. **Exec** (`process_instruction`): Validates nullifier unspent via
   `db_contains_key`. **Apply** (`process_update`): Appends leaf to Merkle
   tree via `merkle_add`, marks nullifier via `db_set`.

## Data Model

```
box_leaf  = poseidon_hash(DOMAIN_SIGNATURE_SECRET, box_id, contents_commit, state_nonce)
nullifier = poseidon_hash(DOMAIN_NULLIFIER, owner_secret, box_id, state_nonce)
owner_pub = poseidon_hash(DOMAIN_SIGNATURE_SECRET, owner_secret)
```

## Database Trees

| Tree | Purpose |
|------|---------|
| `nullifiers` | Spent nullifiers (flat DB) |
| `info` | Merkle tree data, root pointers |
| `box_roots` | Historical Merkle roots |

## Circuit Version

Box is L1. There is one circuit per operation — no version suffixes.
Circuits use domain-separated Poseidon hashes (HAZOP RC3).

## Composing Contracts

| Contract | What the Box Delegates |
|----------|----------------------|
| [escrow](escrow.md) | Seller claim authority, buyer refund authority |
| [drain_protection](drain_protection.md) | Spend authority, proposal rights, vote rights |
| [subscription](subscription.md) | Subscription capability |
| [dao_escrow](dao_escrow.md) | Governance roles |

## References

- [Privacy Model](../arch/privacy.md) — L1/L2, consume+create model, architectural principles
- [Contract WASM Type System](../arch/contract-wasm-type-system.md) — Four encoding boundaries
- [O-Cap Model](../arch/ocap.md) — Box in the O-Cap stack
- [Safety](../dev/contracts/safety.md) — Lesson 22: four-component architecture
- Source: `src/contract/box/`
