# Atomic Swap Contract

Cross-chain atomic swaps via Hashed Timelock Contract (HTLC) pattern.

## Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        Cross-Chain Atomic Swap                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   Alice (Ethereum)                    Bob (DarkFi)                      │
│   ──────────────────                  ─────────────────                  │
│                                                                          │
│   1. Create HTLC                       2. Verify hash                     │
│      hash = SHA256(secret)               matches her hash               │
│      amount = X ETH                      amount = Y DRK                  │
│      timelock = T                        timelock = T + δ               │
│                                                                          │
│   3. Send hash to Bob          ───────────────────────────────────────►  │
│                                                                          │
│   4. Bob creates HTLC           ◄─────────────────────────────────────   │
│      same hash                                                 │         │
│      amount = Y DRK                                         │         │
│      timelock = T + δ                                       │         │
│                                                                  │         │
│   5. Bob confirms HTLC          ───────────────────────────────────────►  │
│                                                                          │
│   6. Alice reveals secret       ───────────────────────────────────────►  │
│      (on Ethereum)                                │                     │
│                                                     ▼                     │
│   7. Bob claims on DarkFi      ◄──────────────────────────────────────   │
│      - Proves knowledge of secret                                          │
│      - Gets Y DRK                                                             │
│                                                                          │
│   8. Bob tells Alice secret     ───────────────────────────────────────►  │
│                                                                          │
│   9. Alice claims on Ethereum  ◄──────────────────────────────────────  │
│      - Gets X ETH                                                            │
│                                                                          │
│   SWAP COMPLETE!                                                         │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## HTLC Pattern

### Hashlock

The swap is secured by a cryptographic hash:

```
secret → HASH(secret) → stored_hash
```

Only the holder of the secret can compute the preimage and claim. This binds both chains atomically.

### Timelock

A fallback mechanism:

```
current_block >= timelock → refund available
```

If the swap isn't completed in time, either party can get their funds back.

### Atomicity

Both chains see the same hash. If Alice claims on Ethereum, she reveals the secret, allowing Bob to claim on DarkFi. If either party fails, the other gets their funds via timelock refund.

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize contract |
| `CreateSwapV1` | `0x01` | Create HTLC, lock funds |
| `ClaimV1` | `0x02` | Claim with secret |
| `RefundV1` | `0x03` | Refund after timelock |

## State Machine

```
┌─────────┐     Claim      ┌─────────┐    External    ┌───────────┐
│ Created │───────────────►│ Claimed │───────────────►│ Completed │
└─────────┘                └─────────┘                 └───────────┘
     │                          │
     │      Timelock             │
     │        Refund             │
     └──────────────────────────┴──────────────► Refunded
```

- **Created**: HTLC exists, funds locked
- **Claimed**: Secret revealed, funds released on DarkFi
- **Completed**: Both sides claimed (external chain confirmed)
- **Refunded**: Timelock expired, funds returned

## Trust Model

| Threat | Protection |
|--------|------------|
| Alice claims without paying | Bob doesn't create HTLC until hash verified |
| Bob steals funds | Only Alice knows secret, can claim anytime |
| Alice disappears | Bob waits for timelock, then refunds |
| Bob disappears | Alice waits for timelock, then refunds |
| Chain reorganization | DarkFi has deterministic blocks |

## Composability with Subscription

Atomic swap can fund subscription payments from external chains:

```
┌─────────────────────────────────────────────────────────────────────┐
│               Cross-Chain Subscription Payment                          │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  User (Ethereum)                DarkFi                               │
│  ────────────────                ───────                             │
│                                                                      │
│  1. User locks ETH in HTLC           │                               │
│     hash = subscription_secret        │                               │
│     amount = subscription_fee         │                               │
│     recipient = DarkFi swap contract │                               │
│                                    │                               │
│  2. hash sent to DarkFi            │◄───────────────                 │
│                                    │  hash verification              │
│  3. Subscription::SubscribeV1       │                                │
│     + DAO-Escrow membership        │                                │
│                                    │                                │
│  4. User reveals secret on ETH     │◄───────────────                 │
│                                    │  Subscription activated         │
│  5. Subscription service          │                                │
│     claims ETH                     │                                │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Supported Chains

| Chain | Hash Function | Status |
|-------|---------------|--------|
| Ethereum | SHA256 | ✅ MVP |
| Bitcoin | SHA256 + RIPEMD160 | 🆕 TODO |
| Solana | Ed25519 | 🆕 TODO |

## ZK Circuits

### CreateSwap (`create_swap_v1.zk`)

```zk
# Proves:
# - Creator knows secret
# - poseidon_hash(secret) is correctly computed and stored
# - Timelock is set

computed_hash = poseidon_hash(secret);
constrain_equal_base(computed_hash, hash);
constrain_equal_base(derived_swap_id, swap_id);
```

### Claim (`claim_v1.zk`)

```zk
# Proves:
# - Claimer knows secret
# - poseidon_hash(secret) matches stored hash (hash binding verified)
# - Nullifier is valid

computed_hash = poseidon_hash(secret);
constrain_equal_base(computed_hash, hash);
nullifier_check = poseidon_hash(swap_id, secret);
constrain_equal_base(nullifier_check, nullifier);
```

## MVP Status

| Component | Status | Notes |
|-----------|--------|-------|
| CreateSwap circuit | ✅ Complete | External hash accepted |
| Claim circuit | ✅ Complete | Secret revealed |
| Refund circuit | 🆕 TODO | Timelock verification needed |
| Hash verification | 🆕 TODO | SHA256 in-circuit |

## Limitations

1. **Hash function trust**: DarkFi uses `poseidon_hash(secret)` which is verified in-circuit. For cross-chain swaps, a bridge/oracle is needed to bind to external chain hashes (e.g., Ethereum SHA256).

2. **Timelock synchronization**: External chain timelock must be > DarkFi timelock for fairness

3. **Delta timing**: Must account for block time differences between chains

4. **Finality**: Must wait for sufficient confirmations on external chain

## See Also

- [Atomic Swap Contract](../../src/contract/atomic_swap/README.md)
- [Subscription Contract](subscription.md)
- [DAO-Escrow Contract](dao_escrow.md)
- [Experimental Opcodes](experimental-opcodes.md)
- [Opcode Universe](opcode_universe.md)
