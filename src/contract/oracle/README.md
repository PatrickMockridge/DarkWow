# Oracle — External Data Feed (L2)

## The Capability

Oracle is the **push-model** external data feed: operators register oracles, push
values (plain or Pedersen-committed), attest values, and aggregate weighted
averages. It is an **L2 static record** contract; ZK proofs authenticate the
operator at the host level, while the attestation contract (a separate genesis
contract) holds the resulting claims.

**Trust tier:** ecosystem infrastructure (genesis counter 6). Not consensus-critical.

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `register_oracle` | `RegisterOracleV2` | Register an oracle feed (proves `oracle_secret`) |
| `0x01` | `push_value` | `PushValueV2` | Push a plaintext value to the feed |
| `0x02` | `attest_value` | `AttestValueV2` | Authorize an attestation over the current value |
| `0x03` | `push_value_commitment` | `PushValueCommitmentV2` | Push a Pedersen commitment (private value) |
| `0x04` | `aggregate` | `AggregateV2` | Weighted average of up to 4 values |
| `0x05` | `set_oracle_active` | — (non-ZK) | Enable/disable a feed (operator-authenticated) |

## Domain Constants

`TX_BINDING = witness_base(3)`, `COIN_COMMIT = witness_base(4)`. The key
derivation base is `NULLIFIER_K`.

## Data Model

```
oracle_pub    = ec_mul_base(oracle_secret, NULLIFIER_K)     # operator key
commitment    = poseidon_hash(4, value, nonce)              # PushValueCommitmentV2 (private value)
tx_binding    = poseidon_hash(3, tx_commitment, tx_nonce)
aggregate     = floor(Σ value_i·weight_i / Σ weight_i)      # AggregateV2 (4 values, 64-bit bounds)
oracle_id     = poseidon_hash([pub_x, pub_y])               # set_oracle_active lookup (no domain const)
```

## Barbs

| Barb | Mechanism |
|------|-----------|
| `↓spend` | `oracle_pub = ec_mul_base(oracle_secret, NULLIFIER_K)` bound to `oracle_pub_x/y` |
| `↓denominate` | `commitment = poseidon_hash(4, value, nonce)` binds the private value |
| `↓aggregate` | `AggregateV2` constrains the quotient–remainder weighted average with `min_result`/`max_result` range checks |
| `↓commit` | `RegisterOracleV2`/`PushValueV2`/`AggregateV2` Apply-write the `Oracle` record; `AttestValueV2`/`PushValueCommitmentV2` are no-op writes (data lives in the attestation contract) |

## The Four-Component Flow

1. **Circuit** — derives `oracle_pub`, commitment, or weighted average; constrains to witnesses.
2. **Params** — caller pre-computes public inputs with domain constants.
3. **Metadata** — echoes the `constrain_instance` values (e.g. `[oracle_id, value, tx_binding, tx_nonce]`).
4. **Exec** — validates the oracle exists + is active (`register` checks not-exists);
   **Apply** — writes the `Oracle` record (`value`, `updated_at`, `is_active`).

`push_value_commitment` keeps the value off-chain as a Pedersen commitment (no
Merkle membership proof — the oracle has no data tree, so membership would prove
nothing); `attest_value` delegates the actual attestation to the attestation
contract. `set_oracle_active` is non-ZK, authorized by `oracle_pub` equality.

## State Trees

| Tree | Purpose |
|------|---------|
| `oracles` | Oracle registrations |
| `attestations` | Attestation records (for attestation-contract integration) |
| `info` | Contract metadata and state |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `oracle_registration` | `0` | `SecretKey, Commitment, ContractId, FuncId` | — (non-consumable) |
| `oracle_value` | `1` | `SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId` | `{ value: u64 }` |
| `attestation` | `2` | `SecretKey, Commitment, Nullifier, ContractId, FuncId` | — |
| `aggregate` | `3` | `SecretKey, Commitment, Nullifier, ContractId, FuncId, AssetId` | `{ value: u64 }` |
| `value_commitment` | `4` | `SecretKey, Commitment, ContractId, FuncId, AssetId` | `{ commitment: pallas_base }` |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `register_oracle` | none | — | `oracle_registration` | `Commit, Dispatch, Gate` |
| `push_value` | `all(oracle_registration)` | — | `oracle_value` | `Commit, Dispatch, Gate, Denominate` |
| `attest_value` | `any(oracle_value)` | `oracle_value` | `attestation` | `Spend, Nullify, Commit, Dispatch, Gate` |
| `push_value_commitment` | `all(oracle_registration)` | — | `value_commitment` | `Commit, Dispatch, Gate, Denominate` |
| `aggregate` | `any(oracle_value)` | `oracle_value` | `aggregate` | `Spend, Nullify, Commit, Dispatch, Gate, Denominate` |

## Authorization

`register_oracle` produces the `oracle_registration` capability (proving
`oracle_secret`); `push_value`/`push_value_commitment` require it. `attest_value`
consumes an `oracle_value` to produce an `attestation`; `aggregate` consumes
`oracle_value`s to produce an `aggregate`. The operator's authority is the
`oracle_secret` behind `oracle_pub` — only the registered operator can push values
for its own feed.

## References

- [Oracle Specification](../../../doc/src/contract/oracle.md)
- [Attestation Contract](../attestation/README.md)
- [Contract Manifest](../../../doc/src/arch/manifest.md)
- [Contract WASM Type System](../../../doc/src/arch/contract-wasm-type-system.md) — Part B (L2)
- Source: `src/contract/oracle/`
