# DarkWow Stablecoin Contract

A privacy-preserving collateralized stablecoin for DarkWow with configurable models and multi-collateral support.

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
- **DRK** (DarkWow) - Native token collateral
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

### Hot (Frequent, Cheap)

| Function | ID | Description | Child Call |
|----------|-----|-------------|------------|
| `InitializeV1` | 0x00 | Initialize pool with model and collateral parameters | - |
| `OpenPositionV1` | 0x01 | Open/deposit collateral into global pool | - |
| `AddCollateralV1` | 0x02 | Add collateral to existing position | - |
| `RemoveCollateralV1` | 0x03 | Withdraw collateral (if ratio allows) | money_v3::transfer_v1 |
| `MintStableV1` | 0x04 | Mint stablecoins against pool | money_v3::transfer_v1 |
| `RepayStableV1` | 0x05 | Repay debt to reduce debt share | - |
| `LiquidateV1` | 0x06 | Liquidate undercollateralized pool | money_v3::transfer_v1 |
| `UpdateConfigV1` | 0x07 | Update pool parameters (governance) | - |

### Cold (Rare, Precise - uses BaseDiv)

| Function | ID | Description | Cost |
|----------|-----|-------------|------|
| `GovernanceReportV1` | 0x08 | Precise collateral/debt ratio for governance | ~500 muls |
| `AccrueInterestV1` | 0x09 | Precise interest accrual calculation | ~500 muls |

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
    pub collateral_type: CollateralType,  // XMR, DRK, or ETH
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

| Circuit | File | Description | Public Inputs |
|---------|------|-------------|---------------|
| `open_position_v1.zk` | ✅ Compiled | Open/deposit collateral position | position_commitment, position_nullifier |
| `mint_stable_v1.zk` | ✅ Compiled | Mint stablecoin against pool | old_commitment, new_commitment, position_nullifier |
| `liquidate_v1.zk` | ✅ Compiled | Liquidate undercollateralized pool | old_commitment, new_commitment, position_nullifier |
| `governance_report_v1.zk` | ✅ Compiled | Precise collateral/debt ratio reporting | total_collateral, total_debt, collateral_ratio_bps, interest_accrued, report_timestamp |
| `accrue_interest_v1.zk` | ✅ Compiled | Precise interest accrual calculation | old_total_debt, new_total_debt, interest_amount |

### open_position_v1.zk

Proves a valid collateral deposit:
- **Public inputs**: position_commitment, position_nullifier
- **Private inputs**: owner_secret, collateral_amount, debt_amount, collateral_type, blinds
- **Verification**: Commitment correctly formed, amount valid, collateralization ratio maintained

### mint_stable_v1.zk

Proves stablecoin can be minted:
- **Public inputs**: old_commitment, new_commitment, position_nullifier
- **Private inputs**: owner_secret, old_collateral, old_debt, mint_amount, blinds
- **Verification**: Pool collateralization ratio maintained after minting

### liquidate_v1.zk

Proves pool is undercollateralized:
- **Public inputs**: old_commitment, new_commitment, position_nullifier
- **Private inputs**: owner_secret, collateral_amount, debt_amount, liquidation_penalty, current_price, liquidator_reward, blinds
- **Verification**: Pool is undercollateralized, liquidation penalty correct, reward calculated

### governance_report_v1.zk (Cold - ~500 field muls)

Proves precise collateral/debt ratio for governance:
- **Public inputs**: total_collateral, total_debt, collateral_ratio_bps, interest_accrued, report_timestamp, reporter_pub_x/y
- **Private inputs**: reporter_secret, rate_per_second, time_elapsed
- **Verification**: Uses BaseDiv for exact ratio calculation

### accrue_interest_v1.zk (Cold - ~500 field muls)

Proves precise interest accrual calculation:
- **Public inputs**: old_total_debt, new_total_debt, interest_amount, rate_per_second, time_elapsed, accumulator_pub_x/y
- **Private inputs**: accumulator_secret, old_total_debt, rate_per_second, time_elapsed
- **Verification**: Uses BaseDiv for exact interest calculation

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

| Aspect | MakerDAO DAI | DarkWow Stablecoin (Pooled) |
|--------|--------------|---------------------------|
| Price oracle | Chainlink | AMM TWAP (decentralized) |
| Debt model | Individual CDP | Pooled debt |
| Privacy | Public | Full ZK privacy |
| Liquidation visibility | Public positions | Pool shortfall only |
| ZK complexity | Per-position | Pool-level |
| Freeze authority | Maker Foundation | None |

## Governance Best Practices

**Governance integration is a compositional concern for deployers**, not the contract itself. The contract provides financial primitives; governance organization is your responsibility.

### Pre-Deployment Checklist

1. **DAO should pre-exist deployment**
   - Create DAO and operational BEFORE stablecoin deployment
   - Define governance token and initial supply
   - Set up voting mechanisms

2. **Deployment wallet = DAO multisig**
   - Deployer wallet should be a DAO multisig, not an individual
   - Dead man switch is backup — primary governance is the DAO
   - All executive actions via DAO voting

3. **Initial parameters via governance**
   - Minimum collateralization ratio
   - Liquidation thresholds
   - PI controller settings
   - Dead man switch configuration

### Staking Integration (External)

Staking tokens to the stablecoin contract for governance weight is configured at the **DAO level**, not the contract level:

```
DAO Configuration (external):
  - Define governance token
  - Set staking rewards
  - Configure voting weight

Stablecoin Contract (this):
  - Provides financial primitives
  - Dead man switch for safety
  - Emits events for governance tracking
```

The contract just provides the financial primitives. How staking integrates with your DAO's governance is your design decision.

### DrainProtection (Optional Layer)

- **Dead man switch is the minimum** (already in contract)
- Deployers can add [DrainProtection](../drain_protection/README.md) as an additional safety layer
- 8 best practices available but not required
- Your governance structure determines which practices make sense

### Summary

| Concern | Where Decided |
|---------|--------------|
| Collateral types | Contract deployment |
| Model selection | Contract deployment |
| Interest rates | DAO governance |
| Emergency shutdown | Dead man switch (contract) + DAO |
| Staking for governance | DAO organization |
| Executive actions | DAO multisig |

**The contract provides tools; your organization provides governance.**

## Implementation Status

### Complete

- [x] Multi-collateral support (ETH, XMR, DRK)
- [x] Configurable models (PooledDebt, Liquity, Fractional, IndividualCdp)
- [x] Dead man switch safety feature
- [x] Hot/cold circuit separation
- [x] LessThanOrEqual verified sound
- [x] BaseDiv implemented
- [x] All 5 ZK circuits compiled (open_position, mint_stable, liquidate, governance_report, accrue_interest)
- [x] Test harness with all 5 circuits loaded
- [x] Heavyweight pipeline endpoint testing (OpenPositionV1 0x01, MintStableV1 0x04 (w/ money_v3 child call), GovernanceReportV1 0x08, AccrueInterestV1 0x09)
- [x] money_v3 child call validation (MintStableV1, RemoveCollateralV1, LiquidateV1)
- [x] Full money_v3::transfer_v1 child call integration

### In Progress

- P2P oracle integration for TWAP price feed
- CDP Note integration with Money contract

### Needed

- AMM pool for price discovery
- Full lifecycle testing (RemoveCollateralV1, LiquidateV1 endpoints)

## References

- [P2P Oracle Design](https://technologytruth.substack.com/p/nether-say-nether-again)
- [Synthetix Pooled Debt](https://synthetix.io/)
- [DarkWow Money Contract](../money/)
- [DarkWow SDK](../../../src/sdk/)
- [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md)
