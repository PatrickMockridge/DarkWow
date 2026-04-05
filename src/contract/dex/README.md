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

### InitializeV1 (0x00)

Initializes the DEX swap contract:
- Creates `swaps` tree for active swaps
- Creates `participants` tree for tracking locked funds (nullifiers)
- Creates `config` tree for settings
- Stores `swap_timeout` and `dex_fee` configuration

### CreateSwapV1 (0x01)

Alice creates a swap proposal:
- Computes `lock_commitment = H(secret, offer_token, offer_amount)`
- Computes `swap_id = H(lock_commitment, request_token, request_amount)`
- Verifies swap doesn't already exist
- Stores swap in `Created` state
- Stores proposer's nullifier to prevent double-spend
- Emits `SwapCreated` event (only swap_id, no amounts revealed)

### AcceptSwapV1 (0x02)

Bob accepts the swap:
- Loads swap and verifies it's in `Created` state
- Verifies swap hasn't expired
- Stores Bob's `lock_commitment` and nullifier
- Updates swap to `Accepted` state
- Emits `SwapAccepted` event

### ExecuteSwapV1 (0x03)

Execute the atomic swap:
- Verifies swap is in `Accepted` state
- Verifies ZK proof of both secrets known
- Atomically transfers: Alice gets Bob's funds, Bob gets Alice's funds
- Updates swap to `Executed` state
- Removes both nullifiers (funds transferred)
- Emits `SwapExecuted` event

### CancelSwapV1 (0x04)

Either party cancels:
- Verifies swap is in `Created` or `Accepted` state
- Verifies caller owns one of the locks via nullifier
- Refunds caller's locked funds
- Updates swap to `Cancelled` state
- Emits `SwapCancelled` event

### UpdateConfigV1 (0x05)

Update contract configuration (governance):
- Update `swap_timeout` (in blocks)
- Update `dex_fee` (in basis points)

## ZK Circuits

The DEX uses four ZK circuits for the atomic swap flow:

### create_swap_v1.zk

Proves the proposer has locked valid funds:
- **Public inputs**: lock_commitment, swap_id, swaps_root
- **Private inputs**: secret, offer_token, offer_amount, request_token, request_amount, merkle_proof
- **Verification**: Commitment valid, swap_id derived correctly, lock exists in money contract tree

### accept_swap_v1.zk

Proves the acceptor has locked matching funds:
- **Public inputs**: swap_id, lock_commitment, swaps_root
- **Private inputs**: secret, offer_token, offer_amount, merkle_proof
- **Verification**: Lock commitment valid, funds exist

### execute_swap_v1.zk

Proves both parties' secrets and locks are valid, with **partial fill support**:

- **Public inputs**: swap_id, alice_lock, bob_lock, alice_nullifier, bob_nullifier
- **Private inputs**: alice_secret, bob_secret, alice_token, alice_amount, bob_token, bob_amount, **fill_amount**
- **Verification**: Both secrets known, both locks valid, both nullifiers unspent, swap consistency, **fill_amount <= alice_amount**
- **Partial fill**: The circuit asserts that the fill amount does not exceed Alice's offered amount

#### Partial Fill Implementation

```zk
# execute_swap_v1.zk - Partial fill check using less_than_strict
#
# The circuit asserts that fill_amount <= alice_amount (fill does not exceed Alice's offer).

# Both amounts must be valid u64
range_check(64, alice_amount);
range_check(64, bob_amount);
range_check(64, fill_amount);

# Assert: fill_amount <= alice_amount
# Pattern: prove fill_amount < alice_amount + 1
ONE = witness_base(1);
alice_amount_plus_one = base_add(alice_amount, ONE);
less_than_strict(fill_amount, alice_amount_plus_one);

# Additionally, Bob's amount must be >= fill_amount
less_than_strict(fill_amount, bob_amount);
```

**Note**: The circuit uses `less_than_strict` for assertion-only checks. LessThanOrEqual (0x55) is now verified sound and could return a Boolean if needed for composability. See [Opcodes Reference](../../../doc/src/arch/opcodes.md).

### cancel_swap_v1.zk

Proves ownership of a lock for cancellation:
- **Public inputs**: swap_id, lock_commitment, nullifier
- **Private inputs**: secret, token, amount
- **Verification**: Caller knows secret, lock valid, not already spent

## Base Field Arithmetic

ZK circuits operate in a finite field — the Pallas field defined by prime `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`. All arithmetic wraps at `p`, which breaks normal integer intuitions:

```zk
# In the field, p-1 ≡ -1, so comparisons must be carefully designed
# Amount comparisons (e.g., fill_amount <= requested_amount) require this
```

**Why this matters for DEX**: Atomic swap conditions like "Alice's offered amount >= Bob's requested amount" require comparing field elements as integers. The field wraparound means naive comparison can give incorrect results when values are near `p`.

**The core challenge**: Proving `a <= b` as integers requires determining whether `a - b` falls in `{0, 1, ..., (p-1)/2}` or `{(p+1)/2, ..., p-1}`. This is straightforward in normal code but requires careful gadget design in circuits.

**For DEX specifically**: Partial fills (filling only part of a swap) require comparing `fill_amount <= swap_amount`. This is handled via bounded comparisons using `less_than_strict`.

**See**:
- [Field Arithmetic Constraints](../../../doc/src/arch/field_arithmetic.md) for the full treatment
- [Opcodes Reference](../../../doc/src/arch/opcodes.md) for LessThanOrEqual verification status

## Opcode Discovery and Validation

**Opcode discovery must go hand-in-hand with building functionality** — not precede it.

When building the DEX contract, we discovered that:
1. The atomic swap flow (`CreateSwap → AcceptSwap → ExecuteSwap`) works with existing opcodes — no comparison needed
2. But partial fills require `LessThanOrEqual` to compare `fill_amount <= swap_amount`
3. The "require on fill, bypass on cancel" pattern requires `IsEqualBase` for equality checks
4. These gaps only became apparent when trying to extend from atomic swaps to partial fills

**The correct workflow**:
1. Build the circuit with what exists
2. When a constraint can't be expressed, document the opcode gap
3. Implement the new opcode only when the actual use case is known
4. Validate the opcode against the specific circuit that needs it — not in isolation

The DEX contract started with atomic swaps (which don't need comparison opcodes). Partial fills are a future enhancement.

## Opcode Status

**LessThanOrEqual (0x55)** is now **verified sound** via Lean 4 exhaustive testing.

**BaseDiv (0x58)** is now **implemented** using binary exponentiation (Fermat's theorem).

| Opcode | Status | Use in DEX |
|--------|--------|------------|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Partial fill comparisons |
| `BaseDiv` (0x58) | ✅ Implemented | Ratio calculations |
| `less_than_strict` | ✅ Sound | Bounded comparisons |
| `IsEqualBase` (0x54) | ⚠️ Bug | Do not use — delta_invert unconstrained when a==b |

**Historical note**: This section previously described LessThanOrEqual as having "technical debt" and safemath as a "workaround". The safemath pattern is retained for legacy reference. See [Safemath](../../../doc/src/arch/safemath.md) and [Opcodes Reference](../../../doc/src/arch/opcodes.md).

### Adding Custom Opcodes

To add a new opcode to the zkVM:

1. Define the opcode in `src/zkas/opcode.rs`
2. Implement the opcode in `src/zk/vm.rs`

For a full example of adding opcodes, see the [zkas bincode documentation](../../../doc/src/zkas/bincode.md).

## Opcode Safety

**Comparison opcodes status**:

| Opcode | Status | Use in DEX | Note |
|--------|--------|------------|------|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Partial fill comparisons | Returns Boolean, full composability |
| `BaseDiv` (0x58) | ✅ Implemented | Ratio calculations | Binary exponentiation |
| `less_than_strict` | ✅ Sound | ✅ Used in circuits | Constrain-only |
| `IsEqualBase` (0x54) | ❌ Bug | Do not use | delta_invert unconstrained when a==b |

See [Opcodes Reference](../../../doc/src/arch/opcodes.md) for verification details.

## Key Blockers

| Blocker | Severity | Description |
|---------|----------|-------------|
| Manual matching required | **High** | `CreateSwap → AcceptSwap → ExecuteSwap` requires third party to call ExecuteSwap |
| Slippage tolerance | **Medium** | BaseDiv enables slippage verification |
| No integration test | **High** | Full atomic swap flow not tested end-to-end |
| SMT order book | **Future** | Needs solver/oracle for matching |

## Roadmap: From MVP to Full Order Book

```
Level 0 (MVP)          Level 1              Level 2              Level 3
─────────────────────────────────────────────────────────────────────────
Atomic swaps only      SMT order book       ZK matching          Differential privacy
Bilateral agreement    Hidden commitments   Anonymized trades    Aggregate market data
No price discovery     ZK proof of match    Unlinkable           Noise for protection
                       Partial fills        Partial fills        Full opt-in transparency
```

### Why This Roadmap?

**Level 0 (NOW)**: Atomic swaps via DAO
- Minimal blast radius for bugs
- Auditable core logic
- No information leakage
- Foundation for everything else

**Level 1 (Future)**: Add SMT order book
- Orders stored as Pedersen commitments
- Solver matches orders via ZK proofs
- No price/amount revealed until match
- Challenge: Encrypted search / ORAM for queries

**Level 2 (Future)**: Anonymized market data
- Differential privacy for aggregate stats
- Time-bucketed volume with noise
- Price bands instead of exact prices
- LPs can model risk without seeing orders

**Level 3 (Future)**: Opt-in transparency
- Traders can choose to reveal their trades
- For regulatory compliance
- Without affecting others' privacy

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
- [x] MVP function definitions (CreateSwap, AcceptSwap, ExecuteSwap, CancelSwap, UpdateConfig)
- [x] Atomic swap DAO implementation (full on-chain logic)
- [x] ZK circuits for swap verification (create_swap, accept_swap, execute_swap, cancel_swap)
- [x] Partial fill support via safemath (fill_amount <= alice_amount)
- [x] Open execution for automatic matching (open_execution + immediate_execute)
- [ ] Integration with money contract for actual token transfers (future)
- [ ] Order book expansion (Level 1 future)

## Level 0 MVP: Complete

**Atomic swap flow is complete** with two execution modes:

### Standard Atomic Swap (Default)
```
1. Alice creates swap (locks funds, holds secret)
2. Bob accepts swap (locks funds)
3. Alice or Bob calls ExecuteSwap (both secrets needed)
```

### Open Execution (Instant Fill)
```
1. Alice creates swap with open_execution=true (locks funds, secret public)
2. Bob accepts with immediate_execute=true
3. Swap executes automatically in same transaction!
```

**Trade-off**: Open execution reveals Alice's secret to network. Use only with trusted counterparties.

| Circuit | Status |
|---------|--------|
| `create_swap_v1.zk` | ✅ Verified |
| `accept_swap_v1.zk` | ✅ Verified |
| `execute_swap_v1.zk` | ✅ Verified with partial fill (LTE) |
| `cancel_swap_v1.zk` | ✅ Verified |

## Partial Fill Support

The `execute_swap_v1.zk` circuit asserts `fill_amount <= alice_amount` using **verified LessThanOrEqual** (0x55):

```zk
# Using verified LessThanOrEqual (returns Boolean)
is_lte = less_than_or_equal(fill_amount, alice_amount);
constrain_equal_base(is_lte, ONE);
```

LessThanOrEqual is now **verified sound** via Lean 4 - no more safemath workaround needed.

## New Capabilities with BaseDiv

With **BaseDiv (0x58) implemented**, the DEX can now support sophisticated market making:

### Slippage Tolerance

```zk
# Verify: received >= min_expected * (1 - slippage_tolerance)
# received = fill_amount * exchange_rate
# min_expected = wanted_amount * (1 - slippage_bps / 10000)

# Using BaseDiv:
tolerance_multiplier = base_div(10000 - slippage_bps, 10000);
min_acceptable = base_mul(wanted_amount, tolerance_multiplier);
# Verify: received >= min_acceptable
less_than_or_equal(min_acceptable, received);
```

### Fee Calculation

```zk
# Calculate fee: amount * fee_bps / 10000
fee = base_div(base_mul(amount, fee_bps), witness_base(10000));
```

### Exchange Rate Bounds

```zk
# Verify exchange rate is within agreed bounds
# rate = given / wanted
actual_rate = base_div(given, wanted);
# Verify: rate_lower <= actual_rate <= rate_upper
less_than_or_equal(rate_lower, actual_rate);
less_than_or_equal(actual_rate, rate_upper);
```

## Future: Level 1 (Order Book)

| Feature | Status | Notes |
|---------|--------|-------|
| Atomic swaps | ✅ Done | None |
| Partial fills | ✅ Done | LessThanOrEqual verified |
| Open execution | ✅ Done | None |
| Slippage tolerance | 🔨 Now | BaseDiv enables |
| Fee calculations | 🔨 Now | BaseDiv enables |
| SMT order book | ❌ Future | Needs solver/oracle |
| Price discovery | ❌ Future | Encrypted search |
| Differential privacy | ❌ Future | Research |

**See**: [Contract MVP Status](../../../doc/src/arch/mvp_status.md) for the full cross-contract analysis.

## References

- [DarkFi DEX Architecture Document](../../../doc/src/arch/dex.md)
- [DarkFi Money Contract](../money/)
- [DarkFi Bridge Contract](../bridge/)
- [Contract MVP Status](../../../doc/src/arch/mvp_status.md)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)

## Betfair Exchange

The DEX can be extended to power a **Decentralized Betfair Exchange**:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    BETFAIR EXCHANGE CONCEPT                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Components:                                                                │
│  - DEX: Match back/lay orders at agreed odds                                 │
│  - BettingStake: LP pool for settlement guarantee                           │
│  - Oracle: Resolve event outcomes                                            │
│  - DAO-Escrow: Commission treasury, governance, disputes                    │
│                                                                              │
│  How it works:                                                              │
│  1. User A backs "Team A wins" @ 2.5 with 100 tokens                        │
│  2. User B lays "Team A wins" @ 2.4 with 104 tokens                        │
│  3. DEX matches: execution at 2.4 (lay price)                              │
│  4. Oracle resolves: Team A wins                                            │
│  5. User A wins: 240 from User B (minus commission)                         │
│  6. Exchange earns 2% commission                                             │
│                                                                              │
│  Key insight: Exchange matches users, doesn't bet against them.            │
│  LP pool only guarantees settlement, doesn't carry outcome risk.            │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

See [Betfair Exchange Concept](../../../doc/src/arch/bet_exchange.md) for full architecture.