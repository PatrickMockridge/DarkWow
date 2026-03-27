# Private Authorization Layer (DRAFT)

*This document describes a reusable pattern for privacy-preserving authorization in DarkFi smart contracts. This pattern appears across all DarkFi privacy-heavy contracts and should be considered a foundational primitive.*

## The Pattern

Every privacy-preserving DarkFi contract needs to solve the same fundamental problem:

**How do you authorize an action without revealing who you are or what you're doing?**

The solution is a reusable pattern with four components:

| Component | Purpose | Appears In |
|-----------|---------|------------|
| **Commitment** | Creates a private capability bound to a secret | All contracts |
| **Nullifier** | Consumes the capability exactly once | All contracts |
| **Proof** | Verifies authorization without revealing secret | All contracts |
| **Revocation** | Allows issuer to invalidate before use | Identity, some others |

```
┌─────────────────────────────────────────────────────────────────┐
│              Private Authorization Lifecycle                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. COMMIT                                                        │
│     User creates commitment = H(secret, params)                    │
│     → Private capability exists on-chain                          │
│     → No one knows the secret                                    │
│     → Capability is bound to user                                 │
│                                                                   │
│  2. PROVE (optional intermediate step)                            │
│     User generates ZK proof                                        │
│     → Proves they know the secret                                 │
│     → Proves commitment is valid                                 │
│     → Proves predicate is satisfied                               │
│     → Nothing revealed to observers                                │
│                                                                   │
│  3. CONSUME                                                       │
│     User provides nullifier = H(secret)                            │
│     → Capability consumed exactly once                             │
│     → Cannot be used again (replay protection)                   │
│     → Action executed atomically                                  │
│                                                                   │
│  4. REVOKE (optional)                                            │
│     Issuer marks nullifier as revoked                             │
│     → Commitment invalidated before use                            │
│     → Issuer can cancel before consumed                          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Why This Pattern Exists

### The Problem with Traditional Authorization

Traditional blockchain authorization reveals too much:
- **Public keys** link transactions to identities
- **Signatures** prove ownership but don't hide the transaction
- **Balances** are visible to everyone
- **Transaction graphs** can be analyzed to deanonymize users

### The Privacy-Preserving Solution

This pattern achieves **authorization without revelation**:

1. **Commitment hides the secret**: `H(secret, params)` means only the holder knows the secret
2. **Nullifier prevents reuse**: `H(secret)` can only be spent once
3. **Proof enables authorization without disclosure**: ZK proof shows the secret is known without revealing it
4. **Revocation provides control**: Issuer can invalidate before use

## Formal Definition

### Commitment

```rust
commitment = H(secret, params...)
```

- **secret**: Known only to the holder
- **params**: Contract-specific parameters (token, amount, etc.)
- **Purpose**: Creates a private capability bound to the secret

Properties:
- Computationally binding: Cannot find two different secrets that produce the same commitment
- Hiding: Commitment reveals nothing about the secret
- Unique per context: Same secret + different params = different commitment

### Nullifier

```rust
nullifier = H(secret, commitment[, extra...])
```

- **secret**: Known only to the holder
- **commitment**: The commitment being consumed
- **extra**: Optional additional binding (e.g., state hash)

Purpose:
- **Replay protection**: Same nullifier cannot be used twice
- **Linkage prevention**: Knowing the commitment doesn't reveal the nullifier

### Proof

The ZK proof demonstrates:
1. Knowledge of the secret
2. Commitment exists and is valid
3. Predicate is satisfied (e.g., `amount > 0`)
4. (Sometimes) Commitment is in a Merkle tree or other accumulator

The proof reveals:
- That the predicate is satisfied
- That the commitment exists

The proof does NOT reveal:
- The secret
- The commitment itself (only membership proof)
- The exact parameter values

### Revocation (Optional)

```rust
revoked_nullifier = H(issuer_secret, commitment)
```

- **issuer_secret**: Known only to the issuer
- **commitment**: The commitment being revoked

Purpose:
- Allows issuer to invalidate before the capability is used
- Enables use cases like credential revocation, refund cancellation

## Cross-Contract Pattern Mapping

All contracts now use domain-separated poseidon hashing for strong separation:

| Contract | Commitment Hash | Nullifier Hash | Revocation |
|----------|-----------------|----------------|------------|
| **Bridge** | `poseidon_hash([9001, owner_x, owner_y, 0x0002, payload_hash, expiry, nonce, blind])` | `poseidon_hash([9002, owner_secret, 0x0002, nonce, commitment])` | None |
| **DEX** | `poseidon_hash([9001, owner_x, owner_y, 0x0003, payload_hash, expiry, nonce, blind])` | `poseidon_hash([9002, owner_secret, 0x0003, nonce, commitment])` | `CancelSwapParams.secret` |
| **Identity** | `poseidon_hash([9001, owner_x, owner_y, 0x0001, payload_hash, expiry, nonce, blind])` | `poseidon_hash([9002, owner_secret, 0x0001, nonce, commitment])` | `RevokeCredentialParams.nullifier` |
| **Stablecoin** | `poseidon_hash([9001, owner_x, owner_y, 0x0004, payload_hash, expiry, nonce, blind])` | `poseidon_hash([9002, owner_secret, 0x0004, nonce, commitment])` | None |

Domain separators (9001 for commitment, 9002 for nullifier) prevent cross-protocol collision.
Namespace constants (0x0001-0x0004) scope intents per application.

## Security Properties

### Correctness

- **Binding**: Commitment can only be opened by someone who knows the secret
- **Uniqueness**: Nullifier can only be spent once
- **Authorization**: Only proof-holder can consume the commitment

### Privacy

- **Hiding**: Commitment reveals nothing about secret or params
- **Unlinkability**: Multiple uses of the same secret produce different commitments
- **Minimal disclosure**: Proof reveals only the predicate result

### Liveness

- **No lock-in**: User can always consume their own commitment
- **No censorship**: Commitment consumption doesn't require issuer cooperation (unless revoked)

## Implementation Notes

### Commitment Design

When designing commitments for a new contract:

1. **Include all relevant parameters**: `H(secret, token, amount, nonce, ...)`
2. **Use domain separation**: Include a prefix: `H("my_contract_v1", ...)`
3. **Bind to context**: Include state hash or other context when needed
4. **Consider nullifier computation**: What data should the nullifier bind to?

**SDK Helper**: Use `compute_commitment::<N>([secret, param1, ...])` from `darkfi_sdk::primitives`.

### Nullifier Design

When designing nullifiers:

1. **Bind to commitment**: `H(secret, commitment)` prevents cross-commitment replay
2. **Include state**: `H(secret, commitment, state_hash)` prevents state-transition replay
3. **Use unique derivation**: Same secret + different context = different nullifier

**SDK Helper**: Use `compute_nullifier(secret, commitment)` from `darkfi_sdk::primitives`.

### Proof Design

When designing ZK circuits:

1. **Verify commitment**: `assert_equal(H(secret, params), commitment)`
2. **Verify membership**: Merkle proof or accumulator membership
3. **Verify predicate**: `assert(predicate(params))`
4. **Include nullifier**: Compute nullifier in circuit to prove uniqueness

### Revocation Design (Optional)

When adding revocation:

1. **Issuer signs revocation**: Prevents unauthorized revocation
2. **Revocation is separate**: Revoked commitments can still exist, just can't be consumed
3. **Consider expiration**: Time-based revocation reduces issuer burden

## SDK Primitives

The DarkFi SDK provides reusable implementations of this pattern in `src/sdk/src/crypto/intent.rs` and `src/sdk/src/crypto/intent_set.rs`:

### PrivateIntent

A generic private intent object with:

| Field | Purpose |
|-------|---------|
| `owner` | PublicKey of who can consume |
| `namespace` | Scopes intent to specific application |
| `payload_hash` | Commits to application-specific data |
| `expiry` | Block height when intent expires |
| `nonce` | Prevents nullifier replay |
| `blind` | Additional blinding factor |

```rust
use darkfi_sdk::crypto::{PrivateIntent, IntentCommitment, IntentNullifier};

// Create intent
let intent = PrivateIntent::new(
    owner_pubkey,
    namespace,       // e.g., IDENTITY_NAMESPACE
    payload_hash,    // H(credential data)
    expiry,         // block height
    nonce,          // fresh random
    blind,          // blinding
);

// Compute commitment for on-chain storage
let commitment = intent.commitment();  // IntentCommitment

// Derive nullifier when consuming
let nullifier = intent.derive_nullifier(owner_secret)?;  // IntentNullifier
```

### IntentSetIndexV1

A generic state machine for managing intent lifecycle:

```rust
use darkfi_sdk::crypto::{IntentSetIndexV1, IntentPostTransitionV1, IntentConsumeTransitionV1};

let mut index = IntentSetIndexV1::new();

// Post new intent
let post = IntentPostTransitionV1 { ... };
index.validate_post(&post)?;
index.apply_post(&post)?;

// Consume intent
let consume = IntentConsumeTransitionV1 { ... };
index.validate_consume(&consume)?;
index.apply_consume(&consume)?;
```

### Namespace Constants

Each application should define its own namespace:

| Application | Namespace (example) |
|------------|-------------------|
| Identity | `0x0001` |
| Bridge | `0x0002` |
| DEX | `0x0003` |
| Stablecoin | `0x0004` |

This allows the same primitives to work across all privacy-preserving contracts.

## Related Patterns

### Intent Pattern (Integrated)

The [intent-amm fork](https://codeberg.org/rusticml/darkfi-intent-amm-proposal) explored reusable primitives that have been integrated into the DarkFi SDK:

- `PrivateIntent`: Reusable private authorization object with domain-separated hashing
- `IntentSetIndexV1`: Generic state machine for post/consume lifecycle
- `IntentPostTransitionV1`: Post-creation transition with Merkle root validation
- `IntentConsumeTransitionV1`: Consumption transition with nullifier tracking
- `Transition payload encoding`: Typed function codes (Post=0x00, Cancel=0x01, Fill=0x02)

This provides a formally verified framework for the lifecycle described here. See [Composability](composability.md) for implementation details.

### Predicate-Based Authorization

This pattern naturally extends to predicate-based authorization:
- "Prove you hold at least 100 tokens"
- "Prove you're a DAO member"
- "Prove you're over 18"

The predicate is verified in the ZK circuit, and only the result (true/false) is revealed.

## Predicate Verification and the Opcode Layer

The authorization pattern above (commitment → proof → nullifier) works for any predicate.
But the **expressiveness of the predicate** is constrained by the zkVM opcode layer.

Simple predicates like `amount > 0` or `balance == 0` can be expressed with existing
opcodes. More complex predicates require comparison opcodes that return values:

| Predicate | Required Opcodes |
|-----------|-----------------|
| `attribute >= threshold` (identity) | `LessThanOrEqual` |
| `collateral >= 2 * debt` (stablecoin) | `LessThanOrEqual`, `BaseMul` |
| `fill_amount <= requested_amount` (DEX) | `LessThanOrEqual` |
| `price <= market_price` (AMM) | `LessThanOrEqual`, `IsEqualBase` |
| `seller knows secret` (escrow claim) | None — `poseidon_hash` only |
| `timeout passed` (escrow refund) | `LessThanStrict` (constrain-only, existing) |

See [zkVM Primitive Layer](zkvm_primitives.md) for the full analysis of why comparison
opcodes that return values are the core gap, and how they compose into higher-level
constructions.

### The Gap: Opcodes That Constrain vs. Opcodes That Return

The existing zkVM has `LessThanStrict` and `LessThanLoose`, but both **constrain only**
— they fail the circuit if `a >= b`, they don't return a value you can use in further
computation. This is why `LessThanOrEqual` (returning 0 or 1) and `IsEqualBase`
(returning 0 or 1) are systematically needed.

The authorization pattern is complete without these opcodes — but the **predicate
expressiveness is limited**. Current circuits use placeholders that always pass.

## References

- [Composability & General Primitives](composability.md)
- [zkVM Primitive Layer](zkvm_primitives.md) — opcode-level analysis of why comparison opcodes are foundational
- [Contract MVP Status](mvp_status.md) — blockers for each contract in the contracts folder
- [Identity Contract](../../src/contract/identity/)
- [Bridge Contract](../../src/contract/bridge/)
- [DEX Contract](../../src/contract/dex/)
- [Stablecoin Contract](../../src/contract/stablecoin/)
- [Escrow Contract MVP](./escrow.md) — conditional payment authorization pattern
- [DAO-Escrow Contract](./dao_escrow.md) — DAO-governed endowment with voting
- [Intent AMM Proposal](https://codeberg.org/rusticml/darkfi-intent-amm-proposal)
- [Response to PatrickM123 (Intent AMM)](https://codeberg.org/rusticml/darkfi-intent-amm-proposal/src/branch/main/docs/response-to-patrickm123.md)
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)