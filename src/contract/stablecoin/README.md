# DarkFi Stablecoin Contract

A privacy-preserving collateralized stablecoin for DarkFi with configurable models and multi-collateral support.

## Overview

This contract enables creation of a stablecoin (e.g., a USD-pegged token) that is:

- **Privacy-preserving**: All positions, amounts, and identities are hidden via ZK proofs
- **Multi-collateral**: Support for XMR, DRK, and ETH (via bridge) as collateral
- **Self-stabilizing**: AMM-based TWAP + PI Controller replaces governance
- **Censorship-resistant**: No trusted price oracles, no centralized control
- **Configurable models**: Choose between Pooled Debt, Liquity, Fractional, or Individual CDP

## Configurable Models

The stablecoin contract supports **four deployment models** selected at initialization:

| Model | Min Collateral | Liquidation | Governance | Use Case |
|-------|---------------|-------------|------------|----------|
| **PooledDebt** (default) | 150% | Global pool shortfall | PI Controller | General purpose |
| **Liquity** | 110% | Stability pool | None | Low collateral, fast redemptions |
| **Fractional** | 80% | Mixed | Partial algorithmic | Capital efficient |
| **IndividualCdp** | 150% | Per-position | Per-asset | Maximum control |

## Multi-Collateral Support

Collateral types supported:
- **XMR** (Monero) - Privacy-native collateral
- **DRK** (DarkFi) - Native token collateral
- **ETH** (Ethereum) - Large cap, DAI-backed collateral

Each collateral type has its own risk parameters:

| Parameter | Description |
|-----------|-------------|
| `haircut` | Value discount applied (e.g., 98% for ETH) |
| `liquidation_threshold` | Per-asset liquidation trigger |
| `max_debt_share` | Max % of total debt this collateral can back |

## Dead Man Switch

The dead man switch is a **default safety feature** that triggers emergency shutdown if executive authority becomes unresponsive:

| Setting | Description |
|---------|-------------|
| `enabled` | Enable/disable dead man switch |
| `timeout_blocks` | Time without executive action before trigger (default: 43200 ≈ 30 days) |
| `action` | What happens when triggered |

**Trigger Actions:**

| Action | Behavior |
|--------|----------|
| `LiquidateAll` | Emergency settlement of all positions at current prices |
| `DisableMinting` | No new debt, existing positions remain |
| `EnableFreeWithdrawals` | Users can withdraw without collateralization checks |

**Use case**: If the governance multisig is compromised or the team disappears, the dead man switch ensures users can exit their positions rather than being locked into an unresponsive system.

## Architecture: Pooled Debt vs Individual CDP

This contract uses **pooled debt** (Synthetix-style), not individual CDP (MakerDAO-style).

### Why Pooled Debt for Privacy

**Individual CDP Model problems:**
- Must prove individual position is valid (complex ZK circuits)
- Liquidators see "position ID X was liquidated" (privacy leak)
- Individual nullifiers/commitments for each position leak information
- ZK circuits need to verify per-position collateralization

**Pooled Model advantages:**
- No individual positions to track
- Liquidation is "pool had shortfall" — no position IDs leaked
- Simpler ZK circuits
- All collateral backs all debt — more capital efficient
- No position IDs that could be observed or tracked

### How It Works

```
Traditional (MakerDAO):
┌─────────┐     ┌─────────┐     ┌─────────┐
│ Position│     │ Position│     │ Position│
│   #1    │     │   #2    │     │   #3    │
└────┬────┘     └────┬────┘     └────┬────┘
     │               │               │
     └───────────────┼───────────────┘
                     ▼
              ┌─────────────┐
              │  Debt Pool   │
              │ (aggregated) │
              └─────────────┘

Pooled (Synthetix-style):
┌─────────┐
│ User A  │──┐
│ deposits│  │
└─────────┘  │    ┌─────────────┐     ┌─────────────┐
            ├────▶│  Collateral │────▶│             │
┌─────────┐  │    │    Pool     │     │  Debt Pool  │
│ User B  │──┼───▶│  (global)   │     │  (global)   │
│ deposits│  │    └─────────────┘     │             │
└─────────┘  │                        └──────┬──────┘
            │    ┌─────────────┐              │
┌─────────┐  │    │  Debt Share │◀─────────────┘
│ User C  │──┴───▶│   Record    │     ┌─────────────┐
│ deposits│       │ (per user)  │     │  Stablecoin  │
└─────────┘       └─────────────┘     │    Token    │
                                     └─────────────┘
```

### User Experience

Users don't have "positions" — they have **debt shares**:

1. **Deposit collateral** → Collateral goes into global pool
2. **Mint stablecoins** → Debt shares minted against pool
3. **No individual tracking** → ZK proofs verify pool-level ratios
4. **Liquidation is global** → Either entire pool is healthy or it isn't

## Design Principles

### Traditional CDP Problems (MakerDAO, etc.)

1. **Oracle dependency**: Single source or median of oracles can be manipulated
2. **Governance overhead**: DAO votes needed for rate adjustments
3. **No privacy**: All positions and amounts are public
4. **Individual positions leak info**: Liquidators see specific positions being liquidated
5. **Complex ZK**: Per-position proofs are expensive and leak data

### P2P Oracle + Pooled Debt Solution

```
Traditional: User → Governance-controlled oracle → Price feed
P2P Oracle:  User → AMM TWAP (XMR/DRK pool) → Price feed

Individual CDP: Position #123 → Complex ZK → Liquidator sees "you"
Pooled Debt:   Pool shortfall → Simple ZK → No position data leaked
```

**Key innovations:**

1. **AMM-based TWAP**: The XMR/DRK constant-product pool itself provides price discovery. TWAP naturally smooths out short-term manipulation.

2. **PI Controller**: A Proportional-Integral controller adjusts redemption rate based on TWAP deviation:
   - TWAP > target (premium): rate increases → less borrowing
   - TWAP < target (discount): rate decreases → more borrowing

3. **Full privacy via ZK**: Pool-level commitments hide all individual activity. ZK proofs verify all operations without revealing amounts or identities.

4. **Minimal governance**: The PI controller replaces most governance decisions. Only emergency interventions require DAO action.

## Contract Functions

| Function | ID | Description |
|----------|-----|-------------|
| `InitializeV1` | 0x00 | Initialize pool with model and collateral parameters |
| `DepositCollateralV1` | 0x01 | Deposit collateral into global pool |
| `WithdrawCollateralV1` | 0x02 | Withdraw collateral (if ratio allows) |
| `MintStableV1` | 0x03 | Mint stablecoins against pool |
| `RepayStableV1` | 0x04 | Repay debt to reduce debt share |
| `LiquidateV1` | 0x05 | Liquidate undercollateralized pool |
| `UpdateConfigV1` | 0x06 | Update pool parameters (governance) |

**Note**: The stablecoin model is selected at initialization via `InitializeParams.model` and cannot be changed afterwards.

## Data Structures

### Global Pool State

```rust
/// Global debt pool state
pub struct DebtPool {
    pub total_debt: u64,           // All stablecoins minted
    pub total_collateral: u64,      // Value in stablecoin terms
    pub accumulated_fees: u64,      // Interest accumulated
    pub last_update: u64,
}

/// Collateral pool for specific type
pub struct CollateralPool {
    pub collateral_type: CollateralType,  // XMR or DRK
    pub total_deposited: u64,
    pub value_ratio: u64,
    pub last_update: u64,
}

/// User's debt share
pub struct DebtShare {
    pub owner_pub: (u8[32], u8[32]),     // Owner's public key
    pub debt_amount: u64,                // Stablecoin debt
    pub commitment: IntentCommitment,    // Privacy commitment
    pub created_at: u64,
    pub updated_at: u64,
}
```

### Operation Parameters

```rust
/// Deposit collateral into the pool
pub struct DepositCollateralParams {
    pub deposit_commitment: IntentCommitment,
    pub collateral_amount: u64,
    pub collateral_type: CollateralType,
    pub proof: Vec<u8>,           // ZK: deposit is valid
    pub fee: u64,
}

/// Withdraw collateral from the pool
pub struct WithdrawCollateralParams {
    pub withdrawal_nullifier: IntentNullifier,
    pub new_commitment: IntentCommitment,
    pub withdraw_amount: u64,
    pub proof: Vec<u8>,           // ZK: withdrawal doesn't violate ratio
    pub fee: u64,
}

/// Mint stablecoin against collateral pool
pub struct MintStableParams {
    pub mint_commitment: IntentCommitment,
    pub mint_amount: u64,
    pub total_debt: u64,          // For ratio check
    pub total_collateral: u64,    // For ratio check
    pub proof: Vec<u8>,           // ZK: mint doesn't violate ratio
    pub fee: u64,
}
```

## Security Model

### Collateralization Requirements

- **Minimum collateralization**: 150% (15000 basis points)
- **Liquidation threshold**: 130% (13000 basis points)
- **Liquidation penalty**: 10% (1000 basis points)

### PI Controller Parameters

- **Kp (proportional gain)**: Configurable
- **Ki (integral gain)**: Configurable
- **TWAP window**: Configurable (e.g., 1 hour)
- **Price deviation threshold**: Configurable (e.g., 5%)

## Model Comparison

### PooledDebt (Synthetix-style)
- All collateral backs all debt (global pool)
- Liquidation is global: pool is either healthy or not
- Simpler ZK circuits, better privacy
- PI Controller for rate adjustments

### Liquity-style
- Minimum 110% collateralization
- Stability pool for redemptions
- Instant liquidation (no oracle delays)
- No governance, no stability fee

### Fractional (Frax-style)
- 80% collateral + 20% algorithmic
- Seigniorage share mechanism
- More capital efficient
- Requires algorithmic minting logic

### Individual CDP
- Per-position tracking (more ZK complex)
- Position IDs and nullifiers leak some data
- More control over individual positions
- Each user chooses: pooled or individual

The **PooledDebt model** is recommended for maximum privacy. Individual CDP is available for use cases requiring per-position control.

## ZK Circuits

The stablecoin contract uses ZK circuits for privacy-preserving pool operations:

### deposit_v1.zk

Proves a valid collateral deposit:
- **Public inputs**: deposit_commitment, collateral_type, total_collateral
- **Private inputs**: collateral_amount, depositor_secret
- **Verification**: Commitment correctly formed, amount valid

### withdraw_v1.zk

Proves collateral can be withdrawn:
- **Public inputs**: withdrawal_nullifier, new_commitment, withdraw_amount, total_pool
- **Private inputs**: withdrawer_secret, current_collateral
- **Verification**: Nullifier valid, pool ratio maintained

### mint_v1.zk

Proves stablecoin can be minted:
- **Public inputs**: mint_commitment, mint_amount, total_debt, total_collateral
- **Private inputs**: minter_secret, debt_share
- **Verification**: Pool collateralization ratio maintained

### liquidate_v1.zk

Proves pool is undercollateralized:
- **Public inputs**: liquidation_commitment, total_debt, total_collateral, current_price, debt_to_cover
- **Private inputs**: liquidator_secret, liquidation_reward
- **Verification**: Pool is undercollateralized, liquidation penalty correct

## Base Field Arithmetic

ZK circuits operate in a finite field — the Pallas field defined by prime `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`. All arithmetic wraps at `p`, which breaks normal integer intuitions:

```zk
# In the field, p-1 ≡ -1, so comparisons must be carefully designed
# Ratio comparisons (e.g., collateral/debt >= 2.0) require cross-multiplication
# Never use division in a circuit — always cross-multiply
```

**Why this matters for stablecoin**: The collateralization ratio check `collateral / debt >= min_ratio` must be expressed as field arithmetic. Direct division (`BaseDiv`) doesn't exist — instead we prove `collateral * 1 >= min_ratio * debt` via cross-multiplication using `base_mul` + `less_than_strict`.

**The pattern (from `dao/exec.zk` lines 118-126)**:
```zk
# To prove: collateral/debt >= 2.0 (200% collateralization)
# We prove: collateral >= 2 * debt
# I.e., collateral - 2*debt >= 0 as integers (not field elements!)

lhs = base_mul(collateral, 1);          # collateral * 1
rhs = base_mul(liquidation_threshold, debt_value);  # threshold * debt
less_than_strict(rhs, lhs);             # Prove: rhs < lhs
# If this passes, we know: threshold * debt < collateral (as integers, not field!)
```

**Why this works**: Cross-multiplication avoids division entirely. We compute `threshold * debt` and prove it's less than `collateral`. The field wraparound doesn't break this because we're proving a relation between products, not computing a quotient.

**The core challenge**: Field wraparound means `a - b` as field subtraction is not the same as `a - b` as integer subtraction when `a < b`. The `less_than_strict` opcode constrains this correctly, but only when the inputs are bounded to prevent wraparound.

**See**: [Field Arithmetic Constraints](../../../doc/src/arch/field_arithmetic.md) for the full treatment.

## Opcode Status: LessThanOrEqual and BaseDiv

**LessThanOrEqual (0x55)** is now **verified sound** via Lean 4 exhaustive testing.

**BaseDiv (0x58)** is now **implemented** using binary exponentiation (Fermat's theorem).

| Opcode | Status | Use in Stablecoin |
|--------|--------|-------------------|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Collateralization checks |
| `BaseDiv` (0x58) | ✅ Implemented | Interest/ratio calculations |
| `less_than_strict` | ✅ Sound | Bounded comparisons |

**Implementation** (from `open_position_v1.zk` line 82):
```zk
is_lte = less_than_or_equal(two_times_debt, collateral_amount);
constrain_equal_base(is_lte, ONE);
```

**Historical note**: This section previously described LessThanOrEqual as having "technical debt" and safemath as a "workaround". The circuits have always used `less_than_or_equal` directly. The safemath pattern is retained for legacy reference — see [Safemath](../../../doc/src/arch/safemath.md).

**See**:
- [Opcodes Reference](../../../doc/src/arch/opcodes.md) for verification details
- [Safemath](../../../doc/src/arch/safemath.md) for legacy pattern reference

## Comparison

| Aspect | MakerDAO DAI | DarkFi Stablecoin (Pooled) |
|--------|--------------|---------------------------|
| Price oracle | Chainlink | AMM TWAP (decentralized) |
| Debt model | Individual CDP | Pooled debt |
| Privacy | Public | Full ZK privacy |
| Liquidation visibility | Public positions | Pool shortfall only |
| ZK complexity | Per-position | Pool-level |
| Freeze authority | Maker Foundation | None |

## Implementation Status

This is a **draft/placeholder** for pooled debt architecture.

### Blockers

1. **No P2P oracle** — The XMR/DRK AMM pool for TWAP price discovery does not yet exist
2. **CDP Note integration** — Money contract's `spend_hook` to CDP engine not implemented
3. **Integration tests needed** — Cannot verify full lifecycle without testnet

### What It Needs

First: P2P oracle / AMM integration to supply TWAP price. Second: integration testing of the full lifecycle.

## References

- [P2P Oracle Design](https://technologytruth.substack.com/p/nether-say-nether-again)
- [Synthetix Pooled Debt](https://synthetix.io/)
- [DarkFi Money Contract](../money/)
- [DarkFi SDK](../../../src/sdk/)
- [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md)
