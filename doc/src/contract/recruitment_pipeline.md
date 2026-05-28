# Case Study: DAO Recruitment Pipeline

## How Four Contracts Compose to Automate Hiring

### Executive Summary

A DAO needs to hire workers, verify their qualifications, accept deliverables, handle disputes, and release payments. On a traditional freelancing platform, a centralized intermediary does all of this — and takes a cut, holds your data, and can freeze your account.

On DarkWow, **four independent smart contracts compose through cross-contract child calls** to do the same thing without a platform. The DAO sets up governance. Workers get credentials and prove them without revealing identity. Employers post capability-gated jobs with payment in escrow. Workers deliver and get paid. Disputes escalate to a DAO vote. No single contract does everything. No intermediary takes a fee. And at every step, ZK proofs reveal only what's necessary — nothing more.

### The Business Problem: Hiring Without Trust

When you hire someone on a platform like Upwork or Fiverr, the platform provides three things:

1. **Identity/Reputation**: The platform vouches that the freelancer has a track record.
2. **Escrow**: The platform holds the employer's payment until the work is done.
3. **Dispute Resolution**: If something goes wrong, the platform arbitrates.

The platform charges 5-20% for these services. It can freeze your funds, suspend your account, or change its terms. And your entire work history — every job, every rating, every payment — lives on their servers.

**The DarkWow answer**: Replace the platform with composable smart contracts. Each contract specializes in one of the three functions. Cross-contract child calls wire them together so they operate as one system — but no single entity controls the whole.

### Meet the Four Contracts

Think of each contract as a department in a company:

| Contract | Department | What It Does |
|---|---|---|
| **Identity** | Credentialing Authority | Issues credentials ("Senior Rust Developer"). Workers prove they hold credentials without revealing who they are. |
| **Labor Market** | Job Board | Employers post jobs with requirements. Workers apply. Deliverables are submitted and tracked. Payments execute on confirmation. |
| **Attestation** | Deliverable Inspector | Employers pre-commit to expected work hashes. Workers prove their deliverable matches. Claims are verified and consumed (cannot be reused). |
| **DAO Escrow** | HR / Finance | Holds treasury and endowment pools. Members vote on proposals. Disputes resolved by multi-oracle attestation. Payments released on approval. |

None of these contracts knows about the others' internal state. The Labor Market contract doesn't know how Identity verifies credentials — it only knows to check that a child call to `Identity::VerifyCapabilityV1 (0x0b)` is present and valid. The DAO Escrow doesn't know what a "job" is — it only knows to process proposals and votes. **The composition is the product.**

### The Pipeline: Step by Step

Each step below shows what happens in business terms, the real-world analogue, and the technical cross-contract call that makes it work.

---

#### Step 1: DAO Sets Up Shop

**What happens:** The organization creates its treasury (endowment pool + treasury pool) and sets governance rules: 60% quorum, 66% approval, 7-day voting window, 3-of-5 oracle threshold for disputes.

**Business analogy:** Incorporating a company — opening a bank account, filing bylaws, appointing a board.

**Contract call:** `dao_escrow::InitializeV1 (0x00)`

No child calls at this step. The DAO-Escrow contract initializes its internal state: mode (TreasuryEndowment), fee split (70/30), governance config, and capability requirements.

---

#### Step 2: Workers Get Credentials

**What happens:** A trusted issuer (certification body, university, professional association) issues a verifiable credential to a worker. The credential says "Senior Rust Developer, 5+ years experience" — but the worker can prove they hold it without revealing their name, exact experience, or who issued it.

**Business analogy:** Getting a professional certification. You can show an employer the certification badge without handing over your full transcript or revealing which university you attended.

**Contract call:** `identity::IssueCredentialV1 (0x01)`

ZK proof attests to credential attributes. The credential is stored on-chain as a Merkle leaf. The holder receives a nullifier that proves they own the credential without revealing which one.

---

#### Step 3: DAO Registers Required Capabilities

**What happens:** The DAO defines what capabilities a worker needs for a given role. For example, "For this smart contract development project, workers must prove they hold the `senior_rust_dev` credential with predicate `years_experience >= 3`."

**Business analogy:** Writing a job description with required qualifications. "Must have CS degree or equivalent, 5+ years Rust experience."

**Contract call:** `identity::RegisterCapabilityV1 (0x09)`

The capability definition includes a predicate (e.g., `years_experience >= 3`) that the worker must satisfy in ZK. Multiple credential paths can be combined via DAGs ("CS degree OR industry certs + 5 years").

---

#### Step 4: Employer Posts a Gated Job

**What happens:** An employer creates a job posting with payment held in escrow. The job requires the worker to prove they hold the `senior_rust_dev` capability. The payment amount is hidden via Pedersen commitment (only employer and worker know the amount).

**Business analogy:** Posting a job on LinkedIn with "5+ years experience required" — but with the payment already deposited and verifiable.

**Contract call:** `labor_market::CreateJobWithCapabilityV1 (0x0c)`

**Child call:** `promissory_note::TransferV1 (0x04)` — employer's payment is transferred to escrow.

The job struct includes `required_capability_id`, `attestation_id` (for deliverable verification), and payment commitment.

---

#### Step 5: Worker Applies With ZK Proof

**What happens:** A worker sees the job and applies. The contract verifies the worker holds the required capability — WITHOUT the employer learning the worker's identity, exact experience, or credential issuer. The employer only learns: "Someone with a valid `senior_rust_dev` credential has accepted this job."

**Business analogy:** A recruiter verifies you have the required degree, but never sees your transcript, never learns your name, and never knows which university certified you. They get exactly one bit of information: "qualified."

**Contract call:** `labor_market::AcceptJobWithCapabilityV1 (0x0d)`

**Child call:** `identity::VerifyCapabilityV1 (0x0b)` — the Identity contract verifies the capability on-chain. The ZK proof in the params also verifies the capability predicate is satisfied (`predicate_result == 1`).

**Validation pattern:**
```
labor_market checks: child_call.data[0] != 0x0b  // Must be Identity::VerifyCapabilityV1
```

If the child call is missing or has the wrong function code, the transaction is rejected atomically — no state mutation occurs.

---

#### Step 6: Worker Submits Deliverable

**What happens:** The worker completes the work and submits proof. For code work, this is a git commit hash (tamper-evident, timestamped). For generic work, it's a zip file hash. The attestation contract verifies the deliverable matches what the employer pre-committed to.

**Business analogy:** Submitting a pull request on GitHub. The git commit proves who did what and when. The employer pre-approved the expected commit hash, so the system verifies "this is the work we agreed on."

**Contract call:** `labor_market::SubmitDeliverableV1 (0x02)` or `SubmitGitDeliverableV1 (0x03)`

**Child call:** `attestation::VerifyClaimV1 (0x04)` — verifies `evidence_commitment == attestation.claim_data[0]`.

**Validation pattern:**
```
labor_market checks: child_call.data[0] != 0x04  // Must be Attestation::VerifyClaimV1
```

A nullifier (`poseidon_hash(job_id, claim_id)`) prevents the worker from submitting the same deliverable twice.

---

#### Step 7: Employer Confirms → Payment Released

**What happens:** The employer reviews the work, confirms it is satisfactory, and payment is automatically released from escrow to the worker. A nullifier prevents the employer from confirming the same job twice.

**Business analogy:** Clicking "Approve Milestone" on a freelancing platform. The funds move from escrow to the freelancer's wallet.

**Contract call:** `labor_market::ConfirmDeliveryV1 (0x04)`

**Child call:** `promissory_note::TransferV1 (0x04)` — transfers funds from escrow to the worker.

The employer's ZK proof verifies they authorized the release. The worker's identity remains hidden — the payment goes to a derived address, not a publicly linked one.

---

#### Step 8: Dispute Escalation (Alternative Path)

**What happens:** If either party is unhappy with the deliverable, they can escalate to the DAO instead of the confirm/refund path. The dispute creates a proposal in the DAO's governance system.

**Business analogy:** Filing a dispute with the platform's arbitration team. Instead of a support ticket going to a centralized team, it becomes a governance proposal that DAO members vote on.

**Contract call:** `labor_market::DisputeV1 (0x05)` or `InitiateDisputeV1 (0x0b)`

**Child call:** `dao_escrow::ProposeClaimV1 (0x07)` — creates a governance proposal for dispute resolution.

**Validation pattern:**
```
labor_market checks: child_call.data[0] != 0x07  // Must be DAO-Escrow::ProposeClaimV1
```

The dispute carries the `dao_escrow_bulla` (which DAO instance handles this job) and a dispute reason hash.

---

#### Step 9: DAO Votes on Dispute

**What happens:** DAO members vote on the dispute. Each vote is a ZK-proofed membership claim — members prove they hold the `member_vote` capability without revealing their identity or vote direction. Only the final tally is public.

**Business analogy:** A jury deliberating a case. Each juror votes privately. Only the verdict — "release payment" or "refund employer" — is announced.

**Contract calls:**
- `dao_escrow::ProposeClaimV1 (0x07)` — member proposes a resolution
- `dao_escrow::VoteClaimV1 (0x08)` — members vote

ZK proofs verify `member_vote` capability. Vote nullifier (`H(capability_secret, proposal_id)`) prevents double-voting. Vote direction is hidden via Pedersen commitment.

---

#### Step 10: Resolution Executed

**What happens:** Once the voting window closes and quorum + approval threshold are met, an arbitrator executes the DAO's decision. Multiple independent oracles confirm the facts on-chain (e.g., 3 of 5 oracles attest to deliverable quality). The contract verifies the oracles' attestations and transfers funds accordingly.

**Business analogy:** A court bailiff executing the jury's verdict after confirming the evidence with multiple independent expert witnesses.

**Contract call:** `dao_escrow::ResolveDisputeV1 (0x0c)`

**Child calls:** Multiple `attestation::VerifyClaimV1 (0x04)` for oracle attestations + `promissory_note::TransferV1 (0x04)` for the payout.

**Anti-replay protection:** `db_contains_key(disputes_db, dispute_id)` check prevents the same dispute from being resolved twice. The `dispute_id` is derived as `poseidon_hash(proposal_id, attestation_count, payout_recipient)` — unique per resolution attempt.

### Complete Call Flow Diagram

```
┌─────────────────────────────────────────────────────────────────────────────────────┐
│                    DAO Recruitment Pipeline — Complete Cross-Contract Call Map         │
├─────────────────────────────────────────────────────────────────────────────────────┤
│                                                                                      │
│  STEP 1-3: Setup (no cross-contract calls)                                           │
│  ┌──────────────┐     ┌──────────────┐     ┌──────────────┐                          │
│  │ DAO-Escrow   │     │  Identity    │     │  Identity    │                          │
│  │ InitializeV1 │     │ IssueCreden- │     │ RegisterCap- │                          │
│  │ (0x00)       │     │ tialV1 (0x01)│     │ abilityV1    │                          │
│  └──────────────┘     └──────────────┘     │ (0x09)       │                          │
│                                             └──────────────┘                          │
│                                                                                      │
│  STEP 4: Employer creates job                                                        │
│  ┌──────────────────────┐                                                            │
│  │ Labor Market         │                                                            │
│  │ CreateJobWithCapV1   │──→ promissory_note::TransferV1 (0x04) [escrow deposit]            │
│  │ (0x0c)               │                                                            │
│  └──────────────────────┘                                                            │
│                                                                                      │
│  STEP 5: Worker accepts job                                                          │
│  ┌──────────────────────┐                                                            │
│  │ Labor Market         │──→ Identity::VerifyCapabilityV1 (0x0b)                     │
│  │ AcceptJobWithCapV1   │──→ promissory_note::TransferV1 (0x04) [acceptance stake]          │
│  │ (0x0d)               │                                                            │
│  └──────────────────────┘                                                            │
│                                                                                      │
│  STEP 6: Worker submits deliverable                                                  │
│  ┌──────────────────────┐                                                            │
│  │ Labor Market         │──→ Attestation::VerifyClaimV1 (0x04)                       │
│  │ SubmitDeliverableV1  │                                                            │
│  │ (0x02)               │                                                            │
│  └──────────────────────┘                                                            │
│                                                                                      │
│  STEP 7: Employer confirms (happy path)                                              │
│  ┌──────────────────────┐                                                            │
│  │ Labor Market         │──→ promissory_note::TransferV1 (0x04) [release to worker]         │
│  │ ConfirmDeliveryV1    │                                                            │
│  │ (0x04)               │                                                            │
│  └──────────────────────┘                                                            │
│                                                                                      │
│  STEP 8-10: Dispute → DAO (unhappy path)                                             │
│  ┌──────────────────────┐                                                            │
│  │ Labor Market         │──→ DAO-Escrow::ProposeClaimV1 (0x07)                       │
│  │ DisputeV1 (0x05)     │                                                            │
│  └──────────────────────┘                                                            │
│           │                                                                          │
│           ▼                                                                          │
│  ┌──────────────────────┐                                                            │
│  │ DAO-Escrow           │──→ Identity::VerifyCapabilityV1 (0x0b) [member vote check] │
│  │ VoteClaimV1 (0x08)   │                                                            │
│  └──────────────────────┘                                                            │
│           │                                                                          │
│           ▼                                                                          │
│  ┌──────────────────────┐                                                            │
│  │ DAO-Escrow           │──→ Attestation::VerifyClaimV1 (0x04) [×N oracle attests]   │
│  │ ResolveDisputeV1     │──→ promissory_note::TransferV1 (0x04) [payout]                    │
│  │ (0x0c)               │                                                            │
│  └──────────────────────┘                                                            │
│                                                                                      │
│  FUNCTION CODE LEGEND:                                                               │
│    0x0b = Identity::VerifyCapabilityV1        0x04 = Attestation::VerifyClaimV1      │
│    0x07 = DAO-Escrow::ProposeClaimV1          0x04 = promissory_note::TransferV1            │
│                                                                                      │
└─────────────────────────────────────────────────────────────────────────────────────┘
```

### What Makes This Composable?

The key insight: **each contract specializes in exactly one thing**. None of them knows about the others' internal state:

- **Identity** knows about credentials and capabilities. It doesn't know what a "job" is.
- **Labor Market** knows about jobs, escrow, and state transitions. It doesn't know how to verify a credential — it delegates to Identity.
- **Attestation** knows about claims and evidence verification. It doesn't know about payments or governance.
- **DAO Escrow** knows about proposals, voting, and treasury management. It doesn't know what a "deliverable" is.

Combined through child calls, they form a complete hiring pipeline — **without any single "platform" contract** that would need to understand all four domains.

This is the opposite of the monolithic smart contract pattern. Instead of one contract with 50 functions and 20 storage trees, you have four contracts with 12-15 functions each, composing at the transaction level.

### Privacy Properties

At every step, ZK proofs reveal only what's necessary:

| Step | What's Revealed | What's Hidden |
|---|---|---|
| Worker gets credential | Credential exists, attributes valid | Worker identity, issuer identity |
| Worker accepts job | "Someone qualified accepted" | Worker identity, exact experience, credential source |
| Worker submits deliverable | Evidence commitment matches attestation | Worker identity, deliverable contents |
| Employer confirms | "Payment authorized" | Employer identity, payment amount |
| DAO member votes | "A valid member voted" | Member identity, vote direction (until tally) |
| Dispute resolved | "Resolution executed" | Arbitrator identity, oracle identities (attestation refs only) |

Payment amounts are hidden via Pedersen commitments. Only the employer and worker — who share the commitment secret — know the actual amount.

### How This Differs from Traditional Smart Contract Platforms

**Ethereum / EVM chains:**
- Each step requires a separate transaction with its own gas cost and confirmation time.
- Contract A calls Contract B via `CALL` opcode — state changes are public, all intermediate state is visible.
- Composing 4 contracts for a hiring pipeline means 6-10 separate transactions over hours or days.
- Privacy requires additional layers (Tornado Cash, Aztec) bolted on top.

**DarkWow:**
- All steps execute in a **single DarkTree transaction**. Atomic: all succeed or all revert.
- ZK proofs at each level mean intermediate state is never public. The verifier learns "predicate satisfied" — not the inputs that satisfied it.
- The four contracts compose natively — no adapter contracts, no proxy patterns.
- Privacy is **baked into the contract logic**, not layered on top.

---

This pipeline is not hypothetical. It is implemented across four contracts, verified by 29 heavyweight integration tests, and exercised through a full 10-step integration test (`test_heavyweight_recruitment_pipeline`) that deploys all four contracts and walks through every step — credential issuance through dispute resolution.

## See Also

- [Composability](composability.md) — how DarkForest/DarkTree child calls work at the technical level
- [Identity Contract README](../../../src/contract/identity/README.md) — full O-Cap specification
- [Labor Market Contract README](../../../src/contract/labor_market/README.md) — job lifecycle and ZK circuits
- [DAO-Escrow Contract README](../../../src/contract/dao_escrow/README.md) — governance modes and capabilities
- [Attestation Contract README](../../../src/contract/attestation/README.md) — claim verification patterns
