# Betting Stake Contract

A composable contract that allows capital providers to stake against betting contracts (Dice, Baccarat, Lottery) in exchange for a share of the house edge over time.

## Purpose

Betting games (Dice, Baccarat, Lottery) require capital to pay out winners. The house edge provides positive expected value, but capital constraints limit bet sizes and growth.

The Betting Stake contract enables:
- **Capital aggregation**: Multiple stakers pool capital
- **Risk sharing**: Losses distributed proportionally
- **Yield generation**: Stakers earn house edge over time

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Betting Stake Layer                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐      │
│  │  Table       │    │  Stake       │    │  Earnings    │      │
│  │  Registry     │◄──►│  Positions   │◄──►│  Tracker     │      │
│  │              │    │              │    │              │      │
│  │ - total_stake│    │ - stake_id  │    │ - accum_earnings
│  │ - losses     │    │ - amount    │    │ - accum_losses│     │
│  │ - earnings   │    │ - earnings  │    │              │      │
│  └──────────────┘    └──────────────┘    └──────────────┘      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
           │                    │                    │
           ▼                    ▼                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Betting Contracts                           │
├──────────────┬──────────────┬──────────────┬────────────────────┤
│    Dice      │   Baccarat   │   Lottery    │   (extensible)     │
│              │              │              │                    │
│ House edge:  │ House edge:  │ House edge:  │                    │
│ ~2%          │ ~1.5%        │ 10-30%       │                    │
└──────────────┴──────────────┴──────────────┴────────────────────┘
```

## Data Model

### TableStakeRegistry

Tracks aggregate staking for each betting table:

```rust
struct TableStakeRegistry {
    betting_contract_id: pallas::Base,
    total_stake: u64,              // Total capital staked
    accumulated_earnings: u64,     // Earnings to distribute
    accumulated_losses: u64,       // Losses to absorb
    staker_count: u64,
    house_edge_bp: u32,            // Basis points
    risk_profile: u8,             // 0=Low, 1=Medium, 2=High
}
```

### Stake

Individual staker position:

```rust
struct Stake {
    stake_id: pallas::Base,
    table_id: pallas::Base,
    staker_pub: PublicKey,
    original_amount: u64,
    current_amount: u64,           // Decreases with losses
    accumulated_earnings: u64,
    created_at: u64,
    unstake_requested_at: Option<u64>,
    is_active: bool,
}
```

## Contract Functions

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | InitializeV1 | Initialize staking for a betting table |
| 0x01 | StakeV1 | Stake capital against a table |
| 0x02 | UnstakeV1 | Withdraw stake + earnings |
| 0x03 | ClaimEarningsV1 | Claim accumulated earnings |
| 0x04 | UpdateRiskV1 | Update after bet settlement |

## Economic Model

### Staker Returns

Staker expected value over N bets:

```
E[return] = N × avg_bet × house_edge × staker_share
           - N × avg_bet × loss_absorption_ratio × loss_probability
```

As N → ∞, variance → 0, and return → positive (house edge - risk premium).

### Risk Profile Premiums

Different games have different risk profiles:

| Profile | Games | Risk Premium | Rationale |
|---------|-------|--------------|----------|
| Low | Dice | ~100bp | Small variance, frequent bets |
| Medium | Baccarat | ~250bp | Moderate variance |
| High | Lottery | ~500bp | Large jackpot variance |

### Loss Absorption

When bets pay out, losses are distributed proportionally:

```
staker_loss_i = payout × (stake_i / total_stake)
```

If total losses exceed total stake, stakers are wiped out (no clawback from contract).

## Integration with Betting Contracts

Betting contracts call `UpdateRiskV1` after settlement:

```
1. Bet settles (player wins or loses)
2. If player wins:
   - payout_amount = winnings
   - house_share = payout × (1 - house_edge)
   - staker_loss = payout - house_share
3. Staking contract updates:
   - table.accumulated_earnings += house_share
   - table.accumulated_losses += staker_loss
   - table.total_stake -= staker_loss
```

## Capital Efficiency

Compared to traditional insurance:

| Metric | Insurance | Betting Stake |
|--------|----------|---------------|
| Capital efficiency | ~10x leverage | ~100x leverage |
| Claims frequency | Discrete | Continuous |
| Risk granularity | Categorical | Continuous |
| Settlement time | Days | Blocks |

## Use Cases

### 1. Dice Table Capital

- Staker deposits $10,000
- Table averages 1000 bets/day at $100 avg
- Daily house edge ≈ $2,000 (2%)
- Staker daily earnings ≈ $1,900 (after 5% risk premium)
- Monthly yield ≈ 5.7%

### 2. Baccarat Liquidity

- Multiple stakers pool $100,000
- Table offers Banker/Player/Tie bets
- Risk-adjusted yield ~4% monthly
- Large wins absorbed proportionally

### 3. Lottery Underwriting

- Lottery house edge 20%
- Jackpot variance high
- Stakers earn premium for jackpot risk
- Positive expected value over many draws

## Comparison with Insurance Market

The [Insurance Market](./insurance_market.md) contract handles categorical risks (smart contract hacks, oracle failures), while Betting Stake handles continuous financial risk from betting games.

Both can use prediction markets for risk pricing.

## Security Considerations

1. **Sufficient capital**: Stakers must maintain stake > expected losses
2. **Correlation risk**: Multiple large payouts could exceed total stake
3. **Front-running**: Unstake timing could affect loss distribution
4. **Oracle risk**: Payout amounts must be verified

## Extension Points

1. **Tiered staking**: Different risk tiers for different staker appetites
2. **Leverage**: Enables amplified returns (and losses)
3. **Cross-table pooling**: Combine risk across multiple games
4. **Prediction market integration**: Dynamic risk pricing
