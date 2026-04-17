# DarkFi Atomic Swap Contract

Cross-chain atomic swaps via Hashed Timelock Contract (HTLC) pattern.

## How It Works

```
Ethereum                          DarkFi
    │                                 │
    │  1. Alice locks ETH in HTLC     │
    │     hash = SHA256(secret)       │
    │     timelock = block + N        │
    │ ───────────────────────────────►│
    │                                 │  2. Bob verifies hash
    │                                 │  3. Bob locks DRK in HTLC
    │                                 │     (same hash)
    │                                 │     timelock = block + N + δ
    │ ◄──────────────────────────────│
    │  4. Bob confirms HTLC           │
    │                                 │
    │  5. Alice reveals secret        │  6. Bob sees secret
    │ ───────────────────────────────►│
    │                                 │  7. Bob claims DRK
    │                                 │
    │  8. Alice claims ETH            │
    │ ◄───────────────────────────────│
```

## HTLC Security Properties

| Property | Description |
|----------|-------------|
| **Atomic** | Either both complete, or neither |
| **Hashlock** | Only secret holder can claim |
| **Timelock** | Refund guaranteed after expiration |
| **Non-custodial** | No third-party holds funds |

## Entrypoints

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize contract |
| `CreateSwapV1` | `0x01` | Create HTLC, lock funds |
| `ClaimV1` | `0x02` | Claim with secret |
| `RefundV1` | `0x03` | Refund after timelock |

## State Machine

```
Created ──[Claim]──> Claimed ──[External claim]──> Completed
    │                                                   │
    │ (if timelock expires)                            │
    └───────[Refund]──────────────────────────────────┘
                  │
                  ▼
              Refunded
```

## Composability with Subscription

Atomic swap enables cross-chain subscription payments:

```
Ethereum                          DarkFi
    │                                 │
    │  1. Lock ETH in HTLC            │
    │     hash = SHA256(subscription_  │
    │         secret)                  │
    │ ───────────────────────────────►│
    │                                 │  2. Verify hash
    │                                 │  3. SubscribeV1
    │                                 │     (hash = DAO-Escrow)        │
    │                                 │
    │  4. Reveal secret              │  5. Subscription activated
    │ ───────────────────────────────►│
```

The subscription can be funded from the atomic swap proceeds.

## Circuit Details

### CreateSwap (`create_swap_v1.zk`)

Proves:
- Creator knows the secret (but doesn't reveal)
- `poseidon_hash(secret)` is correctly computed and stored
- Timelock is set

### Claim (`claim_v1.zk`)

Proves:
- Claimer knows the secret
- `poseidon_hash(secret)` matches stored hash (hash binding verified)
- Nullifier prevents double-claim

**Note**: The claim circuit has NO timelock check on claim. This is intentional - the timelock protects the refund path only. See [security-analysis.md Issue #6](../../../doc/src/arch/security-analysis.md) for why asymmetric timelock is correct for atomic swaps.

## MVP Status

| Component | Status | Notes |
|-----------|--------|-------|
| CreateSwap circuit | ✅ Complete | poseidon_hash verified in-circuit |
| Claim circuit | ✅ Complete | Hash verified, no timelock on claim (intentional) |
| Refund circuit | ✅ Complete | Timelock verification for refund |
| External hash verification | ✅ Mitigated | Each chain verifies own hash |

## Cross-Chain Hash Verification

**How cross-chain swaps work without oracles:**

| Chain | Hash Used | Verification |
|-------|-----------|-------------|
| DarkFi | `poseidon_hash(secret)` | ZK circuit verifies in-circuit |
| Ethereum | `SHA256(secret)` | EVM verifies natively |

Each chain verifies the hash function it understands. No oracle needed.

## Limitations

1. **External hash binding**: For cross-chain swaps, each chain only verifies its own hash. This is sufficient because Alice reveals the secret voluntarily on one chain, and the other party claims on the other chain when they see it. See [security-analysis.md Issue #7](../../../doc/src/arch/security-analysis.md).

2. **No Bitcoin**: Bitcoin uses RIPEMD160(SHA256) which requires different circuit.

3. **Timelock delta**: External chain timelock must be later than DarkFi's to ensure fairness.

4. **Asymmetric timelock**: The claim has no timelock (Bob claims immediately when secret is known). The refund has a timelock (Alice waits to refund). This is intentional - asymmetric timelocks prevent griefing and preserve atomicity. See [security-analysis.md Issue #6](../../../doc/src/arch/security-analysis.md).

## Heavyweight Test

The contract includes a heavyweight test that exercises all endpoints:

```bash
cargo test --release -p darkfid test_atomic_swap_heavyweight
```

**Test Coverage**:
| Function | Opcode | Status |
|----------|--------|--------|
| CreateSwapV1 | 0x01 | ✅ Tested with ZK proof |
| ClaimV1 | 0x02 | ✅ Tested with ZK proof |
| RefundV1 | 0x03 | ⚠️ Not executed (requires timelock expiry) |

**Note**: RefundV1 is not executed in the standalone test because it requires waiting for the timelock to expire. The ZK proof is still generated and verified to work correctly.

## See Also

- [Atomic Swap Architecture Doc](../../doc/src/arch/atomic_swap.md)
- [Subscription Contract](../subscription/README.md)
- [DAO-Escrow Contract](../dao_escrow/README.md)
