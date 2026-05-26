# DarkWow Stablecoin Architecture

*Privacy-preserving collateralized stablecoin with configurable models and multi-collateral support.*

## Overview

The DarkWow stablecoin is a privacy-preserving collateralized stablecoin that supports:

- **Multi-collateral**: XMR, DRKW, and ETH (via bridge) as collateral
- **Configurable models**: PooledDebt, Liquity, Fractional, or IndividualCDP
- **Hot/Cold separation**: Cheap user operations, precise governance
- **Dead man switch**: Emergency shutdown if executive authority unresponsive
- **Full ZK privacy**: All positions, amounts, and identities hidden

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Stablecoin Contract                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │   Collateral │    │   Debt Pool  │    │    PI        │       │
│  │    Pools    │◄──▶│  (global)   │◄──▶│  Controller │       │
│  └──────────────┘    └──────────────┘    └──────────────┘       │
│                                                                   │
│  Collateral Types: XMR, DRKW, ETH                                  │
│  Models: PooledDebt | Liquity | Fractional | IndividualCDP        │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Configurable Models

The stablecoin deployer selects the model at initialization:

| Model | Min Collateral | Liquidation | Governance |
|-------|---------------|-------------|------------|
| **PooledDebt** | 150% | Global pool | PI Controller |
| **Liquity** | 110% | Stability pool | None |
| **Fractional** | 80% | Mixed | Partial algorithmic |
| **IndividualCDP** | 150% | Per-position | Per-asset |

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| `InitializeV1` | `0x00` | Initialize stablecoin with model and parameters |
| `OpenPositionV1` | `0x01` | Open a new CDP position |
| `AddCollateralV1` | `0x02` | Add collateral to existing position |
| `RemoveCollateralV1` | `0x03` | Remove collateral (subject to ratio check) |
| `MintStableV1` | `0x04` | Mint stablecoin against collateral |
| `RepayStableV1` | `0x05` | Repay stablecoin debt |
| `LiquidateV1` | `0x06` | Liquidate undercollateralized position |
| `UpdateConfigV1` | `0x07` | Update configuration parameters |
| `GovernanceReportV1` | `0x08` | Precise collateral/debt ratio (BaseDiv, cold) |
| `AccrueInterestV1` | `0x09` | Precise interest accrual (BaseDiv, cold) |

## ZK Circuits

All 9 circuits compiled to `.zk.bin`:

| Circuit | Purpose |
|---------|---------|
| `init_v1.zk` | Prove initialization parameters |
| `open_position_v1.zk` | Prove CDP position creation |
| `add_collateral_v1.zk` | Prove collateral addition |
| `remove_collateral_v1.zk` | Prove collateral removal with ratio check |
| `mint_stable_v1.zk` | Prove stablecoin minting within limits |
| `repay_stable_v1.zk` | Prove debt repayment |
| `liquidate_v1.zk` | Prove liquidation conditions met |
| `governance_report_v1.zk` | Prove precise ratio report (BaseDiv) |
| `accrue_interest_v1.zk` | Prove precise interest calculation (BaseDiv) |

## Multi-Collateral Support

Collateral types and risk parameters:

| Asset | Haircut | Liquidation Threshold | Max Debt Share |
|-------|---------|---------------------|----------------|
| ETH | 2% | 125% | 50% |
| XMR | 1% | 130% | 30% |
| DRKW | 0% | 150% | 100% |

**Haircut**: Value discount applied before collateral calculation
**Max debt share**: Maximum % of total debt this collateral can back

## Hot/Cold Circuit Separation

Operations are split by computational cost:

### Hot (Cheap, Frequent)

| Operation | Method | Cost |
|-----------|--------|------|
| Deposit | LTE + cross-mul | ~100 constraints |
| Mint | LTE + cross-mul | ~100 constraints |
| Withdraw | LTE + cross-mul | ~100 constraints |
| Repay | LTE + cross-mul | ~100 constraints |

### Cold (Expensive, Rare)

| Operation | Method | Cost |
|-----------|--------|------|
| `GovernanceReportV1` | BaseDiv | ~500 field muls |
| `AccrueInterestV1` | BaseDiv | ~500 field muls |

Cold operations are for monthly governance reporting and precise interest calculations. Hot operations handle user actions.

## Dead Man Switch

Emergency shutdown mechanism if executive authority becomes unresponsive:

| Setting | Default | Description |
|---------|---------|-------------|
| `enabled` | false | Opt-in safety feature |
| `timeout_blocks` | 43200 | ~30 days at 1 block/min |

**Trigger actions:**
- `LiquidateAll`: Emergency settlement at current prices
- `DisableMinting`: No new debt, positions remain
- `EnableFreeWithdrawals`: Users can exit without ratio checks

## Price Feed

AMM-based TWAP price discovery (P2P Oracle):

```
External Pool → TWAP → PI Controller → Redemption Rate
```

No centralized oracles - the AMM pool itself provides price discovery.

## Opcode Status

| Opcode | Status | Use |
|--------|--------|-----|
| `LessThanOrEqual` (0x55) | ✅ Verified Sound | Collateralization checks |
| `BaseDiv` (0x58) | ✅ Implemented | Interest/ratio calculations |
| `less_than_strict` | ✅ Sound | Bounded comparisons |

## Relationship to Bridge

The bridge provides multi-collateral support:

| Chain | Token | Integration |
|-------|-------|-------------|
| Ethereum | ETH | Native via bridge |
| Monero | XMR | Privacy-native |
| Zcash | ZEC | Shielded |
| Litecoin | LTC | Trade pair |

## Governance: Compositional Concern

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

Staking tokens to the stablecoin for governance weight is configured at the **DAO level**, not the contract level. The contract provides financial primitives; how staking integrates with your DAO's governance is your design decision.

### DrainProtection (Optional)

- **Dead man switch is the minimum** (already in contract)
- Deployers can add [DrainProtection](./drain_protection.md) as an additional layer
- 8 best practices available but not required
- Your governance structure determines which practices make sense

### Summary: Where Decisions Are Made

| Concern | Where Decided |
|---------|----------------|
| Collateral types | Contract deployment |
| Model selection | Contract deployment |
| Interest rates | DAO governance |
| Emergency shutdown | Dead man switch (contract) + DAO |
| Staking for governance | DAO organization |
| Executive actions | DAO multisig |

## See Also

- [Stablecoin Contract](../../../src/contract/stablecoin/)
- [Bridge](./bridge.md)
- [Opcodes](../arch/zk/opcodes.md)
- [DrainProtection](./drain_protection.md)
