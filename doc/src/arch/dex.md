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

```
Level 0 (MVP - NOW)
├── Atomic swaps via DAO
├── No order book
├── Bilateral price agreement
└── ZK proofs for validity

Level 1 (Future)
├── Add SMT order book
├── Hidden order commitments
└── ZK proof of order matching

Level 2 (Future)
├── Aggregate depth proofs
├── Time-bucketed volume bands
└── Differential privacy noise

Level 3 (Future)
└── Opt-in full transparency
```

## Key Components

### ZK Circuits for Atomic Swaps

**create_swap_v1.zk**: Proves proposer has locked valid funds for swap

**accept_swap_v1.zk**: Proves acceptor has locked matching funds

**execute_swap_v1.zk**: Proves both secrets known, locks valid, swap consistent

**cancel_swap_v1.zk**: Proves ownership of lock for cancellation

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
| **BaseDiv** | Division for ratio checks | Not implemented |
| **LessThanOrEqual** | Price comparison for matching | Implemented but experimental |

### The BaseDiv Absentee and Its Impact

`BaseDiv` (a / b mod p) is not implemented. This affects the DEX in several ways:

**Impact 1: Price Ratio Comparisons**

Level 1+ DEX needs to match orders by price ratio:

```zk
# Hypothetical price matching (requires BaseDiv):
alice_price = alice_amount / alice_request;
bob_price = bob_amount / bob_offer;
price_match = less_than_or_equal(alice_price, bob_price);
```

Without `BaseDiv`, we cannot compute price ratios in the circuit. The workaround:

```zk
# Cross-multiplication instead of division:
# Want: alice_request / alice_amount >= bob_offer / bob_amount
# i.e.: alice_request * bob_amount >= bob_offer * alice_amount
lhs = base_mul(alice_request, bob_amount);
rhs = base_mul(bob_offer, alice_amount);
less_than_or_equal(rhs, lhs);  # Assert rhs <= lhs
```

This works for assertion but requires careful formulation.

**Impact 2: Partial Fill Calculations**

Partial fills require computing ratios:

```zk
# Hypothetical partial fill (requires BaseDiv):
fill_ratio = fill_amount / total_amount;
filled_value = base_mul(fill_amount, fill_ratio);  # Needs division
```

The current `execute_swap_v1.zk` circuit uses safemath pattern for partial fills:

```zk
# SAFEMATH PATTERN: Assert fill_amount <= total_amount
# Pattern: prove fill_amount < total_amount + 1
ONE = witness_base(1);
total_plus_one = base_add(total_amount, ONE);
less_than_strict(fill_amount, total_plus_one);
```

### The LessThanOrEqual Experimental Status

`LessThanOrEqual` is implemented but marked experimental due to soundness concerns:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    LessThanOrEqual Soundness Concern                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Gate constraint:                                                            │
│  a_offset = out * (b - a) + (1 - out) * (a - b - 1)                       │
│  out * (1 - out) = 0  # out must be 0 or 1                                 │
│                                                                              │
│  Concern: Prover could assign out=0 and a_offset=a-b-1 incorrectly         │
│           and still pass the range check                                      │
│                                                                              │
│  Mitigation: Range check limits feasible incorrect assignments                │
│  Status: Grey-market - works for honest provers, unverified for malicious    │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

The DEX's `execute_swap_v1.zk` uses `less_than_strict` (constrain-only) instead
of `LessThanOrEqual` (returns value):

```zk
# From execute_swap_v1.zk:
# SAFEMATH WORKAROUND: This circuit uses the safemath pattern to assert
# that fill_amount <= alice_amount (the fill does not exceed Alice's offer).
#
# The safemath pattern for a <= b is: a < b + 1
ONE = witness_base(1);
alice_amount_plus_one = base_add(alice_amount, ONE);
less_than_strict(fill_amount, alice_amount_plus_one);
```

This is assertion-only (no Boolean return) but is sound because `less_than_strict`
is proven to correctly fail when a >= b.

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

### What BaseDiv Would Enable

With `BaseDiv`, the DEX could:

**1. Compute exact price ratios for order matching (Level 1+)**

```zk
# If BaseDiv existed:
alice_price = base_div(alice_request_amount, alice_offer_amount);
bob_price = base_div(bob_offer_amount, bob_request_amount);
# Compare: alice_price >= bob_price
```

**2. Compute percentage-based fees**

```zk
# Fee calculation:
fee_amount = base_div(total_amount, fee_basis_points);
# fee_amount = total * 100 / 10000  (for 1% fee)
```

**3. Compute exchange rates with precision**

```zk
# DEX to external price feed:
exchange_rate = base_div(dex_amount, external_amount);
```

### What LessThanOrEqual (production-ready) Would Enable

With `LessThanOrEqual` returning a Boolean value:

**1. Predicate-based order matching**

```zk
# If LessThanOrEqual returned 0/1:
is_match = less_than_or_equal(my_price, market_price);
constrain_instance(is_match);  # Public output
```

**2. Conditional execution based on comparison**

```zk
# Execute only if fill meets minimum:
fills_enough = less_than_or_equal(min_fill_amount, fill_amount);
execute_if_valid = cond_select(fills_enough, execute, abort);
```

**3. Complex order types**

```zk
# Stop-loss order:
stop_triggered = less_than_or_equal(stop_price, current_price);
constrain_equal_base(stop_triggered, 1);
```

### Current Trade-offs in the DEX Design

| Limitation | Workaround | Risk |
|------------|-----------|------|
| No `BaseDiv` | Cross-multiplication for ratios | Limited expressiveness |
| `LessThanOrEqual` experimental | Safemath assertion pattern | Cannot return Boolean |
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
- Safemath workarounds (native opcodes)

### Related Opcode Documentation

See [zkVM Primitive Layer](zkvm_primitives.md) for:
- Full analysis of `LessThanOrEqual` soundness concerns
- `BaseDiv` implementation status and use cases
- Why comparison opcodes are foundational to contract expressiveness

See [Private Authorization Layer](privauth.md) for:
- How the signature/authorization pattern works across contracts
- Why the split verification model exists

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
4. **How to handle partial fills with ZK?**
5. **Can we do limit orders with ZK without revealing price?**

## References

- [DarkFi DEX Contract](../../src/contract/dex/)
- [DarkFi Money Contract](./money.md)
- [DarkFi Bridge Contract](./bridge.md)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)
- [Differential Privacy](https://en.wikipedia.org/wiki/Differential_privacy)