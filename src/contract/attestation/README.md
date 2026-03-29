# Attestation Contract

A generalized attestation and claims system that provides a reusable module for implementing claim verification patterns across DarkFi contracts.

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
    pub state: AttestationState,      // Active, Revoked, Expired
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
    pub evidence_commitment: Vec<u8>,  // H(evidence)
    pub revealed_result: Vec<u8>,      // Minimal result (e.g., true/false)
    pub state: ClaimState,            // Pending, Verified, Consumed, Rejected
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
| `RevokeAttestationV1` | 0x01 | Attestor revokes |
| `ExpireAttestationV1` | 0x02 | Mark as expired |
| `CreateClaimV1` | 0x03 | Claimant creates a claim |
| `VerifyClaimV1` | 0x04 | Verify claim (ZK + on-chain) |
| `ConsumeClaimV1` | 0x05 | Mark claim as used (prevents replay) |
| `ValidateClaimV1` | 0x06 | Fast path: verify without consuming |

## Integration Patterns

### Labor Market Integration

Employer attests to a deliverable hash, worker claims completion:

```rust
// Employer creates attestation for job deliverable
let attestation_id = attestation::create_attestation(
    attestor: employer_pubkey,
    claim_type: Predicate::Matches,
    claim_data: vec![deliverable_hash],
)?;

// Worker submits claim that their work matches
let claim_id = attestation::create_claim(
    attestation_id,
    claimant: worker_pubkey,
    predicate: Predicate::Matches,
    evidence_commitment: poseidon_hash(submitted_deliverable),
)?;

// On-chain: attestation.claim_data[0] == submitted_deliverable
```

### Tender Integration

Requester attests to competency requirements, bidders claim competency:

```rust
// Requester creates attestation for competency requirement
let attestation_id = attestation::create_attestation(
    attestor: requester_pubkey,
    claim_type: Predicate::Custom,
    claim_data: vec![requirement_commitment],
)?;

// Bidder submits claim of competency
let claim_id = attestation::create_claim(
    attestation_id,
    claimant: bidder_pubkey,
    predicate: Predicate::Matches,
    evidence_commitment: bidder_competency_commitment,
)?;
```

### Oracle Integration

Attestor (oracle) attests to external data (push model):

```rust
// Oracle attests to price data
let attestation_id = attestation::create_attestation(
    attestor: oracle_pubkey,
    claim_type: Predicate::GreaterOrEqual,
    claim_data: vec![min_price],
    expires_at: Some(current_block + 100),
)?;

// Consumer claims price meets threshold
let claim_id = attestation::create_claim(
    attestation_id,
    claimant: consumer_pubkey,
    predicate: Predicate::GreaterOrEqual,
    evidence_commitment: poseidon_hash(current_price),
)?;
```

## ZK Circuits

| Circuit | Purpose |
|---------|---------|
| `create_attestation_v1.zk` | Proves attestor knows secret key |
| `create_claim_v1.zk` | Proves claimant knows secret, attestation exists |
| `verify_claim_v1.zk` | Proves predicate satisfied |
| `consume_claim_v1.zk` | Proves claim consumed with nullifier |

## Database Trees

- `attestations`: Attestation structs keyed by ID
- `claims`: Claim structs keyed by ID
- `nullifiers`: Spent claims to prevent replay
- `attestation_index`: Index by attestor for lookup

## Benefits

1. **Code reuse**: Single implementation of claim/attestation logic
2. **Composability**: Contracts can reference each other's attestations
3. **Auditability**: Core pattern audited once
4. **Oracle integration**: Oracle contracts can create attestations
5. **Cross-contract claims**: Claim from contract A's attestation in contract B