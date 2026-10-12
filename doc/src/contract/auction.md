# Auction Contract Architecture

A privacy-preserving auction contract that uses the escrow contract for bid deposits.

## Overview

The auction contract enables:
- **English auctions** (ascending price, highest bidder wins)
- **Sealed-bid auctions** (all bids hidden until close)
- **Dutch auctions** (descending price, first to accept wins)

The key insight is that the auction contract **composes with** the promissory-note contract for value rather than reimplementing it.

## Composition with PromissoryNote

The auction makes **no call to the escrow contract** — it was documented as composing with escrow, and the
only child calls any entrypoint enforces are `promissory_note::transfer_v1 (0x04)`:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Auction Contract Composition                                │
├─────────────────────────────────────────────────────────────────────────────┤
│   Auction Contract                     PromissoryNote Contract                 │
│   │                                     │                                   │
│   │  SettleAuction ────────────────────>│  promissory_note::transfer_v1     │
│   │  ClaimWinnings ────────────────────>│  promissory_note::transfer_v1     │
│   │  RefundBid ─────────────────────────>│  promissory_note::transfer_v1     │
│   │  (CreateAuction / PlaceBid /         │                                   │
│   │   CloseAuction take no child)        │                                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

Each of the three requires exactly **one** child, and the child's contract id is validated against the
configured promissory-note id. Bids are held in the auction's own state rather than in per-bid escrows.

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

`BidState` has exactly four variants and `Won` and `Refunded` are both terminal — there is no
`Claimed` state (the diagram below showed one until 2026-10-12, and `ClaimWinningsV1` moves the
bid to nothing further):

```
Active ──[NewHigherBid]──> Outbid ──[Refund]──> Refunded
   │
   └──[Close]──> Won
```

| State | Description | Transitions |
|-------|-------------|-------------|
| Active | Currently highest bid | → Outbid (new higher bid) |
| Outbid | Another bid is higher | → Refunded (refund claimed) |
| Won | Won the auction | terminal — `ClaimWinningsV1` pays and leaves the bid `Won` |
| Refunded | Bid amount returned | terminal |

## Data Structures

### Auction

```rust
pub struct Auction {
    pub version: u8,
    pub id: AuctionId,                    // H(seller_pub, item, price, token, deadline)
    pub seller_pubkey: PublicKey,
    pub item_commitment: pallas::Base,    // H(item_description)
    pub reserve_price: u64,               // Minimum bid
    pub asset_id: pallas::Base,
    pub deadline_block: u64,
    pub state: AuctionState,
    pub highest_bid: Option<u64>,
    pub highest_bidder: Option<PublicKey>,
    pub highest_bid_id: Option<BidId>,
    pub bid_count: u64,
    pub created_at: u64,
    pub instance_seed: [u8; 32],
}
```

### Bid

```rust
pub struct Bid {
    pub version: u8,
    pub id: BidId,                        // H(auction_id, bidder_pub, amount, nonce)
    pub auction_id: AuctionId,
    pub bidder_pubkey: PublicKey,
    pub amount: u64,
    pub escrow_id: pallas::Base,          // the raw commitment, not an EscrowId nominal
    pub state: BidState,
    pub created_at: u64,
    pub instance_seed: [u8; 32],
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
| `create_auction.zk` | Prove auction commitment is valid |
| `place_bid.zk` | Prove bid is valid and exceeds current highest |
| `close_auction.zk` | Prove auction deadline reached |
| `claim_winnings.zk` | Prove winner authorization |
| `settle_auction.zk` | Prove seller receives payment |
| `refund_bid.zk` | Prove outbid bidder receives refund |

### create_auction.zk

**Purpose**: Prove auction commitment is valid

**Public inputs**: `auction_id`, `seller_commitment`

**Circuit**:
```zk
seller_pub = ec_mul_base(seller_secret, NULLIFIER_K);
seller_commitment = poseidon_hash(seller_pub_x, seller_pub_y);
auction_id = poseidon_hash(seller_pub_x, seller_pub_y, item_commitment,
                           reserve_price, asset_id, deadline_block);
less_than_strict(current_block, deadline_block);
```

### place_bid.zk

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
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis
