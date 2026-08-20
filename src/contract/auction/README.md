# DarkWow Auction Contract

A privacy-preserving auction contract that uses the escrow contract for bid deposits. Enables sealed-bid or English-style auctions.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Auction Flow                                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  SELLER                              AUCTION CONTRACT                         │
│     │                                    │                                    │
│     │  CreateAuction                    │                                    │
│     │──────────────────────────────────>│                                    │
│     │                                    │                                    │
│     │                         Auction Created                                 │
│     │                         (reserve_price set)                             │
│     │                                                                         │
│  BIDDER 1                            │                                        │
│     │  PlaceBid (escrow 1)           │                                        │
│     │──────────────────────────────────>│                                    │
│     │                          Creates Escrow 1 for bid amount               │
│     │                          (bidder=1, seller=auction)                     │
│     │                                                                         │
│  BIDDER 2                            │                                        │
│     │  PlaceBid (escrow 2)           │                                        │
│     │──────────────────────────────────>│                                    │
│     │                          Creates Escrow 2 for bid amount               │
│     │                          Outbids Escrow 1                              │
│     │                          Escrow 1 marked "Outbid"                       │
│     │                                                                         │
│     │                          CloseAuction (after deadline)                 │
│     │                                                    │                    │
│     │                   Highest Bid = Escrow 2                                │
│     │                   Winner = Bidder 2                                     │
│     │                                                                         │
│     │  RefundBid (Escrow 1)       │                                           │
│     │<─────────────────────────────                                           │
│     │   Bidder 1 gets refund      │                                           │
│     │                                                                         │
│     │  SettleAuction              │                                           │
│     │──────────────────────────────────>│                                    │
│     │                          Seller claims Escrow 2 (bid amount)            │
│     │                                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Insight: Composing with Escrow

The auction contract **uses** the escrow contract as a building block, rather than rebuilding escrow functionality:

- Each bid creates a separate escrow to hold the deposit
- When outbid, the bidder refunds via `escrow.refund()`
- Winner claims via `escrow.claim()`
- Seller settles to receive the winning bid amount

## Auction State Machine

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

## Bid State Machine

```
Active ──[Outbid]──> Outbid ──[Refund]──> Refunded
   │
   └──[Close]──> Won ──[Claim]──> Claimed
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `CreateAuctionV1` | 0x00 | Seller creates auction |
| `PlaceBidV1` | 0x01 | Bidder places bid (creates escrow) |
| `CloseAuctionV1` | 0x02 | Seller closes after deadline |
| `ClaimWinningsV1` | 0x03 | Winner claims the item |
| `SettleAuctionV1` | 0x04 | Seller settles to receive funds |
| `RefundBidV1` | 0x05 | Outbid bidder refunds |

## ZK Circuits

### create_auction_v1.zk

Proves the auction commitment is correctly formed:
- **Public inputs**: `auction_id`, `seller_commitment`
- **Private inputs**: `seller_secret`, `item_commitment`, `reserve_price`, `asset_id`, `deadline_block`
- **Verification**: Public key derivation + auction ID hash
- **Privacy**: `seller_commitment` hides seller_pub on-chain

### place_bid_v1.zk

Proves the bid is valid:
- **Public inputs**: `auction_id`, `bid_id`, `amount`
- **Private inputs**: `bidder_secret`, `bid_nonce`, `auction_deadline`, `current_block`
- **Verification**: Auction still active + bid > current high bid

### close_auction_v1.zk

Proves the auction can be closed:
- **Public inputs**: `auction_id`, `winner_bid_id`
- **Private inputs**: `seller_secret`, `auction_deadline`, `current_block`
- **Verification**: Deadline passed + seller authorization

### claim_winnings_v1.zk

Proves the winner legitimately claims:
- **Public inputs**: `auction_id`, `winner_bid_id`, `winner_pub_x`, `winner_pub_y`
- **Private inputs**: `winner_secret`
- **Verification**: Winner pubkey matches highest bidder

### settle_auction_v1.zk

Proves the seller settles:
- **Public inputs**: `auction_id`, `seller_pub_x`, `seller_pub_y`, `settlement_nullifier`
- **Private inputs**: `seller_secret`, `highest_bid_amount`
- **Verification**: Seller authorization + correct nullifier

### refund_bid_v1.zk

Proves the bidder legitimately refunds:
- **Public inputs**: `bid_id`, `bidder_pub_x`, `bidder_pub_y`, `refund_nullifier`
- **Private inputs**: `bidder_secret`
- **Verification**: Bidder matches original + correct nullifier

## Architecture

```
auction/
├── proof/                    # ZK proof circuits (.zk files)
│   ├── create_auction_v1.zk
│   ├── place_bid_v1.zk
│   ├── close_auction_v1.zk
│   ├── claim_winnings_v1.zk
│   ├── settle_auction_v1.zk
│   └── refund_bid_v1.zk
├── src/
│   ├── client/
│   │   └── mod.rs           # Builder structs
│   ├── entrypoint.rs         # WASM entrypoint
│   ├── error.rs              # AuctionError enum
│   ├── lib.rs                # Contract definitions
│   └── model/
│       └── mod.rs            # Data structures
└── README.md
```

## Building

```bash
# Build WASM contract
cargo build -p darkfi_auction_contract

# Compile ZK circuits (requires zkas binary)
for f in proof/*.zk; do ~/Darkfi/dwow/target/release/zkas "$f"; done

# Run tests
cargo test -p darkfi_auction_contract
```

## Integration with Escrow

The auction contract integrates with the escrow contract for managing bid deposits:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Integration Architecture                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   Escrow Contract                                                           │
│   ├── Manages bid deposits as separate escrows                              │
│   ├── Fund: Bidder funds escrow with bid amount                             │
│   ├── Claim: Seller claims winning bid (via auction settlement)             │
│   ├── Refund: Outbid bidders get deposits back                              │
│                                                                            │
│   Auction Contract                                                          │
│   ├── Manages auction state machine                                         │
│   ├── Tracks highest bid and winner                                         │
│   ├── Verifies ZK proofs for bid validity                                   │
│   └── Emits events for escrow operations                                    │
│                                                                            │
│   Flow:                                                                     │
│   1. Bidder calls PlaceBid → Auction creates escrow for deposit              │
│   2. Outbid → Bidder calls RefundBid → Auction calls escrow.Refund()        │
│   3. Win → Seller calls SettleAuction → Auction calls escrow.Claim()        │
│                                                                            │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Privacy Properties

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| Auction exists (commitment) | Item details (in commitment) |
| Bid placed (nullifier) | Actual bid amounts (in escrow) |
| Winner revealed (at close) | Other bidders' identities |
| Auction ended (block check) | Reserve price (in commitment) |

## Use Cases

### Single-Item Auction
```rust
// Seller auctions one NFT
let auction = CreateAuctionBuilder::new()
    .seller_pubkey(seller_pubkey)
    .item_commitment(nft_hash)  // Hash of NFT metadata
    .reserve_price(1000)         // Minimum 1000 DARK
    .asset_id(DRK_TOKEN_ID)
    .deadline_block(current_block + 1000)  // ~1 week
    .build()?;
```

### Dutch Auction (Descending Price)
```rust
// Seller starts high, price drops over time
// Each block, the price decreases
// First bidder to accept wins
```

### Sealed-Bid Auction
```rust
// All bids hidden until deadline
// Only commitments revealed (bid_id)
// Highest commitment wins
// Winner reveals to claim
```

## Security Considerations

### Bid Binding
The `bid_id = H(auction_id, bidder_pub, amount, nonce)` ensures:
- Bid cannot be changed after placement
- Bid cannot be copied by another bidder
- Nonce prevents bid ID collisions

### No Sniping
The deadline-based closing ensures:
- Last-minute bids can still be placed
- Auction cannot be "sniped" at exact deadline
- Seller must explicitly close

### Nullifier Uniqueness
Settlement and refund nullifiers prevent:
- Double-claiming of winnings
- Double-refunding of bids

## O-Cap Integration Assessment

The auction contract does **not** require O-Cap integration. Here's why:

**Current design is sufficient:**
- Escrow handles deposit management (composable, not built-in)
- ZK circuits handle all authorization (seller, bidder, winner)
- Privacy model already achieves: sealed bids, hidden amounts, identity protection via commitments
- State machine handles all edge cases (outbid, refund, settlement)

**O-Cap (0x09-0x0d) doesn't unlock new functionality here because:**
- Auction is about **value exchange**, not identity/capability verification
- The escrow contract already provides economic guarantees for deposits
- ZK circuits already handle authorization without revealing identity
- Adding capability verification would be orthogonal to the core purpose

**Where O-Cap could theoretically help (not needed):**
- KYC for high-value auctions (escrow provides sufficient trust)
- Seller reputation tiers (economic guarantees via escrow suffice)
- Anti-collusion via "not_blacklisted" (auction economics already deter this)

The contract is composable as-is. O-Cap is most valuable when:
- Identity/capability verification is the core purpose (Identity, Labor Market, Tender, Insurance)
- Cross-contract authorization is required (Tender → Labor Market → Insurance pipeline)
- Privacy of who-you-are matters more than what-you-prove

For auction, what-you-prove (bid amount, winner status) already works with ZK proofs alone.

## References

- [DarkWow Escrow Contract](../escrow/)
- [DarkWow zkVM](../zkas/)
