# Tender Contract

A privacy-preserving sealed-bid tendering system that integrates with the attestation contract for competency verification and the labor market for job execution.

## Overview

The tender contract enables requesters to create tenders for projects with:
- **Sealed bids**: Bidders submit encrypted bids that remain hidden until the reveal deadline
- **Attestation-based competency**: Bidders prove qualifications via attestation claims
- **Integration with labor market**: Winner automatically creates a job in the labor market
- **Task tracking via Tau**: Created jobs are tracked in Tau for delivery management

## Trust Model

1. **Requester creates attestation** with competency requirements
2. **Requester creates tender** referencing the attestation
3. **Workers submit claims** against the attestation proving qualifications
4. **Workers submit sealed bids** with claim IDs and encrypted bid details
5. **Bids revealed** after the bidding deadline
6. **Winner selected** based on competency (via attestation) + price evaluation
7. **Job created** via Labor Market for execution
8. **Task tracked** via Tau for progress monitoring

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

## Attestation Integration

The tender contract uses the [Attestation Contract](../attestation/README.md) for competency verification:

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
│     │◄──────────────────────────────── CreateTender(attestation_id)        │
│     │                                                                       │
│     │                              Worker                                   │
│     │                                 │                                     │
│     │                                 │ CreateClaim(evidence_commitment)   │
│     │                                 ▼                                     │
│     │                              Claim(Verified)                          │
│     │                                 │                                     │
│     │                                 │ claim_id                            │
│     │                                 │                                     │
│     │◄── SubmitBid(tender_id, claim_id, encrypted_amount)                │
│     │                                                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateTenderV1` | 0x00 | Requester creates a new tender (references attestation) |
| `SubmitBidV1` | 0x01 | Worker submits a sealed bid with claim_id |
| `RevealBidV1` | 0x02 | Worker reveals their bid amount |
| `CloseTenderV1` | 0x03 | Requester closes bidding, starts reveal period |
| `SelectWinnerV1` | 0x04 | Requester selects winning bid |
| `CancelTenderV1` | 0x05 | Requester cancels tender |
| `RejectBidV1` | 0x06 | Requester rejects a revealed bid |

## Data Structures

### Tender

```rust
pub struct Tender {
    pub id: TenderId,
    pub requester_pubkey: PublicKey,
    pub title: String,
    pub specification: pallas::Base,  // Hash of spec document
    pub attestation_id: pallas::Base,  // Attestation for competency requirements
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
    pub claim_id: pallas::Base,         // Attestation claim proving competency
    pub encrypted_payload: Vec<u8>,     // Encrypted bid details
    pub state: BidState,
    pub revealed_amount: Option<u64>,
    pub created_at: u64,
}
```

## Integration Flow

```
┌──────────────┐     ┌─────────────┐     ┌────────────────┐
│  Attestation │────>│   Tender    │────>│  Labor Market  │
│  (Competency │     │  (Sealed    │     │   (Job for     │
│   Claims)    │     │   Bidding)  │     │   Winner)      │
└──────────────┘     └─────────────┘     └────────────────┘
                            │
                            v
                     ┌─────────────┐
                     │     Tau     │
                     │   (Task     │
                     │  Tracking)  │
                     └─────────────┘
```

1. **Attestation**: Requester creates attestation for competency requirements
2. **Tender**: Worker creates claim on attestation, submits bid with claim_id
3. **Winner Selected**: Job automatically created in labor market
4. **Tau**: Job tracked for delivery and completion

## ZK Circuits

| Circuit | Purpose |
|---------|---------|
| `create_tender_v1.zk` | Proves requester knows secret key |
| `submit_bid_v1.zk` | Proves bid with attestation claim |
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
- Attestation claims provide competency verification without exposing worker details
- Nullifiers prevent bid submission replay
- Attestation verification is handled by the attestation contract

## See Also

- [Attestation Contract](../attestation/README.md) - Generalized attestation and claims
- [Labor Market Contract](../labor_market/README.md) - Job execution after tender award