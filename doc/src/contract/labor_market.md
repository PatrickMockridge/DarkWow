# Labor Market Contract Architecture

A job/labor market built on DarkWow primitives: escrow for payments, DAO for dispute resolution, and attestation for deliverable verification.

## Design Goals

1. **Trustless payments**: Employer deposits payment, worker delivers, payment releases only on confirmation
2. **Attestation-based verification**: Deliverables verified via the attestation contract
3. **Multiple delivery types**: Generic (zip hash) and Git (commit hash)
4. **Dispute resolution**: DAO governance handles contested jobs
5. **Privacy**: Payment amounts hidden via Pedersen commitments, parties derived from secrets

## Composability

Labor Market composes with four other contracts via cross-contract child calls in a single DarkTree transaction. For a detailed explanation of the child call mechanism, see [Composability](composability.md).

### Wired Cross-Contract Calls

```
┌─────────────────────────────────────────────────────────────────────────┐
│              Labor Market Cross-Contract Child Calls                      │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  labor_market::AcceptJobWithCapabilityV1 (0x0d)                         │
│      │                                                                   │
│      ├── child[0] → identity::VerifyCapabilityV1 (0x0b)                 │
│      │              Validates worker holds required capability on-chain  │
│      │                                                                   │
│      └── child[1] → money_v3::TransferV1 (0x04)                         │
│                     Escrow deposit from worker                           │
│                                                                          │
│  labor_market::SubmitDeliverableV1 (0x02)                               │
│  labor_market::SubmitGitDeliverableV1 (0x03)                            │
│      │                                                                   │
│      └── child[0] → attestation::VerifyClaimV1 (0x04)                   │
│                     Verifies deliverable matches pre-committed hash      │
│                                                                          │
│  labor_market::DisputeV1 (0x05)                                         │
│  labor_market::InitiateDisputeV1 (0x0b)                                 │
│      │                                                                   │
│      └── child[0] → dao_escrow::ProposeClaimV1 (0x07)                   │
│                     Escalates dispute to DAO governance                  │
│                                                                          │
│  labor_market::CreateJobV1 (0x00)                                       │
│  labor_market::AcceptJobV1 (0x01)                                       │
│  labor_market::ConfirmDeliveryV1 (0x04)                                 │
│  labor_market::RefundV1 (0x06)                                          │
│  labor_market::CancelV1 (0x07)                                          │
│  labor_market::CreateJobWithMilestonesV1 (0x08)                         │
│  labor_market::CreateJobWithCapabilityV1 (0x0c)                         │
│      │                                                                   │
│      └── child[0] → money_v3::TransferV1 (0x04)                         │
│                     Payment escrow / transfer / refund                   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Validation Pattern

Every function that accepts child calls validates them in the `instruction` phase (before any state mutation):

```rust
let this_call = &calls[call_idx];
if this_call.children_indexes.len() != 1 {
    return Err(LaborMarketError::InvalidChildrenIndexes.into())
}
let child_call = &calls[this_call.children_indexes[0]].data;
if child_call.data[0] != EXPECTED_FUNCTION_CODE {
    return Err(LaborMarketError::InvalidChildCall.into())
}
```

If the child call has the wrong function code, the entire transaction is rejected atomically — no state is mutated.

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

All 9 circuits compiled to `.zk.bin`:

| Circuit | Public Inputs | Proves |
|---------|-------------|--------|
| `create_job_v1.zk` | employer_pub, attestation_id | Employer knows secret |
| `accept_job_v1.zk` | job_id, worker_pub | Worker knows secret |
| `accept_job_with_capability_v1.zk` | job_id, worker_pub, capability_proof | Worker holds O-Cap credential |
| `submit_deliverable_v1.zk` | job_id, claim_id, worker_pub, nullifier | Worker assigned, deadline not passed, valid claim |
| `submit_git_deliverable_v1.zk` | job_id, claim_id, worker_pub, nullifier | Same + git claim verified |
| `confirm_delivery_v1.zk` | job_id, employer_pub, nullifier | Employer authorizes release |
| `milestone_payment_v1.zk` | job_id, milestone_id, employer_pub, nullifier | Milestone completed, employer authorizes payment |
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

1. **Milestone payments**: Implemented via `CreateJobWithMilestonesV1` (0x08), `SubmitMilestoneV1` (0x09), `ConfirmMilestoneV1` (0x0a). The `milestone_payment_v1.zk` circuit is compiled and registered. Milestones are now populated from params (fixed bug: previously `create_job_with_milestones_apply_v1` used empty `vec![]` instead of `params.milestones`).
2. **Reputation system**: Track worker performance across jobs
3. **Escrow tiers**: Different trust levels for different payment amounts
4. **Time tracking**: For ongoing hourly work, integrate with subscription
5. **Arbitration marketplace**: Multiple DAOs can compete to handle disputes

## O-Cap Integration (Capability-Aware Jobs)

With O-Cap authorization (Identity contract 0x09-0x0d), jobs can require workers to prove specific capabilities without revealing identity.

### New Function IDs

| ID | Function | Description |
|----|----------|-------------|
| `0x0c` | `CreateJobWithCapabilityV1` | Create job requiring worker capability |
| `0x0d` | `AcceptJobWithCapabilityV1` | Accept job with capability proof |
| `0x0e` | `CreateJobWithMilestonesAndCapabilityV1` | Milestone job with capability requirement |

### How It Works

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                O-Cap Labor Market Flow                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  IDENTITY CONTRACT (O-Cap):                                                 │
│     │                                                                       │
│     │  RegisterCapability("can_work_on_freelance_jobs"):                    │
│     │    - requires: credential.professional_license                       │
│     │    - requires: predicate(experience >= 3)                             │
│     │                                                                       │
│     │  IssueCapability(worker, "can_work_on_freelance_jobs"):               │
│     │    - Worker proves qualifications via ZK                            │
│     │    - Hides: name, employer, exact experience                          │
│     │                                                                       │
│     ▼                                                                       │
│  EMPLOYER creates job:                                                      │
│     │                                                                       │
│     │  CreateJobWithCapabilityV1(                                          │
│     │    required_capability_id: can_work_on_freelance_jobs,               │
│     │    ...                                                                │
│     │  )                                                                    │
│     ▼                                                                       │
│  WORKER accepts job:                                                       │
│     │                                                                       │
│     │  AcceptJobWithCapabilityV1(                                          │
│     │    capability_proof: VerifyCapabilityV1(can_work_on_...),          │
│     │    ...                                                                │
│     │  )                                                                    │
│     │                                                                       │
│     Result: Worker identity hidden, only capability verified                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Job Struct Extension

```rust
pub struct Job {
    // ... existing fields ...
    /// Required capability ID for workers (None = any worker can accept)
    pub required_capability_id: Option<[u8; 32]>,
    /// Required DAG ID for multi-path qualification (None = no DAG requirement)
    pub required_dag_id: Option<[u8; 32]>,
}
```

### Use Cases

| Job Type | Capability Required | DAG Path |
|----------|---------------------|----------|
| Senior Developer | `can_work_as_senior_dev` | CS Degree OR Industry Certs + 5yrs exp |
| Medical Consultant | `can_consult_healthcare` | License + Board Cert + 5yrs exp |
| Legal Advisor | `can_provide_legal_advice` | Bar membership + JD + 3yrs exp |
| Financial Auditor | `can_audit_finance` | CPA OR CA + Experience + References |

### ZK Circuit: `accept_job_with_capability_v1.zk`

The circuit verifies:
- Worker knows their secret key
- Worker's public key is correctly derived
- Capability predicate result is 1 (satisfied)

In addition to the ZK proof, the on-chain instruction phase validates a child call to `Identity::VerifyCapabilityV1 (0x0b)`. This cross-contract call provides on-chain verification that the Identity contract recognizes the capability as valid and non-revoked — the ZK proof proves the predicate, the child call proves the capability exists on-chain.

```zk
worker_pub = ec_mul_base(worker_secret, NULLIFIER_K);
derived_pub_x = ec_get_x(worker_pub);
constrain_equal_base(derived_pub_x, worker_pub_x);

# Verify capability predicate is satisfied
ONE = witness_base(1);
constrain_equal_base(capability_predicate_result, ONE);
```

### Benefits

1. **Worker privacy**: Employer learns only "worker has required capability", not who they are
2. **Skill filtering**: Jobs can require specific qualifications without discrimination
3. **DAG composition**: Multiple credential paths allow flexible qualification requirements
4. **Revocation**: If capability is revoked (credential expires), job acceptance fails

## Cross-Contract Child Calls

Labor Market uses cross-contract child calls to delegate authorization, verification, and payment to specialized contracts. For the complete cross-contract call map and mechanism, see [Composability](composability.md).

### Identity Verification (0x0d → 0x0b)

`AcceptJobWithCapabilityV1` requires a child call to `Identity::VerifyCapabilityV1 (0x0b)`. The Identity contract verifies on-chain that the worker's capability exists and has not been revoked. This is a defense-in-depth pattern: the ZK proof verifies the predicate, and the child call verifies the capability's on-chain state.

### Attestation Verification (0x02, 0x03 → 0x04)

`SubmitDeliverableV1` and `SubmitGitDeliverableV1` require a child call to `Attestation::VerifyClaimV1 (0x04)`. The Attestation contract checks `evidence_commitment == attestation.claim_data[0]` — proving the worker's deliverable matches the employer's pre-committed expectation.

### DAO Dispute Escalation (0x05, 0x0b → 0x07)

`DisputeV1` and `InitiateDisputeV1` require a child call to `DAO-Escrow::ProposeClaimV1 (0x07)`. This creates a governance proposal so DAO members can vote on the dispute outcome (release payment, refund, or split).

### Money Transfer (multiple → 0x04)

`CreateJobV1`, `AcceptJobV1`, `ConfirmDeliveryV1`, `RefundV1`, `CancelV1`, `CreateJobWithMilestonesV1`, and `CreateJobWithCapabilityV1` all require a child call to `money_v3::TransferV1 (0x04)` for payment escrow and release.

## See Also

- [Composability](composability.md) — child call mechanism and full call map
- [Recruitment Pipeline Case Study](recruitment_pipeline.md) — end-to-end walkthrough of all four contracts
- [DAO-Escrow Contract](dao_escrow.md) — dispute resolution destination
- [Attestation Contract](attestation.md) — deliverable verification destination
- [Labor Market Contract README](../../src/contract/labor_market/README.md)
