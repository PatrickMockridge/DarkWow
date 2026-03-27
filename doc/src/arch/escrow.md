# Escrow Contract MVP

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
- **Public inputs**: `commitment = H(buyer_pub.x, buyer_pub.y, seller_pub.x, seller_pub.y, value, token_id, timeout)`
- **Private inputs**: `buyer_pub_x, buyer_pub_y, seller_pub_x, seller_pub_y, value, token_id, timeout, buyer_secret`
- **Verification**: Public key derivation + commitment hash

### fund_v1.zk

Proves the value commitment is valid:
- **Public inputs**: `escrow_id`, `value_commit.x`, `value_commit.y`
- **Private inputs**: `value`, `value_blind`
- **Verification**: Pedersen commitment `C = value * G + value_blind * H`

### claim_v1.zk

Proves the seller legitimately claims funds:
- **Public inputs**: `escrow_id`, `seller_pub_x`, `seller_pub_y`, `spent_nullifier`
- **Private inputs**: `seller_secret`
- **Verification**:
  1. `seller_pub = seller_secret * G` matches escrow.seller_pubkey
  2. `spent_nullifier = H(escrow_id, seller_secret)`

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

## Reasoned Opcodes

### `LessThanStrict(a, b)` (Constrain-Only)

**Purpose**: Constrains `a < b` without returning a value
**Used in**: `refund_v1.zk` for timeout verification

**Why it's sufficient here**:
The refund circuit only needs to verify `current_block > timeout`. It doesn't need to compute or return how much time remains. The `LessThanStrict` opcode constrains the relation without producing an output — exactly what we need.

**See also**: [zkVM Primitive Layer](./zkvm_primitives.md) for the full mathematical reasoning behind comparison opcodes.

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

The escrow contract source is in `src/contract/escrow/`. See the contract [README](../../src/contract/escrow/README.md) for the full architecture.

```
src/contract/escrow/
├── proof/                    # ZK proof circuits (.zk files)
│   ├── create_escrow_v1.zk  # Commitment creation
│   ├── fund_v1.zk           # Value commitment
│   ├── claim_v1.zk          # Seller claim
│   └── refund_v1.zk         # Buyer refund (with timeout)
├── src/
│   ├── client/              # Builder structs
│   ├── entrypoint.rs        # WASM entrypoint
│   ├── error.rs             # Error types
│   ├── lib.rs               # Contract definitions
│   └── model/               # Data structures
└── README.md
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
│   2. Escrow::Claim → spend_hook → Money::Burn (consumes coin)      │
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

**Partial MVP** — Core structure exists, ZK circuits are placeholder stubs.

| Circuit | Status | Notes |
|---------|--------|-------|
| `create_escrow_v1.zk` | Placeholder | Uses existing opcodes, needs full implementation |
| `fund_v1.zk` | Placeholder | Pedersen commitment, needs merkle integration |
| `claim_v1.zk` | Placeholder | Key derivation + nullifier, needs full ZK wiring |
| `refund_v1.zk` | Placeholder | LessThanStrict + key derivation, needs full ZK wiring |

### What It Needs

1. **ZK Circuit Compilation**: Convert `.zk` files to `.zk.bin` using zkas
2. **Entry Point Implementation**: Wire ZK proof verification into `get_metadata()`
3. **State Management**: Implement actual escrow state transitions in `process_update()`
4. **Money Integration**: Phase 2 spend_hook integration

### No Blockers

Unlike other contracts, escrow has **no opcode blockers**. All required functionality (`poseidon_hash`, `ec_mul_base`, `less_than_strict`) already exists in the zkVM.

## References

- [DarkFi Escrow README](../../src/contract/escrow/README.md)
- [DarkFi DAO Contract](./dao.md)
- [DarkFi Money Contract](../spec/contract/money/money.md)
- [zkVM Primitive Layer](./zkvm_primitives.md)
- [Contract MVP Status](./mvp_status.md)
- [Field Arithmetic Constraints](./field_arithmetic.md)
