# Relayer Endowment Contract

> **Note:** The endowment model described here is aspirational. It depends on the multi-chain bridge and relayer infrastructure which are still under development.

A composable contract that enables external capital providers ("backers") to deploy capital to relayers in exchange for a share of the relayer's bridge fees.

## Purpose

Relayers need additional capital coverage for guaranteed withdrawals. The Endowment contract enables:

- **Capital deployment**: External backers provide additional stake to relayers
- **Fee sharing**: Backers earn a percentage of relayer's bridge fees
- **Yield generation**: Backers receive yield for providing capital backing

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                      Relayer Endowment Layer                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐      │
│  │  Endowment  │    │  Deployment  │    │    Fee       │      │
│  │  Registry    │◄──►│    Tree      │◄──►│  Accumulator │      │
│  │              │    │              │    │              │      │
│  │ - relayer_pub│    │ - deployment │    │ - accumulated│      │
│  │ - total_dep  │    │ - backer_pub│    │ - fees       │      │
│  │ - active_cnt │    │ - amount    │    │              │      │
│  └──────────────┘    └──────────────┘    └──────────────┘      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
           │                    │                    │
           ▼                    ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Relayer Operations                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  1. Backer deploys capital to relayer's endowment               │
│  2. Relayer earns fees, accumulates for backers                 │
│  3. Backer claims proportional fee share                         │
│  4. Backer can withdraw deployment + earnings                   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Data Model

### RelayerEndowmentAccount

Tracks aggregate endowment for each relayer:

```rust
struct RelayerEndowmentAccount {
    relayer_pub: PublicKey,
    total_deployed: u64,           // Total capital from all deployments
    active_deployments: u64,       // Number of active deployments
    accumulated_fees: u64,         // Fees to distribute to backers
    default_backer_cut_bp: u32,    // Default fee cut (basis points)
    created_at: u64,
    is_active: bool,
}
```

### EndowmentDeployment

Individual backer deployment:

```rust
struct EndowmentDeployment {
    deployment_id: pallas::Base,
    relayer_pub: PublicKey,
    backer_pub: PublicKey,
    amount: u64,
    backer_cut_bp: u32,            // Backer's cut of fees (basis points)
    accumulated_fees: u64,
    deployed_at: u64,
    withdraw_requested_at: Option<u64>,
    withdrawn: bool,
}
```

## Contract Functions

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | InitializeV1 | Initialize endowment account for a relayer |
| 0x01 | DeployCapitalV1 | Backer deploys capital to a relayer's endowment |
| 0x02 | WithdrawDeploymentV1 | Backer withdraws their deployment |
| 0x03 | ClaimRelayerFeesV1 | Backer claims their share of relayer fees |
| 0x04 | SettleFeesV1 | Relayer settles fees to backers |
| 0x05 | UpdateConfigV1 | Update fee configuration |

## Economic Model

### Fee Split

The `backer_cut_bp` parameter (in basis points) determines the split:

```
backer_earnings = total_fees × backer_cut_bp / 10000
relayer_earnings = total_fees × (10000 - backer_cut_bp) / 10000
```

For example, with `backer_cut_bp = 2000` (20%):
- Backer earns 20% of bridge fees
- Relayer keeps 80% of bridge fees

## Composability

This contract composes with:

- **money contract**: For token transfers (deploy/withdraw)
- **pool_stake**: For coverage backing of guaranteed withdrawals

## See Also

- [Pool Stake Contract](pool_stake.md) - Pooled coverage for withdrawals
- [DAO-Escrow Contract](../contract/dao_escrow.md) - Similar endowment fund patterns
- [Relayer Economics](relayer_economics.md) - Economic layer overview