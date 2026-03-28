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

## MVP Status

| Component | Status | Notes |
|-----------|--------|-------|
| CreateSwap circuit | ✅ Complete | Accepts external hash |
| Claim circuit | ✅ Complete | Reveals secret |
| Refund circuit | 🆕 TODO | Needs timelock verification |
| External hash verification | 🆕 TODO | SHA256 in-circuit |

## Limitations

1. **External hash function**: For cross-chain swaps, DarkFi uses `poseidon_hash(secret)` which is verified in-circuit. However, binding to external chains (Ethereum SHA256) requires a bridge/oracle to verify the cross-chain hash. See [security-analysis.md](../../../doc/src/arch/security-analysis.md) for details.

2. **No Bitcoin**: Bitcoin uses RIPEMD160(SHA256) which requires different circuit.

3. **Timelock delta**: External chain timelock must be later than DarkFi's to ensure fairness.

## See Also

- [Atomic Swap Architecture Doc](../../doc/src/arch/atomic_swap.md)
- [Subscription Contract](../subscription/README.md)
- [DAO-Escrow Contract](../dao_escrow/README.md)
