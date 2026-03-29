# Labor Market Contract Architecture

A job/labor market built on DarkFi primitives: escrow for payments, DAO for dispute resolution.

## Design Goals

1. **Trustless payments**: Employer deposits payment, worker delivers, payment releases only on confirmation
2. **Multiple delivery types**: Generic (zip hash) and Git (commit hash)
3. **Dispute resolution**: DAO governance handles contested jobs
4. **Privacy**: Payment amounts hidden via Pedersen commitments, parties derived from secrets

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
└─────────────────────────────────────────────────────────────────────────┘
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

**Proof**: `hash(zip_file)` = deterministic hash of compressed deliverable

**Verification**:
1. Worker computes `SHA256(zip_file)` locally
2. Worker submits hash to contract
3. Employer downloads zip, verifies hash matches
4. Employer confirms delivery

**Strengths**:
- Works for any file type
- Simple to implement
- Deterministic

**Weaknesses**:
- Requires employer to verify content manually
- No timestamping

### Git (Commit Hash)

**Proof**: `commit_hash = SHA(author + message + parent + timestamp + diff)`

**Verification**:
1. Worker pushes commit to repo
2. Worker submits `git rev-parse HEAD` as proof
3. Employer verifies commit exists in expected branch
4. Employer reviews diff/content

**Strengths**:
- Tamper-evident (changing files changes hash)
- Timestamped (proves work existed at specific time)
- Full audit trail

**Weaknesses**:
- Only for code work
- Requires git knowledge

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
| `create_job_v1.zk` | job_id, payment_commit | Employer knows secret, deadline valid |
| `accept_job_v1.zk` | job_id, worker_pub | Worker knows secret |
| `submit_deliverable_v1.zk` | job_id, worker_pub, nullifier | Worker assigned, deadline not passed |
| `submit_git_deliverable_v1.zk` | job_id, commit_hash, worker_pub, nullifier | Same + commit hash submitted |
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

1. **Delivery hash binding**: The deliverable_hash is committed in `create_job_v1.zk`. Worker cannot substitute a different deliverable.

2. **Deadline enforcement**: `less_than_strict` in circuits ensures deadlines are enforced at circuit level.

3. **Double-submission prevention**: Nullifiers prevent worker from submitting multiple times.

4. **Authorization**: Only employer can confirm or refund. Only assigned worker can submit deliverable.

5. **DAO as arbiter**: Disputes go to governance, not a single party.

## Future Improvements

1. **Milestone payments**: Split job into multiple deliverables with partial payments
2. **Reputation system**: Track worker performance across jobs
3. **Escrow tiers**: Different trust levels for different payment amounts
4. **Time tracking**: For ongoing hourly work, integrate with subscription
5. **Arbitration marketplace**: Multiple DAOs can compete to handle disputes
