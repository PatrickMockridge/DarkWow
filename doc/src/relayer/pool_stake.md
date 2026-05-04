# Pool Stake Contract

A composable contract that enables pooled coverage for relayer withdrawals. Stakers deposit capital into a shared pool to provide guaranteed withdrawal coverage, earning a share of bridge fees in return.

## Purpose

Relayers need coverage capacity to execute guaranteed withdrawals. The Pool Stake contract enables:

- **Coverage pooling**: Multiple stakers combine capital for shared coverage
- **Risk sharing**: Coverage obligations distributed proportionally
- **Yield generation**: Stakers earn bridge fees for providing coverage

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Pool Stake Layer                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐    │
│  │   Pool       │    │   Member     │    │  Coverage    │    │
│  │  Registry    │◄──►│   Stakes     │◄──►│  Allocations │    │
│  │              │    │              │    │              │    │
│  │ - pool_id    │    │ - stake_id  │    │ - allocation_id
│  │ - total_stake│    │ - pool_id   │    │ - pool_id    │    │
│  │ - available  │    │ - member_pub│    │ - amount     │    │
│  │ - allocated   │    │ - amount    │    │ - nullifier  │    │
│  └──────────────┘    └──────────────┘    └──────────────┘    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
           │                    │                    │
           ▼                    ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Bridge (Guaranteed Withdrawals)                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  When relayer needs coverage:                                    │
│  1. Allocate coverage from pool                                 │
│  2. Execute guaranteed withdrawal                               │
│  3. On success: release coverage, distribute fees               │
│  4. On failure: slash coverage proportionally                    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Data Model

### PoolStakeRegistry

Tracks aggregate staking for each pool:

```rust
struct PoolStakeRegistry {
    pool_id: pallas::Base,
    owner_pub: PublicKey,
    total_stake: u64,              // Total capital staked
    available_coverage: u64,        // Coverage not yet allocated
    allocated_coverage: u64,        // Coverage currently in use
    member_count: u64,
    max_coverage_ratio: u32,       // Basis points (e.g., 8000 = 80%)
    created_at: u64,
}
```

### PoolMemberStake

Individual staker position:

```rust
struct PoolMemberStake {
    stake_id: pallas::Base,
    pool_id: pallas::Base,
    member_pub: PublicKey,
    relayer_id: [u8; 32],          // Target relayer
    stake_amount: u64,
    coverage_contribution: u64,
    coverage_share_bp: u32,         // Basis points of pool coverage
    accumulated_fees: u64,
    created_at: u64,
    unstake_requested_at: Option<u64>,
    is_active: bool,
}
```

### CoverageAllocation

Active coverage for a withdrawal:

```rust
struct CoverageAllocation {
    allocation_id: pallas::Base,
    pool_id: pallas::Base,
    withdrawal_nullifier: IntentNullifier,
    amount: u64,
    contributing_members: Vec<[u8; 32]>,
    created_at: u64,
    timeout_height: u64,
}
```

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

## Economic Model

### Coverage Ratio

Each pool has a `max_coverage_ratio` determining how much coverage can be allocated:

```
available_coverage = total_stake × max_coverage_ratio / 10000
```

For example, a pool with 1M DAI staked and 80% coverage ratio:
- Available coverage = 800K DAI

### Fee Distribution

When a guaranteed withdrawal executes successfully:

```
total_fees = bridge fees earned
staker_share = total_fees × (coverage_used / total_allocated_coverage)
```

## Composability

This contract composes with:

- **money contract**: For token transfers (stake in/out)
- **bridge contract**: For guaranteed withdrawal integration

## See Also

- [Relayer Endowment Contract](endowment.md) - External capital backing for relayers
- [Bridge Architecture](../contract/bridge.md) - Guaranteed withdrawal execution
- [Relayer Economics](relayer_economics.md) - Economic layer overview