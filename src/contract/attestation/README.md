# Attestation — Claims & Verification Framework (L2)

## The Capability

Attestation is the reusable **claim verification** framework: trusted issuers
create attestations, holders create claims against them, verifiers verify and
consume those claims. It is an **L2 static record** contract; the ZK circuits
bind the transaction and (for `ConsumeClaim`) the holder's identity, while
predicate evaluation is enforced in exec.

**Trust tier:** ecosystem infrastructure (genesis counter 7). Depends on
`promissory_note` and `native_token_v1`.

## Functions

| Code | Function | Proof circuit | Description |
|------|----------|---------------|-------------|
| `0x00` | `create_attestation` | `CreateAttestationV2` | Create an on-chain attestation |
| `0x01` | `revoke_attestation` | — (non-ZK) | Revoke an attestation (attestor) |
| `0x02` | `expire_attestation` | — (non-ZK) | Expire an attestation |
| `0x03` | `create_claim` | `CreateClaimV2` | Create a claim (Pending) against an attestation |
| `0x04` | `verify_claim` | `VerifyClaimV2` | Verify a claim (→ Verified/Rejected) |
| `0x05` | `consume_claim` | `ConsumeClaimV2` | Consume a verified claim (→ Consumed, nullifier) |
| `0x06` | `validate_claim` | — (non-ZK) | On-chain predicate compare |
| `0x07` | `check_not_revoked` | `CheckNotRevokedV2` | Non-revocation proof |
| `0x08` | `delegate_attestation` | `DelegateAttestationV2` | Delegate attestation authority |
| `0x09` | `verify_chain` | `VerifyChainV2` | Verify a delegation chain |
| `0x0a` | `update_delegation` | `UpdateDelegationV2` | Update a delegation record |
| `0x0b` | `attest_slash` | `AttestSlashV2` | Slash an attestation (hardening) |
| `0x0c` | `commit_fee_schedule` | `CommitFeeScheduleV2` | Commit a fee schedule (hardening) |

## Domain Constants

`NULLIFIER = witness_base(1)` (consume_claim), `TX_BINDING = witness_base(3)`,
`COIN_COMMIT = witness_base(4)`. Key derivation base `NULLIFIER_K`.

## Data Model

```
attestation_id = poseidon_hash([attestor_x, attestor_y, claim_type, data_hash, attestor_secret])
claim_id       = poseidon_hash([attestation_id, claimant_x, claimant_y, predicate, evidence_hash, claimant_secret])
consume_nullifier = poseidon_hash(1, claim_id, consumer_secret)          # DOMAIN_NULLIFIER
delegatee_leaf = poseidon_hash(4, delegatee_pub_x, delegatee_pub_y)      # DOMAIN_COIN_COMMIT
tx_binding     = poseidon_hash(3, tx_commitment, tx_nonce)
```

- `AttestationState`: `Active(0) → Revoked(1) | Expired(2)`.
- `ClaimState`: `Pending(0) → Verified(1) | Consumed(2) | Rejected(3)`.
- `Predicate`: `Matches(0), GreaterOrEqual(1), LessOrEqual(2), Contains(3), Custom(4)`.

## Barbs

| Barb | Mechanism |
|------|-----------|
| `↓spend` | `ConsumeClaimV2` binds `consumer_pub = ec_mul_base(consumer_secret, NULLIFIER_K)` |
| `↓nullify` | `nullifier = poseidon_hash(1, claim_id, consumer_secret)` |
| `↓verify` | predicate gate is enforced in **exec** (per-predicate `revealed_result`/evidence compare), not in-circuit — most circuits expose only `[tx_binding, tx_nonce]` |
| `↓commit` | Apply writes the `Attestation`/`Claim` record and (consume) `db_mark_spent(nullifier)` |

## The Four-Component Flow

1. **Circuit** — most circuits (Create/Verify/Delegate/…) constrain only
   `tx_binding`/`tx_nonce`; `ConsumeClaimV2` additionally derives the nullifier.
2. **Params** — caller pre-computes `tx_binding` (and nullifier) with domain constants.
3. **Metadata** — echoes `[tx_binding, tx_nonce]` (or `[claim_id, cx, cy, nullifier, tx_binding, tx_nonce]` for consume).
4. **Exec** — enforces the lifecycle + predicate logic (state transitions, attestation
   match, per-predicate compare, rate limit); **Apply** — writes the record.

## State Trees

| Tree | Purpose |
|------|---------|
| `attestations` | Attestation records |
| `claims` | Claim records |
| `nullifiers` | Nullifier SMT (consume double-spend prevention) |
| `attestation_index` | Attestation lookup index |
| `claim_rate_limits` | Per-claim rate-limit tracking |
| `delegations` | Delegation records |

## Capabilities & Actions

| Capability | Discriminant | Primitives | Note schema |
|------------|--------------|------------|-------------|
| `attestation` | `0` | `SecretKey, Commitment, ContractId, FuncId` | — (non-consumable) |
| `claim` | `1` | `SecretKey, Commitment, Nullifier, ContractId, FuncId` | — (consumable) |
| `delegation` | `2` | `SecretKey, Commitment, ContractId, FuncId` | — (non-consumable, revocable) |

| Action | Requires | Consumes | Produces | Barbs |
|--------|----------|----------|----------|-------|
| `create_attestation` | none | — | `attestation` | `Commit, Dispatch, Gate` |
| `create_claim` | `any(attestation)` | — | `claim` | `Commit, Dispatch, Gate` |
| `verify_claim` | `any(claim)` | `claim` | — | `Spend, Nullify, Commit, Dispatch, Gate` |
| `consume_claim` | `any(claim)` | `claim` | — | `Spend, Nullify, Commit, Dispatch, Gate` |
| `delegate_attestation` | `any(attestation)` | — | `delegation` | `Commit, Dispatch, Gate` |

## Authorization

- **Attestor** — creates and revokes/expires attestations (`attestor_pub` match).
- **Holder** — creates a claim against an active attestation, then verifies/consumes it.
- **Verifier** — verifies (`verify_claim`) or validates (`validate_claim`) the claim;
  consuming (`consume_claim`) spends the claim via the nullifier.

The ZK proof authenticates the transaction and (for consume) the holder; the actual
predicate trust rests on the on-chain attestation + the exec-side evaluation — the
three-layer trust model (`contract-trust-model.md`).

## References

- [Attestation Specification](../../../doc/src/contract/attestation.md)
- [Contract Trust Model](../../../doc/src/arch/contract-trust-model.md)
- [Contract Manifest](../../../doc/src/arch/manifest.md)
- [Contract WASM Type System](../../../doc/src/arch/contract-wasm-type-system.md) — Part B (L2)
- Source: `src/contract/attestation/`
