# DarkFi Stablecoin Architecture

*Privacy-preserving collateralized stablecoin with configurable models and multi-collateral support.*

## Overview

The DarkFi stablecoin is a privacy-preserving collateralized stablecoin that supports:

- **Multi-collateral**: XMR, DRK, and ETH (via bridge) as collateral
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
│  Collateral Types: XMR, DRK, ETH                                  │
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

## Multi-Collateral Support

Collateral types and risk parameters:

| Asset | Haircut | Liquidation Threshold | Max Debt Share |
|-------|---------|---------------------|----------------|
| ETH | 2% | 125% | 50% |
| XMR | 1% | 130% | 30% |
| DRK | 0% | 150% | 100% |

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

## See Also

- [Stablecoin Contract](../../src/contract/stablecoin/)
- [Bridge](./bridge.md)
- [Opcodes](./opcodes.md)
- [Safemath](./safemath.md)
