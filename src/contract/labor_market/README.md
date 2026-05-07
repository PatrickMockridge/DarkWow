# DarkWow Labor Market Contract

A job/labor market contract using escrow, DAO governance, and the attestation contract for deliverable verification.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Labor Market Flow                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Employer                          Worker                                    │
│     │                                │                                       │
│     │  Create Attestation            │                                       │
│     │  (deliverable_hash)            │                                       │
│     │───────────────────────────────►│                                       │
│     │                                │                                        │
│     │  Create Job (with payment)     │                                       │
│     │  (references attestation)      │                                       │
│     │────────────────────────────────►│                                       │
│     │                                │                                        │
│     │        Accept Job              │                                       │
│     │◄────────────────────────────────│                                       │
│     │                                │                                        │
│     │                                │  Create Claim (attestation)            │
│     │                                │  (evidence_commitment)                  │
│     │                                │                                        │
│     │                                │  Submit Deliverable                    │
│     │                                │  (claim_id)                            │
│     │◄────────────────────────────────│                                       │
│     │                                │                                        │
│     │  Confirm (release payment)      │                                       │
│     │────────────────────────────────►│                                       │
│     │                                │                                        │
│     OR: Timeout ──► Refund           │                                       │
│     OR: Dispute ──► DAO Resolution    │                                       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Delivery Types

| Type | Description | Use Case |
|------|-------------|----------|
| **Generic** | Submit claim against attestation | Any deliverable: documents, images, etc. |
| **Git** | Submit claim against attestation | Code work: tamper-evident, timestamped |

## How It Works

### Generic Delivery (with Attestation)

1. **Employer creates attestation** with expected deliverable hash
2. **Employer creates job** referencing the attestation
3. **Worker completes work**, zips it, computes hash
4. **Worker creates claim** on the attestation (evidence_commitment = hash)
5. **Worker submits deliverable** with claim_id
6. **Contract verifies** the attestation claim is valid
7. **Employer confirms** -> payment released

### Git Delivery (with Attestation)

1. **Employer creates attestation** with expected commit ref
2. **Employer creates job** referencing the attestation
3. **Worker pushes** work to a git repo
4. **Worker creates claim** on attestation (evidence_commitment = commit hash)
5. **Worker submits deliverable** with claim_id
6. **Attestation contract verifies** the commit matches
7. **Employer confirms** -> payment released

**Why attestation is better:**
- Reusable: same attestation can be used for multiple jobs
- Composable: other contracts can reference the same attestation
- Standardized: single implementation for all deliverable types

## State Machine

```
Created ──[Accept]──> InProgress ──[Deliver]──> Delivered
                                              │
                      ┌───────────┬───────────┴───────────┬───────────┐
                      │           │                       │           │
              [Confirm]     [Dispute]               [Timeout]   [Cancel]
                  │           │                       │           │
                  ▼           ▼                       ▼           ▼
            Confirmed    Disputed                  Refunded    Cancelled
```

## Attestation Integration

The labor market uses the [Attestation Contract](../attestation/README.md) for deliverable verification:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Labor Market + Attestation Flow                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Employer                                                                   │
│     │                                                                       │
│     │ CreateAttestation(deliverable_hash)                                   │
│     ▼                                                                       │
│  Attestation(Active)                                                        │
│     │                                                                       │
│     │ attestation_id                                                        │
│     │                                                                       │
│     │◄──────────────────────────────── CreateJob(job, attestation_id)      │
│     │                                                                       │
│     │                              Worker                                   │
│     │                                 │                                     │
│     │                                 │ CreateClaim(evidence_commitment)   │
│     │                                 ▼                                     │
│     │                              Claim(Verified)                         │
│     │                                 │                                     │
│     │                                 │ claim_id                            │
│     │                                 │                                     │
│     │◄── SubmitDeliverable(job_id, claim_id)                              │
│     │                                                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Attestation Predicate

The attestation uses `Predicate::Matches` to verify the deliverable:
- Worker submits `evidence_commitment = poseidon_hash(deliverable)`
- Attestation verifies `evidence_commitment == attestation.claim_data[0]`

## Entrypoints

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateJobV1` | 0x00 | Employer posts job (references attestation) |
| `AcceptJobV1` | 0x01 | Worker accepts job |
| `SubmitDeliverableV1` | 0x02 | Worker submits claim_id for generic delivery |
| `SubmitGitDeliverableV1` | 0x03 | Worker submits claim_id for git delivery |
| `ConfirmDeliveryV1` | 0x04 | Employer confirms, releases payment |
| `DisputeV1` | 0x05 | Either party escalates to DAO |
| `RefundV1` | 0x06 | Timeout triggers refund |
| `CancelV1` | 0x07 | Cancel before accepting |

## ZK Circuits

| Circuit | Purpose | Key Opcodes |
|---------|---------|-------------|
| `create_job_v1.zk` | Prove job creation + attestation_id | `ec_mul_base`, `constrain_instance` |
| `accept_job_v1.zk` | Prove worker accepts job | `ec_mul_base` |
| `submit_deliverable_v1.zk` | Prove deliverable submission + claim_id | `ec_mul_base`, `poseidon_hash`, `less_than_strict` |
| `submit_git_deliverable_v1.zk` | Prove git commit submission + claim_id | `ec_mul_base`, `poseidon_hash`, `less_than_strict` |
| `confirm_delivery_v1.zk` | Prove employer confirmation | `ec_mul_base`, `poseidon_hash` |
| `dispute_v1.zk` | Prove dispute initiation | `ec_mul_base`, `poseidon_hash` |
| `refund_v1.zk` | Prove timeout refund | `ec_mul_base`, `poseidon_hash`, `less_than_strict` |

All circuits use **proven opcodes only** (no grey-market `LessThanOrEqual` or `IsEqualBase`).

## DAO-Escrow Integration

**Dispute Resolution:**

When a dispute is raised, the job moves to `Disputed` state and DAO governance takes over:

1. Worker or employer calls `DisputeV1`
2. Job escalated to DAO-Escrow
3. DAO votes on resolution:
   - **Release**: Payment to worker (work acceptable)
   - **Refund**: Payment to employer (work not done)
   - **Partial**: Negotiated split
4. DAO executes resolution via `auth_calls`

**DAO as Employer:**

A DAO-Escrow can act as the organizational employer:
- Posts jobs on behalf of the DAO
- Pays out from treasury/endowment
- Membership notes prove contributor status

## Composability

```
Labor Market Contract
├── Uses Attestation for deliverable verification
├── Uses Escrow pattern for payment
├── Integrates with DAO-Escrow for dispute resolution
└── Composable with Subscription for recurring wages
```

## Example Flow: Code Review Job

```
Employer:
1. Creates attestation: expected commit hash = abc123...
2. Creates job: "Review PR #123, pay 100 DRK"
   - attestation_id = <from step 1>
   - deadline = block 50000

Worker:
3. Reviews the PR, pushes review commit
4. Creates claim on attestation: evidence = poseidon_hash(abc123...)
5. Submits deliverable: claim_id = <from step 4>
   (proves they did the work by that block)

Employer:
6. Attestation contract verifies claim is valid
7. Confirms delivery -> 100 DRK to worker

OR: Worker never delivers by block 50000
-> Employer calls Refund -> 100 DRK returned
```

## See Also

- [Attestation Contract](../attestation/README.md) - Generalized attestation and claims
- [DAO-Escrow Contract](../dao_escrow/README.md) - Dispute resolution and organization
- [Escrow Contract](../escrow/README.md) - HTLC-style payment escrow
- [Subscription Contract](../subscription/README.md) - Recurring payments