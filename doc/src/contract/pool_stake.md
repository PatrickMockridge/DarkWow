# Pool Stake Contract

A composable contract that enables pooled coverage for relayer withdrawals. Stakers deposit capital into a shared pool to provide guaranteed withdrawal coverage for relayers, earning a share of bridge fees in return.

## Token Movement

PoolStake uses [PromissoryNote](promissory_note.md) `TransferV1` (0x04) child calls
for all token movement. Join, Leave, Slash, and ClaimFees all use `transfer_v1`
rather than Purse operations. Stakes and coverage allocations are tracked via direct
DB state.

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
   guaranteed              contribution
```

### Pool Stake Flow

1. **Create Pool**: A pool is created with coverage parameters
2. **Join Pool**: Stakers deposit capital to provide coverage
3. **Allocate Coverage**: When a relayer needs guaranteed withdrawal, coverage is allocated
4. **Release/Slash**: On success, coverage is released; on failure, coverage is slashed
5. **Claim Fees**: Stakers earn a proportional share of bridge fees

## Contract Functions

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | CreatePoolV1 | Create a new staking pool |
| 0x01 | JoinPoolV1 | Join existing pool (stake DAI/NETHER) |
| 0x02 | LeavePoolV1 | Leave pool (after cooldown) |
| 0x03 | AllocateCoverageV1 | Allocate coverage for withdrawal |
| 0x04 | ReleaseCoverageV1 | Release after successful execution |
| 0x05 | SlashCoverageV1 | Slash for failed guaranteed withdrawal |
| 0x06 | ClaimFeesV1 | Claim accumulated relayer fees |
| 0x07 | UpdatePoolConfigV1 | Update pool parameters |
| 0x08 | RebalancePoolSharesV1 | Rebalance staker shares after slash/coverage event |

## Data Model

### PoolStakeRegistry

```rust
pub struct PoolStakeRegistry {
    pub pool_id: pallas::Base,
    pub owner_pub: PublicKey,
    pub total_stake: u64,
    pub available_coverage: u64,
    pub allocated_coverage: u64,
    pub member_count: u64,
    pub max_coverage_ratio: u32,
    pub operator_fee_bp: u32,
    pub created_at: u64,
    pub is_active: bool,
}
```

### PoolMemberStake

```rust
pub struct PoolMemberStake {
    pub stake_id: pallas::Base,
    pub pool_id: pallas::Base,
    pub member_pub: PublicKey,
    pub relayer_id: [u8; 32],
    pub original_amount: u64,
    pub current_amount: u64,
    pub coverage_contribution: u64,
    pub pool_share_bp: u32,
    pub accumulated_fees: u64,
    pub created_at: u64,
    pub leave_requested_at: Option<u64>,
    pub is_active: bool,
}
```

### CoverageAllocation

```rust
pub struct CoverageAllocation {
    pub allocation_id: pallas::Base,
    pub pool_id: pallas::Base,
    pub withdrawal_nullifier: [u8; 32],
    pub amount: u64,
    pub contributing_members: Vec<pallas::Base>,
    pub created_at: u64,
    pub timeout_height: u64,
    pub executed: bool,
    pub slashed: bool,
}
```

## Economic Model

| Role | Deposit | Earn | Risk |
|------|---------|------|------|
| Staker | Coverage capital | Bridge fee share | Slashing on failed withdrawal |
| Relayer | None | Full bridge fees | Premium paid for guaranteed mode |

### Coverage Ratio

Each pool has a `max_coverage_ratio` determining how much coverage can be allocated:

```
available_coverage = total_stake × max_coverage_ratio / 10000
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

## Comparison with Betting Stake

| Aspect | Betting Stake | Pool Stake |
|--------|---------------|------------|
| Purpose | Pay betting winnings | Provide withdrawal coverage |
| Risk | Absorb bet losses | Slash on failed withdrawal |
| Earnings | House edge share | Bridge fee share |
| Pooled | Yes | Yes |

## Promissory Note Lifecycle Integration

The Pool Stake contract is a **token mover** in the Promissory Note ecosystem — it pools
staker capital for withdrawal coverage and distributes fees via TransferV1.

### Why Pool Stake Uses TransferV1

All Pool Stake PN child calls use **TransferV1 (0x04)** exclusively:

| Operation | PN Child Call | What Actually Happens |
|-----------|--------------|----------------------|
| JoinPoolV1 | TransferV1 | Staker deposits capital into the coverage pool |
| LeavePoolV1 | TransferV1 | Contract returns stake to staker after cooldown |
| ClaimFeesV1 | TransferV1 | Contract pays accumulated bridge fees to staker |

This is architecturally correct: Pool Stake manages existing tokens on behalf of
stakers. It does not mint or burn — tokens are created and destroyed by the
[stablecoin](stablecoin.md) contract.

### Custody Model

Pool Stake pools capital to provide withdrawal coverage guarantees. Coverage is
allocated when a relayer requests a guaranteed withdrawal and released or slashed
depending on execution outcome. Stakers earn a proportional share of bridge fees.

### Cross-Contract Validation

Child calls validate both `contract_id` and `value_commit` to prevent routing attacks
and ensure the correct deposit, withdrawal, or fee amount is transferred.

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [Bridge Contract](./bridge.md) - Guaranteed withdrawal execution
- [Betting Stake](./betting_stake.md) - Similar pooled capital pattern
- [Relayer Endowment](./relayer_endowment.md) - External capital backing for relayers