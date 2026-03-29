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

Proves both parties' secrets and locks are valid:
- **Public inputs**: swap_id, alice_lock, bob_lock, alice_nullifier, bob_nullifier
- **Private inputs**: alice_secret, bob_secret, alice_token, alice_amount, bob_token, bob_amount
- **Verification**: Both secrets known, both locks valid, both nullifiers unspent, swap consistency

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

**For DEX specifically**: Partial fills (filling only part of a swap) would require comparing `fill_amount <= swap_amount`. This can be addressed via:
- **Safemath** (current): `assert_lte_u64_v1.zk` template for assertion-only checks
- **Native opcode** (future): `LessThanOrEqual` with Boolean return for full composability

**See**:
- [Field Arithmetic Constraints](../../../doc/src/arch/field_arithmetic.md) for the full treatment
- [Safemath](../../../doc/src/arch/safemath.md) for the safemath workaround

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

The DEX contract started with atomic swaps (which don't need comparison opcodes) to avoid the `LessThanOrEqual` gap. Partial fills — which would require the opcode — are a future enhancement.

## Reasoned Opcodes

The DEX circuits primarily use standard zkVM opcodes. Future expansions may require:

### `LessThanOrEqual(a, b)` (Reasoned)
**Purpose**: Compare if `Base a <= Base b`
**Reasoning**: Could enable minimum amount checks, swap size limits, or partial fill conditions.
**Alternative**: [Safemath assert_lte](https://codeberg.org/rusticml/darkfi-safemath) templates available for assertion-only use.

### `IsEqualBase(a, b)` (Reasoned)
**Purpose**: Returns `0` or `1` for equality comparison
**Reasoning**: From intent-amm experimentation. Enables "require this on fill, bypass it on cancel" logic.

### Adding Custom Opcodes

To add a new opcode to the zkVM:

1. Define the opcode in `src/zkas/opcode.rs`
2. Implement the opcode in `src/zk/vm.rs`

For a full example of adding opcodes, see the [zkas bincode documentation](../../../doc/src/zkas/bincode.md).

## Opcode Safety

**Comparison opcodes status**:

| Opcode | Status | Use in DEX |
|--------|--------|------------|
| `LessThanOrEqual` | Implemented (experimental) | Would enable partial fills via safemath or native opcode |
| `IsEqualBase` | Implemented (experimental) | Would enable intent fill conditions |
| `less_than_strict` | Sound (constrain-only) | ✅ Used in circuits |

**The DEX's current status**: The current circuits use `constrain_equal_base` and `range_check`, not comparison opcodes. Atomic swaps work without `LessThanOrEqual`. Partial fills can use safemath assertion gadgets.

**See**:
- [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for the full analysis
- [Safemath](../../../doc/src/arch/safemath.md) for the workaround templates

## Key Blockers

| Blocker | Severity | Description |
|---------|----------|-------------|
| Manual matching required | **High** | `CreateSwap → AcceptSwap → ExecuteSwap` requires third party to call ExecuteSwap |
| No amount comparison | **High** | `execute_swap_v1.zk` doesn't compare Alice's offered vs Bob's requested amounts |
| No partial fills | **Medium** | Would require safemath or native `LessThanOrEqual` for amount checks |
| No integration test | **High** | Full atomic swap flow not tested end-to-end |

**Note on LessThanOrEqual**: Partial fills can use safemath assertion gadgets for bounded amount checks. Native opcode (when formally verified) would enable full Boolean return for composability. See [Safemath](../../../doc/src/arch/safemath.md).

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
- [ ] Integration with money contract for actual token transfers
- [ ] Order book expansion (future)

## MVP Status

**Partial MVP** — atomic swap flow works, but automatic order matching is not yet implemented.

| Circuit | Status |
|---------|--------|
| `create_swap_v1.zk` | Verified |
| `accept_swap_v1.zk` | Verified |
| `execute_swap_v1.zk` | Verified |
| `cancel_swap_v1.zk` | Verified |

### Blockers

1. **Manual matching required** — The `CreateSwap → AcceptSwap → ExecuteSwap` flow requires a third party to call `ExecuteSwap` after both locks are posted. There is no automatic order matching.
2. **No amount comparison** — `execute_swap_v1.zk` verifies amounts are valid (`range_check(64, amount)`) but does not compare Alice's offered amount against Bob's requested amount.
3. **No partial fills** — If a swap is posted for 100 tokens but someone wants to fill only 50, there is no mechanism. Implementing this would require `LessThanOrEqual` (0x55), which is implemented but **experimental** — see [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md).

### What It Needs

Either document the atomic swap matching flow explicitly, or update circuits to use `LessThanOrEqual` for amount comparison and partial fills. Note that `LessThanOrEqual` is grey-market — see the experimental status section in [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md).

**See**: [Contract MVP Status](../../../doc/src/arch/mvp_status.md) for the full cross-contract analysis.

## References

- [DarkFi DEX Architecture Document](../../../doc/src/arch/dex.md)
- [DarkFi Money Contract](../money/)
- [DarkFi Bridge Contract](../bridge/)
- [Contract MVP Status](../../../doc/src/arch/mvp_status.md)
- [SPV Privacy Problem](https://en.bitcoin.it/wiki/Thin_Client_Security)