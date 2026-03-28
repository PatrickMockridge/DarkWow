# DarkFi Subscription Contract

Privacy-preserving member subscription service with block-based time locks, DAO treasury, and endowment fund insurance.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Subscription Service DAO                          │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  Treasury (subscription fees → governance)                        │ │
│  │  - DAO votes on subscription pricing                            │ │
│  │  - DAO controls fee parameters                                   │ │
│  │  - Endowment share accumulates here                             │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  Endowment Fund (insurance reserve)                              │ │
│  │  - Covers refunds if service fails                              │ │
│  │  - Grows from endowment_share of each subscription               │ │
│  │  - DAO-controlled drawdown                                       │ │
│  └─────────────────────────────────────────────────────────────────┘ │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │  Subscription Plans (Merkle tree registry)                        │ │
│  │  - Monthly, yearly, premium tiers                                │ │
│  │  - DAO can add/update/deactivate plans                          │ │
│  │  - Merkle proof verifies plan validity                          │ │
│  └─────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

## Composability Case Study: Subscription + DAO-Escrow

This contract demonstrates DarkFi's **composable contract pattern** through integration with DAO-Escrow.

### The Composability Pattern

```
┌──────────────────────┐         ┌──────────────────────┐
│     DAO-Escrow       │         │    Subscription       │
│                      │         │                       │
│  ┌────────────────┐  │         │  ┌────────────────┐  │
│  │ pay_premium()  │──┼───┐     │  │ subscribe()    │  │
│  └────────────────┘  │   │     │  └───────┬────────┘  │
│                      │   │     │          │            │
│  State: Merklized    │   │     │  Verifies via:      │
│  Membership tree     │   │     │  ┌────────▼────────┐  │
│                      │   │     │  │ Merkle proof    │  │
│                      │   │     │  │ + expiry check  │  │
│                      │   │     │  │ + pubkey link   │  │
└──────────────────────┘   │     │  └────────────────┘  │
                           │     │                       │
         ┌─────────────────┘     └───────────────────────┘
         │                         |
         │    Cross-Contract       │
         │    ZK Verification     │
         ▼                         ▼
┌─────────────────────────────────────────────────────────┐
│                   Composability                         │
│                                                         │
│  No direct state sharing!                               │
│  Pure Merkle proof verification.                        │
│  Nullifiers prevent double-spending.                    │
└─────────────────────────────────────────────────────────┘
```

### How It Works

**1. DAO-Escrow Issues Membership** (`pay_premium_v1.zk`):
```zk
# Member pays premium → receives membership note
membership_note = poseidon_hash(
    member_pub_x,
    member_pub_y,
    value,
    token_id,
    expiry,
    membership_blind,
);

# Note is stored in DAO-Escrow's Merkle tree
```

**2. Subscription Verifies Membership** (`subscribe_v1.zk`):
```zk
# Verify DAO-Escrow membership via Merkle proof
dao_root = merkle_root(dao_leaf_pos, dao_path, dao_membership_note);
constrain_equal_base(dao_root, dao_escrow_merkle_root);

# Verify membership hasn't expired
less_than_strict(current_block, dao_membership_expiry);

# Verify member public key matches subscription
constrain_equal_base(dao_member_pub_x, subscriber_pub_x);
```

### Benefits

| Benefit | Description |
|---------|-------------|
| **Insurance Tier** | DAO-Escrow members get subscription discounts |
| **No Trust Assumption** | Subscription doesn't trust DAO-Escrow - it verifies |
| **Privacy** | Merkle proofs don't reveal membership details |
| **Atomic Transactions** | Both can execute in single transaction |

### Code Location

- **DAO-Escrow Integration**: [proof/subscribe_v1.zk](proof/subscribe_v1.zk) (Phase 5)
- **Membership Types**: [src/model/mod.rs](src/model/mod.rs)

## State Machine

```
                    ┌──────────────────────────────────────┐
                    │                                      │
                    │         ┌──────────────┐            │
                    │         │   Active     │            │
    Subscribe ─────►│         │ lock_until   │            │
                    │         │   > current   │            │
                    │         └──────┬───────┘            │
                    │                │                     │
                    │                │ Cancel              │
                    │                ▼                     │
                    │         ┌──────────────┐            │
                    │         │  Cancelled   │            │
                    │         │ refund@lock │            │
                    │         └──────┬───────┘            │
                    │                │                     │
                    │         (time passes)                │
                    │                │ lock reached        │
                    │                ▼                     │
                    │         ┌──────────────┐            │
                    │         │   Expired    │            │
                    │         │ refund@lock │            │
                    │         └──────┬───────┘            │
                    │                │                     │
                    │                │ Renew               │
                    │                ▼                     │
                    │         ┌──────────────┐            │
                    └────────►│   Active     │◄────────────┘
                              │ (new period) │
                              └──────────────┘
```

## Trust Model

### Block-Based Time Locks (No Oracle)

DarkFi has deterministic block heights. Subscriptions use block numbers instead of timestamps:

```rust
lock_until_block = current_block + plan.duration_blocks
```

### Object Capability Security

The subscription grants a **capability** derived via Poseidon hash:

```rust
capability = PoseidonHash(
    subscriber_pubkey,  // Who
    plan_id,           // What plan
    subscription_id,   // Which subscription
    permissions,        // What they can do
    lock_until_block,  // Until when
    nonce,             // Unpredictable
    dao_escrow_bulla,  // Insurance tier
);
```

## Entrypoints

| Function | Opcode | Description |
|----------|--------|-------------|
| `SubscribeV1` | `0x01` | Create new subscription (verifies DAO-Escrow) |
| `CancelV1` | `0x02` | User cancels, refund at lock |
| `RenewV1` | `0x03` | Extend subscription period |
| `VerifyAccessV1` | `0x04` | ZK proof of valid subscription |
| `DaoControlV1` | `0x05` | DAO governance actions |

## Circuits

### Subscribe V1 (`subscribe_v1.zk`)

**Purpose**: Create a subscription with deposit locked in escrow.

**Proves**:
1. Subscriber knows the secret key
2. Plan ID is valid (Merkle proof)
3. `current_block < lock_until_block` (subscription is active)
4. Deposit commitment is valid (Pedersen commitment via `ec_mul_short` + `ec_add`)
5. **DAO-Escrow membership** (Merkle proof + expiry + pubkey)

**Public Inputs**:
- `subscription_id`: Commitment hash
- `subscriber_pub_x/y`: Public key coordinates
- `plan_id`: Subscription tier
- `deposit`: Amount locked
- `token_id`: Which token
- `lock_until_block`: Expiration height
- `value_commit_x/y`: Pedersen commitment to deposit
- `plan_merkle_root`: Plan registry root
- `dao_escrow_bulla`: Insurance pool identifier
- `dao_membership_note`: Membership proof
- `dao_escrow_merkle_root`: Insurance pool's membership root

### Verify Access V1 (`verify_access_v1.zk`)

**Purpose**: Prove subscription is valid for access control.

**Proves**:
1. Subscriber knows the secret key
2. `current_block < lock_until_block` (still active)
3. Capability digest matches expected

## MVP Status

| Component | Status | Notes |
|-----------|--------|-------|
| Subscribe circuit | ✅ Complete | DAO-Escrow integration (all 3 modes) |
| VerifyAccess circuit | ✅ Complete | Block-based expiry, Merkle proof |
| DAO treasury integration | ✅ Complete | Via DAO-Escrow `MODE_TREASURY` |
| Endowment fund | ✅ Complete | Via DAO-Escrow `MODE_TREASURY_ENDOWMENT` |
| Escrow-only mode | ✅ Complete | Via DAO-Escrow `MODE_ESCROW` |

**All three DAO-Escrow modes are supported:**
- `MODE_ESCROW`: Pure insurance for subscription deposits
- `MODE_TREASURY`: Treasury for subscription fees
- `MODE_TREASURY_ENDOWMENT`: Treasury + Endowment combined

## See Also

- [Subscription Architecture Doc](../../../doc/src/arch/subscription.md)
- [DAO-Escrow Contract](../../../src/contract/dao_escrow/README.md)
- [DAO Contract](../../../doc/src/arch/dao.md)
- [Composability Pattern](../../../doc/src/arch/composability.md)
