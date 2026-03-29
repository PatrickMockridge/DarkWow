# DarkFi Stablecoin Contract (Pooled Debt)

A privacy-preserving collateralized stablecoin for DarkFi using Synthetix-style pooled debt.

## Overview

This contract enables creation of a stablecoin (e.g., a USD-pegged token) that is:

- **Privacy-preserving**: All positions, amounts, and identities are hidden via ZK proofs
- **Pooled debt**: All collateral backs all debt — no individual position tracking
- **Self-stabilizing**: AMM-based TWAP + PI Controller replaces governance
- **Censorship-resistant**: No trusted price oracles, no centralized control

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
| `InitializeV1` | 0x00 | Initialize pool with parameters |
| `DepositCollateralV1` | 0x01 | Deposit collateral into global pool |
| `WithdrawCollateralV1` | 0x02 | Withdraw collateral (if ratio allows) |
| `MintStableV1` | 0x03 | Mint stablecoins against pool |
| `RepayStableV1` | 0x04 | Repay debt to reduce debt share |
| `LiquidateV1` | 0x05 | Liquidate undercollateralized pool |
| `UpdateConfigV1` | 0x06 | Update pool parameters (governance) |

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

## Why Not Individual CDP?

Individual CDP is **possible** but **not part of MVP** because:

1. **ZK complexity**: Per-position proofs require demonstrating each position's collateralization separately
2. **Privacy leakage**: Position IDs and nullifiers allow observing who was liquidated
3. **Liquidation complexity**: Liquidators must identify specific undercollateralized positions
4. **Implementation time**: Pooled model is simpler and faster to implement

### To Add Individual CDP Later

Individual CDP can be layered on top of pooled debt:

1. Layer individual position tracking on top of pooled debt
2. Use attestation contract to verify individual positions
3. Each user chooses: pooled (simpler) or individual (more control)

The pooled model provides a **privacy-preserving foundation**. Individual CDP adds complexity and information leakage that may not be acceptable in an anonymous context.

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

## Opcode Safety

**`LessThanOrEqual` is a grey-market good — buyer beware.**

The stablecoin circuits use experimental opcodes for collateralization checks:

| Opcode | Purpose | Status |
|--------|---------|--------|
| `LessThanOrEqual` | Ratio comparison | **Experimental** |
| `less_than_strict` | Constrain-only comparison | Safe (no return value) |

**What production readiness requires**:
1. Integration test: deposit → mint → repay → liquidate (adversarial)
2. Formal soundness proof or concrete bound on delta-invert failure
3. Audit by a ZK circuit expert with formal verification
4. Fuzzing with adversarial inputs: values near `p`, near 0, overflow cases

**See**: [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md) for the full delta-invert analysis.

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

1. **`LessThanOrEqual` is experimental** — Grey-market goods; delta-invert soundness concern
2. **No P2P oracle** — The XMR/DRK AMM pool for TWAP price discovery does not yet exist
3. **Integration tests needed** — Cannot verify full lifecycle without testnet

### What It Needs

First: P2P oracle / AMM integration to supply TWAP price. Second: integration testing of the full lifecycle.

## References

- [P2P Oracle Design](https://technologytruth.substack.com/p/nether-say-nether-again)
- [Synthetix Pooled Debt](https://synthetix.io/)
- [DarkFi Money Contract](../money/)
- [DarkFi SDK](../../../src/sdk/)
- [zkVM Primitive Layer](../../../doc/src/arch/zkvm_primitives.md)
