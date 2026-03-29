# DarkFi Labor Market Contract

A job/labor market contract using escrow and DAO governance for trustless conditional payments.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           Labor Market Flow                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Employer                          Worker                                   │
│     │                                 │                                       │
│     │  Create Job (with payment)     │                                       │
│     │────────────────────────────────►│                                       │
│     │                                 │                                       │
│     │         Accept Job             │                                       │
│     │◄────────────────────────────────│                                       │
│     │                                 │                                       │
│     │                                 │  Deliver Work                          │
│     │                                 │  (hash or commit)                       │
│     │◄────────────────────────────────│                                       │
│     │                                 │                                       │
│     │  Confirm (release payment)      │                                       │
│     │────────────────────────────────►│                                       │
│     │                                 │                                       │
│     OR: Timeout ──► Refund            │                                       │
│     OR: Dispute ──► DAO Resolution     │                                       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Delivery Types

| Type | Description | Use Case |
|------|-------------|----------|
| **Generic** | Submit `hash(zip_file)` as proof | Any deliverable: documents, images, etc. |
| **Git** | Submit `commit_hash` as proof | Code work: tamper-evident, timestamped |

## How It Works

### Generic Delivery (Zip Hash)

1. **Employer creates job** with expected deliverable hash
2. **Worker completes work**, zips it, computes `SHA256(zip_file)`
3. **Worker submits** the hash as proof of completed work
4. **Employer verifies** the zip matches the hash
5. **Employer confirms** -> payment released to worker

### Git Delivery (Commit Hash)

1. **Employer creates job** with expected commit ref
2. **Worker pushes** work to a git repo
3. **Worker submits** `git commit SHA` as proof
4. **Employer verifies** commit exists and contains expected work
5. **Employer confirms** -> payment released

**Why git is better for code work:**
- Git commit hash = `SHA(author + message + parent + timestamp + diff)`
- Tamper-evident: change any file, hash changes
- Timestamped: proves work existed before deadline
- Immutable audit trail

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

## Entrypoints

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateJobV1` | 0x00 | Employer posts job, deposits payment |
| `AcceptJobV1` | 0x01 | Worker accepts job |
| `SubmitDeliverableV1` | 0x02 | Worker submits zip hash |
| `SubmitGitDeliverableV1` | 0x03 | Worker submits commit hash |
| `ConfirmDeliveryV1` | 0x04 | Employer confirms, releases payment |
| `DisputeV1` | 0x05 | Either party escalates to DAO |
| `RefundV1` | 0x06 | Timeout triggers refund |
| `CancelV1` | 0x07 | Cancel before accepting |

## ZK Circuits

| Circuit | Purpose | Key Opcodes |
|---------|---------|-------------|
| `create_job_v1.zk` | Prove job creation is valid | `ec_mul_base`, `poseidon_hash`, `less_than_strict` |
| `accept_job_v1.zk` | Prove worker accepts job | `ec_mul_base` |
| `submit_deliverable_v1.zk` | Prove deliverable submission | `ec_mul_base`, `poseidon_hash`, `less_than_strict` |
| `submit_git_deliverable_v1.zk` | Prove git commit submission | `ec_mul_base`, `poseidon_hash`, `less_than_strict` |
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
├── Uses Escrow pattern for payment
├── Integrates with DAO-Escrow for dispute resolution
└── Composable with Subscription for recurring wages
```

## Example Flow: Code Review Job

```
Employer:
1. Creates job: "Review PR #123, pay 100 DRK"
   - deliverable_hash = expected commit hash
   - deadline = block 50000

Worker:
2. Reviews the PR, pushes review commit
3. Submits: commit_hash = abc123...
   (proves they did the work by that block)

Employer:
4. Verifies commit abc123 exists in repo
5. Confirms delivery -> 100 DRK to worker

OR: Worker never delivers by block 50000
-> Employer calls Refund -> 100 DRK returned
```

## See Also

- [DAO-Escrow Contract](../dao_escrow/README.md) - Dispute resolution and organization
- [Escrow Contract](../escrow/README.md) - HTLC-style payment escrow
- [Subscription Contract](../subscription/README.md) - Recurring payments
