# Roulette Contract

A privacy-preserving roulette game with fixed-odds betting. Unlike lottery (parimutuel), roulette has fixed maximum payouts, making it ideal for capital staking via the BettingStake contract.

## Overview

The Roulette contract implements:

1. **European (37 numbers)** and **American (38 numbers)** wheel variants
2. **Fixed-odds betting**: Known maximum payouts for each bet type
3. **Provably fair spinning**: Block hash entropy for randomness
4. **Capital-efficient**: House only needs to cover max single-spin loss
5. **Composable with BettingStake**: Capital providers can stake for house edge yield

## Key Features

- **Fixed Maximum Payouts**: Unlike lottery, roulette odds are fixed (e.g., straight bet pays 35:1)
- **American vs European**: 38 numbers (0, 00) vs 37 numbers (0)
- **Block Hash Entropy**: Wheel spin uses blockchain randomness
- **BettingStake Integration**: Capital providers can stake against the table

## Roulette Economics

**Fixed Odds vs Parimutuel**:

| Aspect | Roulette (Fixed) | Lottery (Parimutuel) |
|--------|------------------|----------------------|
| Payout | Fixed (e.g., 35:1) | From pool (variable) |
| Jackpot cap | N/A | pool × jackpot% |
| Capital needed | max_payout × stake_ratio | pool × odds |
| BettingStake | Works natively | Band-aid solution |

For roulette, the house only needs capital to cover **maximum single-spin loss**:
- Table capital = max_straight_bet × 35
- This is deterministic and bounded

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | House creates new roulette table |
| PlaceBetV1 | 0x01 | Player places bet on table |
| SpinWheelV1 | 0x02 | House spins wheel (after bets close) |
| SettleBetsV1 | 0x03 | House settles bets and pays winners |
| HouseCloseV1 | 0x04 | House closes table |

## Bet Types and Odds

| Bet Type | Numbers | Payout | European HE | American HE |
|----------|---------|--------|-------------|-------------|
| Straight | 1 | 35:1 | 2.7% | 5.26% |
| Split | 2 | 17:1 | 2.7% | 5.26% |
| Street | 3 | 11:1 | 2.7% | 5.26% |
| Corner | 4 | 8:1 | 2.7% | 5.26% |
| Six Line | 6 | 5:1 | 2.7% | 5.26% |
| Dozen | 12 | 2:1 | 2.7% | 5.26% |
| Column | 12 | 2:1 | 2.7% | 5.26% |
| Even Money | 18 | 1:1 | 2.7% | 5.26% |

**House Edge Calculation**:
- European: 1/37 = 2.70%
- American: 2/38 = 5.26%

## Capital Requirements

Unlike lottery where the pool grows with sales, roulette has **fixed maximum exposure** per spin:

```
max_loss_per_spin = max_straight_bet × 35  (straight bet worst case)
table_capital = max_loss_per_spin × safety_factor
```

For a table with max_straight_bet = 1000:
- Max straight payout = 35,000
- Recommended capital = 50,000 (safety factor ~1.4x)

## BettingStake Integration

Roulette works **natively** with BettingStake because:

1. **Bounded risk**: Max payout is known before spin
2. **No cascade failure**: One spin doesn't affect next
3. **Predictable house edge**: 2.7% or 5.26% guaranteed

```
┌─────────────────┐      ┌──────────────────┐      ┌─────────────────┐
│    Roulette     │ ───► │  Betting Stake   │ ───► │    Staker       │
│  (Fixed Odds)   │      │   (Layer)         │      │   (Capital)     │
└─────────────────┘      └──────────────────┘      └─────────────────┘
        │                         │                         │
        │ Payout = bet × 35       │ Distributes              │ Provides
        │ (max known)            │ earnings/losses          │ capital for
        ▼                         ▼                         ▼ max payout
   House needs            Stakers absorb              Earn house edge
   capital ≤ max_payout   losses proportionally       share over time
```

See [BettingStake Contract](../betting_stake/) for staking infrastructure.

## Why Roulette vs Lottery?

| Scenario | Roulette | Lottery |
|----------|----------|---------|
| Small jackpot ($10K) | Works with BettingStake | Works with BettingStake |
| Large jackpot ($10M) | Still bounded risk | BettingStake is band-aid |
| Instant payout | Guaranteed | Pool may not have enough |
| Capital efficiency | High (low margin) | Low (large reserves needed) |

**Lottery needs BettingStake as a band-aid** because:
- Jackpot can theoretically exceed collected pool
- Large lottos require massive capital reserves
- Insurance/reinsurance is more appropriate for true cat risks

**Roulette works natively with BettingStake** because:
- Max payout is always bounded and known
- No "jackpot exceeds pool" problem
- Capital efficiently matched to risk

## Building

```bash
# Compile ZK circuits (when implemented)
./target/debug/zkas proof/place_bet_v1.zk -o proof/place_bet_v1.zk.bin
./target/debug/zkas proof/settle_bet_v1.zk -o proof/settle_bet_v1.zk.bin

# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_roulette_contract

# Run tests
cargo test -p darkfi_roulette_contract --lib
```

## Usage Example

```rust
use darkfi_roulette_contract::{
    model::*, BetType, RouletteFunction, EUROPEAN_WHEEL_SIZE, AMERICAN_WHEEL_SIZE,
};

// Initialize European table
let init_params = InitializeParamsV1 {
    house_pub: house_pubkey,
    american_wheel: false,     // European (37 numbers)
    house_capital: 50_000,     // $50K capital
    max_straight_bet: 1_000,   // Max $1K straight bet
    duration_blocks: 100,       // 100 blocks of betting
};

// Place a straight bet on 17
let bet_params = PlaceBetParamsV1 {
    table_id: roulette_table_id,
    player_pub: player_pubkey,
    bet_type: BetType::Straight,
    numbers: vec![17],
    amount: 100,
    signature: player_sig,
};

// House spins wheel after bets close
let spin_params = SpinWheelParamsV1 {
    table_id: roulette_table_id,
    nonce: house_nonce,
};

// House settles all bets
let settle_params = SettleBetsParamsV1 {
    table_id: roulette_table_id,
    bet_ids: vec![bet_id1, bet_id2, ...],
};
```

## Wheel Drawing Algorithm

Winning number is derived from block hash using the [Entropy Module](../entropy/):

```rust
use darkfi_sdk::crypto::entropy::draw_single;

// Draw winning number (0 to wheel_size-1)
let winning_number = draw_single(block_hash, nonce, wheel_size);
```

## Security Considerations

- **Block hash unpredictability**: Ensures fair spinning
- **Nullifiers**: Prevent double-spending of bets
- **Signature verification**: Only house can spin/settle
- **Capital reservation**: Payout reserved when bet placed
- **Bets close block**: Prevents bets after spin decision

## See Also

- [Entropy Module](../entropy/) - Provably fair randomness for all betting contracts
- [BettingStake Contract](../betting_stake/) - Capital staking for betting games
- [Lottery Contract](../lottery/) - Parimutuel betting (bridge to insurance)
- [DarkToshi Dice Contract](../darktoshi_dice/) - Fixed-odds betting reference
- [Baccarat Contract](../baccarat/) - Multi-round game with capital efficiency
- [Promissory Note](../promissory_note/) - Value transfer integration