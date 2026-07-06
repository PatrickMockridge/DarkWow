# Sealed Bid Tender Contract Architecture

A privacy-preserving tendering system that combines sealed-bid auctions with attestation-based competency verification and labor market integration.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Project Tender System                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ATTESTATION/            SEEDED BID           LABOR            TAU         │
│   COMPETENCY             TENDER               MARKET                          │
│   Contract                Contract             Contract          Task Manager     │
│       │                    │                   │                │            │
│       │                    │                   │                │            │
│       ▼                    ▼                   ▼                ▼            │
│  ┌─────────┐        ┌─────────┐        ┌─────────┐        ┌─────────┐    │
│  │ Attest  │        │ Create  │        │ Create  │        │ Create  │    │
│  │ skills  │───────>│ tender  │        │ job     │        │ task    │    │
│  │ & track │        │         │        │         │        │         │    │
│  └─────────┘        │         │        │         │        │         │    │
│                      │ Submit  │        │ Accept  │        │ Start   │    │
│                      │ sealed  │───────>│ bid     │───────>│ work    │    │
│                      │ bids    │        │         │        │         │    │
│                      │         │        │ Deliver │        │ Complete│    │
│                      │ Reveal │        │ work    │───────>│ deliver │    │
│                      │ bids    │        │         │        │ ables  │    │
│                      │         │        │ Confirm │        │         │    │
│                      │ Select │        │ payment │        │         │    │
│                      │ winner │───────>│         │        │         │    │
│                      └─────────┘        └─────────┘        └─────────┘    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Innovation: Sealed Bids + Attestation Verification

Unlike a standard auction where only price matters, sealed bid tendering allows:
- **Price-only bids**: Pure auction mode
- **Attested competency + Price bids**: Select based on verified skills + cost
- **DAO governance**: Community can select winner via vote

## Attestation Integration

The tender uses the [Attestation Contract](./attestation.md) for competency verification:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Tender + Attestation Flow                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Requester                                                                  │
│     │                                                                       │
│     │ CreateAttestation(requirement_commitment)                              │
│     ▼                                                                       │
│  Attestation(Active)                                                         │
│     │                                                                       │
│     │ attestation_id                                                        │
│     │                                                                       │
│     │◄──────────────────────────────── CreateTender(tender, attestation_id) │
│     │                                                                       │
│     │                              Worker                                   │
│     │                                 │                                     │
│     │                                 │ CreateClaim(evidence_commitment)    │
│     │                                 ▼                                     │
│     │                              Claim(Verified)                          │
│     │                                 │                                     │
│     │                                 │ claim_id                            │
│     │                                 │                                     │
│     │◄── SubmitBid(tender_id, claim_id, encrypted_amount)                  │
│     │                                                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Tender State Machine

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Tender State Machine                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Created ──[SubmitBid]──> Bidding ──[Close]──> Revealed ──[Select]──> Awarded │
│                                                │                            │
│                                                │                            │
│                                        [Cancel Tender]                       │
│                                                │                            │
│                                                ▼                            │
│                                           Cancelled                          │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Bid State Machine

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            Bid State Machine                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Sealed ──[Reveal]──> Revealed ──[Accept]──> Accepted                       │
│    │                           │                                            │
│    │                           └──[Reject]──> Rejected                       │
│    │                                                                           │
│    └──[Timeout]──> Expired                                                    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Integration with Existing Contracts

### With Attestation

Workers prove competency via attestation claims:
```rust
// Requester creates attestation for competency requirements
let attestation_id = attestation::create_attestation(
    attestor: requester_pubkey,
    claim_type: Predicate::Matches,
    claim_data: vec![requirement_commitment],
)?;

// Worker creates claim proving they meet requirements
let claim_id = attestation::create_claim(
    attestation_id,
    claimant: worker_pubkey,
    predicate: Predicate::Matches,
    evidence_commitment: worker_competency_commitment,
)?;

// Worker submits bid with claim_id
let bid = SubmitBidParamsV1 {
    claim_id,
    // ...
};
```

### With Labor Market

Winner creates job via labor market:
```rust
// After tender awarded:
let job = CreateJobBuilder::new()
    .employer(requester_pubkey)
    .worker(winner_pubkey)
    .specification(tender.specification)
    .attestation_id(tender.attestation_id)
    .payment(winning_bid_amount)
    .deadline(tender.delivery_deadline)
    .build()?;
```

### With Tau

Task tracking throughout:
```rust
// Tau integration
tau.create_task(Task {
    title: tender.title,
    description: tender.specification,
    assignee: winner_pubkey,
    project: tender.project_id,
    due: tender.delivery_deadline,
});

// Progress tracked via tau commands
tau.start(task_id);
tau.comment(task_id, "milestone 1 complete");
tau.stop(task_id);
```

## Data Structures

### Tender

```rust
pub struct Tender {
    pub version: u8,
    pub id: TenderId,
    pub requester_pub_x: pallas::Base,
    pub requester_pub_y: pallas::Base,
    pub title: String,
    pub specification: pallas::Base,     // Hash of spec document
    pub attestation_id: pallas::Base,   // Attestation for competency requirements
    pub min_bid: u64,
    pub max_bid: u64,
    pub bid_deadline: u64,
    pub reveal_deadline: u64,
    pub delivery_deadline: u64,
    pub state: TenderState,
    pub selected_bid_id: Option<BidId>,
    pub bid_count: u64,
    pub created_at: u64,
    pub required_capability: Option<[u8; 32]>,
    pub required_dag_id: Option<[u8; 32]>,
}

pub enum TenderState {
    Created = 0,
    Bidding = 1,
    Revealed = 2,
    Awarded = 3,
    Cancelled = 4,
}
```

### Bid

```rust
pub struct Bid {
    pub version: u8,
    pub id: BidId,
    pub tender_id: TenderId,
    pub bidder_pubkey: PublicKey,
    pub amount: u64,
    pub claim_id: pallas::Base,         // Attestation claim proving competency
    pub encrypted_payload: Vec<u8>,     // Encrypted bid details
    pub state: BidState,
    pub revealed_amount: Option<u64>,
    pub created_at: u64,
}

pub enum BidState {
    Sealed = 0,
    Revealed = 1,
    Accepted = 2,
    Rejected = 3,
    Expired = 4,
}
```

## ZK Circuits

### submit_bid_v1.zk

**Purpose**: Submit sealed bid with attestation claim

**Public Inputs**:
- `tender_id`
- `bid_id`
- `bidder_pub_x`
- `bidder_pub_y`

**Witnesses**:
- `bidder_secret`
- `amount`
- `bid_nonce`

**Circuit**:
```zk
bidder_pub = ec_mul_base(bidder_secret, NULLIFIER_K);
# Verify bidder public key matches
# Compute bid ID
bid_id = poseidon_hash(tender_id, bidder_pub, amount, nonce);
# claim_id is verified by attestation contract
```

### reveal_bid_v1.zk

**Purpose**: Reveal sealed bid amount

**Public Inputs**:
- `tender_id`
- `bid_id`
- `revealed_amount`

**Circuit**:
```zk
# Verify bid was submitted
# Verify reveal deadline not passed
less_than_strict(current_block, reveal_deadline);
```

### select_winner_v1.zk

**Purpose**: Requester selects winning bid

**Public Inputs**:
- `tender_id`
- `winner_bid_id`

**Circuit**:
```zk
# Verify requester authorized
# Verify winner bid was revealed
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateTenderV1` | 0x00 | Create new tender (references attestation) |
| `SubmitBidV1` | 0x01 | Submit sealed bid with claim_id |
| `RevealBidV1` | 0x02 | Reveal bid amount |
| `CloseTenderV1` | 0x03 | Close bidding period |
| `SelectWinnerV1` | 0x04 | Select winning bid |
| `CancelTenderV1` | 0x05 | Cancel tender |
| `RejectBidV1` | 0x06 | Reject a bid |
| `CreateTenderWithCapabilityV1` | 0x07 | Create tender requiring O-Cap proof |
| `SubmitBidWithCapabilityV1` | 0x08 | Submit bid with O-Cap proof |

## Privacy Properties

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| Tender exists | All bid amounts until reveal |
| Your bid after reveal | Other bidders' amounts |
| Winner selected | losers' identities |
| Competency attested | Specific credential data |

## Breaking Changes (v2)

- **Removed**: `Tender.requirement_commitment` - replaced with `Tender.attestation_id`
- **Removed**: `Bid.competency_commitment` - replaced with `Bid.claim_id`
- Workers now create attestation claims proving competency, reference claim_id in bid

## Comparison

| Feature | Traditional Tender | DarkWow Tender |
|---------|-------------------|---------------|
| Bid privacy | Sealed envelope | Cryptographically sealed |
| Competency check | KYC documents | Attestation claims |
| Winner selection | Single criteria | Multi-criteria (skill + price) |
| Dispute resolution | Central authority | DAO governance |
| Task tracking | Separate system | Tau integration |
| Payment release | Escrow service | Labor market integration |

## Use Cases

### Open Tender (Community Funded)
```rust
// DAO creates tender for grant work
let attestation_id = attestation::create_attestation(
    attestor: dao_pubkey,
    claim_type: Predicate::Matches,
    claim_data: vec![dao_member_commitment],
)?;

let tender = CreateTenderBuilder::new()
    .title("ZK Documentation")
    .specification(doc_hash)
    .attestation_id(attestation_id)
    .min_bid(1000)
    .max_bid(10000)
    .bid_deadline(current_block + 1000)
    .reveal_deadline(current_block + 1100)
    .build()?;
```

### Private Tender (Select Bidders)
```rust
// Only pre-approved workers can bid
let attestation_id = attestation::create_attestation(
    attestor: requester_pubkey,
    claim_type: Predicate::Custom,
    claim_data: vec![competency_requirement],
)?;

let tender = CreateTenderBuilder::new()
    .title("Smart Contract Audit")
    .specification(spec_hash)
    .attestation_id(attestation_id)
    .build()?;
```

### Competency-Weighted Auction
```rust
// Winner selected by: highest_composite_score
// composite_score = bid_amount * competency_multiplier
// competency_multiplier = verified_skills / required_skills
```

## Integration Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                     Complete Project Flow                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  REQUESTER                          SYSTEM                                     │
│     │                                                                         │
│     │  1. Create Attestation (requirements)                                  │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│     │  2. Create Tender (spec + attestation_id)                              │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│  WORKERS                                                                         │
│     │  3. Create Claim (attestation)                                         │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│     │  4. Submit sealed bid with claim_id (Tender contract)                  │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│     │  5. Reveal bids (after deadline)                                        │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│  REQUESTER                          │                                         │
│     │  6. Select winner              │                                         │
│     │────────────────────────────────>│                                        │
│     │                                 │                                         │
│     │                    7. Create job (Labor Market)                         │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │                    8. Accept and start work                              │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │                    9. Create task (Tau)                                │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │                    10. Track progress (Tau commands)                    │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │  11. Submit deliverable (Labor Market)                                 │
│     │────────────────────────────────>│                                        │
│     │                                 │                                         │
│     │  12. Confirm + Release payment                                          │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │                    13. Complete task (Tau)                             │
│     │<───────────────────────────────────────────────────────────────────────│
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Files

```
src/contract/tender/
├── proof/
│   ├── create_tender_v1.zk
│   ├── submit_bid_v1.zk
│   ├── reveal_bid_v1.zk
│   └── select_winner_v1.zk
├── src/
│   ├── lib.rs
│   ├── entrypoint.rs
│   ├── model/mod.rs
│   ├── error.rs
│   └── client/mod.rs
└── README.md
```

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [Attestation Contract](./attestation.md) - Generalized attestation and claims
- [Labor Market Contract](./labor_market.md) - Job execution after tender award
- [Tau Task Manager](../misc/tau.md)
