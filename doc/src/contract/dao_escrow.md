# DAO-Escrow Contract

A flexible contract supporting three operating modes: **Escrow-Only**, **Treasury-Only**, and **Treasury+Endowment**, with **OCap-based governance** for proposal, voting, execution, and multi-oracle dispute resolution.

## Box Composition

DAO-Escrow composes with the genesis [Box](box.md) primitive. Four governance roles —
member_vote, board_treasury, board_endowment, and dispute_arbitrator — are delegated
via Boxes. The DAO creates a Box per role per member. Exercising a role (proposing,
voting, treasury spending, endowment withdrawal, dispute resolution) calls
`Box::TakeV1` to consume the capability. The Box contract handles nullifier replay
internally — a Box can only be consumed once. This replaces the hand-rolled
`CapabilityProof` system with a standardized genesis primitive.

See [Box](box.md) for the genesis primitive.

## Three Operating Modes

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              DAO-Escrow Modes                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  MODE_ESCROW (0x00) ────────────────────────────────────────────────────── │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Escrow-Only (Pure Insurance Pool)                                   │    │
│  │                                                                       │    │
│  │  Members ──pay premiums──► Endowment Pool                             │    │
│  │                                    │                                 │    │
│  │                                    │ DAO votes                      │    │
│  │                                    ▼                                 │    │
│  │                            Claims paid out                           │    │
│  │                                                                       │    │
│  │  No treasury. No operational costs. Pure mutual insurance.          │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  MODE_TREASURY (0x01) ───────────────────────────────────────────────────── │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Treasury-Only (Same as DarkWow DAO)                                  │    │
│  │                                                                       │    │
│  │  Members ──pay fees──► Treasury Pool                                 │    │
│  │                                  │                                   │    │
│  │                    ┌─────────────┼─────────────┐                    │    │
│  │                    │   Propose   │   Vote     │   Exec              │    │
│  │                    └─────────────┴─────────────┘                    │    │
│  │                                  │                                   │    │
│  │                                  ▼                                   │    │
│  │                          Treasury spent                              │    │
│  │                                                                       │    │
│  │  Grants, development, operational costs. No insurance.               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  MODE_TREASURY_ENDOWMENT (0x02) ────────────────────────────────────────── │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │  Treasury + Endowment (Full-Featured)                                │    │
│  │                                                                       │    │
│  │  Members ──pay premiums──► ┌──────────┬──────────┐                 │    │
│  │                            │          │          │                     │    │
│  │              treasury_share %          % endowment_share                 │    │
│  │                            │          │          │                     │    │
│  │                            ▼          ▼          │                     │    │
│  │                      Treasury      Endowment      │                     │    │
│  │                         │             │          │                     │    │
│  │                         │             │ DAO vote │                     │    │
│  │                         ▼             ▼          │                     │    │
│  │                    Operational    Claims/          │                     │    │
│  │                    (grants etc)  Refunds          │                     │    │
│  │                                                                       │    │
│  │  Best of both: DAO-funded operations + insurance backing.           │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Mode Comparison

| Feature | MODE_ESCROW | MODE_TREASURY | MODE_TREASURY_ENDOWMENT |
|---------|-------------|---------------|------------------------|
| Membership notes | ✅ | ❌ | ✅ |
| Endowment pool | ✅ | ❌ | ✅ |
| Treasury pool | ❌ | ✅ | ✅ |
| DAO governance | ✅ | ✅ | ✅ |
| OCap governance | ✅ | ✅ | ✅ |
| Insurance payouts | ✅ | ❌ | ✅ |
| Operational funding | ❌ | ✅ | ✅ |
| Fee split | N/A | N/A | ✅ (configurable) |

## Entrypoints

### Core Functions (0x00-0x06)

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Create new DAO-Escrow (mode selected) |
| `UpdateV1` | `0x01` | Update parameters |
| `PayPremiumV1` | `0x02` | Pay premium, get membership note |
| `WithdrawV1` | `0x03` | Withdraw with capability-gated authorization |
| `EndowmentWithdrawV1` | `0x04` | Endowment withdrawal (capability or proposal) |
| `TreasurySpendV1` | `0x05` | Treasury spending (capability or proposal) |
| `EnableDrainProtectionV1` | `0x06` | Enable DrainProtection on existing DAO-Escrow |

### OCap Governance Functions (0x07-0x0e)

| Function | Opcode | Capability Required | Description |
|----------|--------|--------------------|-------------|
| `ProposeClaimV1` | `0x07` | `member_vote` | Propose claim (endowment/treasury/dispute) |
| `VoteClaimV1` | `0x08` | `member_vote` | Vote on pending proposal |
| `ExecuteClaimV1` | `0x09` | None (quorum is authority) | Execute approved proposal |
| `RegisterCapabilityRequirementV1` | `0x0a` | `board_treasury` | Map role to Identity contract capability |
| `VerifyMemberCapabilityV1` | `0x0b` | None (this IS verification) | Verify holder possesses a capability |
| `ResolveDisputeV1` | `0x0c` | `dispute_arbitrator` | Multi-oracle dispute resolution |
| `CancelClaimV1` | `0x0d` | Proposer identity match | Cancel pending claim |
| `SetGovernanceConfigV1` | `0x0e` | `board_treasury` | Update governance configuration |

## OCap Governance Model

DAO-Escrow uses **capability-based governance** via cross-contract verification with the [Identity contract](../../../src/contract/identity/README.md). Authority is proven ("I hold capability X") rather than asserted ("I am user Y").

### Capability Types

| Capability | Issued By | Purpose |
|-----------|-----------|---------|
| `member_vote` | Identity contract | Basic voting on proposals |
| `board_treasury` | Identity contract | Treasury release control |
| `board_endowment` | Identity contract | Endowment release control |
| `dispute_arbitrator` | Identity contract | Dispute resolution via oracle attestation |

### Governance Lifecycle

```
1. Identity contract issues capabilities to holders
2. dao_escrow registers capability requirements (0x0a)
3. Member proposes claim with member_vote capability proof (0x07)
4. Members vote with member_vote capability proof (0x08)
5. When quorum + approval ratio met → execute claim (0x09)
6. Disputes resolved by arbitrator with multi-oracle attestation (0x0c)
```

### Backward Compatibility

A `governance_active: bool` flag acts as a feature toggle:
- **`false`** (default): Capability checks bypassed, existing owner-pubkey behavior preserved
- **`true`**: Capability proofs mandatory for all protected operations

### Access Control Comparison

| Action | Governance Inactive | Governance Active |
|--------|--------------------|--------------------|
| Withdraw | Owner pubkey | `board_treasury` capability |
| Endowment withdraw | Open (any caller) | `board_endowment` capability or approved proposal |
| Treasury spend | Open (any caller) | `board_treasury` capability or approved proposal |
| Propose claim | Open | `member_vote` capability + ZK proof |
| Vote | Open | `member_vote` capability + ZK proof |
| Dispute resolution | N/A | `dispute_arbitrator` capability + multi-oracle attestation |

## Dispute Resolution Flow

Disputes are resolved via multi-oracle attestation with an arbitrator. Labor Market disputes escalate here via a child call to `ProposeClaimV1 (0x07)`:

```
1. Off-chain event → Oracle(s) push values (Oracle::PushValueV1)
2. Oracle(s) create attestations (Attestation::CreateAttestationV1)
3. Labor Market escalates dispute:
   labor_market::DisputeV1 (0x05) → dao_escrow::ProposeClaimV1 (0x07) [child call]
4. DAO members vote: ProposeClaimV1 (0x07) + VoteClaimV1 (0x08)
5. Arbitrator calls ResolveDisputeV1 (0x0c) with:
   - Multiple oracle attestation references
   - dispute_arbitrator capability proof (ZK)
   - Payout amount + recipient
6. Contract verifies:
   a. Multi-oracle threshold met (e.g., 3/5 oracles attested)
   b. Each oracle attestation validated via Attestation::VerifyClaimV1 (0x04) [child call]
   c. Consumes attestations to prevent replay
   d. Transfers funds via promissory_note::TransferV1 (0x04) [child call]
7. Anti-replay: db_contains_key(disputes_db, dispute_id) check prevents double-resolution
```

### ResolveDisputeV1 Anti-Replay Protection

`resolve_dispute_apply_v1` checks `db_contains_key(disputes_db, dispute_id)` before storing a resolution record. The `dispute_id` is derived as `poseidon_hash(proposal_id, attestation_count, payout_recipient)` — unique per resolution attempt. This prevents the same dispute from being resolved twice, even if multiple arbitrators attempt to process it.

### Cross-Contract Child Calls

DAO-Escrow uses cross-contract child calls for capability verification, payment, and attestation validation. For the complete mechanism, see [Composability](composability.md).

**VerifyMemberCapabilityV1 (0x0b → Identity 0x0b):**

`VerifyMemberCapabilityV1` validates a child call to `Identity::VerifyCapabilityV1 (0x0b)`. This is a double-check pattern: the ZK proof in params proves the capability, and the child call provides on-chain verification that the Identity contract recognizes the capability as non-revoked. The child call must be the first child in the DarkTree; if absent or using the wrong function code, the call fails with `InvalidChildCall`.

**PayPremiumV1 (0x02 → promissory_note 0x04):**

`PayPremiumV1` validates a child call to `promissory_note::TransferV1 (0x04)` for the premium payment.

**ResolveDisputeV1 (0x0c → Attestation + promissory_note):**

`ResolveDisputeV1` expects multiple child calls: one or more `Attestation::VerifyClaimV1 (0x04)` calls for oracle attestations, plus a `promissory_note::TransferV1 (0x04)` for the payout. It validates `!children_indexes.is_empty()` rather than checking for a specific count, since the number of oracle attestations varies per dispute.

## Case Study: Community Insurance Fund

A walkthrough of setting up and operating a community insurance fund using dao_escrow in `MODE_TREASURY_ENDOWMENT` — the full-featured mode that demonstrates the complete governance lifecycle.

### Setup Phase

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    SETUP: Identity + DAO-Escrow Initialization                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. IDENTITY CONTRACT DEPLOYMENT                                             │
│     │                                                                        │
│     │  RegisterCapabilityV1("member_vote")                                  │
│     │  RegisterCapabilityV1("board_treasury")                               │
│     │  RegisterCapabilityV1("board_endowment")                              │
│     │  RegisterCapabilityV1("dispute_arbitrator")                           │
│     │                                                                        │
│     │  IssueCapabilityV1(bob, "board_treasury")                             │
│     │    bob proves: credential.trustee, stake >= threshold                 │
│     │  IssueCapabilityV1(alice, "member_vote")                              │
│     │    alice proves: credential.member, premium paid                      │
│     │  ... repeat for all members ...                                       │
│     │                                                                        │
│     ▼                                                                        │
│  2. DAO-ESCROW INITIALIZATION                                                │
│     │                                                                        │
│     │  InitializeV1({                                                       │
│     │    mode: MODE_TREASURY_ENDOWMENT,                                     │
│     │    treasury_share: 70,                                                │
│     │    endowment_share: 30,                                               │
│     │    governance_config: Some(GovernanceConfig {                        │
│     │      quorum_pct: 50,                                                  │
│     │      approval_ratio_pct: 60,                                         │
│     │      voting_window_blocks: 10080,   // ~7 days                       │
│     │      execution_window_blocks: 1440,  // ~1 day                       │
│     │      max_claim_ratio_pct: 80,                                        │
│     │      oracle_threshold: (3, 5),       // 3 of 5 oracles               │
│     │      governance_active: true,                                         │
│     │    }),                                                                │
│     │  })                                                                   │
│     │                                                                        │
│     │  SetGovernanceConfigV1({                                              │
│     │    capability_proof: ZK(VerifyCapability("board_treasury")),         │
│     │    // registers capability-to-role mappings                           │
│     │  })                                                                   │
│     │                                                                        │
│     ▼                                                                        │
│  RESULT: DAO-Escrow active with OCap governance. Members hold capabilities, │
│          treasury and endowment pools are empty, waiting for premiums.       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Premium Payment Phase

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 1: Members Pay Premiums                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  MEMBER (alice)                                                              │
│     │                                                                        │
│     │  PayPremiumV1({                                                        │
│     │    dao_escrow_bulla,                                                  │
│     │    value: 1000,                                                        │
│     │    token_id: DRK,                                                     │
│     │    membership_blind,                                                   │
│     │  })                                                                    │
│     │                                                                        │
│     │  ZK proof verifies: member knows secret key, commitment valid        │
│     │                                                                        │
│     ▼                                                                        │
│  ┌──────────────────────────────────────────────────────────────┐           │
│  │                        Premium Payment (1000)                  │           │
│  └──────────────────────────────────────────────────────────────┘           │
│                              │                                               │
│              ┌───────────────┴───────────────┐                              │
│              │                               │                               │
│    treasury_share (70% = 700)     endowment_share (30% = 300)               │
│              │                               │                               │
│              ▼                               ▼                               │
│    ┌─────────────────┐             ┌─────────────────┐                      │
│    │    Treasury     │             │   Endowment     │                      │
│    │  (operational)  │             │  (insurance)    │                      │
│    └─────────────────┘             └─────────────────┘                      │
│                                                                              │
│  RESULT: Alice receives membership note (time-locked, stored in              │
│          membership Merkle tree). Treasury: 700, Endowment: 300.             │
│          Membership note proves Alice is a member WITHOUT revealing          │
│          her identity, contribution amount, or membership tier.              │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Governance: Propose → Vote → Execute

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 2: Propose, Vote, Execute a Claim                    │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  MEMBER (alice)                                                              │
│     │                                                                        │
│     │  ProposeClaimV1({                                                      │
│     │    dao_escrow_bulla,                                                  │
│     │    claim_type: ClaimType::Endowment,                                  │
│     │    recipient: flood_victim_pubkey,                                    │
│     │    value: 200,                                                         │
│     │    capability_proof: ZK(VerifyCapability("member_vote")),             │
│     │  })                                                                    │
│     │                                                                        │
│     │  ZK proof verifies:                                                   │
│     │    ✓ Alice holds valid member_vote capability                         │
│     │    ✓ Capability was issued by trusted Identity contract               │
│     │    ✓ Capability has not expired                                       │
│     │    ✓ Proposal nullifier = H(capability_secret, proposal_id)           │
│     │      → prevents double-propose                                        │
│     │    ✗ Alice's identity NEVER revealed                                  │
│     │                                                                        │
│     ▼                                                                        │
│  ┌──────────────────────────────────────────────────────────────┐           │
│  │  Proposal #7: "Flood relief payout 200 DRK"                   │           │
│  │  State: Pending    │  Proposer: <hidden>                      │           │
│  │  Voting window: blocks 50000-60080                           │           │
│  └──────────────────────────────────────────────────────────────┘           │
│                              │                                               │
│                              ▼                                               │
│  MEMBERS (bob, carol, dave, ...)                                             │
│     │                                                                        │
│     │  VoteClaimV1({                                                         │
│     │    proposal_id: 7,                                                     │
│     │    vote_type: VoteType::Approve,                                      │
│     │    capability_proof: ZK(VerifyCapability("member_vote")),             │
│     │  })                                                                    │
│     │                                                                        │
│     │  ZK proof verifies:                                                   │
│     │    ✓ Voter holds member_vote capability                               │
│     │    ✓ Vote nullifier = H(capability_secret, proposal_id)               │
│     │      → prevents double-vote on same proposal                          │
│     │    ✗ Vote direction hidden from other voters (only final tally)       │
│     │                                                                        │
│     ▼                                                                        │
│  ┌──────────────────────────────────────────────────────────────┐           │
│  │  Votes: 42 Approve / 12 Reject / 6 Abstain                    │           │
│  │  Quorum: 60% ✓ (50% required)                                 │           │
│  │  Approval: 78% ✓ (60% required)                               │           │
│  │  → Proposal APPROVED                                           │           │
│  └──────────────────────────────────────────────────────────────┘           │
│                              │                                               │
│                              ▼                                               │
│  ANY MEMBER                                                                  │
│     │                                                                        │
│     │  ExecuteClaimV1({                                                      │
│     │    proposal_id: 7,                                                     │
│     │  })                                                                    │
│     │                                                                        │
│     │  Contract verifies:                                                    │
│     │    ✓ Proposal state is Approved                                       │
│     │    ✓ Execution window has not expired                                  │
│     │    ✓ Claim value (200) ≤ max_claim_ratio (80% of 300 = 240)          │
│     │                                                                        │
│     │  → promissory_note::transfer_v1(endowment → flood_victim, 200)              │
│     │                                                                        │
│     ▼                                                                        │
│  RESULT: 200 DRK transferred from endowment to flood victim.                 │
│          Proposal #7 marked Executed. Endowment balance: 100.                │
│          No identity revealed at any step.                                    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Dispute Resolution

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    PHASE 3: Multi-Oracle Dispute Resolution                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  OFF-CHAIN EVENT: Flood victim disputes claim denial.                        │
│                                                                              │
│  ORACLES (5 independent weather/data providers)                              │
│     │                                                                        │
│     │  Oracle::PushValueV1(flood_depth_cm: 145)       // oracle_1           │
│     │  Oracle::PushValueV1(flood_depth_cm: 152)       // oracle_2           │
│     │  Oracle::PushValueV1(flood_depth_cm: 140)       // oracle_3           │
│     │  Oracle::PushValueV1(flood_depth_cm: 0)         // oracle_4 (offline) │
│     │  Oracle::PushValueV1(flood_depth_cm: 0)         // oracle_5 (offline) │
│     │                                                                        │
│     │  Attestation::CreateAttestationV1(flood_data)                          │
│     │                                                                        │
│     ▼                                                                        │
│  ARBITRATOR (holds dispute_arbitrator capability)                            │
│     │                                                                        │
│     │  ResolveDisputeV1({                                                    │
│     │    dao_escrow_bulla,                                                   │
│     │    dispute_id,                                                         │
│     │    attestations: [oracle_1_ref, oracle_2_ref, oracle_3_ref],          │
│     │    capability_proof: ZK(VerifyCapability("dispute_arbitrator")),     │
│     │    payout: 200,                                                        │
│     │    recipient: flood_victim_pubkey,                                    │
│     │  })                                                                    │
│     │                                                                        │
│     │  Contract verifies (in order):                                        │
│     │    a. Identity::VerifyCapabilityV1("dispute_arbitrator") → VALID     │
│     │    b. Attestation::VerifyClaimV1(oracle_1_ref) → VALID               │
│     │    c. Attestation::VerifyClaimV1(oracle_2_ref) → VALID               │
│     │    d. Attestation::VerifyClaimV1(oracle_3_ref) → VALID               │
│     │    e. 3 valid attestations ≥ 3/5 threshold ✓                         │
│     │    f. Consumes all 3 attestations (prevents replay)                    │
│     │    g. promissory_note::transfer_v1(endowment → victim, 200)                 │
│     │                                                                        │
│     ▼                                                                        │
│  RESULT: Dispute resolved. 3 of 5 oracles confirmed flood.                   │
│          Attestations consumed (cannot be reused).                           │
│          Funds transferred atomically in same transaction.                   │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Key Insights from the Case Study

1. **Identity never revealed**: At every step — premium payment, proposal, voting, execution, dispute — members prove capabilities, not identity. The verifier learns "a valid member proposed this" not "Alice proposed this."

2. **Authority is bounded**: Alice's `member_vote` capability lets her propose and vote — nothing more. She cannot withdraw treasury funds directly. Bob's `board_treasury` capability is separate, issued based on different credentials.

3. **Nullifiers prevent every replay**: Proposal nullifier (`H(capability_secret, proposal_id)`), vote nullifier (`H(capability_secret, proposal_id)`), dispute nullifier (`H(capability_secret, dispute_id)`) — each action is exactly-once.

4. **Composability is simple**: dao_escrow doesn't implement its own capability verification. It calls `Identity::VerifyCapabilityV1` as a child call. The Identity contract is the single source of truth for all authorization.

5. **Multi-oracle trust model**: No single oracle can force a payout. The 3/5 threshold means the arbitrator needs attestations from a majority of independent oracles, each of whom pushed their own on-chain data.

## Standard Governance Setup

The dao_escrow contract ships with a **standard governance configuration** that works out-of-the-box for most use cases. Every parameter is adjustable per-deployment and gated behind specific OCap capabilities.

### Default Parameters

| Parameter | Default | Controlled By | Description |
|-----------|---------|---------------|-------------|
| `governance_active` | `false` | `board_treasury` | Feature toggle — set `true` to enable OCap governance |
| `quorum_pct` | 50% | `board_treasury` | % of members who must vote for proposal to be valid |
| `approval_ratio_pct` | 60% | `board_treasury` | % of votes that must be "approve" for proposal to pass |
| `voting_window_blocks` | 10080 (~7 days) | `board_treasury` | Blocks from proposal creation to vote deadline |
| `execution_window_blocks` | 1440 (~1 day) | `board_treasury` | Blocks after approval to execute before expiry |
| `treasury_share` | 70% | `board_treasury` | % of premium directed to treasury pool |
| `endowment_share` | 30% | `board_endowment` | % of premium directed to endowment pool |
| `max_claim_ratio_pct` | 80% | `board_endowment` | Max single claim as % of endowment balance |
| `oracle_threshold` | 3 of 5 | `board_treasury` | Min oracle attestations for dispute resolution |

### Modifying Governance Parameters

Parameters are modified via `SetGovernanceConfigV1`, which requires a `board_treasury` capability proof:

```rust
let (params, _) = SetGovernanceConfigV1Builder::new(dao_escrow_bulla)
    .quorum_pct(66)                    // Require 66% quorum instead of 50%
    .approval_ratio_pct(75)            // Require 75% approval instead of 60%
    .voting_window_blocks(20160)       // Extend to ~14 days
    .capability_proof(board_proof)     // ZK proof of board_treasury capability
    .build()?;
```

### Capability Control Matrix

| Action | Required Capability | Who Typically Holds It |
|--------|--------------------|-----------------------|
| Propose claim | `member_vote` | All premium-paying members |
| Vote on proposal | `member_vote` | All premium-paying members |
| Execute approved proposal | None (quorum result is authority) | Any caller |
| Modify governance config | `board_treasury` | Elected trustees / core team |
| Withdraw from treasury | `board_treasury` | Elected trustees |
| Withdraw from endowment | `board_endowment` | Separate threshold (separation of powers) |
| Resolve dispute | `dispute_arbitrator` | Independent arbitrators |
| Change fee split | `board_endowment` | Endowment trustees |
| Register capability requirement | `board_treasury` | Treasury trustees |
| Cancel own proposal | Proposer identity match | Original proposer |

### Migration Path: Owner-Key → OCap Governance

The `governance_active` flag enables gradual migration without breaking existing deployments:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Migration Path                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PHASE 1: Deploy (governance_active = false)                     │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ Owner pubkey controls all operations.                       │ │
│  │ Existing behavior preserved. Zero changes.                  │ │
│  │ Members pay premiums, receive membership notes.             │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                           │                                        │
│                           ▼                                        │
│  PHASE 2: Bootstrap capabilities (governance_active = false)      │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ Identity contract deploys.                                  │ │
│  │ Capabilities issued to members (member_vote).               │ │
│  │ Capabilities issued to trustees (board_treasury,            │ │
│  │   board_endowment).                                          │ │
│  │ Arbitrators receive dispute_arbitrator capability.          │ │
│  │ Capability requirements registered in dao_escrow.           │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                           │                                        │
│                           ▼                                        │
│  PHASE 3: Activate (governance_active = true)                     │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ SetGovernanceConfigV1 called by board_treasury holder.     │ │
│  │ All capability checks become mandatory.                     │ │
│  │ Owner pubkey bypass disabled.                               │ │
│  │ DAO is now fully OCap-governed.                             │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
│  KEY PROPERTY: Each phase is reversible. If governance_active    │
│  is set back to false, the contract falls back to owner-key      │
│  behavior. No funds are locked. No state is lost.               │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Customizing OCap Requirements

Each dao_escrow instance can require different capabilities than the defaults via `RegisterCapabilityRequirementV1`:

```rust
// Example: Require a custom "senior_member" capability for voting
// instead of the default "member_vote"
let (params, _) = RegisterCapabilityRequirementV1Builder::new()
    .role("vote")                          // The action being gated
    .capability_id(senior_member_cap_id)   // Custom capability from Identity
    .identity_contract_bulla(identity_bulla)
    .capability_proof(board_treasury_proof)
    .build()?;
```

This composability means a single Identity contract can serve multiple dao_escrow instances, each with different capability requirements — some requiring basic membership, others requiring elevated stake or domain-specific credentials.

## Fee Split (TreasuryEndowment Mode)

Fee split follows the same flow shown in [Premium Payment Phase](#premium-payment-phase) above. The `treasury_share` and `endowment_share` parameters control the split ratio, configured per DAO instance.

## ZK Circuits

| Circuit | Public Inputs | Status |
|---------|--------------|--------|
| `init_v1.zk` | `dao_bulla`, `endowment_bulla` | Compiled |
| `pay_premium_v1.zk` | `dao_escrow_bulla`, `membership_note`, `value_commit.x`, `value_commit.y` | Compiled |
| `propose_claim_v1.zk` | `dao_escrow_bulla`, `claim_id`, `capability_id`, `proposal_nullifier`, `claim_commit` | Source complete |
| `vote_claim_v1.zk` | `proposal_id`, `capability_id`, `vote_nullifier`, `vote_commit.x`, `vote_commit.y` | Source complete |
| `verify_member_capability_v1.zk` | `capability_id`, `dao_escrow_bulla`, `holder_commit` | Source complete |
| `resolve_dispute_v1.zk` | `capability_id`, `dao_escrow_bulla`, `dispute_id`, `attestation_root`, `resolution_commit`, `dispute_nullifier` | Source complete |

## Database Trees

| Tree | Purpose |
|------|---------|
| `info` | Contract version and configuration |
| `bullas` | Endowment instances |
| `membership` | Time-limited membership notes |
| `endowment` | Endowment pool funds |
| `proposals` | Governance proposals/claims |
| `votes` | Vote records per proposal |
| `capability_requirements` | Required capability IDs per role |
| `disputes` | Dispute resolution records |
| `nullifiers` | Prevents double-vote, double-propose |
| `governance` | Governance configuration |

## Trust Model

| Aspect | How It's Protected |
|--------|-------------------|
| Treasury funds | OCap governance (propose/vote/exec) or DrainProtection |
| Endowment | Capability-gated withdrawal or approved proposal required |
| Membership notes | Block-based expiry enforced in circuit |
| Double-spend | Nullifiers prevent redemption twice |
| Double-vote | Vote nullifier = H(capability_secret, proposal_id) |
| Dispute replay | Dispute nullifier = H(capability_secret, dispute_id) |
| Mass exit / drain | Optional DrainProtection with rate limiting and exit queue |

## DrainProtection Integration

DAO-Escrow can integrate with the [DrainProtection contract](drain_protection.md) for governance-level fund protections.

```rust
// During initialization
let dao_escrow = InitializeBuilder::new()
    .mode(DaoEscrowMode::TreasuryEndowment)
    .enable_drain_protection(true)
    .build()?;

// Or enable later via governance
let enable_dp = EnableDrainProtectionBuilder::new()
    .dao_escrow_bulla(dao_escrow.bulla)
    .drain_protection_bulla(dp_instance.bulla)
    .build()?;
```

## Build & Test Status

```
Build:    ✅ Clean (zero warnings)
Tests:    ✅ 20/20 integration tests passing
Circuits: 2 compiled, 4 source complete (needs zkas compilation)
```

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [DAO-Escrow Contract README](../../../src/contract/dao_escrow/README.md)
- [Identity Contract README](../../../src/contract/identity/README.md)
- [O-Cap Architecture](../arch/ocap.md)
- [Identity Architecture](../arch/identity.md)
- [Composability](composability.md) — cross-contract child call mechanism
- [Recruitment Pipeline Case Study](recruitment_pipeline.md) — end-to-end DAO hiring walkthrough
- [Subscription Contract](subscription.md)
- [DrainProtection Contract](drain_protection.md)
