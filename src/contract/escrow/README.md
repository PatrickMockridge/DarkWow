# DarkFi Escrow Contract

Privacy-preserving conditional payment contract. Funds are locked in a commitment and released to the seller upon proof of knowledge of a secret, or returned to the buyer after a timeout.

## The Problem: Trust in Commerce

Traditional escrow requires a trusted third party:
- **Bank/Escrow Service**: Holds funds, releases on dispute resolution
- **Problem**: Single point of failure, privacy violation, counterparty risk
- **Alternative**: Smart contracts, but public visibility reveals sensitive terms

**What if you could have trustless escrow with privacy?**

## Our Solution: Hashed Timelock Escrow

```
┌─────────────────────────────────────────────────────────────────────┐
│                  Escrow Contract Flow                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   BUYER                                    SELLER                     │
│      │                                        │                      │
│      │  1. Create Escrow                      │                      │
│      │     value + timeout                     │                      │
│      │──────────────────────────────→         │                      │
│      │                                        │                      │
│      │  2. Fund Escrow                        │                      │
│      │     (locks funds in commitment)         │                      │
│      │──────────────────────────────→         │                      │
│      │                                        │                      │
│      │                    3. Claim Funds     │                      │
│      │                    (proves knowledge  │                      │
│      │                     of seller_secret)  │                      │
│      │←──────────────────────────────────────│                      │
│                                                                       │
│   BUYER (fallback)                          SELLER                     │
│      │                                        │                      │
│      │  4. Refund (after timeout)            │                      │
│      │     (proves current_block >= timeout) │                      │
│      │←──────────────────────────────────────│                      │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

## Trust Model: Hashed Timelock (Variant 3)

Three variants exist in the wild. DarkFi uses Variant 3:

| Variant | Claim Mechanism | Refund Mechanism | Trust Model |
|---------|-----------------|------------------|-------------|
| **V1: Secret Hash** | Reveal secret R | Timeout only | Seller must reveal R to claim |
| **V2: Hashlock** | H(R) = preimage | Timeout only | Atomic swap via hash preimage |
| **V3: Public Key** | Prove knowledge of seller_secret | Timeout + buyer_secret | No secret reveal required |

**Why Variant 3?**
- Seller claims without revealing secret (unlike V1)
- Buyer can always recover after timeout (unlike V2)
- Both parties have cryptographic guarantees

## Privacy Properties

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| Escrow exists (commitment) | Value (in Pedersen commitment) |
| Claim or refund occurred (nullifier) | Which party claimed |
| Timeout passed (block check) | Actual amounts |
| Parties (public keys derived from secrets) | Real identities |

## State Machine

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Escrow State Machine                             │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   Created ──[Fund]──> Funded ──[Claim]──> Claimed                    │
│     │                 │                │                            │
│     │                 │                └──[Refund]──> Refunded       │
│     │                 │                                             │
│     │                 └──[Cancel]──> Cancelled                        │
│     │                                                                   │
│     └── (timeout never reached)                                        │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

| State | Description | Who Can Transition |
|-------|-------------|-------------------|
| **Created** | Escrow created, not yet funded | Buyer → Fund or Cancel |
| **Funded** | Value locked in commitment | Seller → Claim, Buyer → Refund (after timeout) |
| **Claimed** | Seller claimed funds | Terminal state |
| **Refunded** | Buyer refunded after timeout | Terminal state |
| **Cancelled** | Buyer cancelled before funding | Terminal state |

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| InitializeV1 | 0x00 | Initialize escrow contract state |
| CreateEscrowV1 | 0x01 | Create new escrow commitment |
| FundV1 | 0x02 | Fund escrow with Pedersen commitment |
| ClaimV1 | 0x03 | Seller claims funds with secret proof |
| RefundV1 | 0x04 | Buyer refunds after timeout |
| CancelV1 | 0x05 | Buyer cancels before funding |

## ZK Circuits

### create_escrow_v1.zk

Proves the escrow commitment is correctly formed:
- **Public inputs**: `commitment = H(buyer_pub.x, buyer_pub.y, H(seller_pub), value, token_id, timeout)`
- **Private inputs**: `buyer_pub_x, buyer_pub_y, seller_pub_x, seller_pub_y, value, token_id, timeout, buyer_secret`
- **Verification**: Public key derivation + commitment hash
- **Privacy**: `H(seller_pub)` hides seller_pub on-chain

### fund_v1.zk

Proves the value commitment is valid:
- **Public inputs**: `escrow_id`, `value_commit.x`, `value_commit.y`
- **Private inputs**: `value`, `value_blind`
- **Verification**: Pedersen commitment `C = value * G + value_blind * H`

### claim_v1.zk

Proves the seller legitimately claims funds:
- **Public inputs**: `escrow_id`, `seller_commitment = H(seller_pub)`, `spent_nullifier`
- **Private inputs**: `seller_secret`
- **Verification**:
  1. `seller_pub = seller_secret * G` internally verified against `seller_commitment`
  2. `spent_nullifier = H(escrow_id, seller_secret)`
- **Privacy**: seller_pub is NOT exposed on-chain (verified via poseidon_hash)

### refund_v1.zk

Proves the buyer legitimately refunds:
- **Public inputs**: `escrow_id`, `timeout`, `current_block`, `buyer_pub_x`, `buyer_pub_y`, `spent_nullifier`
- **Private inputs**: `buyer_secret`
- **Verification**:
  1. `less_than_strict(timeout, current_block)` — timeout passed
  2. `buyer_pub = buyer_secret * G` matches escrow.buyer_pubkey
  3. `spent_nullifier = H(escrow_id, buyer_secret)`

## Opcode Requirements

**Good news: No new opcodes needed for MVP!**

| Circuit | Opcodes Used | Status |
|---------|-------------|--------|
| `create_escrow_v1.zk` | `poseidon_hash`, `ec_mul_base`, `ec_get_x`, `ec_get_y`, `constrain_eq` | Existing |
| `fund_v1.zk` | `ec_mul_short`, `ec_mul`, `ec_add`, `ec_get_x`, `ec_get_y` | Existing |
| `claim_v1.zk` | `ec_mul_base`, `poseidon_hash`, `constrain_eq` | Existing |
| `refund_v1.zk` | `less_than_strict`, `ec_mul_base`, `poseidon_hash`, `constrain_eq` | Existing (constrain-only) |

The `refund_v1.zk` circuit uses `less_than_strict(timeout, current_block)` which is a **constrain-only** opcode — it constrains `current_block > timeout` without producing a usable output value. This is sufficient because the block proposer reveals `current_block` and the circuit simply verifies it's greater than `timeout`.

## Base Field Arithmetic

ZK circuits operate in a finite field — the Pallas field defined by prime `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`. All arithmetic wraps at `p`, which breaks normal integer intuitions:

```zk
# In the field, p-1 ≡ -1, so comparisons must be carefully designed
# Timeout checks (current_block > timeout) must handle field wraparound
```

**Why this matters for escrow**: The timeout check `current_block > timeout` is a comparison. In normal code, this is trivial. In a ZK circuit, `current_block - timeout` as field subtraction is not the same as integer subtraction when `current_block < timeout` (field wraps around).

**The escrow's approach**: The `refund_v1.zk` circuit uses `less_than_strict(timeout, current_block)` which is a **constrain-only** opcode — it fails the circuit if `current_block <= timeout`, but doesn't return a value. This is sufficient because:
1. The block proposer reveals `current_block` (authenticated by consensus)
2. We only need to verify the relation is satisfied, not compute anything from it
3. No field wraparound concern because we're constraining, not computing

**The key insight**: Not every comparison needs to return a value. If you only need to verify `a < b` holds (not compute a result from it), the constrain-only opcodes are sufficient and sound.

**See**: [Field Arithmetic Constraints](../../../doc/src/arch/field_arithmetic.md) for the full treatment.

## Opcode Discovery and Validation

**Opcode discovery must go hand-in-hand with building functionality** — not precede it.

When building the escrow contract, we discovered that:
1. Commitment creation uses `poseidon_hash` and `ec_mul_base` — both existing opcodes
2. The timeout check uses `less_than_strict` — a constrain-only opcode, sufficient for the use case
3. No comparison opcode that returns a value is needed — the escrow doesn't need to compare amounts or compute from comparisons
4. This simplicity was discovered during implementation, not anticipated

**The correct workflow**:
1. Build the circuit with what exists
2. When a constraint can't be expressed, document the opcode gap
3. Implement the new opcode only when the actual use case is known
4. Validate the opcode against the specific circuit that needs it — not in isolation

The escrow contract is notable because it required **no new opcodes at all**. The timeout check uses `less_than_strict` correctly — constraining the relation without needing a return value.

## Reasoned Opcodes

### `LessThanStrict(a, b)` (Constrain-Only)

**Purpose**: Constrains `a < b` without returning a value
**Used in**: `refund_v1.zk` for timeout verification

**Why it's sufficient here**:
The refund circuit only needs to verify `current_block > timeout`. It doesn't need to compute or return how much time remains. The `LessThanStrict` opcode constrains the relation without producing an output — exactly what we need.

**See also**: [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for the full mathematical reasoning behind comparison opcodes.

## Opcode Safety

**Escrow has no grey-market opcode concerns — it uses only proven opcodes.**

The escrow contract uses only opcodes that are well-established in the zkVM:

| Opcode | Status |
|--------|--------|
| `poseidon_hash` | Proven — widely used, well-audited |
| `ec_mul_base` | Proven — standard EC operation |
| `ec_mul_short` | Proven — Pedersen commitment basis |
| `ec_add` | Proven — EC point addition |
| `constrain_eq` | Proven — equality constraint |
| `less_than_strict` | Proven — constrain-only, sound by design |

**No experimental opcodes required.** The escrow's simplicity is a strength — its security doesn't depend on unproven comparison opcodes.

## Key Blockers

| Blocker | Severity | Description |
|---------|----------|-------------|
| ZK circuit compilation | **High** | `.zk` files need compilation to `.zk.bin` via zkas |
| ZK proof verification | **High** | Wire proofs into `get_metadata()` |
| Money integration (Phase 2) | **Medium** | Spend_hook integration with money contract |

**Escrow has no opcode blockers** — unlike other contracts, escrow's requirements are fully satisfied by existing opcodes.

## Use Cases

### Trustless OTC Trade
```rust
// Alice wants to sell 1000 DARK for 1 ETH to Bob
// Neither trusts the other

// Step 1: Alice creates escrow
let escrow = CreateEscrowBuilder::new()
    .buyer_pubkey(bob_pubkey)
    .seller_pubkey(alice_pubkey)
    .value(1000)
    .token_id(DARK_TOKEN_ID)
    .timeout(current_block + 1000)  // ~1 week
    .build()?;

// Step 2: Alice funds escrow (locks 1000 DARK)

// Step 3a: Alice claims — proves she knows seller_secret
//           Gets the 1000 DARK

// OR Step 3b: After timeout, Bob refunds — proves timeout passed
//             Gets the 1000 DARK back
```

### Conditional Payment
```rust
// Contractor delivers work to Client
// Payment released on proof of delivery

// Step 1: Client creates escrow for contract amount
// Step 2: Client funds escrow
// Step 3: Contractor delivers (off-chain)
// Step 4: Client approves → Contractor claims
// OR Step 4: Dispute → Timeout → Client refunded
```

### Time-Locked Gift
```rust
// Parent locks funds for child, accessible after age 18

// Step 1: Parent creates escrow with child as buyer
// Step 2: Parent funds escrow
// Step 3: After timeout (when child is 18), child refunds to themselves
```

## Architecture

```
escrow/
├── proof/                    # ZK proof circuits (.zk files)
│   ├── create_escrow_v1.zk  # Commitment creation
│   ├── fund_v1.zk           # Value commitment
│   ├── claim_v1.zk          # Seller claim
│   └── refund_v1.zk         # Buyer refund (with timeout)
├── src/
│   ├── client/
│   │   └── mod.rs           # Builder structs (CreateEscrowBuilder, etc.)
│   ├── entrypoint.rs         # WASM entrypoint
│   ├── error.rs              # EscrowError enum
│   ├── lib.rs                # Contract definitions
│   └── model/
│       └── mod.rs            # Data structures
├── Cargo.toml
└── README.md
```

## Building

```bash
# Build WASM contract
cargo build -p darkfi_escrow_contract

# Compile ZK circuits (requires zkas binary)
make proof

# Run tests
cargo test -p darkfi_escrow_contract
```

## Integration with Money Contract

The escrow contract manages its own value commitments but integrates with the Money contract for token operations:

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Integration Architecture                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   Money Contract                                                     │
│   ├── Owns coin ledger (coins, nullifiers, Merkle tree)             │
│   ├── Issues tokens (Mint/Burn)                                      │
│   └── Transfer semantics                                              │
│                                                                       │
│   Escrow Contract                                                    │
│   ├── Owns escrow state machine (Created → Funded → Claimed/Refunded)│
│   ├── Verifies ZK proofs for claim/refund                            │
│   └── Emits spend_hook calls to Money for fund release               │
│                                                                       │
│   Flow:                                                               │
│   1. User creates escrow + funds via Money::Transfer                 │
│   2. Escrow::Claim → spend_hook → Money::Burn (consumes coin)        │
│                    + Money::Mint (mints to seller)                   │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Standalone (MVP)
- Escrow manages its own value pool
- No money contract integration
- Simplified trust model

### Phase 2: Full Integration
- Uses Money contract's `spend_hook` mechanism
- Funds locked as Money contract coins
- Claim/refund triggers atomic burn+mint

## Security Considerations

### Double-Spend Prevention
The `spent_nullifier = H(escrow_id, secret)` ensures:
- Claim and Refund are mutually exclusive
- Only one of them can succeed
- The first one to be finalized wins

### Timeout Integrity
`LessThanStrict(timeout, current_block)` ensures:
- Buyer cannot refund before timeout
- Seller cannot be prevented from claiming before timeout
- Block proposers cannot manipulate `current_block` (they're authenticated)

### Value Privacy
Pedersen commitment `C = value * G + blind * H` ensures:
- On-chain value is hidden
- Commitment is binding (can't change value after commit)
- Opening the commitment reveals value + blind

## Comparison

| Feature | Traditional Escrow | Smart Contract (Public) | DarkFi Escrow |
|---------|-------------------|------------------------|---------------|
| Privacy | Full privacy | Zero (terms visible) | Full (commitment only) |
| Trust | Third party | Trustless, public | Trustless, private |
| Customizability | Limited | Full | Full |
| Counterparty risk | High (escrow hack) | Zero | Zero |
| Auditability | Limited | Full | Partial (ZK-verified) |

## MVP Status

**Core Implementation Complete** — Entrypoint written with state transitions.

| Component | Status | Notes |
|---------|--------|-------|
| Entrypoint (`entrypoint.rs`) | ✅ Complete | init, get_metadata, process_instruction, process_update |
| State Machine | ✅ Complete | Created → Funded → Claimed/Refunded/Cancelled |
| ZK Circuits | ⚠️ Stubs | .zk files need compilation via zkas |
| Money Integration | ❌ TODO | Phase 2 spend_hook integration |

### What It Needs

1. **ZK Circuit Compilation**: Convert `.zk` files to `.zk.bin` using zkas
2. **ZK Proof Verification**: Wire ZK proof verification into `get_metadata()`
3. **Money Integration**: Phase 2 spend_hook integration

### No Opcode Blockers

Unlike other contracts, escrow has **no opcode blockers**. All required functionality (`poseidon_hash`, `ec_mul_base`, `less_than_strict`) already exists in the zkVM.

**See**: [Escrow Contract MVP Analysis](../../../doc/src/arch/escrow.md) for the full technical analysis.

## References

- [DarkFi Escrow MVP Analysis](../../../doc/src/arch/escrow.md)
- [DarkFi Money Contract](../money/)
- [DarkFi DAO Contract](../dao/)
- [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md)
- [Contract MVP Status](../../../doc/src/arch/mvp_status.md)
- [Field Arithmetic Constraints](../../../doc/src/arch/field_arithmetic.md)
