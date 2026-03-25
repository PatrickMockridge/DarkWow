Anonymous DEX (DRAFT)
===================

*This document describes the conceptual design for a privacy-preserving
decentralized exchange on DarkFi, starting with a Level 0 MVP and
expanding toward transparency incrementally.*

## The Core Problem: SPV De-anonymization

Bitcoin's SPV (Simplified Payment Verification) was supposed to provide
privacy: lightweight clients only request transactions they care about,
without downloading the full blockchain.

**But bloom filters leaked information.** When you query a node for
"give me all transactions involving address X," the bloom filter reveals
that you're interested in address X. Nodes can cluster addresses and
deanonymize users.

```
SPV Problem:
1. Alice queries node for txs involving her addresses
2. Bloom filter reveals which addresses she cares about
3. Node can now link Alice's addresses together
4. Privacy = broken
```

**Key insight**: Revealing what you're looking for IS a privacy leak.

## Our Solution: Incremental Transparency

Instead of building a transparent system and adding privacy (hard),
we start completely dark and carefully reveal transparency as needed.

### Privacy Gradient

| Level | Description | Use Case | Privacy Leakage |
|-------|-------------|----------|-----------------|
| **Level 0** | Complete darkness | Maximum sovereignty | None |
| **Level 1** | Aggregate market data only | Price discovery | Minimal |
| **Level 2** | Anonymized trades | Regulatory compliance | Low |
| **Level 3** | Full transparency | Audit-required entities | None (opt-in) |

### Why This Matters

Moving toward transparency too quickly creates a death spiral:

```
High transparency → LPs see order flow → LPs model risk
Low transparency → LPs don't see order flow → LPs won't provide liquidity
                         ↓
              Thin order books → Wide spreads → Traders leave
```

The solution: Give LPs **just enough** aggregate data to model risk
while keeping individual traders anonymous.

## Level 0 MVP: DAO with Atomic Swaps

The MVP is NOT a full order book. It's a **DAO that coordinates
bilateral atomic swaps**:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Level 0 MVP: Swap DAO                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Alice wants to swap: 100 DRK ↔ 1 ETH                           │
│  Bob wants to swap: 1 ETH ↔ 100 DRK                            │
│                                                                   │
│  1. Proposal: Alice creates swap proposal, locks 100 DRK        │
│     → Contract holds DRK, emits swap_created event              │
│                                                                   │
│  2. Acceptance: Bob sees proposal, locks 1 ETH                  │
│     → Contract holds ETH, emits swap_accepted event             │
│                                                                   │
│  3. Execution: DAO verifies both locks are valid                │
│     → Atomic swap: Alice gets ETH, Bob gets DRK                  │
│     → Both atomic, both simultaneous                            │
│                                                                   │
│  4. Timeout: If Bob doesn't accept in time                       │
│     → Alice's DRK refunded automatically                        │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Why This Works as Level 0

- **No order book**: No one knows what swaps are possible
- **No price discovery**: Parties agree bilaterally
- **No information leakage**: Swap either happens or refunds
- **Trustless**: No trusted third party
- **ZK-ready**: Can add proofs without redesign

### The Existing OTC Swap

DarkFi already has OTC swaps. The MVP extends this to:

1. **DAO coordination**: Multiple parties can participate
2. **Timeout mechanism**: Automatic refunds if counterparty doesn't appear
3. **ZK proofs**: Prove swap validity without revealing amounts

## Level 1: Order Book with Hidden Commitments

After MVP, we add a **Sparse Merkle Tree order book** where orders are
stored as commitments:

```
                    SMT Root (hidden)
                       │
        ┌──────────────┼──────────────┐
        │              │              │
   Order A         Order B         Order C
   (hidden)        (hidden)        (hidden)
      │              │              │
   Pedersen      Pedersen       Pedersen
      └──────────────┴──────────────┘
              All amounts hidden
```

### The Matching Problem

Traditional AMM: Pool reserves determine price automatically
DarkFi DEX: Orders are matched via ZK proofs

```
┌─────────────────────────────────────────────────────────────────┐
│                    Order Matching Flow                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Alice places order: "Sell 100 DRK at price ≥ 0.01 ETH"   │
│     → Creates note: Pedersen(secret, amount=100, price)         │
│     → Inserts commitment into SMT                                 │
│     → No one knows the price or amount!                         │
│                                                                   │
│  2. Bob places order: "Buy 100 DRK at price ≤ 0.02 ETH"       │
│     → Creates hidden note in SMT                                 │
│                                                                   │
│  3. Solver finds match:                                         │
│     → PROVES Order A and B exist in SMT                         │
│     → PROVES prices compatible (0.01 ≤ 0.02)                  │
│     → PROVES amounts sufficient                                  │
│     → Generates match ZK proof                                   │
│     → No one knows the actual prices!                          │
│                                                                   │
│  4. Contract verifies proof:                                     │
│     → Updates both orders atomically                            │
│     → Alice gets ETH, Bob gets DRK                              │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### The SPV-Style Leak Problem

**Problem**: When solver queries for matching orders, the SMT query
itself reveals what the solver is looking for.

**Solution**: Use encrypted search / oblivious RAM:

```
Naive approach: Query SMT for "orders where price >= X"
→ Reveals you're looking for orders at price X

DarkFi approach:
→ Solver computes match in encrypted domain
→ Only submits ZK proof of valid match
→ No one learns what orders were queried!
```

## Avoiding De-anonymization

### Attack: Timing Analysis

If Alice always places orders just before Bob, they're clearly coordinating.

**Mitigation**: Minimum time delays between order placement and matching.

### Attack: Dust Attacks

Tiny orders can be used to probe the order book.

**Mitigation**: Minimum order size thresholds.

### Attack: Volume Fingerprinting

Low-volume epochs mean your trade size = epoch volume.

**Mitigation**: Batch trades and add differential privacy noise.

### Attack: Linkage via Keys

If Alice uses the same key for all orders, they're linkable.

**Mitigation**: Different key per order, different key per transaction.

## Expanding from MVP

```
MVP (Level 0)
    │
    │ Add SMT order book
    │ Add ZK matching
    │
Level 1: Hidden order book with ZK matching
    │
    │ Add aggregate depth proofs
    │ Add time-bucketed volume
    │
Level 2: Anonymized market data
    │
    │ Add opt-in transparency
    │
Level 3: Full transparency (audit mode)
```

## Key Components

### ZK Circuits

**place_order.zk**: Proves order commitment is valid and doesn't exist in book

**match_orders.zk**: Proves two orders are compatible without revealing prices

**cancel_order.zk**: Proves ownership and cancels order

### Data Structures

```
Order Commitment = H(secret, amount, price, token, side)
Order Nullifier = H(secret)
LP Share = H(secret, pool_id, share_amount)
```

### The DAO Layer

The Swap DAO handles:
- Swap proposal creation
- Counterparty acceptance
- Atomic execution
- Timeout refunds
- Governance (fee updates, etc.)

## Comparison

| Feature | UniSwap | Curve | DarkFi MVP | DarkFi L1+ |
|---------|---------|-------|------------|------------|
| Order visibility | Full | Full | None | Hidden |
| Amount privacy | None | None | ZK | ZK |
| Identity privacy | Pseudonym | Pseudonym | Hidden | Hidden |
| Price discovery | AMM formula | AMM formula | Bilateral | ZK match |
| Front-running | Vulnerable | Vulnerable | Resistant | Resistant |

## Open Questions

1. **How does solver find matches without revealing queries?**
2. **How do LPs assess risk with hidden order flow?**
3. **What aggregate data can we reveal without deanonymizing?**
4. **How to handle partial fills with ZK?**
5. **Can we do limit orders with ZK without revealing price?**

## References

- [DarkFi DEX Contract](../../src/contract/dex/)
- [DarkFi Money Contract](./money.md)
- [DarkFi Bridge Contract](./bridge.md)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)
- [Differential Privacy](https://en.wikipedia.org/wiki/Differential_privacy)