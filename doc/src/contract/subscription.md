# Subscription Contract

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

DarkWow has deterministic block heights. Subscriptions use block numbers instead of timestamps:

```rust
lock_until_block = current_block + plan.duration_blocks
```

**Advantage over Ethereum**: Miners cannot manipulate block numbers the way they can timestamps. A block number N means "the Nth block in the chain" - not an approximation of time.

### Object Capability Security

The subscription grants a **capability** derived via Poseidon hash:

```rust
capability = PoseidonHash(
    subscriber_pubkey,  // Who
    plan_id,           // What plan
    subscription_id,   // Which subscription
    permissions,        // What they can do
    lock_until_block,  // Until when
    nonce              // Unpredictable
);
```

**Properties**:
- **Unforgeable**: Only the subscriber knows the secret key
- **Transferable**: Capability can be shared (but tracked via nullifiers)
- **Revocable**: DAO can slash malicious subscribers

### Endowment Fund Insurance

Each subscription splits the deposit:

| Share | Destination | Purpose |
|-------|-------------|---------|
| `price` | Treasury | DAO governance funding |
| `endowment_share` | Endowment Fund | Insurance reserve |

If the service fails or is malicious, the DAO can authorize refunds from the endowment fund.

## Entrypoints

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize contract with DAO parameters |
| `SubscribeV1` | `0x01` | Create new subscription |
| `CancelV1` | `0x02` | User cancels, refund at lock |
| `RenewV1` | `0x03` | Extend subscription period |
| `VerifyAccessV1` | `0x04` | ZK proof of valid subscription |
| `DaoControlV1` | `0x05` | DAO governance actions |
| `UpdateUsageV1` | `0x06` | Update usage counters (rate limiting) |

## Composability: Subscription + DAO-Escrow

This contract demonstrates DarkWow's **composable contract pattern** through integration with DAO-Escrow.

### The Pattern

```
┌──────────────────────┐         ┌──────────────────────┐
│     DAO-Escrow       │         │    Subscription       │
│                      │         │                       │
│  ┌────────────────┐  │         │  ┌────────────────┐  │
│  │ pay_premium()  │──┼───┐     │  │ subscribe()    │  │
│  └────────────────┘  │   │     │  └───────┬────────┘  │
│                      │   │     │          │            │
│  State: Merklized    │   │     │  Verifies via:      │
│  Membership tree     │   │     │  ┌────────▼────────┐ │
│                      │   │     │  │ Merkle proof     │ │
│                      │   │     │  │ + expiry check   │ │
│                      │   │     │  │ + pubkey link    │ │
└──────────────────────┘   │     │  └────────────────┘  │
                           │     │                       │
         ┌─────────────────┘     └───────────────────────┘
         │                           │
         │    Cross-Contract         │
         │    ZK Verification       │
         ▼                           ▼
┌─────────────────────────────────────────────────────────┐
│                   Composability                         │
│                                                         │
│  No direct state sharing!                              │
│  Pure Merkle proof verification.                        │
│  Nullifiers prevent double-spending.                   │
└─────────────────────────────────────────────────────────┘
```

### How DAO-Escrow Integration Works

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

# Note stored in DAO-Escrow's Merkle tree
```

**2. Subscription Verifies Membership** (`subscribe_v1.zk` Phase 5):
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
| **No Trust Assumption** | Subscription verifies, doesn't trust DAO-Escrow |
| **Privacy** | Merkle proofs don't reveal membership details |
| **Atomic Transactions** | Both execute in single transaction |

### Composability Principles

DarkWow's contract composability follows three rules:

1. **Merkle State**: Contract state is Merklized for privacy
2. **ZK Verification**: Other contracts verify via Merkle proofs
3. **Nullifier Namespace**: Shared nullifiers prevent double-spending

This pattern enables:
- Tiered services (member vs non-member pricing)
- Insurance-backed subscriptions
- Cross-contract loyalty programs
- Composable DeFi primitives

## Circuits

### Subscribe V1 (`subscribe_v1.zk`)

**Purpose**: Create a subscription with deposit locked in escrow.

**Proves**:
1. Subscriber knows the secret key
2. Plan ID is valid (Merkle proof)
3. `current_block < lock_until_block` (subscription is active)
4. Deposit commitment is valid
5. **DAO-Escrow membership** (Merkle proof + expiry + pubkey link)

**Public Inputs**:
- `subscription_id`: Commitment hash
- `subscriber_pub_x/y`: Public key coordinates
- `plan_id`: Subscription tier
- `deposit`: Amount locked
- `token_id`: Which token
- `lock_until_block`: Expiration height
- `plan_merkle_root`: Plan registry root
- `dao_escrow_bulla`: Insurance pool identifier
- `dao_membership_note`: Membership proof from DAO-Escrow
- `dao_escrow_merkle_root`: Insurance pool's membership root

### Verify Access V1 (`verify_access_v1.zk`)

**Purpose**: Prove subscription is valid for access control.

**Proves**:
1. Subscriber knows the secret key
2. `current_block < lock_until_block` (still active)
3. Capability digest matches expected
4. Claimed tier >= required tier (tiered access check)

**Use Cases**:
- Gated content access
- Service authentication
- API rate limiting

## Permission Checking: Tiered Access vs Bitmask Access

The subscription contract supports two permission models:

### Tiered Access (Current Implementation)

The circuit uses a tiered approach that **reveals tier level on-chain**:

```zk
# Tier definitions:
#   TIER_BASIC = 1    -> READ only
#   TIER_PREMIUM = 2  -> READ + WRITE
#   TIER_ADMIN = 3    -> READ + WRITE + ADMIN

# Check claimed_tier >= required_tier
less_than_strict(required_tier - 1, permissions_claimed);
```

**Privacy Consideration**: The tier level (1, 2, or 3) is revealed on-chain. Anyone observing can tell if a subscriber has BASIC, PREMIUM, or ADMIN tier access.

**Tradeoffs**:
- ✅ Prevents unauthorized access (tier must be >= required)
- ✅ Works with sound `less_than_strict` opcode
- ❌ Reveals tier level (privacy consideration)
- ❌ Cannot have arbitrary bitmask combinations (e.g., READ+ADMIN without WRITE)

### True Bitmask Access (Future Enhancement)

With `base_div` now implemented (0x58), true bitmask checking is achievable:

```zk
# Bit definitions:
#   READ = 0b0001
#   WRITE = 0b0010
#   ADMIN = 0b0100

# Goal: Verify (claimed & required) == claimed
# Meaning: all bits in 'required' are also set in 'claimed'

# With base_div, we can verify bitwise subset relationship:
# The check (claimed & ~required) == 0 requires base_div for bitwise AND
# This would preserve zero-knowledge (only "has access" vs "doesn't")
```

**Benefits of Bitmask Access**:
- ✅ No tier level leak
- ✅ Arbitrary permission combinations
- ✅ True zero-knowledge permission verification

## Cross-Chain Integration

### Atomic Swap Flow

```
Ethereum                          DarkWow
    │                                 │
    │  1. Lock ETH in HTLC            │
    │     (hash of DarkWow secret)     │
    │ ───────────────────────────────►│
    │                                 │  2. Reveal secret
    │                                 │     Atomic swap executes
    │                                 │     SubscribeV1 called
    │                                 │
    │  3. Claim ETH with secret       │  4. Subscription activated
    │◄────────────────────────────────│
```

DarkWow's `atomic_swap` contract handles the cross-chain payment. The subscription contract integrates at step 3 - verifying the atomic swap completed before activating the subscription.

See [Atomic Swap Contract](atomic_swap.md) for full HTLC details.

## Integration with Existing Contracts

| Component | Integration Point |
|-----------|-------------------|
| `money` | Token transfers for deposits and refunds |
| `dao` | Governance for subscription parameters |
| `dao_escrow` | Treasury and endowment fund management |
| `atomic_swap` | Cross-chain subscription payments |
| `atomic_swap` | Cross-chain payment settlement |

## MVP Status

| Component | Status | Notes |
|-----------|--------|-------|
| Subscribe circuit | ✅ Complete | DAO-Escrow integration (all 3 modes) |
| VerifyAccess circuit | ✅ Complete | Block-based expiry, Merkle proof |
| DAO treasury integration | ✅ Complete | Via DAO-Escrow `MODE_TREASURY` |
| Endowment fund | ✅ Complete | Via DAO-Escrow `MODE_TREASURY_ENDOWMENT` |
| Escrow-only mode | ✅ Complete | Via DAO-Escrow `MODE_ESCROW` |
| Cross-chain atomic swap | ✅ Complete | Integration with `atomic_swap` via hash link |

**All three DAO-Escrow modes supported:**
- `MODE_ESCROW`: Pure insurance for subscription deposits
- `MODE_TREASURY`: Treasury for subscription fees
- `MODE_TREASURY_ENDOWMENT`: Treasury + Endowment combined

## Security Considerations

1. **Block finality**: Subscription locks depend on DarkWow's consensus. If chain reorganizes, the block number could change.

2. **DAO trust**: The endowment fund requires trustworthy DAO governance. A malicious DAO could drain the fund.

3. **Endowment sufficiency**: If many subscriptions are cancelled simultaneously, the endowment must have sufficient reserves.

4. **Capability transfer**: Once a capability is revealed (for access), it could be used by anyone who sees it.

## See Also

- [Object Capability Model](https://en.wikipedia.org/wiki/Object_capability_model)
- [DAO Contract](dao.md)
- [DAO-Escrow Contract](dao_escrow.md)
- [Opcodes Reference](../arch/zk/opcodes.md)
- [Complete Opcode Universe](../arch/zk/opcode_universe.md)
- [Escrow Contract](escrow.md)
