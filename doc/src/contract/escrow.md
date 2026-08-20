# Escrow Contract MVP

Privacy-preserving conditional payment contract. Funds are locked in a commitment and released to the seller upon proof of knowledge of a secret, or returned to the buyer after a timeout.

## Box + Purse Composition

The escrow composes with two genesis O-Cap primitives:
- **Purse**: Fund calls `Purse::DepositV1` as a child call to lock the escrowed amount. The Purse contract tracks the balance via Pedersen commitment — the escrow no longer needs its own value arithmetic.
- **Box** (×2): The seller holds a claim Box; the buyer holds a refund Box. Claim calls `Box::TakeV1` to consume the seller's capability. Refund calls `Box::TakeV1` for the buyer. The Box contract handles nullifier replay internally.

See [Purse](purse.md) and [Box](box.md) for the genesis primitives.

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

Three variants exist in the wild. DarkWow uses Variant 3:

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

### create_escrow.zk

Proves the escrow commitment is correctly formed:
- **Public inputs**: `commitment = H(buyer_pub.x, buyer_pub.y, seller_pub.x, seller_pub.y, value, asset_id, timeout)`
- **Private inputs**: `buyer_pub_x, buyer_pub_y, seller_pub_x, seller_pub_y, value, asset_id, timeout, buyer_secret`
- **Verification**: Public key derivation + commitment hash

### fund.zk

Proves the value commitment is valid:
- **Public inputs**: `escrow_id`, `value_commit.x`, `value_commit.y`
- **Private inputs**: `value`, `value_blind`
- **Verification**: Pedersen commitment `C = value * G + value_blind * H`

### claim.zk

Proves the seller legitimately claims funds:
- **Public inputs**: `escrow_id`, `seller_pub_x`, `seller_pub_y`, `spent_nullifier`
- **Private inputs**: `seller_secret`
- **Verification**:
  1. `seller_pub = seller_secret * G` matches escrow.seller_pubkey
  2. `spent_nullifier = H(escrow_id, seller_secret)`

### refund.zk

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
| `create_escrow.zk` | `poseidon_hash`, `ec_mul_base`, `ec_get_x`, `ec_get_y`, `constrain_equal_base` | Existing |
| `fund.zk` | `ec_mul_short`, `ec_mul`, `ec_add`, `ec_get_x`, `ec_get_y` | Existing |
| `claim.zk` | `ec_mul_base`, `poseidon_hash`, `constrain_equal_base` | Existing |
| `refund.zk` | `less_than_strict`, `ec_mul_base`, `poseidon_hash`, `constrain_equal_base` | Existing (constrain-only) |

The `refund.zk` circuit uses `less_than_strict(timeout, current_block)` which is a **constrain-only** opcode — it constrains `current_block > timeout` without producing a usable output value. This is sufficient because the block proposer reveals `current_block` and the circuit simply verifies it's greater than `timeout`.

## Reasoned Opcodes

### `LessThanStrict(a, b)` (Constrain-Only)

**Purpose**: Constrains `a < b` without returning a value
**Used in**: `refund.zk` for timeout verification

**Why it's sufficient here**:
The refund circuit only needs to verify `current_block > timeout`. It doesn't need to compute or return how much time remains. The `LessThanStrict` opcode constrains the relation without producing an output — exactly what we need.

**See also**: [zkVM Primitive Layer](../arch/zk/zkvm_primitives.md) for the full mathematical reasoning behind comparison opcodes.

## Use Cases

### Trustless OTC Trade
```rust
// Alice wants to sell 1000 DRKW for 1 ETH to Bob
// Neither trusts the other

// Step 1: Alice creates escrow
let escrow = CreateEscrowBuilder::new()
    .buyer_pubkey(bob_pubkey)
    .seller_pubkey(alice_pubkey)
    .value(1000)
    .asset_id(DRKW_ASSET_ID)
    .timeout(current_block + 1000)  // ~1 week
    .build()?;

// Step 2: Alice funds escrow (locks 1000 DRKW)

// Step 3a: Alice claims — proves she knows seller_secret
//           Gets the 1000 DRKW

// OR Step 3b: After timeout, Bob refunds — proves timeout passed
//             Gets the 1000 DRKW back
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

The escrow contract source is in `src/contract/escrow/`. See the contract [README](../../../src/contract/escrow/README.md) for the full architecture.

```
src/contract/escrow/
├── proof/                    # ZK proof circuits (.zk files)
│   ├── create_escrow.zk  # Commitment creation
│   ├── fund.zk           # Value commitment
│   ├── claim.zk          # Seller claim
│   └── refund.zk         # Buyer refund (with timeout)
├── src/
│   ├── client/              # Builder structs
│   ├── entrypoint.rs        # WASM entrypoint
│   ├── error.rs             # Error types
│   ├── lib.rs               # Contract definitions
│   └── model/               # Data structures
└── README.md
```

## Integration with PromissoryNote

The escrow contract manages its own value commitments but integrates with PromissoryNote for token operations:

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Integration Architecture                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│   PromissoryNote Contract                                            │
│   ├── Owns coin ledger (coins, nullifiers, Merkle tree)             │
│   ├── Issues tokens (MintV1/BurnV1)                                  │
│   └── Transfer semantics (TransferV1)                                 │
│                                                                       │
│   Escrow Contract                                                    │
│   ├── Owns escrow state machine (Created → Funded → Claimed/Refunded)│
│   ├── Verifies ZK proofs for claim/refund                            │
│   └── Emits spend_hook calls to PromissoryNote for fund release      │
│                                                                       │
│   Flow:                                                               │
│   1. User creates escrow + funds via PromissoryNote::TransferV1      │
│   2. Escrow::Claim → spend_hook → PromissoryNote::BurnV1 (consumes)  │
│                    + PromissoryNote::MintV1 (mints to seller)        │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

### Phase 1: Standalone (MVP)
- Escrow manages its own value pool
- No PromissoryNote integration
- Simplified trust model

### Phase 2: Full Integration
- Uses PromissoryNote's `spend_hook` mechanism
- Funds locked as PromissoryNote coins
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

| Feature | Traditional Escrow | Smart Contract (Public) | DarkWow Escrow |
|---------|-------------------|------------------------|---------------|
| Privacy | Full privacy | Zero (terms visible) | Full (commitment only) |
| Trust | Third party | Trustless, public | Trustless, private |
| Customizability | Limited | Full | Full |
| Counterparty risk | High (escrow hack) | Zero | Zero |
| Auditability | Limited | Full | Partial (ZK-verified) |

## Implementation Status

**Full MVP** — All 6 functions and 4 ZK circuits implemented, compiled, and functional.

| Circuit | Binary | Status |
|---------|--------|--------|
| `create_escrow.zk` | `create_escrow.zk.bin` | Compiled — commitment formation verified in-circuit |
| `fund.zk` | `fund.zk.bin` | Compiled — Pedersen value commitment verified |
| `claim.zk` | `claim.zk.bin` | Compiled — seller secret proof + nullifier derivation |
| `refund.zk` | `refund.zk.bin` | Compiled — timeout check + buyer secret proof + nullifier |

### What's Done

1. **ZK Circuit Compilation**: All 4 circuits compiled to `.zk.bin` via zkas
2. **Entry Point Implementation**: Full `get_metadata()` + `process_instruction()` wired with ZK proof verification
3. **State Management**: 5-state state machine (Created → Funded → Claimed/Refunded/Cancelled) with 30 error variants
4. **PromissoryNote Integration**: Phase 1 standalone value pool; Phase 2 spend_hook integration for promissory_note

## References

- [DarkWow Escrow README](../../../src/contract/escrow/README.md)
- [DarkWow DAO Escrow Contract](./dao_escrow.md)
- [PromissoryNote Contract](promissory_note.md)
- [zkVM Primitive Layer](../arch/zk/zkvm_primitives.md)
- [Field Arithmetic Constraints](../arch/zk/field_arithmetic.md)

## See Also

- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis
