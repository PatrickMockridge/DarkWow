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

## Level 1: Order Book with Hidden Commitments (Future)

After MVP, we expand to a **Sparse Merkle Tree order book** where orders are
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

The transparency level is **configurable at deployment** - different DEXes can serve different privacy/compliance needs:

```
Level 0 (Dark - Default)
├── Complete darkness - nothing revealed
├── Atomic swaps via DAO
├── No order book
├── Bilateral price agreement
└── ZK proofs for validity

Level 1 (Aggregate)
├── Aggregate market data - price ranges, volume bands
├── Partial fills with slippage tolerance
├── Fee calculations
└── ZK proof of order matching

Level 2 (Anonymized)
├── Unlinkable trade data
├── Anonymity groups
└── Differential privacy noise

Level 3 (Full)
├── Opt-in full transparency
├── Everything revealed
└── Full compliance
```

**Key insight**: DarkFi will have multiple DEXes with different transparency levels, not one size fits all. The level is set at deployment via `InitializeParams.transparency_config`.

## Key Components

### ZK Circuits for Atomic Swaps

**create_swap_v1.zk**: Proves proposer has locked valid funds for swap

**accept_swap_v1.zk**: Proves acceptor has locked matching funds

**execute_swap_v1.zk**: Proves both secrets known, locks valid, swap consistent, partial fill

**execute_swap_slippage_v1.zk**: Proves swap execution with slippage tolerance (BaseDiv)

**execute_swap_fee_v1.zk**: Proves swap execution with fee deduction (BaseDiv)

**cancel_swap_v1.zk**: Proves ownership of lock for cancellation

### Per-Level Circuit Selection

| Circuit | Dark | Aggregate | Anonymized | Full |
|---------|------|-----------|------------|------|
| `create_swap_v1.zk` | ✅ | ✅ | ✅ | ✅ |
| `accept_swap_v1.zk` | ✅ | ✅ | ✅ | ✅ |
| `execute_swap_v1.zk` | ✅ | ✅ | ✅ | ✅ |
| `cancel_swap_v1.zk` | ✅ | ✅ | ✅ | ✅ |
| `execute_swap_slippage_v1.zk` | ❌ | ✅ | ✅ | ✅ |
| `execute_swap_fee_v1.zk` | ❌ | ✅ | ✅ | ✅ |

### Data Structures

```
Lock Commitment = H(secret, token, amount)
Swap ID = H(lock_commitment, request_token, request_amount)
Swap Nullifier = H(secret)
```

The atomic swap uses:
- **Lock commitment**: Proves funds are locked without revealing amount/token
- **Swap ID**: Unique identifier derived from proposer's lock and requested swap
- **Nullifier**: Prevents double-spend / ensures lock hasn't been used

### The DAO Layer

The Swap DAO contract handles:
- **Swap proposal creation**: Alice locks funds, creates swap proposal
- **Counterparty acceptance**: Bob locks matching funds, accepts swap
- **Atomic execution**: ZK proof verifies both secrets, contract executes atomically
- **Timeout refunds**: If swap not executed in time, either party can cancel and refund
- **Governance**: Update timeout and fee parameters

## Signature Verification and the Opcode Layer

This section explains how the DEX implements signature verification and why certain
opcode limitations shape the current design.

### The Signature Verification Flow

The DEX uses a split verification model where:

1. **Client computes signature**: The proposer signs swap parameters using `SchnorrSecret::sign()`
2. **Host verifies signature**: Before the contract runs, the host verifies the signature
3. **ZK circuit constrains public key**: The circuit derives `signature_public` from witness and constrains coordinates

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    DEX Signature Verification Flow                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Client (off-chain)                                                          │
│     │                                                                       │
│     │ 1. Create swap parameters                                            │
│     │ 2. Sign with secret key: signature = secret.sign(swap_data)          │
│     │ 3. Derive public key: signature_public = secret.to_public()         │
│     ▼                                                                       │
│  Host (verifies before contract)                                             │
│     │                                                                       │
│     │ 4. Verify signature: public.verify(signature)                       │
│     │    - If invalid, reject transaction                                  │
│     │    - If valid, continue                                              │
│     ▼                                                                       │
│  Contract (executes if host verification passed)                             │
│     │                                                                       │
│     │ 5. Receive signature_public as parameter                              │
│     │ 6. Pass signature_public.x, signature_public.y to ZK circuit       │
│     │ 7. ZK circuit derives same and constrains equality                   │
│     ▼                                                                       │
│  ZK Circuit                                                                  │
│     │                                                                       │
│     │ 8. Witness: signature_secret                                          │
│     │ 9. Derive: signature_public = ec_mul_base(signature_secret, K)     │
│     │ 10. Constrain: ec_get_x(signature_public) == public_x              │
│     │     Constrain: ec_get_y(signature_public) == public_y              │
│     ▼                                                                       │
│  Result: Prover proved they know secret without revealing it                 │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Why Not Verify Signature Directly in ZK Circuit?

Ideally, the ZK circuit would verify the signature directly:

```zk
# Hypothetical - if we had the right opcodes:
signature_valid = schnorr_verify(signature_public, message, signature);
constrain_equal_base(signature_valid, 1);
```

This would eliminate the need for host-level signature verification. However, this
requires opcodes that don't exist yet:

| Opcode | Why It's Needed | Status |
|--------|----------------|--------|
| **schnorr_verify** | Verify signature in circuit | Not implemented |
| **BaseDiv** | Division for ratio checks | **Implemented** (0x58) - slippage & fees |
| **LessThanOrEqual** | Price comparison for matching | **Verified sound** (0x55) - partial fills |

### BaseDiv Implementation Impact

`BaseDiv` (a / b mod p) is now **implemented** (opcode 0x58 using binary exponentiation). This enables:

**Impact 1: Price Ratio Comparisons**

Level 1+ DEX can now match orders by price ratio directly:

```zk
# Price matching with BaseDiv:
alice_price = base_div(alice_amount, alice_request);
bob_price = base_div(bob_amount, bob_offer);
price_match = less_than_or_equal(alice_price, bob_price);
```

**Optimization**: Cross-multiplication remains useful for simple assertions (avoids ~500 field multiplications):

```zk
# Cross-multiplication optimization for ratio assertions:
# Want: alice_request / alice_amount >= bob_offer / bob_amount
# i.e.: alice_request * bob_amount >= bob_offer * alice_amount
lhs = base_mul(alice_request, bob_amount);
rhs = base_mul(bob_offer, alice_amount);
less_than_or_equal(rhs, lhs);  # Assert rhs <= lhs (cross-mul optimization)
```

**Impact 2: Partial Fill Calculations**

With BaseDiv, partial fills can compute ratios directly:

```zk
# Partial fill with BaseDiv:
fill_ratio = base_div(fill_amount, total_amount);
filled_value = base_mul(fill_amount, fill_ratio);
```

Cross-multiplication optimization remains useful for simple assertions:

```zk
# CROSS-MUL OPTIMIZATION: Assert fill_amount <= total_amount
# Pattern: prove fill_amount < total_amount + 1
ONE = witness_base(1);
total_plus_one = base_add(total_amount, ONE);
less_than_strict(fill_amount, total_plus_one);
```

### LessThanOrEqual Verification Status

`LessThanOrEqual` (0x55) is now **formally verified sound** via Lean 4 exhaustive testing.

The DEX's `execute_swap_v1.zk` uses verified `less_than_or_equal` directly:

```zk
# From execute_swap_v1.zk - using verified LessThanOrEqual:
ONE = witness_base(1);
is_lte = less_than_or_equal(fill_amount, alice_amount);
constrain_equal_base(is_lte, ONE);
```

LessThanOrEqual returns a Boolean for full composability. The circuit now uses
it directly rather than the safemath workaround pattern.

### Signature Verification Without BaseDiv

The current signature verification doesn't need `BaseDiv`:

```zk
# Signature verification uses ONLY these opcodes:
signature_public = ec_mul_base(signature_secret, NULLIFIER_K);
constrain_instance(ec_get_x(signature_public));
constrain_instance(ec_get_y(signature_public));
```

The Schnorr signature verification equation:
```
R = g^k
e = H(R || pubkey || message)
s = k + e*x
verify: g^s == R * pubkey^e
```

This requires `BaseDiv` to compute e = H(...)/something? No - the challenge
is computed via hash, not division. The verification happens at the host level
(external to circuit), so the circuit only needs to constrain the public key.

### Why Signature Verification Is Split

The split model (host verifies, circuit constrains) exists because:

1. **No schnorr_verify opcode**: Circuit can't verify signatures directly
2. **Public key commitment**: Circuit constrains that prover knows the private key
3. **Host as guard**: Invalid signatures rejected before contract runs

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Split Verification Architecture                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Host (Rust)                           ZK Circuit                            │
│  ─────────────────                     ────────────                           │
│  Verify signature with:                Constrain signature_public:           │
│  - schnorr_verify()                    - ec_mul_base(sig_secret, K)          │
│  - Returns bool                        - constrain_instance(coord_x)        │
│                                                                              │
│  Prover MUST have valid signature     Prover MUST know secret key           │
│  to reach contract execution          to satisfy circuit constraints         │
│                                                                              │
│  Result: Both conditions required for valid execution                       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### The Trusted Setup Workaround for Money Contract Integration

The DEX must verify that locked funds exist in the Money contract. This requires
cross-contract state verification, which needs opcodes that don't exist:

| Required | Status | Impact |
|----------|--------|--------|
| Cross-contract ZK composition | Not implemented | Can't call Money circuit |
| On-chain Merkle root verification | Not implemented | Can't verify proofs |
| Event-based state sync | Not implemented | Can't react to Money changes |

**Current workaround**: DEX initialized with `trusted_money_merkle_root`

```rust
// In InitializeParams:
pub struct InitializeParams {
    pub trusted_money_merkle_root: [u8; 32],  // Set at initialization
}
```

```rust
// In verify_lock_proof:
fn verify_lock_proof(lock_commitment, lock_proof) {
    // Recompute Merkle root from proof
    computed_root = merkle_root(lock_commitment, lock_proof);

    // Compare against trusted root (set at init)
    if computed_root != trusted_root {
        return Err(InvalidMerkleProof);
    }
}
```

**Security trade-off**: If `trusted_money_merkle_root` is wrong/stale, invalid
lock proofs may be accepted.

### What BaseDiv Enables

With `BaseDiv` (0x58) **now implemented**, the DEX supports:

**1. Slippage tolerance circuits**

```zk
# From execute_swap_slippage_v1.zk:
# Verify: received >= min_expected * (1 - slippage_tolerance)
tolerance_multiplier = base_div(BPS - slippage_bps, BPS);
min_acceptable = base_mul(bob_amount, tolerance_multiplier);
less_than_or_equal(min_acceptable, received);
```

**2. Fee calculation circuits**

```zk
# From execute_swap_fee_v1.zk:
# Calculate fee = fill_amount * fee_bps / 10000
fee_numerator = base_mul(fill_amount, fee_bps);
fee = base_div(fee_numerator, BPS);
net_received = fill_amount - fee;
```

**3. Exchange rate bounds verification**

```zk
# Verify exchange rate is within agreed bounds
actual_rate = base_div(given, wanted);
less_than_or_equal(rate_lower, actual_rate);
less_than_or_equal(actual_rate, rate_upper);
```

### What LessThanOrEqual Enables

With `LessThanOrEqual` (0x55) **verified sound** via Lean 4:

**1. Partial fill with verified comparison**

```zk
# From execute_swap_v1.zk:
is_lte = less_than_or_equal(fill_amount, alice_amount);
constrain_equal_base(is_lte, ONE);
```

**2. Predicate-based order matching**

```zk
# LessThanOrEqual returns 0/1 Boolean:
is_match = less_than_or_equal(my_price, market_price);
constrain_instance(is_match);  # Public output
```

**3. Conditional execution based on comparison**

```zk
# Execute only if fill meets minimum:
fills_enough = less_than_or_equal(min_fill_amount, fill_amount);
execute_if_valid = cond_select(fills_enough, execute, abort);
```

**4. Complex order types**

```zk
# Stop-loss order:
stop_triggered = less_than_or_equal(stop_price, current_price);
constrain_equal_base(stop_triggered, 1);
```

### Current Trade-offs in the DEX Design

| Capability | Status | Use in DEX |
|------------|--------|-----------|
| `BaseDiv` (0x58) | ✅ Implemented | Slippage tolerance, fee calculation |
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Partial fill comparisons |
| `less_than_strict` | ✅ Sound | Bounded comparisons |
| No schnorr_verify in circuit | Split verification (host + circuit) | Extra verification step |
| No cross-contract ZK | Trusted Merkle root setup | Trust assumption |

### The Long-Term Solution

When the opcode layer is complete, the DEX signature verification could be:

```zk
# Future circuit with full opcode support:
circuit "CreateSwapV1" {
    witness {
        Base secret,
        Base signature_secret,
        # ...
    }

    # Derive lock commitment
    computed_lock = poseidon_hash([secret, ...]);
    constrain_instance(computed_lock);

    # Full signature verification in circuit
    signature_valid = schnorr_verify(
        signature_public,
        swap_data,
        signature
    );
    constrain_equal_base(signature_valid, 1);

    # Price ratio with BaseDiv
    price_ratio = base_div(request_amount, offer_amount);
    constrain_instance(price_ratio);

    # Boolean comparison for conditions
    sufficient_funds = less_than_or_equal(minimum, amount);
    constrain_equal_base(sufficient_funds, 1);
}
```

This would eliminate:
- Host-level signature verification (circuit does it all)
- Trusted setup for Money contract (cross-contract ZK composition)
- The need for some workarounds (native opcodes now available)

### Related Opcode Documentation

See [zkVM Primitive Layer](zkvm_primitives.md) for:
- Full analysis of `LessThanOrEqual` soundness concerns
- `BaseDiv` implementation status and use cases
- Why comparison opcodes are foundational to contract expressiveness

See [Private Authorization Layer](privauth.md) for:
- How the signature/authorization pattern works across contracts
- Why the split verification model exists

## Multi-Chain Token Support

The DEX **theoretically supports all bridged tokens** from the universal bridge:

| Chain | Token | DEX Status | Notes |
|-------|-------|------------|-------|
| **Ethereum** | ETH | Supported | Native gas token |
| **Monero** | XMR | Supported | Privacy-native |
| **Zcash** | ZEC | Supported | Shielded transactions |
| **Aztec** | ETH/DAI | Supported | Private rollup |
| **Litecoin** | LTC | Supported | The Monero trade pair |

This means the DEX can facilitate:
- **XMR/ETH swaps**: Private exchange via atomic swaps
- **ZEC/LTC swaps**: Shielded-to-transparent with LTC as stepping stone
- **DAI/ETH swaps**: Private stablecoin trades via Aztec
- **Cross-chain liquidity**: All tokens tradable within DarkFi's privacy layer

### Bridged DAI as Price Anchor

**Bridged DAI (via Aztec)** is particularly valuable for the DEX:

```
┌─────────────────────────────────────────────────────────────────┐
│              DAI as Price Anchor in DarkFi                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  External: DAI/USD ≈ $1.00                                        │
│                        ↓                                          │
│  Aztec Bridge: wDAI on DarkFi                                     │
│                        ↓                                          │
│  DarkFi AMM Pool: DAI/DRK or DAI/NETHER                         │
│                        ↓                                          │
│  TWAP Price Feed → Stablecoin PI Controller                       │
│                                                                   │
│  Result:                                                         │
│  - DAI provides natural USD price reference                       │
│  - NETHER can be redeemed for DAI (indirect USD peg)             │
│  - Arbitrage keeps NETHER price stable                           │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Price Signals In and Out of DarkFi

The DEX creates **bidirectional price signals**:

**Price signals INTO DarkFi:**
- External ETH/USD → via bridge data → collateral valuation
- External XMR/USD → via bridge data → LP pool pricing
- External DAI/USD → via Aztec bridge → stablecoin redemption rate

**Price signals OUT of DarkFi:**
- DarkFi NETHER/USD → via bridge redemption → external markets
- DarkFi DRK/token prices → observable via bridge withdrawal data
- DarkFi AMM TWAP → published via data bridge for external consumption

This makes DarkFi a **privacy-preserving price discovery layer** that:
1. Hides trader identity and amounts
2. Publishes aggregate price data
3. Enables arbitrage across privacy boundary
4. Maintains price peg stability for the stablecoin

## Comparison

| Feature | UniSwap | Curve | DarkFi MVP (Atomic Swaps) |
|---------|---------|-------|---------------------------|
| Order visibility | Full | Full | None |
| Amount privacy | None | None | ZK (commitments) |
| Identity privacy | Pseudonym | Pseudonym | Hidden |
| Price discovery | AMM formula | AMM formula | Bilateral agreement |
| Front-running | Vulnerable | Vulnerable | Resistant (atomic) |
| Order book | Yes | Yes | No (Level 0) |
| Liquidity model | AMM | AMM | Bilateral locks |

## Open Questions

1. **How does solver find matches without revealing queries?**
2. **How do LPs assess risk with hidden order flow?**
3. **What aggregate data can we reveal without deanonymizing?**
4. **How to handle partial fills with ZK?** - ✅ Resolved with LessThanOrEqual
5. **Can we do limit orders with ZK without revealing price?**

## References

- [DarkFi DEX Contract](../../src/contract/dex/)
- [DarkFi Money Contract](./money.md)
- [DarkFi Bridge Contract](./bridge.md)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)
- [Differential Privacy](https://en.wikipedia.org/wiki/Differential_privacy)