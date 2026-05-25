# Attestation Contract

A generalized attestation and claims system that provides a reusable module for implementing claim verification patterns across DarkWow contracts.

## Overview

The attestation contract implements a common pattern found in many contracts:

```
Attestor → Attestation → Claimant → Claim → Validation
```

This allows contracts to:
- Create attestations (commitments to claims or conditions)
- Allow claimants to make claims against attestations
- Verify claims using predicates
- Consume claims to prevent replay

## Core Concepts

### Attestation

An **Attestation** is a party's commitment to a claim or condition:

```rust
pub struct Attestation {
    pub id: AttestationId,           // Commitment hash
    pub attestor_pubkey: PublicKey,  // Who attests
    pub claim_type: Predicate,        // Type of claim
    pub claim_data: Vec<pallas::Base>, // The commitment/hash data
    pub metadata: Vec<u8>,             // Optional encrypted metadata
    pub state: AttestationState,      // Active, Revoked, Expired
    pub created_at: u64,
    pub expires_at: Option<u64>,
}
```

### Claim

A **Claim** is a claimant's assertion based on an attestation:

```rust
pub struct Claim {
    pub id: ClaimId,
    pub attestation_id: AttestationId, // Links to the attestation
    pub claimant_pubkey: PublicKey,
    pub predicate: Predicate,          // What is being proven
    pub evidence_commitment: Vec<u8>,   // H(evidence)
    pub revealed_result: Vec<u8>,       // Minimal result (e.g., true/false)
    pub proof: Vec<u8>,                 // ZK proof
    pub state: ClaimState,              // Pending, Verified, Consumed, Rejected
    pub created_at: u64,
    pub consumed_at: Option<u64>,
}
```

### Predicate

Predicates define what verification is required:

| Predicate | Description |
|-----------|-------------|
| `Matches` | Evidence must match attestation data exactly |
| `GreaterOrEqual` | Value >= threshold |
| `LessOrEqual` | Value <= threshold |
| `Contains` | Data contains a pattern |
| `Custom` | Custom predicate via ZK circuit |

## State Machines

### Attestation State

```
Active ──[Revoke]──> Revoked
    │
    └──[Expire]──> Expired
```

### Claim State

```
Pending ──[Verify:valid]──> Verified ──[Consume]──> Consumed
    │
    └──[Verify:invalid]──> Rejected
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateAttestationV1` | 0x00 | Attestor creates an attestation |
| `RevokeAttestationV1` | 0x01 | Attestor revokes an attestation |
| `ExpireAttestationV1` | 0x02 | Mark attestation as expired |
| `CreateClaimV1` | 0x03 | Claimant creates a claim |
| `VerifyClaimV1` | 0x04 | Verify claim (ZK + on-chain) |
| `ConsumeClaimV1` | 0x05 | Mark claim as consumed (prevents replay) |
| `ValidateClaimV1` | 0x06 | Fast path: verify without consuming |
| `CheckNotRevokedV1` | 0x07 | Verify credential not revoked (Identity contract integration) |
| `DelegateAttestationV1` | 0x08 | Delegate attestation authority |
| `VerifyChainV1` | 0x09 | Multi-step attestation chain verification |
| `UpdateDelegationV1` | 0x0a | Update delegation parameters |
| `AttestSlashV1` | 0x0b | Slash attestor for false attestation |
| `CommitFeeScheduleV1` | 0x0c | Commit to fee schedule for attestation services |

## Integration Patterns

### Labor Market Integration

Employer attests to a deliverable hash, worker claims completion. Labor Market submits the claim verification as a cross-contract child call:

```
┌─────────────────────────────────────────────────────────────────┐
│                  Labor Market + Attestation Flow                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Employer                                                        │
│     │                                                            │
│     │ CreateAttestation(deliverable_hash)                        │
│     ▼                                                            │
│  Attestation(Active) ──────────────────────────────────────────┐│
│                                                                  │
│                                              Worker              │
│                                                 │               │
│                                                 │ CreateClaim() │
│                                                 ▼               │
│                                              Claim(Pending)     │
│                                                 │               │
│  labor_market::SubmitDeliverableV1 (0x02)       │               │
│     │                                           │               │
│     └── child call → attestation::VerifyClaimV1 (0x04)          │
│                      Verifies claim on-chain,                   │
│                      checks evidence_commitment                 │
│                                                 │               │
│  labor_market::SubmitGitDeliverableV1 (0x03)    │               │
│     └── child call → attestation::VerifyClaimV1 (0x04)          │
│                                                                  │
│  Claim consumed, prevents replay                                 │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### Cross-Contract Call Verification

`VerifyClaimV1 (0x04)` is the primary function called by other contracts as a child call. It provides on-chain attestation verification without requiring the calling contract to understand the attestation's internal state:

- **Labor Market**: `SubmitDeliverableV1 (0x02)` and `SubmitGitDeliverableV1 (0x03)` both require a child call to `VerifyClaimV1 (0x04)`. The calling contract validates `child_call.data[0] != 0x04`.
- **DAO-Escrow**: `ResolveDisputeV1 (0x0c)` includes multiple `VerifyClaimV1` child calls for oracle attestation validation.

For the complete cross-contract call map, see [Composability](composability.md).

### Tender Integration

Requester attests to competency requirements, bidders claim competency:

```
┌─────────────────────────────────────────────────────────────────┐
│                  Tender + Attestation Flow                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Requester                                                      │
│     │                                                           │
│     │ CreateAttestation(requirement_commitment)                 │
│     ▼                                                           │
│  Attestation(Active) ──────────────────────────────────────────┐ │
│                                                                   │
│                                              Bidder              │ │
│                                                 │               │ │
│                                                 │ CreateClaim() │ │
│                                                 ▼               │ │
│                                              Claim(Pending)     │ │
│                                                                   │
│  Requester selects winner based on verified claims              │
│                                                                   │
└───────────────────────────────────────────────────────────────────┘
```

### Oracle Integration

Attestor (oracle) attests to external data (push model):

```
┌─────────────────────────────────────────────────────────────────┐
│                    Oracle + Attestation Flow                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Oracle                                                         │
│     │                                                           │
│     │ CreateAttestation(price_data)                            │
│     ▼                                                           │
│  Attestation(Active) ──────────────────────────────────────────┐ │
│     │                                                           │
│     │ expires_at: current_block + 100                          │ │
│     │                                                           │ │
│     │                              Consumer                     │ │
│     │                                │                         │ │
│     │                                │ CreateClaim(price)      │ │
│     │                                ▼                         │ │
│     │                             Claim(Verified)               │ │
│     │                                │                         │ │
│     │                                │ ConsumeClaim()          │ │
│     │                                ▼                         │ │
│     │                             Claim(Consumed)               │ │
│                                                                   │ │
└───────────────────────────────────────────────────────────────────┘
```

## ZK Circuits

All 10 circuits compiled to `.zk.bin`:

| Circuit | Purpose |
|---------|---------|
| `create_attestation_v1.zk` | Prove attestor knows secret key; commitment correctly formed |
| `create_claim_v1.zk` | Prove claimant knows secret, attestation exists |
| `verify_claim_v1.zk` | Prove predicate satisfied against attestation data |
| `consume_claim_v1.zk` | Prove claim consumed with nullifier (prevents replay) |
| `check_not_revoked_v1.zk` | Prove credential not revoked (Identity contract integration) |
| `delegate_attestation_v1.zk` | Prove valid delegation of attestation authority |
| `verify_chain_v1.zk` | Prove multi-step attestation chain valid |
| `update_delegation_v1.zk` | Prove delegation update authorized |
| `attest_slash_v1.zk` | Prove attestor submitted false attestation |
| `commit_fee_schedule_v1.zk` | Prove fee schedule commitment correctly formed |

## Database Trees

| Tree | Purpose |
|------|---------|
| `attestations` | Attestation structs keyed by ID |
| `claims` | Claim structs keyed by ID |
| `nullifiers` | Spent claims to prevent replay |
| `attestation_index` | Index by attestor for lookup |
| `claim_rate_limits` | Rate limiting for claim operations |
| `delegations` | Delegation records for delegated attestation authority |

## Benefits

1. **Code reuse**: Single implementation of claim/attestation logic
2. **Composability**: Contracts can reference each other's attestations
3. **Auditability**: Core pattern audited once
4. **Oracle integration**: Oracle contracts can create attestations
5. **Cross-contract claims**: Claim from contract A's attestation in contract B

## See Also

- [Composability](composability.md) - Cross-contract composition patterns
- [Labor Market Contract](labor_market.md) - Uses attestation for deliverable verification
- [Tender Contract](tender.md) - Uses attestation for competency claims