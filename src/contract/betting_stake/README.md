# Betting Stake Contract

A composable contract that allows capital providers to stake against betting contracts (Dice, Baccarat, Lottery) in exchange for a share of the house edge over time.

## Overview

This contract solves the capital requirements problem for betting games:

1. **Betting contracts** (Dice, Baccarat, Lottery) need capital to pay winners
2. **Capital providers** want yield for bearing payout risk
3. **This contract** matches capital supply with capital demand

## How It Works

```
┌─────────────────┐      ┌──────────────────┐      ┌─────────────────┐
│  Betting Game   │ ───► │ Betting Stake    │ ───► │    Staker       │
│  (House Edge)   │      │   (Layer)        │      │  (Capital)      │
└─────────────────┘      └──────────────────┘      └─────────────────┘
        │                         │                         │
        │ Payout happens          │ Distributes             │ Provides
        │ (winners paid)         │ earnings/losses         │ capital
        ▼                         ▼                         ▼
   House needs            Stakers absorb              Earn house edge
   capital for           losses proportional           share over time
   payouts               to their stake
```

### Stake Flow

1. **Stake**: Capital provider stakes funds against a specific betting table
2. **Earn**: Provider earns a share of the house edge from that table's bets
3. **Risk**: Provider absorbs losses when bets pay out (up to stake amount)
4. **Withdraw**: Provider can withdraw stake + accumulated earnings

## Risk/Reward Profile

| Scenario | Outcome |
|----------|---------|
| Table loses money | Staker absorbs loss, stake decreases |
| Table breaks even | Staker earns nothing |
| Table wins (house wins) | Staker earns house edge share |

Over time, with many bets, the law of large numbers means stakers should earn the positive expected value of the house edge minus a risk premium.

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize staking for a betting table |
| StakeV1 | 0x01 | Stake capital against a table |
| UnstakeV1 | 0x02 | Withdraw stake + earnings |
| ClaimEarningsV1 | 0x03 | Claim accumulated earnings |
| UpdateRiskV1 | 0x04 | Update stake after a payout |

## Risk Profiles

Different betting games have different volatility:

| Game | Volatility | House Edge | Risk Premium | Notes |
|------|------------|------------|--------------|-------|
| Dice | Low | ~2% | ~1% | Native fit |
| Baccarat | Medium | ~1.5% | ~2.5% | Native fit |
| Roulette (EU) | Low | 2.7% | ~1% | Native fit, fixed odds |
| Roulette (US) | Low | 5.26% | ~1.5% | Native fit, fixed odds |
| Lottery | High | 10-30% | ~5% | Band-aid only |

The risk premium compensates stakers for:
- Variance in payout outcomes
- Risk of large single losses
- Correlation between outcomes

## Capital Requirements

A betting table can only accept bets up to its available capital:

```
max_bet = min(
    table_capital / max_payout_ratio,
    staker_total_capital * MAX_STAKE_RATIO
)
```

For example, a Baccarat table with $1M staked:
- Banker bet (0.95:1 payout): max ≈ $1.05M
- Tie bet (8:1 payout): max ≈ $125K

This ensures every bet can be fully paid if won.

## Integration with Betting Contracts

Betting contracts call `UpdateRiskV1` after each bet settles:

```rust
// In betting contract (Dice/Baccarat/Lottery)
let update_risk = UpdateRiskParamsV1 {
    table_id: staking_table_id,
    payout_amount: player_winnings,
    house_share: house_take,
};

// Staking contract distributes:
// - House earnings to stakers
// - Losses to stakers proportionally
```

## Usage Example

```rust
use darkfi_betting_stake_contract::{InitializeParamsV1, StakeParamsV1};

// 1. Initialize staking for a Dice table
let init_params = InitializeParamsV1 {
    betting_contract_id: dice_contract_id,
    house_edge_bp: 200,  // 2%
    risk_profile: 0,      // Low volatility
    signature: house_sig,
};

// 2. Stake capital
let stake_params = StakeParamsV1 {
    table_id: staking_table_id,
    staker_pub: my_pubkey,
    amount: 10_000,       // $10k stake
    signature: my_sig,
};

// 3. Over time, claim earnings
let claim_params = ClaimEarningsParamsV1 {
    stake_id: my_stake_id,
    signature: my_sig,
};

// 4. Unstake when done
let unstake_params = UnstakeParamsV1 {
    stake_id: my_stake_id,
    signature: my_sig,
};
```

## Expected Value Analysis

For a staker in a fair betting game:

```
E[staker_return] = Σ(bets) × house_edge_rate × staker_share
                 - Σ(payouts) × loss_absorption_ratio

Over many bets, variance decreases and return approaches house_edge_rate.
```

The risk premium ensures stakers are compensated for:
- Single bet variance (which can be large)
- Correlation risk (losing streaks)
- Model risk (if house edge estimates are wrong)

## Insurance Comparison

This is similar to insurance but with key differences:

| Aspect | Insurance | Betting Stake |
|---------|-----------|---------------|
| Premium | Fixed rate | Proportional to house edge |
| Claims | Discrete events | Continuous (every bet) |
| Risk | Categorical | Graduated |
| Capital efficiency | Lower | Higher |

## See Also

- [DarkToshi Dice Contract](../darktoshi_dice/) - Betting contract with ~2% house edge
- [Baccarat Contract](../baccarat/) - Betting contract with ~1.5% house edge
- [Roulette Contract](../roulette/) - Fixed-odds betting (native BettingStake fit)
- [Lottery Contract](../lottery/) - Parimutuel betting (BettingStake as band-aid)
- [Insurance Market Contract](../insurance_market/) - Underwriting infrastructure for categorical risks
- [DEX Contract](../dex/) - Matching engine for peer-to-peer betting
- [Oracle Contract](../oracle/) - Event resolution for betting markets
