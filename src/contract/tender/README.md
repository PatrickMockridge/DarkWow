# Tender Contract

A privacy-preserving sealed-bid tendering system that integrates with the identity/competency framework, labor market, and Tau task manager.

## Overview

The tender contract enables requesters to create tenders for projects with:
- **Sealed bids**: Bidders submit encrypted bids that remain hidden until the reveal deadline
- **Competency verification**: Bidders provide competency commitments proving their qualifications
- **Integration with labor market**: Winner automatically creates a job in the labor market
- **Task tracking via Tau**: Created jobs are tracked in Tau for delivery management

## Trust Model

1. **Requester creates tender** with specifications, requirements, and deadlines
2. **Workers submit sealed bids** with competency proofs and encrypted bid details
3. **Bids revealed** after the bidding deadline
4. **Winner selected** based on competency + price evaluation
5. **Job created** via Labor Market for execution
6. **Task tracked** via Tau for progress monitoring

## State Machines

### Tender State Machine

```
Created ──[SubmitBid]──> Bidding ──[Close]──> Revealed ──[Select]──> Awarded
                                               │
                                               └──[Cancel]──> Cancelled
```

### Bid State Machine

```
Sealed ──[Reveal]──> Revealed ──[Accept]──> Accepted
  │                        │
  └──[Timeout]──> Expired  └──[Reject]──> Rejected
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateTenderV1` | 0x00 | Requester creates a new tender |
| `SubmitBidV1` | 0x01 | Worker submits a sealed bid |
| `RevealBidV1` | 0x02 | Worker reveals their bid amount |
| `CloseTenderV1` | 0x03 | Requester closes bidding, starts reveal period |
| `SelectWinnerV1` | 0x04 | Requester selects winning bid |
| `CancelTenderV1` | 0x05 | Requester cancels tender |
| `RejectBidV1` | 0x06 | Requester rejects a revealed bid |

## Data Structures

### Tender

```rust
pub struct Tender {
    pub id: TenderId,              // Commitment hash
    pub requester_pubkey: PublicKey,
    pub title: String,
    pub specification: pallas::Base,  // Hash of spec document
    pub requirement_commitment: pallas::Base,
    pub min_bid: u64,
    pub max_bid: u64,
    pub bid_deadline: u64,
    pub reveal_deadline: u64,
    pub delivery_deadline: u64,
    pub state: TenderState,
    pub selected_bid_id: Option<BidId>,
    pub bid_count: u64,
    pub created_at: u64,
}
```

### Bid

```rust
pub struct Bid {
    pub id: BidId,
    pub tender_id: TenderId,
    pub bidder_pubkey: PublicKey,
    pub amount: u64,                    // Hidden until reveal
    pub competency_commitment: pallas::Base,
    pub encrypted_payload: Vec<u8>,      // Encrypted bid details
    pub state: BidState,
    pub revealed_amount: Option<u64>,
    pub created_at: u64,
}
```

## Integration Flow

```
┌──────────────┐     ┌─────────────┐     ┌────────────────┐
│   Identity   │────>│   Tender    │────>│  Labor Market  │
│  (Competency │     │  (Sealed    │     │   (Job for     │
│   Credentials)     │   Bidding)  │     │   Winner)      │
└──────────────┘     └─────────────┘     └────────────────┘
                            │
                            v
                     ┌─────────────┐
                     │     Tau     │
                     │   (Task     │
                     │  Tracking)  │
                     └─────────────┘
```

1. **Identity**: Worker obtains competency credentials
2. **Tender**: Worker submits bid with competency commitment
3. **Winner Selected**: Job automatically created in labor market
4. **Tau**: Job tracked for delivery and completion

## ZK Circuits

| Circuit | Purpose |
|---------|---------|
| `create_tender_v1.zk` | Proves requester knows secret key |
| `submit_bid_v1.zk` | Proves bid with competency commitment |
| `reveal_bid_v1.zk` | Reveals sealed bid amount |
| `select_winner_v1.zk` | Proves winner selection is valid |

## Database Trees

- `tenders`: Stores Tender structs by ID
- `bids`: Stores Bid structs by ID
- `nullifiers`: Prevents double-spending
- `info`: Contract metadata

## Security Considerations

- Bids remain sealed until the reveal deadline
- Only the requester can select winners or cancel tenders
- Competency commitments allow verification without exposing worker details
- Nullifiers prevent bid submission replay