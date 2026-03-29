# Labor Market Contract Architecture

A job/labor market built on DarkFi primitives: escrow for payments, DAO for dispute resolution, and attestation for deliverable verification.

## Design Goals

1. **Trustless payments**: Employer deposits payment, worker delivers, payment releases only on confirmation
2. **Attestation-based verification**: Deliverables verified via the attestation contract
3. **Multiple delivery types**: Generic (zip hash) and Git (commit hash)
4. **Dispute resolution**: DAO governance handles contested jobs
5. **Privacy**: Payment amounts hidden via Pedersen commitments, parties derived from secrets

## Composability

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Labor Market Composability                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────────┐         ┌──────────────────────┐              │
│  │     DAO-Escrow       │         │      Escrow          │              │
│  │                      │         │                       │              │
│  │  - Dispute resolution│         │  - Payment held      │              │
│  │  - Organization      │◄────────│  - Timeout refund     │              │
│  │  - Treasury          │  uses    │  - Release on confirm│              │
│  └──────────────────────┘         └──────────────────────┘              │
│                                                                          │
│  ┌──────────────────────┐         ┌──────────────────────┐              │
│  │   Labor Market      │         │    Subscription       │              │
│  │                      │         │                       │              │
│  │  - Job posting       │────────►│  - Recurring wages    │              │
│  │  - Delivery types    │         │  - Ongoing employment │              │
│  │  - Worker selection  │         │                       │              │
│  └──────────────────────┘         └──────────────────────┘              │
│                                                                          │
│  ┌──────────────────────┐                                                  │
│  │    Attestation       │                                                  │
│  │                      │                                                  │
│  │  - Deliverable verify│◄────────│  - Worker claims completion           │
│  │  - Reusable attest   │  uses    │  - Employer attests expected hash    │
│  └──────────────────────┘                                                  │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Attestation Integration

The labor market uses the [Attestation Contract](./attestation.md) for deliverable verification:

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
│     │                              Claim(Verified)                          │
│     │                                 │                                     │
│     │                                 │ claim_id                            │
│     │                                 │                                     │
│     │◄── SubmitDeliverable(job_id, claim_id)                              │
│     │                                                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

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
                         │
                         ▼
                   DAO Resolution
                         │
              ┌──────────┴──────────┐
              │                     │
         Release               Refund
```

## Delivery Verification

### Generic (Zip Hash)

**Attestation**: Employer creates attestation with `claim_data = [deliverable_hash]`

**Claim**: Worker creates claim with `evidence_commitment = poseidon_hash(zip_file)`

**Verification**: Attestation contract verifies `evidence_commitment == claim_data[0]`

**Flow**:
1. Worker computes `poseidon_hash(zip_file)` locally
2. Worker creates claim on attestation with evidence_commitment
3. Worker submits deliverable with claim_id
4. Contract verifies claim is valid via attestation contract
5. Employer confirms delivery

### Git (Commit Hash)

**Attestation**: Employer creates attestation with `claim_data = [expected_commit_hash]`

**Claim**: Worker creates claim with `evidence_commitment = poseidon_hash(submitted_commit)`

**Verification**: Attestation contract verifies commit hash matches

**Flow**:
1. Worker pushes commit to repo
2. Worker creates claim with `evidence_commitment = poseidon_hash(git_commit_SHA)`
3. Worker submits deliverable with claim_id
4. Attestation contract verifies commit hash matches

**Git Benefits**:
- Tamper-evident (changing files changes hash)
- Timestamped (proves work existed at specific time)
- Full audit trail

## ZK Circuit Design

All circuits use **proven opcodes only**:

- `ec_mul_base`: Public key derivation
- `poseidon_hash`: Commitments, nullifiers
- `less_than_strict`: Block-based time locks (constrain-only)
- `constrain_equal_base`: Equality checks

No grey-market opcodes (`LessThanOrEqual`, `IsEqualBase`).

### Key Circuits

| Circuit | Public Inputs | Proves |
|---------|-------------|--------|
| `create_job_v1.zk` | employer_pub, attestation_id | Employer knows secret |
| `accept_job_v1.zk` | job_id, worker_pub | Worker knows secret |
| `submit_deliverable_v1.zk` | job_id, claim_id, worker_pub, nullifier | Worker assigned, deadline not passed, valid claim |
| `submit_git_deliverable_v1.zk` | job_id, claim_id, worker_pub, nullifier | Same + git claim verified |
| `confirm_delivery_v1.zk` | job_id, employer_pub, nullifier | Employer authorizes release |
| `dispute_v1.zk` | job_id, disputer_pub, dao_bulla, nullifier | Disputer is party to job |
| `refund_v1.zk` | job_id, employer_pub, nullifier | Deadline passed, employer authorizes |

## Dispute Resolution Flow

```
1. Either party calls DisputeV1
   └── Job moves to Disputed state

2. DAO governance receives dispute
   └── Proposal created with job details

3. DAO votes:
   ├── Release to worker: "Work was completed satisfactorily"
   ├── Refund to employer: "Work not delivered / unsatisfactory"
   └── Split: "Partial completion agreed"

4. DAO executes resolution
   └── auth_calls releases funds accordingly
```

## Security Considerations

1. **Attestation binding**: The attestation_id is committed in `create_job_v1.zk`. Worker cannot bypass attestation verification.

2. **Claim verification**: Actual deliverable verification is handled by the attestation contract. Labor market only verifies claim existence and validity.

3. **Deadline enforcement**: `less_than_strict` in circuits ensures deadlines are enforced at circuit level.

4. **Double-submission prevention**: Nullifiers prevent worker from submitting multiple times.

5. **Authorization**: Only employer can confirm or refund. Only assigned worker can submit deliverable.

6. **DAO as arbiter**: Disputes go to governance, not a single party.

## Breaking Changes (v2)

- **Removed**: `Job.deliverable_hash` - replaced with `Job.attestation_id`
- **Removed**: `SubmitDeliverableParamsV1.deliverable_hash` - replaced with `claim_id`
- **Removed**: `SubmitGitDeliverableParamsV1.commit_hash` - replaced with `claim_id`
- **Added**: Workers must create attestation claim before submitting deliverable

## Future Improvements

1. **Milestone payments**: Split job into multiple deliverables with partial payments
2. **Reputation system**: Track worker performance across jobs
3. **Escrow tiers**: Different trust levels for different payment amounts
4. **Time tracking**: For ongoing hourly work, integrate with subscription
5. **Arbitration marketplace**: Multiple DAOs can compete to handle disputes
