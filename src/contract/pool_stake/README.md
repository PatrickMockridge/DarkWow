# Pool Stake Contract

A composable contract that enables pooled coverage for relayer withdrawals. Stakers deposit capital into a shared pool to provide guaranteed withdrawal coverage for relayers, earning a share of bridge fees in return.

## Overview

This contract solves the coverage requirements problem for relayer withdrawals:

1. **Relayers** need coverage to execute guaranteed withdrawals
2. **Stakers** want yield for providing coverage backing
3. **This contract** pools capital to provide shared coverage

## How It Works

```
┌─────────────────┐      ┌──────────────────┐      ┌─────────────────┐
│     Relayer     │ ───► │   Pool Stake     │ ───► │    Staker       │
│ (Needs Coverage)│      │    (Layer)       │      │  (Provides $)   │
└─────────────────┘      └──────────────────┘      └─────────────────┘
        │                         │                         │
        │ Requests coverage       │ Distributes              │ Provides
        │ for withdrawal          │ fees                     │ coverage
        ▼                         ▼                         ▼
   Bridge contract          Stakers earn              Earn fee share
   uses coverage            proportional to            for providing
   to execute               their coverage             coverage backing
   guaranteed              贡献
```

### Pool Stake Flow

1. **Create Pool**: A pool is created with coverage parameters
2. **Join Pool**: Stakers deposit capital to provide coverage
3. **Allocate Coverage**: When a relayer needs guaranteed withdrawal, coverage is allocated
4. **Earn Fees**: Stakers earn a proportional share of bridge fees
5. **Leave Pool**: Stakers can request to leave after cooldown

## Economic Model

| Role | Deposit | Earn | Risk |
|------|---------|------|------|
| Staker | Coverage capital | Bridge fee share | Slashing on failed withdrawal |
| Relayer | None | Full bridge fees | Premium paid for guaranteed mode |

### Coverage Ratio

Each pool has a `max_coverage_ratio` determining how much coverage can be allocated:

```
available_coverage = total_stake × max_coverage_ratio
```

For example, a pool with 1M DAI staked and 80% coverage ratio:
- Available coverage = 800K DAI
- Single withdrawal can use up to 800K DAI of coverage

### Fee Distribution

When a guaranteed withdrawal executes successfully:

```
relayer_fees = bridge fees earned
staker_share = relayer_fees × coverage_used / total_allocated_coverage
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| CreatePoolV1 | 0x00 | Create a new staking pool |
| JoinPoolV1 | 0x01 | Join existing pool (stake DAI/NETHER) |
| LeavePoolV1 | 0x02 | Leave pool (after cooldown) |
| AllocateCoverageV1 | 0x03 | Allocate coverage for withdrawal |
| ReleaseCoverageV1 | 0x04 | Release after successful execution |
| SlashCoverageV1 | 0x05 | Slash for failed guaranteed withdrawal |
| ClaimFeesV1 | 0x06 | Claim accumulated relayer fees |
| UpdatePoolConfigV1 | 0x07 | Update pool parameters |

## Pool Parameters

| Parameter | Description | Example |
|-----------|-------------|---------|
| `max_coverage_ratio` | Max coverage % of total stake (basis points) | 8000 = 80% |
| `min_stake` | Minimum stake amount | 1,000,000 = 1 DAI |
| `leave_cooldown_blocks` | Blocks before leave executes | 100 |

## Composability

This contract composes with:

- **money contract**: For token transfers (stake in/out)
- **bridge contract**: For guaranteed withdrawal integration
- **betting_stake**: Similar proportional share logic

```
┌────────────────────────────────────────────────────────────────┐
│                    Composability Stack                          │
├────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐         ┌─────────────────┐                │
│  │   pool_stake    │ ◄────── │     bridge     │                │
│  │  (Coverage Pool)│         │ (Guaranteed    │                │
│  └────────┬────────┘         │  Withdrawals)  │                │
│           │                   └────────┬────────┘                │
│           │                            │                        │
│           ▼                            ▼                        │
│  ┌────────────────────────────────────────────┐                │
│  │              money contract                  │                │
│  │         (Token transfers, staking)            │                │
│  └────────────────────────────────────────────┘                │
│                                                                 │
└────────────────────────────────────────────────────────────────┘
```

## Usage Example

```rust
use darkfi_pool_stake_contract::{CreatePoolParamsV1, JoinPoolParamsV1};

// 1. Create a pool
let create_params = CreatePoolParamsV1 {
    pool_id: my_pool_id,
    owner_pub: my_pubkey,
    max_coverage_ratio: 8000,  // 80%
    min_stake: 1_000_000,       // 1 DAI minimum
};

// 2. Join pool
let join_params = JoinPoolParamsV1 {
    pool_id: my_pool_id,
    member_pub: staker_pub,
    relayer_id: target_relayer_id,
    amount: 10_000_000,  // 10 DAI stake
    coverage_share_bp: 5000,  // 50% of coverage rights
};

// 3. Claim fees
let claim_params = ClaimFeesParamsV1 {
    stake_id: my_stake_id,
};
```

## Risk Profile

| Scenario | Outcome |
|----------|---------|
| Guaranteed withdrawal succeeds | Coverage released, stakers earn fees |
| Guaranteed withdrawal fails | Coverage slashed proportionally |
| Withdrawal is standard (not guaranteed) | No coverage used, no risk |

## Comparison with Betting Stake

| Aspect | Betting Stake | Pool Stake |
|--------|---------------|------------|
| Purpose | Pay betting winnings | Provide withdrawal coverage |
| Risk | Absorb bet losses | Slash on failed withdrawal |
| Earnings | House edge share | Bridge fee share |
| Pooled | Yes | Yes |

## See Also

- [Bridge Contract](../bridge/) - Guaranteed withdrawal execution
- [Betting Stake Contract](../betting_stake/) - Similar pooled capital pattern
- [Relayer Endowment Contract](../relayer_endowment/) - External capital backing for relayers
- [Money Contract](../money/) - Token transfers and staking primitives