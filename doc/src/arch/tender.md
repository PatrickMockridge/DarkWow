# Sealed Bid Tender Contract Architecture

A privacy-preserving tendering system that combines sealed-bid auctions with identity verification and labor market integration.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        Project Tender System                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   IDENTITY/            SEEDED BID           LABOR            TAU            │
│   COMPETENCY           TENDER               MARKET                          │
│   Framework            Contract             Contract          Task Manager     │
│       │                    │                   │                │            │
│       │                    │                   │                │            │
│       ▼                    ▼                   ▼                ▼            │
│  ┌─────────┐        ┌─────────┐        ┌─────────┐        ┌─────────┐    │
│  │ Prove   │        │ Create  │        │ Create  │        │ Create  │    │
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

## Key Innovation: Sealed Bids + Competency Verification

Unlike a standard auction where only price matters, sealed bid tendering allows:
- **Price-only bids**: Pure auction mode
- **Competency + Price bids**: Select based on verified skills + cost
- **DAO governance**: Community selects winner via vote

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

### With Identity/Competency Framework

Workers prove competency via ZK credentials:
```rust
// In tender circuit, bidder proves:
claim_proof = verify_zk_credential(
    credential: CompetencyCredential,
    predicate: "skill_level >= required_level"
);
```

Competency DAG records:
- Skills verified by issuers
- Work history from completed tenders
- Reputation scores

### With Labor Market

Winner creates job via labor market:
```rust
// After tender awarded:
let job = CreateJobBuilder::new()
    .employer(requester_pubkey)
    .worker(winner_pubkey)
    .specification(tender.specification)
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
    pub id: TenderId,
    pub requester_pubkey: PublicKey,
    pub title: String,
    pub specification: pallas::Base,        // Hash of spec document
    pub requirement_commitment: pallas::Base, // Competency requirements
    pub min_bid: u64,
    pub max_bid: u64,
    pub bid_deadline: u64,
    pub reveal_deadline: u64,
    pub delivery_deadline: u64,
    pub state: TenderState,
    pub selected_bid_id: Option<BidId>,
    pub created_at: u64,
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
    pub id: BidId,
    pub tender_id: TenderId,
    pub bidder_pubkey: PublicKey,
    pub amount: u64,
    pub competency_proof: pallas::Base,     // ZK proof of competency
    pub encrypted_payload: Vec<u8>,         // Encrypted bid details
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

**Purpose**: Submit sealed bid with competency proof

**Public Inputs**:
- `tender_id`
- `bid_id`
- `competency_commitment` (proves bidder has required skills)

**Witnesses**:
- `bidder_secret`
- `amount`
- `competency_proof`

**Circuit**:
```zk
bidder_pub = ec_mul_base(bidder_secret, NULLIFIER_K);
# Verify competency proof
verify_credential(competency_proof, requirement_commitment);
# Compute bid ID
bid_id = poseidon_hash(tender_id, bidder_pub, amount, nonce);
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
# Verify winner meets competency requirements
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateTenderV1` | 0x00 | Create new tender |
| `SubmitBidV1` | 0x01 | Submit sealed bid |
| `RevealBidV1` | 0x02 | Reveal bid amount |
| `CloseTenderV1` | 0x03 | Close bidding period |
| `SelectWinnerV1` | 0x04 | Select winning bid |
| `CancelTenderV1` | 0x05 | Cancel tender |
| `RejectBidV1` | 0x06 | Reject a bid |

## Privacy Properties

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| Tender exists | All bid amounts until reveal |
| Your bid after reveal | Other bidders' amounts |
| Winner selected |losers' identities |
| Competency verified | Specific credential data |

## Comparison

| Feature | Traditional Tender | DarkFi Tender |
|---------|-------------------|----------------|
| Bid privacy | Sealed envelope | Cryptographically sealed |
| Competency check | KYC documents | ZK credentials |
| Winner selection | Single criteria | Multi-criteria (skill + price) |
| Dispute resolution | Central authority | DAO governance |
| Task tracking | Separate system | Tau integration |
| Payment release | Escrow service | Labor market integration |

## Use Cases

### Open Tender (Community Funded)
```rust
// DAO creates tender for grant work
let tender = CreateTenderBuilder::new()
    .title("ZK Documentation")
    .specification(doc_hash)
    .requirement(competency_dao_member())  // Must be DAO member
    .min_bid(1000)
    .max_bid(10000)
    .bid_deadline(current_block + 1000)
    .reveal_deadline(current_block + 1100)
    .build()?;
```

### Private Tender (Select Bidders)
```rust
// Only pre-approved workers can bid
let tender = CreateTenderBuilder::new()
    .title("Smart Contract Audit")
    .specification(spec_hash)
    .requirement(competency_proof("security_audit", 3))  // Level 3+
    .invited_bidders(vec![alice_pub, bob_pub, charlie_pub])
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
│     │  1. Create Tender (spec + requirements)                                │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│  WORKERS                                                                         │
│     │  2. Prove competency (Identity contract)                                │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│     │  3. Submit sealed bid (Tender contract)                                │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│     │  4. Reveal bids (after deadline)                                        │
│     │───────────────────────────────────────────────────────────────────────>│
│     │                                                                         │
│  REQUESTER                          │                                         │
│     │  5. Select winner              │                                         │
│     │────────────────────────────────>│                                        │
│     │                                 │                                         │
│     │                    6. Create job (Labor Market)                         │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │                    7. Accept and start work                              │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │                    8. Create task (Tau)                                │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │                    9. Track progress (Tau commands)                    │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │  10. Submit deliverable (Labor Market)                                 │
│     │────────────────────────────────>│                                        │
│     │                                 │                                         │
│     │  11. Confirm + Release payment                                          │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                 │                                         │
│     │                    12. Complete task (Tau)                             │
│     │<───────────────────────────────────────────────────────────────────────│
│     │                                                                         │
│     │  13. Update competency record                                           │
│     │────────────────────────────────>│ (Identity contract)                    │
│     │                                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Files to Create

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

## References

- [DarkFi Identity Contract](../identity/)
- [DarkFi Labor Market Contract](../labor_market/)
- [DarkFi Auction Contract](../auction/)
- [Tau Task Manager](../../misc/tau.md)
