# DarkFi DEX Contract

Anonymous decentralized exchange for privacy-preserving token swaps.

## Design Philosophy: Incremental Transparency

The DarkFi DEX starts as a **Level 0 MVP** (complete darkness) and can
expand toward transparency as functionality requires. This approach avoids
the SPV de-anonymization problem where revealing what you're looking for
itself creates privacy leaks.

### The SPV Problem

Bitcoin's SPV (Simplified Payment Verification) was supposed to provide
privacy by only requesting relevant transactions. But bloom filters
leaked enough information to cluster addresses and deanonymize users.
**Revealing what you're interested in = privacy failure.**

### Our Solution: Gradual Transparency

Instead of building a transparent system and trying to add privacy,
we start completely dark and **carefully reveal only aggregate data**
that enables functionality without compromising individuals.

```
Level 0: Complete darkness (MVP - atomic swaps via DAO)
Level 1: Aggregate market data only (price ranges, volume bands)
Level 2: Anonymized trades (unlinkable)
Level 3: Full transparency (opt-in)
```

## Level 0 MVP: DAO with Atomic Swaps

The MVP is not a full order book - it's a **DAO that coordinates
atomic swaps** between two parties:

```
┌─────────────────────────────────────────────────────────────────┐
│                    MVP: Atomic Swap DAO                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Alice and Bob agree to swap: Alice DRK ↔ Bob's ETH          │
│                                                                   │
│  2. Both lock funds in the Swap DAO contract                     │
│                                                                   │
│  3. DAO verifies both locks are valid                            │
│                                                                   │
│  4. Atomic swap executes: Alice gets ETH, Bob gets DRK           │
│                                                                   │
│  5. If one party cheats, DAO refunds both after timeout          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

**Why this is Level 0:**
- No order book (no one knows what trades are possible)
- No price discovery (parties agree bilaterally)
- No information leakage (swap either completes or refunds)
- Essentially: private bilaterally-matched trades

### Why Start Here?

1. **Auditable core**: The DAO contract itself can be audited
2. **No information leakage**: No order flow to analyze
3. **ZK-ready**: Easy to add ZK proofs later without redesign
4. **Trustless**: No trusted third party for the swap

## Future: Order Book with Privacy

After MVP, expanding to order book requires solving:

### Problem: Where do prices come from?

With atomic swaps, parties bilaterally agree. With order books,
you need price discovery. But revealing order prices = deanonymization.

### Solution Approaches

**Approach A: Encrypted Order Book**

Orders are committed (hidden), solver matches them, but:
- Solver sees commitments, not plaintext
- Match proof proves compatibility without revealing prices
- Problem: Solver can still see order sizes

**Approach B: Differential Privacy**

Add noise to aggregate data so individual orders can't be inferred:
- Total volume in epoch: 1000 ± 50 DRK
- Noise prevents exact order reconstruction
- But LPs get enough signal to assess risk

**Approach C: Time-Locked Disclosure**

Match is revealed only after trade settles:
- During matching: completely dark
- After settlement: match becomes public
- Traders can choose between privacy and finality

### Avoiding SPV-Style Leaks

The key is **not revealing what you're looking for**:

| SPV Problem | DarkFi Solution |
|-------------|-----------------|
| Bloom filters reveal tx of interest | Order commitments reveal nothing until match |
| Address clustering via SPV queries | Different key per order prevents linkage |
| Trade size = epoch volume | Differential privacy noise |
| Timing analysis | Batched trades with random delays |

## Contract Functions

### MVP (Implemented)

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize swap contract |
| CreateSwapV1 | 0x01 | Create atomic swap proposal |
| AcceptSwapV1 | 0x02 | Accept swap (provide liquidity) |
| CancelSwapV1 | 0x03 | Cancel and refund |
| ExecuteSwapV1 | 0x04 | Execute atomic swap |

### Future (Planned)

| Function | ID | Description |
|----------|-----|-------------|
| PlaceOrderV1 | 0x05 | Add order to hidden book |
| CancelOrderV1 | 0x06 | Cancel order |
| MatchOrdersV1 | 0x07 | Match orders (ZK proof) |
| AddLiquidityV1 | 0x08 | Add LP liquidity |

## ZK Circuits

### MVP Circuits

- `create_swap.zk`: Proves swap parameters valid
- `accept_swap.zk`: Proves acceptor has funds
- `execute_swap.zk`: Proves atomic swap conditions met

### Future Circuits

- `place_order.zk`: Order commitment + SMT non-existence
- `match_orders.zk`: Order compatibility without revealing prices
- `cancel_order.zk`: Order cancellation proof

## Privacy Properties

### What Level 0 MVP Hides

- Who is trading with whom
- What amounts are being swapped
- What tokens are involved (until match)

### What Level 0 Reveals

- That a swap occurred (eventually)
- Aggregate volume (after batch)

### What Future Levels Add

- Level 1: Price ranges, volume bands (aggregate only)
- Level 2: Anonymized match data (unlinkable)
- Level 3: Full transparency (opt-in)

## Implementation Status

- [x] Contract structure and entrypoint
- [x] MVP function definitions
- [ ] Atomic swap DAO implementation
- [ ] ZK circuits for swap verification
- [ ] Order book expansion (future)

## References

- [DarkFi DEX Architecture Document](../../doc/src/arch/dex.md)
- [DarkFi Money Contract](../money/)
- [DarkFi Bridge Contract](../bridge/)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)