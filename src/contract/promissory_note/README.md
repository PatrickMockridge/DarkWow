# Promissory Note — Universal DeFi Token Primitive (L1)

## The Capability

Promissory Note (PN) is the universal DeFi primitive: fully-fungible private
commitments with token-type creation, mint authority, private transfer, atomic OTC
swap, and redemption. Commitments are L1 **consume+create** capabilities — spending
nullifies an input and creates blind outputs; conservation is proven in-circuit
per `token_commit`. Value is carried as a promissory note (a redemption
capability), not a native asset.

**Trust tier:** ecosystem infrastructure (genesis counter 3). Not
consensus-critical — depends on `native_token_v1`.

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `register_type` | `RegisterTypeV2` | Create a token type; `asset_id` derived from `auth_parent`, `user_data`, `blind` |
| `0x01` | `redeem` | `RedeemV2` | Redeem a commitment — destroys value, mints a zero-value receipt commitment |
| `0x02` | `issue` | `IssueV2` | Mint commitments of an existing type — proves knowledge of the `mint_secret` |
| `0x03` | `revoke` | `RevokeV2` | Burn commitments — publishes nullifiers; dispatches `spend_hook` |
| `0x04` | `transfer` | `TransferV2` | Atomic burn-and-blind-output with per-token value conservation |
| `0x05` | `otc_swap` | `TransferV2` | Peer-to-peer atomic swap — exactly 2 inputs / 2 outputs, both parties sign |

## Domain Constants

`NULLIFIER = witness_base(1)`, `TOK_COMMIT = witness_base(2)`,
`TX_BINDING = witness_base(3)`, `COMMITMENT = witness_base(4)`,
`USER_DATA_ENC = witness_base(6)`, `SIGNATURE_SECRET = witness_base(7)`.

## Data Model

```
pub             = poseidon_hash(7, spend_secret)                                # field-element pubkey
commitment            = poseidon_hash(4, public, value, asset_id, spend_hook, user_data, blind)
nullifier       = poseidon_hash(1, spend_secret, commitment)
asset_id        = poseidon_hash(2, token_auth_parent, token_user_data, token_blind)
token_commit    = poseidon_hash(2, asset_id, token_blind)
value_commit    = pedersen_commit(value, value_blind)                          # ec_mul_short(V) + ec_mul(R)
tx_binding      = poseidon_hash(3, tx_commitment, tx_nonce)
```

PN uses a **field-element public key** (`poseidon_hash(7, secret)`), unlike
native_token's EC-point key — no EC operations in the commitment hash.

## Barbs

| Barb | Mechanism (representative) |
|------|---------------------------|
| `↓spend` | `pub = poseidon_hash(7, spend_secret)` proves secret knowledge |
| `↓nullify` | `nullifier = poseidon_hash(1, spend_secret, commitment)` |
| `↓prove-inclusion` | `merkle_root(leaf_pos, path, commitment) == expected_root` (zero-value guard in revoke) |
| `↓denominate` | `token_commit = poseidon_hash(2, asset_id, token_blind)` |
| `↓conserve` | per `token_commit`, `Σ input value_commit == Σ output value_commit` (Pedersen point equality) |
| `↓commit` | Apply `merkle_add` new commitments, `db_mark_spent` nullifiers |

## The Four-Component Flow

1. **Circuit** — computes commitment/nullifier/commitments, constrains equal to
   caller witnesses; `constrain_instance` order is the public-input order.
2. **Params** — caller pre-computes every public input with matching domain constants.
3. **Metadata** — pure echo of the `constrain_instance` values.
4. **Exec** — validates nullifiers unspent + roots exist (register_type/issue also
   validate token registry); **Apply** — writes commitments, `db_mark_spent`, `merkle_add`.

## State Trees

| Tree | Purpose |
|------|---------|
| `commitment_set` | Commitment Merkle tree |
| `nullifiers` | Flat nullifier markers (no SMT) |
| `info` | Contract metadata, roots, total supply |
| `commitment_roots` | Historical commitment-tree roots |
| `nullifier_roots` | Historical nullifier-tree roots |
| `token_registry` | `asset_id → token_auth_parent` mint authority |
| `token_registry_roots` | Historical token-registry roots |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `commitment` | `0` | `SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId, MerkleNode` | `{ value: u64, asset_id, spend_hook, user_data, commitment_blind, value_blind, token_blind, memo }` |
| `mint_authority` | `1` | `SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId` | — (non-consumable) |
| `receipt` | `2` | `SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId, MerkleNode` | `{ value, asset_id, spend_hook, user_data, commitment_blind, value_blind, token_blind, memo }` |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `register_type` | none | — | `mint_authority` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `issue` | `all(mint_authority)` | — | `commitment` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `revoke` | `any(commitment)` | `commitment` | — | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `transfer` | `any(commitment)` | `commitment` | `commitment` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `redeem` | `any(commitment)` | `commitment` | `receipt` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |
| `otc_swap` | `any(commitment)` | `commitment` | `commitment` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |

## Authorization

`register_type` produces a `mint_authority` (proving the `backing_secret`); `issue`
requires it (`issue_public == token_auth_parent`). `commitment` capabilities are spent by
proving the `spend_secret` in the nullifier. `redeem` produces a non-transferable
zero-value `receipt` (proof of redemption). `otc_swap` requires both parties' input
commitments and cross-token pairing (`inputs[0].token_commit == outputs[1].token_commit`).

## References

- [Promissory Note Specification](../../../doc/src/contract/promissory_note.md)
- [Contract Manifest](../../../doc/src/arch/manifest.md)
- [Contract WASM Type System](../../../doc/src/arch/contract-wasm-type-system.md)
- [Type System](../../../doc/src/arch/type-system.md)
- [Privacy Model](../../../doc/src/arch/privacy.md)
- Source: `src/contract/promissory_note/`
