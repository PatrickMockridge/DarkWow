# Auction Contract Architecture

A privacy-preserving auction contract that uses the escrow contract for bid deposits.

## Overview

The auction contract enables:
- **English auctions** (ascending price, highest bidder wins)
- **Sealed-bid auctions** (all bids hidden until close)
- **Dutch auctions** (descending price, first to accept wins)

The key insight is that the auction contract **composes with** the escrow contract rather than reimplementing escrow functionality.

## Composition with Escrow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Auction Contract Composition                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   Auction Contract                       Escrow Contract                      │
│   │                                     │                                   │
│   │  CreateAuction ─────────────────────>│  (creates escrow for item)       │
│   │                                     │                                   │
│   │  PlaceBid ──────────────────────────>│  (creates escrow for bid)        │
│   │                                     │                                   │
│   │  CloseAuction                       │                                   │
│   │                                     │                                   │
│   │  SettleAuction ────────────────────>│  (claims winning bid)              │
│   │                                     │                                   │
│   │  RefundBid ─────────────────────────>│  (refunds outbid deposits)       │
│   │                                     │                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## State Machines

### Auction State

```
Created ──[PlaceBid]──> Active ──[Close]──> Closed ──[Settle]──> Settled
                                           │
                        ┌──────────────────┴──────────────────┐
                        │                                     │
                  [ClaimWinnings]                      [RefundBid]
                        │                                     │
                        ▼                                     ▼
                   Winner Paid                          Bids Refunded
```

| State | Description | Transitions |
|-------|-------------|-------------|
| Created | Auction created, awaiting bids | → Active (first bid) |
| Active | Accepting bids | → Closed (deadline) |
| Closed | Auction ended | → Settled (all claims) |
| Settled | All funds distributed | Terminal |

### Bid State

```
Active ──[NewHigherBid]──> Outbid ──[Refund]──> Refunded
   │
   └──[Close]──> Won ──[Claim]──> Claimed
```

| State | Description | Transitions |
|-------|-------------|-------------|
| Active | Currently highest bid | → Outbid (new higher bid) |
| Outbid | Another bid is higher | → Refunded (refund claimed) |
| Won | Won the auction | → Claimed (item claimed) |

## Data Structures

### Auction

```rust
pub struct Auction {
    pub id: AuctionId,                    // H(seller_pub, item, price, token, deadline)
    pub seller_pubkey: PublicKey,
    pub item_commitment: pallas::Base,    // H(item_description)
    pub reserve_price: u64,               // Minimum bid
    pub token_id: pallas::Base,
    pub deadline_block: u64,
    pub state: AuctionState,
    pub highest_bid: Option<u64>,
    pub highest_bidder: Option<PublicKey>,
    pub highest_bid_id: Option<BidId>,
    pub bid_count: u64,
}
```

### Bid

```rust
pub struct Bid {
    pub id: BidId,                        // H(auction_id, bidder_pub, amount, nonce)
    pub auction_id: AuctionId,
    pub bidder_pubkey: PublicKey,
    pub amount: u64,
    pub escrow_id: EscrowId,              // Links to escrow holding deposit
    pub state: BidState,
}
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateAuctionV1` | `0x00` | Create new auction with item commitment |
| `PlaceBidV1` | `0x01` | Place a bid (deposit held in escrow) |
| `CloseAuctionV1` | `0x02` | Close bidding period |
| `ClaimWinningsV1` | `0x03` | Winner claims the auctioned item |
| `SettleAuctionV1` | `0x04` | Settle auction, transfer payment to seller |
| `RefundBidV1` | `0x05` | Refund outbid deposits |

## ZK Circuits

All 6 circuits compiled to `.zk.bin`:

| Circuit | Purpose |
|---------|---------|
| `create_auction_v1.zk` | Prove auction commitment is valid |
| `place_bid_v1.zk` | Prove bid is valid and exceeds current highest |
| `close_auction_v1.zk` | Prove auction deadline reached |
| `claim_winnings_v1.zk` | Prove winner authorization |
| `settle_auction_v1.zk` | Prove seller receives payment |
| `refund_bid_v1.zk` | Prove outbid bidder receives refund |

### create_auction_v1.zk

**Purpose**: Prove auction commitment is valid

**Public inputs**: `auction_id`, `seller_commitment`

**Circuit**:
```zk
seller_pub = ec_mul_base(seller_secret, NULLIFIER_K);
seller_commitment = poseidon_hash(seller_pub_x, seller_pub_y);
auction_id = poseidon_hash(seller_pub_x, seller_pub_y, item_commitment,
                           reserve_price, token_id, deadline_block);
less_than_strict(current_block, deadline_block);
```

### place_bid_v1.zk

**Purpose**: Prove bid is valid and exceeds current highest

**Public inputs**: `auction_id`, `bid_id`, `amount`

**Circuit**:
```zk
bidder_pub = ec_mul_base(bidder_secret, NULLIFIER_K);
less_than_strict(current_block, auction_deadline);
# Cross-multiplication for bid > highest_bid
bid_id = poseidon_hash(auction_id, bidder_pub_x, bidder_pub_y, amount, nonce);
```

## Security Properties

### Bid Privacy
- Bid amounts hidden in escrow commitments
- Bid identities use public key derivation
- Only winner revealed at close

### Bid Binding
- `bid_id = H(auction_id, bidder_pub, amount, nonce)`
- Cannot change bid after placement
- Cannot copy another bidder's bid

### No Sniping
- Deadline-based closing
- Last-second bids allowed
- Explicit seller close required

### Double-Spend Prevention
- Nullifiers for settlement and refunds
- State machine prevents invalid transitions

## Integration Patterns

### With PromissoryNote
```rust
// Winner pays and receives item
let tx = Transaction::new()
    .add_call(&escrow_contract, "claim", winner_proof)
    .add_call(&promissory_note_contract, "transfer", payment_proof);
```

### With DAO Governance
```rust
// Auction house governed by DAO
// Reserve prices set by governance
// Disputes resolved by DAO vote
```

## Comparison

| Feature | Traditional Auction | On-Chain Auction | DarkWow Auction |
|---------|--------------------|--------------------|----------------|----------------|
| Privacy | None | Pseudonymous | Full privacy |
| Bid hiding | Sealed envelope | Commitment only | Hidden in escrow |
| Trust | Auction house | Smart contract | ZK-verified |
| Bid binding | Legal | Cryptographic | Cryptographic |
| Reveal | Live | On-chain | ZK-verified |

## See Also

- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis
