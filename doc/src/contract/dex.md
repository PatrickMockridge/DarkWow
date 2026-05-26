# Anonymous DEX Architecture

*Privacy-preserving decentralized exchange using incremental transparency.*

## The Core Problem: SPV De-anonymization

Bitcoin's SPV was supposed to provide privacy by only requesting relevant transactions. But bloom filters leak which addresses you care about - revealing what you're looking for IS a privacy failure.

```
SPV Problem:
1. Alice queries for transactions involving her addresses
2. Bloom filter reveals which addresses she cares about
3. Node can link Alice's addresses together
4. Privacy = broken
```

## Our Solution: Incremental Transparency

Instead of building a transparent system and trying to add privacy, we start completely dark and carefully reveal transparency as needed.

### Privacy Gradient

| Level | Description | Privacy Leakage |
|-------|-------------|-----------------|
| **Dark** | Complete darkness | None |
| **Aggregate** | Price ranges, volume bands | Minimal |
| **Anonymized** | Unlinkable trades | Low |
| **Full** | Opt-in transparency | None (opt-in) |

## Level 0 MVP: Atomic Swap DAO

The MVP is a **DAO that coordinates bilateral atomic swaps** - not a full order book:

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize DEX with config and governance key |
| `CreateSwapV1` | `0x01` | Proposer locks funds, creates swap proposal |
| `AcceptSwapV1` | `0x02` | Acceptor locks matching funds |
| `ExecuteSwapV1` | `0x03` | Execute atomic swap (partial fill support) |
| `CancelSwapV1` | `0x04` | Cancel swap, refund proposer |
| `UpdateConfigV1` | `0x05` | Update DEX configuration parameters |
| `SetTransparencyLevelV1` | `0x06` | Set transparency level per deployment |
| `ExecuteSwapFeeV1` | `0x07` | Execute swap with fee deduction (BaseDiv) |
| `ExecuteSwapSlippageV1` | `0x08` | Execute swap with slippage tolerance (BaseDiv) |

```
Alice wants: 100 DRKW ↔ 1 ETH (Bob's offer)
Bob wants: 1 ETH ↔ 100 DRKW (Alice's offer)

1. Proposal: Alice locks 100 DRKW, creates swap proposal
2. Acceptance: Bob locks 1 ETH, accepts swap
3. Execution: Atomic swap executes
4. Timeout: Auto-refund if no acceptance
```

### Why This Works as Level 0

- **No order book**: No one knows what swaps are possible
- **No price discovery**: Parties agree bilaterally
- **No information leakage**: Swap either happens or refunds
- **Trustless**: No trusted third party

## Modular Transparency Architecture

Different DEX deployments can choose different transparency levels at deployment:

```
┌─────────────────────────────────────────────────────────────────────┐
│  DEX Deployment: InitializeParams.transparency_config               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Dark (Default)                                                     │
│  └── swap_id only in events - maximum privacy                       │
│                                                                     │
│  Aggregate                                                          │
│  └── price bands, volume buckets - market data for LPs              │
│                                                                     │
│  Anonymized                                                         │
│  └── unlinkable trades - compliance-friendly                        │
│                                                                     │
│  Full                                                               │
│  └── everything revealed - full auditability                         │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Per-Level Circuit Availability

| Circuit | Dark | Aggregate | Anonymized | Full |
|---------|------|-----------|------------|------|
| `execute_swap_slippage_v1.zk` | ❌ | ✅ | ✅ | ✅ |
| `execute_swap_fee_v1.zk` | ❌ | ✅ | ✅ | ✅ |

Higher transparency levels enable more sophisticated market-making circuits.

## ZK Circuits

Six ZK circuits power the DEX:

| Circuit | Purpose |
|---------|---------|
| `create_swap_v1.zk` | Proves proposer locked valid funds |
| `accept_swap_v1.zk` | Proves acceptor locked matching funds |
| `execute_swap_v1.zk` | Proves both secrets known, partial fill |
| `cancel_swap_v1.zk` | Proves ownership for cancellation |
| `execute_swap_slippage_v1.zk` | Proves slippage tolerance (BaseDiv) |
| `execute_swap_fee_v1.zk` | Proves fee deduction (BaseDiv) |

### Data Structures

```
Lock Commitment = H(secret, token, amount)
Swap ID = H(proposer_lock, request_token, request_amount)
Nullifier = H(secret)
```

## Signature Verification

The DEX uses split verification (host + circuit):

```
Host verifies: schnorr_verify(public, signature)
Circuit constrains: ec_mul_base(sig_secret, K) == public
```

This exists because no `schnorr_verify` opcode exists yet.

## Capabilities Enabled by Opcodes

### BaseDiv (0x58)

Division for ratio calculations enables:

**Slippage tolerance**:
```zk
tolerance_multiplier = base_div(BPS - slippage_bps, BPS);
min_acceptable = base_mul(bob_amount, tolerance_multiplier);
```

**Fee calculation**:
```zk
fee = base_div(base_mul(amount, fee_bps), BPS);
```

### LessThanOrEqual (0x55)

Verified sound comparison enables:

**Partial fill**:
```zk
is_lte = less_than_or_equal(fill_amount, alice_amount);
constrain_equal_base(is_lte, ONE);
```

**Predicate-based matching**:
```zk
is_match = less_than_or_equal(my_price, market_price);
```

## Avoiding De-anonymization

### Attack Vectors

| Attack | Mitigation |
|--------|------------|
| Timing analysis | Minimum time delays between order and match |
| Dust attacks | Minimum order size thresholds |
| Volume fingerprinting | Batched trades + differential privacy |
| Key linkage | Different key per order |

### Key Insight

The key is **not revealing what you're looking for**:

| SPV Problem | DarkWow Solution |
|-------------|-----------------|
| Bloom filters reveal addresses | Order commitments hidden until match |
| Address clustering | Different key per order |
| Trade size = epoch volume | Privacy noise in aggregates |
| Timing analysis | Batched trades with random delays |

## Future: Order Book with Privacy

After MVP, expanding to order book requires:

```
Problem: Where do prices come from?
- Bilateral agreement: parties agree directly
- Order book: need price discovery
- But revealing order prices = deanonymization

Solutions:
- Encrypted order book ( commitments, ZK matching )
- Differential privacy ( noise in aggregates )
- Time-locked disclosure ( reveal after settlement )
```

### The SPV-Style Leak Problem

When solver queries for matching orders, the SMT query reveals what they're looking for:

```
Naive: Query SMT for "orders where price >= X"
      → Reveals you're looking for price X

DarkWow: Solver computes in encrypted domain
        → Only submits ZK proof of valid match
        → No one learns what orders were queried
```

## Multi-Chain Token Support

The DEX supports bridged tokens from the universal bridge:

| Chain | Token | Notes |
|-------|-------|-------|
| Ethereum | ETH | Native gas token |
| Monero | XMR | Privacy-native |
| Zcash | ZEC | Shielded |
| Aztec | ETH/DAI | Private rollup |
| Litecoin | LTC | Trade pair |

## Comparison

| Feature | UniSwap | Curve | DarkWow DEX |
|---------|---------|-------|------------|
| Order visibility | Full | Full | Level-dependent |
| Amount privacy | None | None | ZK commitments |
| Identity privacy | Pseudonym | Pseudonym | Hidden |
| Price discovery | AMM formula | AMM formula | Bilateral |
| Front-running | Vulnerable | Vulnerable | Resistant |
| Liquidity model | AMM | AMM | Bilateral locks |

## References

- [DarkWow DEX Contract](../../../src/contract/dex/)
- [DarkWow Money Contract](../spec/contract/money/money.md)
- [DarkWow Bridge Contract](./bridge.md)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)
- [Differential Privacy](https://en.wikipedia.org/wiki/differential_privacy)
