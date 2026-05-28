# Relayer Endowment Contract

> **Note:** The endowment contract is implemented and deployed on the DarkWow testnet. It underwent internal security hardening (May 2026) with proportional slashing, fee caps, and force settlement now live. See [Security Audit](../contract/audit.md).

A composable contract that enables external capital providers ("backers") to deploy capital to relayers in exchange for a share of the relayer's bridge fees.

## Purpose

Relayers need additional capital coverage for guaranteed withdrawals. The Endowment contract enables:

- **Capital deployment**: External backers provide additional stake to relayers
- **Reputation-gated deployment**: Backers can set minimum performance thresholds (`min_success_rate_bp`, `max_slash_count`) to filter relayers by proven track record
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
    total_slashed: u64,            // Lifetime slash count (May 2026)
    total_successful: u64,         // Lifetime successful withdrawals (May 2026)
    created_at: u64,
    last_settlement_height: u64,   // Block of last settlement (May 2026)
    total_collected_fees_log: u64, // Backer-auditable fee log (May 2026)
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
| 0x01 | DeployCapitalV1 | Backer deploys capital (with optional reputation thresholds: `min_success_rate_bp`, `max_slash_count` — rejects relayers that fail thresholds, error `ReputationCheckFailed`) |
| 0x02 | WithdrawDeploymentV1 | Backer withdraws their deployment |
| 0x03 | ClaimRelayerFeesV1 | Backer claims their share of relayer fees |
| 0x04 | SettleFeesV1 | Relayer settles fees to backers (resets `last_settlement_height`) |
| 0x05 | UpdateConfigV1 | Update fee configuration |
| 0x06 | ForceSettleV1 | Backer force-settles fees after 1000-block relayer inactivity (May 2026) |

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

## Reputation-Gated Deployment (May 2026 Phase 2d)

`DeployCapitalV1` now accepts two optional reputation thresholds:

- `min_success_rate_bp: Option<u64>` — Minimum success rate in basis points (e.g., 9500 = 95%)
- `max_slash_count: Option<u64>` — Maximum allowed lifetime slash count

Before capital is deployed, the contract checks:
1. If `max_slash_count` is set: rejects if `account.total_slashed > max_slash_count`
2. If `min_success_rate_bp` is set: computes `total_events = total_successful + total_slashed`, then `success_rate = (total_successful * 10000) / total_events` (defaults to 10000 if no events). Rejects if `success_rate < min_success_rate_bp`.

This prevents backer capital from flowing to poorly-performing relayers and directly addresses adverse selection (finding #15).

## Force Settlement (May 2026 Hardening)

If a relayer fails to call `SettleFeesV1` within 1000 blocks, any backer can call `ForceSettleV1` (opcode `0x06`) to force a pro-rata distribution of accumulated fees. This prevents relayer fee evasion and gives backers on-chain recourse if a relayer becomes unresponsive or dishonest.

## Composability

This contract composes with:

- **promissory_note contract**: For token transfers (deploy/withdraw)
- **pool_stake**: For coverage backing of guaranteed withdrawals

## See Also

- [Pool Stake Contract](pool_stake.md) - Pooled coverage for withdrawals
- [DAO-Escrow Contract](../contract/dao_escrow.md) - Similar endowment fund patterns
- [Relayer Economics](relayer_economics.md) - Economic layer overview