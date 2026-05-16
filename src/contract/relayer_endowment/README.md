# Relayer Endowment Contract

> **USE AT YOUR OWN RISK.** This contract has undergone internal simulation-based security review (May 2026) but has NOT been independently audited. Deployed capital is exposed to relayer operational risk. See [AUDIT.md](../AUDIT.md) for full findings, mitigations, and residual risks.

A composable contract that enables external capital providers ("backers") to deploy capital to relayers in exchange for a share of the relayer's bridge fees. This enables relayers to operate with more coverage than their own stake alone.

## Overview

This contract solves the capital requirements problem for relayers:

1. **Relayers** need more capital coverage for guaranteed withdrawals
2. **Backers** want yield for deploying capital to relayers
3. **This contract** matches capital supply with relayer demand

## How It Works

```
┌─────────────────┐      ┌───────────────────────┐      ┌─────────────────┐
│     Relayer     │ ───► │  Relayer Endowment    │ ───► │     Backer      │
│ (Needs Capital) │      │      (Layer)          │      │  (Provides $)   │
└─────────────────┘      └───────────────────────┘      └─────────────────┘
        │                         │                         │
        │ Initializes             │ Distributes              │ Deploys capital
        │ endowment               │ fees                     │ for yield
        ▼                         ▼                         ▼
   Relayer can           Backers earn                Backer receives
   accept more           proportional                 backer_cut_bp
   deployments           to their                      of relayer fees
                         deployment
```

### Endowment Flow

1. **Initialize**: A relayer initializes an endowment account
2. **Deploy Capital**: Backer deploys DAI/NETHER to the relayer's endowment
3. **Earn Fees**: Backer receives a percentage (`backer_cut_bp`) of relayer's earned fees
4. **Withdraw**: Backer can withdraw their deployment + accumulated fees

## Economic Model

| Role | Deposit | Earn | Withdraw |
|------|---------|------|----------|
| Backer | Capital to relayer | `backer_cut_bp` of bridge fees | Principal + earnings |
| Relayer | Own stake | `(10000 - backer_cut_bp) / 10000` of fees | Full fees minus backer share |

### Fee Split

The `backer_cut_bp` parameter (in basis points) determines the split:

```
backer_earnings = total_fees × backer_cut_bp / 10000
relayer_earnings = total_fees × (10000 - backer_cut_bp) / 10000
```

For example, with `backer_cut_bp = 2000` (20%):
- Backer earns 20% of bridge fees
- Relayer keeps 80% of bridge fees

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize endowment account for a relayer |
| DeployCapitalV1 | 0x01 | Backer deploys capital to a relayer's endowment |
| WithdrawDeploymentV1 | 0x02 | Backer withdraws their deployment |
| ClaimRelayerFeesV1 | 0x03 | Backer claims their share of relayer fees |
| SettleFeesV1 | 0x04 | Relayer settles fees to backers |
| UpdateConfigV1 | 0x05 | Update fee configuration |
| ForceSettleV1 | 0x06 | Backer force-settles fees after relayer inactivity timeout |

## Key Parameters

| Parameter | Description | Example |
|-----------|-------------|---------|
| `default_backer_cut_bp` | Default fee cut for backers (basis points) | 2000 = 20% |
| `backer_cut_bp` | Per-deployment fee cut (basis points) | 2000 = 20% |
| `min_deploy` | Minimum deployment amount | 1,000,000 = 1 DAI |
| `force_settlement_timeout` | Blocks before backers can force-settle | 1000 (~8.3 hours at 30s blocks) |

## Hardening (May 2026)

The endowment contract was hardened in May 2026 to address 3 failure modes related to fee settlement evasion. See [AUDIT.md](../AUDIT.md) for the full audit report.

### Force Settlement

**Problem**: Relayers could earn bridge fees but never call `SettleFeesV1`. Backers had zero on-chain recourse — deployed capital earned zero yield indefinitely.

**Fix**: `ForceSettleV1` (opcode `0x06`) — after `FORCE_SETTLEMENT_TIMEOUT` blocks (1000) without settlement, any backer with an active deployment can force a pro-rata fee distribution. The contract computes the backer's share based on `total_collected_fees_log` and credits it to their deployment.

### Fee Logging

`RelayerEndowmentAccount` now tracks `last_settlement_height` (updated on each `SettleFeesV1` or `ForceSettleV1` call) and `total_collected_fees_log` (for backer audit). These fields let backers verify that the relayer is settling fees regularly and detect evasion before the force timeout elapses.

### Updated Account Model

```rust
pub struct RelayerEndowmentAccount {
    pub relayer_pub: PublicKey,
    pub total_deployed: u64,
    pub active_deployments: u64,
    pub accumulated_fees: u64,
    pub default_backer_cut_bp: u32,
    pub created_at: u64,
    pub last_settlement_height: u64,      // NEW: track settlement activity
    pub total_collected_fees_log: u64,    // NEW: backer-auditable fee log
    pub is_active: bool,
}
```

## Composability

This contract composes with:

- **money contract**: For token transfers (deploy/withdraw)
- **dao_escrow**: Similar endowment fund management patterns
- **pool_stake**: For coverage backing of guaranteed withdrawals

```
┌────────────────────────────────────────────────────────────────┐
│                    Composability Stack                          │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────────┐       ┌─────────────────┐             │
│  │ relayer_endowment   │ ◄──── │    pool_stake   │             │
│  │ (External Capital)  │       │ (Coverage Pool) │             │
│  └──────────┬──────────┘       └────────┬────────┘             │
│             │                           │                      │
│             │         ┌────────┐         │                      │
│             └────────►│ bridge │◄────────┘                      │
│                       │(Guar.) │                                 │
│                       └────┬───┘                                 │
│                            │                                    │
│                            ▼                                    │
│              ┌─────────────────────────┐                       │
│              │     money contract       │                       │
│              │  (Token transfers, stake)│                       │
│              └─────────────────────────┘                       │
│                                                                 │
└────────────────────────────────────────────────────────────────┘
```

## Usage Example

```rust
use darkfi_relayer_endowment_contract::{
    InitializeParamsV1, DeployCapitalParamsV1, ClaimFeesParamsV1
};

// 1. Initialize endowment for a relayer
let init_params = InitializeParamsV1 {
    default_backer_cut_bp: 2000,  // 20% default backer cut
};

// 2. Backer deploys capital
let deploy_params = DeployCapitalParamsV1 {
    relayer_pub: relayer_pubkey,
    amount: 10_000_000,  // 10 DAI
    backer_cut_bp: 2500,  // 25% backer cut for this deployment
};

// 3. Backer claims fee share
let claim_params = ClaimFeesParamsV1 {
    deployment_id: my_deployment_id,
};

// 4. Backer withdraws deployment
let withdraw_params = WithdrawDeploymentParamsV1 {
    deployment_id: my_deployment_id,
};
```

## Comparison with Pool Stake

| Aspect | Pool Stake | Relayer Endowment |
|--------|------------|------------------|
| Purpose | Provide withdrawal coverage | Provide deployment capital |
| Deposit | Stake for coverage | Deploy capital for yield |
| Earnings | Bridge fee share | `backer_cut_bp` of relayer fees |
| Risk | Slashing on failure | Relayer operational risk |
| Relationship | Pooled (shared) | Direct (per-relayer) |

## Comparison with DAO-Escrow Endowment

| Aspect | DAO-Escrow Endowment | Relayer Endowment |
|--------|---------------------|-------------------|
| Purpose | Insurance mutual pool | Relayer capital backing |
| Governance | DAO votes on payouts | Automatic fee distribution |
| Membership | Requires membership note | Open to any backer |
| Fee Model | Premiums into endowment | Deployment share of fees |

## See Also

- [Pool Stake Contract](../pool_stake/) - Pooled coverage for withdrawals
- [DAO-Escrow Contract](../dao_escrow/) - Endowment fund management patterns
- [Bridge Contract](../bridge/) - Guaranteed withdrawal execution
- [Money Contract](../money/) - Token transfers and staking primitives