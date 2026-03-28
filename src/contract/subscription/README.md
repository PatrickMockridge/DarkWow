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
4. [SHIT VERSION] Claimed tier >= required tier
5. [PROVISIONAL] Subscription is in Active state (via Merkle proof)

**Public Inputs**:
- `expected_capability`: Capability digest
- `subscription_id`: Subscription identifier
- `current_block`: Current block height
- `subscriber_pub_x/y`: Subscriber's public key
- `plan_id`: Plan ID
- `lock_until_block`: Subscription expiry
- `subscription_state_root`: Merkle root of subscription state tree

**Privacy Note**: The provisional cancellation fix (Phase 5) makes capabilities effectively **single-use** - each successful access reveals the `spent_nullifier`, linking all uses together. See Issue #3 in security-analysis.md.

## Permission Checking: SHIT VERSION vs PROPER VERSION

**THIS IS THE SHIT VERSION** - Uses tiered access with `less_than_strict`. See security-analysis.md Issue #2 for full analysis.

### SHIT VERSION: Tiered Access (Current Implementation)

The circuit uses a tiered approach that **LEAKS TIER LEVEL on-chain**:

```zk
# Tier definitions:
#   TIER_BASIC = 1    -> READ only
#   TIER_PREMIUM = 2  -> READ + WRITE
#   TIER_ADMIN = 3    -> READ + WRITE + ADMIN

# SHIT VERSION: Check claimed_tier >= required_tier
less_than_strict(required_tier - 1, permissions_claimed);
```

**Privacy Leak**: The tier level (1, 2, or 3) is revealed on-chain. Anyone observing can tell if a subscriber has BASIC, PREMIUM, or ADMIN tier access.

**Tradeoffs**:
- ✅ Prevents unauthorized access (tier must be >= required)
- ✅ Works with proven `less_than_strict` opcode
- ❌ Leaks tier level (privacy regression)
- ❌ Cannot have arbitrary bitmask combinations (e.g., READ+ADMIN without WRITE)

### PROPER VERSION: True Bitmask Checking (Requires base_div)

**Requires**: `base_div` opcode (not yet available)

The proper implementation would be:

```zk
# Bit definitions:
#   READ = 0b0001
#   WRITE = 0b0010
#   ADMIN = 0b0100

# Goal: Verify (claimed & required) == claimed
# Meaning: all bits in 'required' are also set in 'claimed'

# PROPER base_div approach:
# Step 1: Verify claimed >= required (field ordering)
less_than_strict(required_tier - 1, permissions_claimed);

# Step 2: With base_div, verify bitwise subset:
# The check (claimed & ~required) == 0 requires base_div for bitwise AND
# This verifies all required bits are present in claimed

# Pseudocode for proper bitmask check with base_div:
#   excess_bits = base_div(claimed, required);
#   # If required ⊆ claimed, the quotient relationship holds
#   # and no bits outside of claimed appear in the result
```

**Benefits of Proper Version**:
- ✅ No tier level leak (only "has access" vs "doesn't have access")
- ✅ Arbitrary permission combinations (READ+ADMIN, WRITE only, etc.)
- ✅ True zero-knowledge permission verification

**Status**: SHIT VERSION implemented to enable access control. PROPER VERSION awaits `base_div` opcode.

## Cancellation Verification: PROVISIONAL FIX

**⚠️ THIS IS A PROVISIONAL FIX FOR ISSUE #3** - Makes capabilities single-use. See [security-analysis.md](../../doc/src/arch/security-analysis.md) Issue #3 for full analysis.

### PROVISIONAL FIX: Single-Use via Merkle Proof

The circuit verifies subscription state via Merkle proof:

```zk
# Compute subscription leaf hash
subscription_leaf = poseidon_hash(
    subscription_id,
    subscription_state,      # 0 = Active
    subscription_spent_nullifier,
);

# Verify Merkle proof
verified_root = merkle_root(subscription_leaf_pos, subscription_path, subscription_leaf);

# Verify state is Active (0)
less_than_strict(subscription_state, 1);
```

**IMPORTANT: After successful verify, contract MUST mark `subscription_spent_nullifier` as spent.**

### Privacy Leak

This makes capabilities **effectively single-use**:
- Each access reveals the `spent_nullifier`
- All accesses of the same subscription are linkable
- Usage patterns and frequency can be observed

### The PROPER Fix

Would require a different architecture:
1. Don't mark nullifier as spent on each use
2. Use state Merkle tree with non-membership proofs
3. Separate "cancellation" from "usage tracking"

**Status**: PROVISIONALLY FIXED - enables cancellation enforcement but with privacy tradeoff.

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
