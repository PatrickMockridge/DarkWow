# MultiSig — Threshold Signature Factory (L2)

## The Capability

MultiSig is the **N-of-M threshold signature** primitive: it creates signing
groups, collects partial Schnorr signatures from members, and finalizes them into
an **approval capability** that any contract can compose with. It is an **L2
static record** contract; the N-of-M threshold is enforced in exec (the ZK
circuits bind the transaction and the signer's key).

**Trust tier:** ecosystem infrastructure (genesis counter 10). Enables
multi-party authorization for any contract that composes with approval
capabilities.

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `initialize` | — | Seed trees, store circuits |
| `0x01` | `create_group` | `CreateGroupV2` | Create an N-of-M group from a public-key list + threshold |
| `0x02` | `sign` | `SignV2` | Sign a message as a member — proves `signer_pub ∈ group.pubkeys` |
| `0x03` | `finalize` | `FinalizeV2` | Finalize when ≥ threshold partial signatures exist — produces approval |

## Domain Constants

`TX_BINDING = witness_base(3)`, `COIN_COMMIT = witness_base(4)`. Key derivation
base `NULLIFIER_K`.

## Data Model

```
group_id        = poseidon_hash([first_pk_x, first_pk_y, threshold, total_keys])
signer_pub      = ec_mul_base(signer_secret, NULLIFIER_K)
signature_nullifier = poseidon_hash([group_id, message_hash, signer_pk_x, signer_pk_y])
approval_commit = poseidon_hash(4, group_id, message_hash)        # DOMAIN_COIN_COMMIT (circuit)
tx_binding      = poseidon_hash(3, tx_commitment, tx_nonce)
```

## Barbs

| Barb | Mechanism |
|------|-----------|
| `↓spend` | `SignV2` constrains `signer_pub = ec_mul_base(signer_secret, NULLIFIER_K)` and exec verifies `signer_pub ∈ group.pubkeys` |
| `↓nullify` | signature nullifier `poseidon_hash([group_id, message_hash, pk_x, pk_y])` (derived in exec) |
| `↓prove` | `FinalizeV2` constrains `approval_commit == poseidon_hash(4, group_id, message_hash)` |
| `↓commit` | Apply `db_set` the group / signature; `finalize` `db_del` the consumed partial signatures (replay fix) |

**N-of-M enforcement is in exec** (`finalize`): it counts collected partial
signatures from the `signatures` tree and rejects if
`consumed.len() < group.threshold`. The `FinalizeV2` circuit does **not**
constrain `threshold`/`signature_count` — those are witnesses only.

## The Four-Component Flow

1. **Circuit** — `create_group` binds `group_id`/`threshold`/`total_keys`;
   `sign` proves the signer key; `finalize` proves the approval commitment.
2. **Params** — caller pre-computes `tx_binding` (and group id) with domain constants.
3. **Metadata** — echoes `[tx_binding, tx_nonce, group_id, …]` per circuit order.
4. **Exec** — `create_group` validates key list + threshold; `sign` verifies group
   membership; `finalize` counts signatures against the threshold. **Apply** — writes
   the group/signature records; `finalize` deletes consumed partial signatures.

## State Trees

| Tree | Purpose |
|------|---------|
| `groups` | MultiSig groups (keyed by `group_id`) |
| `signatures` | Partial signatures (keyed by signature nullifier) |
| `nullifiers` | Spent signature nullifiers |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `group_membership` | `0` | `SecretKey, Commitment, ContractId, FuncId` | — (non-consumable) |
| `partial_signature` | `1` | `SecretKey, Commitment, Nullifier, ContractId, FuncId` | — (consumable) |
| `approval` | `2` | `SecretKey, Commitment, Nullifier, ContractId, FuncId` | — |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `create_group` | none | — | `group_membership` | `Commit, Dispatch, Gate` |
| `sign` | `all(group_membership)` | — | `partial_signature` | `Commit, Dispatch, Gate` |
| `finalize` | `all(partial_signature)` | `partial_signature` | `approval` | `Spend, Nullify, Commit, Dispatch, Gate` |

## Authorization

`create_group` produces `group_membership` for each member; `sign` requires it and
produces a `partial_signature` (keyed by a nullifier, so each member signs once);
`finalize` consumes the partial signatures and produces an `approval` — the
threshold-gated capability that downstream contracts compose with. Membership is
proven by the `signer_pub ∈ group.pubkeys` check in exec.

## References

- [MultiSig Specification](../../../doc/src/contract/multisig.md)
- [Contract Manifest](../../../doc/src/arch/manifest.md)
- [Contract WASM Type System](../../../doc/src/arch/contract-wasm-type-system.md) — Part B (L2)
- [Type System](../../../doc/src/arch/type-system.md)
- Source: `src/contract/multisig/`
