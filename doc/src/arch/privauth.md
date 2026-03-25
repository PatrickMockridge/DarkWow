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

| Contract | Commitment | Nullifier | Revocation |
|----------|------------|------------| ------------|
| **Bridge** | `DepositParams.commitment = H(recipient_secret, amount)` | `WithdrawParams.nullifier = H(recipient_secret)` | None |
| **DEX** | `CreateSwapParams.lock_commitment = H(secret, token, amount)` | `AcceptSwapParams.lock_commitment` (reused) | `CancelSwapParams.secret` |
| **Identity** | `IssueCredentialParams.commitment = H(issuer_key, holder_key, schema, attrs)` | `Credential.nullifier = H(holder_secret, credential_secret)` | `RevokeCredentialParams.nullifier` |
| **Stablecoin** | `OpenPositionParams.commitment = H(owner_secret, collateral, debt)` | `LiquidateParams.nullifier = H(position_secret)` | None |

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

## Related Patterns

### Intent Pattern

The [intent-amm fork](https://codeberg.org/rusticml/darkfi-intent-amm-proposal) explores extending this pattern with:
- `PrivateIntent`: Reusable intent lifecycle
- `IntentPostTransitionV1`: Post-creation transition
- `IntentConsumeTransitionV1`: Consumption transition

This provides a more formal framework for the lifecycle described here.

### Predicate-Based Authorization

This pattern naturally extends to predicate-based authorization:
- "Prove you hold at least 100 tokens"
- "Prove you're a DAO member"
- "Prove you're over 18"

The predicate is verified in the ZK circuit, and only the result (true/false) is revealed.

## References

- [Identity Contract](../../src/contract/identity/)
- [Bridge Contract](../../src/contract/bridge/)
- [DEX Contract](../../src/contract/dex/)
- [Stablecoin Contract](../../src/contract/stablecoin/)
- [Intent AMM Proposal](https://codeberg.org/rusticml/darkfi-intent-amm-proposal)
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)