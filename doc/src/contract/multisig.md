# MultiSig Contract

MultiSig is a genesis-deployed (counter 10) threshold signature factory. It creates
N-of-M groups, collects partial Schnorr signatures from group members, and produces
approval capabilities that other contracts compose with.

## Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | 0x00 | Standard genesis init — registers ZK proofs, creates DB trees |
| `CreateGroupV1` | 0x01 | Create an N-of-M group. Takes list of public keys + threshold M. Produces a `group_capability`. |
| `SignV1` | 0x02 | Sign a message as a group member. Proves membership via public key in the group. Produces a `partial_signature` capability. |
| `FinalizeV1` | 0x03 | When threshold partial signatures for a message exist, consumes them. Produces an `approval` capability. |

## Architecture

MultiSig follows the factory pattern: the genesis contract creates groups, and
individual groups produce approvals. Other contracts compose with approval
capabilities via `Box::Take` or direct capability checks — they don't need to
know the threshold or group membership.

### Capability Types

| Capability | Discriminant | Produced By | Description |
|---|---|---|---|
| `group_capability` | 0x00 | `CreateGroupV1` | Holder can manage the group |
| `partial_signature` | 0x01 | `SignV1` | Intermediate — consumed by FinalizeV1 |
| `approval` | 0x02 | `FinalizeV1` | The thing other contracts compose with |

### State Trees

| Tree | Key | Value |
|------|-----|-------|
| `groups` | `group_id` | `MultiSigGroup { pubkeys, threshold, total_keys }` |
| `signatures` | `nullifier` | `PartialSignature { group_id, message_hash, signer_pubkey }` |
| `nullifiers` | `nullifier` | Empty — replay protection |
| `info` | — | Metadata |

## ZK Circuits

### create_group_v1.zk
Verifies group creation: threshold ≥ 1, threshold ≤ total keys, transaction binding.

### sign_v1.zk
Verifies partial signature: signer's public key derived from secret, Schnorr
signature over message, transaction binding.

### finalize_v1.zk
Verifies threshold finalization: approval commitment from group_id + message_hash,
transaction binding. The actual signature counting is done in the WASM entrypoint.

## Genesis Inclusion

MultiSig is included in genesis (counter 10) because it extracts duplicated
threshold logic from DAO-Escrow and DrainProtection into a shared primitive.
Every contract that needs N-of-M authorization composes with MultiSig rather
than hand-rolling threshold verification.

## See Also

- [Box](box.md) — Capability delegation primitive
- [Purse](purse.md) — Fungible asset container
- [Wallet Architecture](../arch/wallet.md)
- [Contract Manifest](../arch/manifest.md)
