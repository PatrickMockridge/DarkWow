# Roulette Contract Architecture

## Overview

The Roulette contract implements a privacy-preserving roulette game with fixed-odds betting. It serves as the counter-example to the Lottery contract: where lottery requires parimutuel economics (pool = payouts), roulette's fixed odds allow it to work natively with the BettingStake contract.

## Key Design Decision: Fixed Odds

Roulette odds are **fixed** regardless of pool size:
- Straight bet: 35:1 (if you bet on one number, you win 35× your bet)
- This is fundamentally different from lottery where jackpot = pool × percentage

This means:
1. Maximum payout is **known before the spin**
2. House only needs capital to cover **max single-spin loss**
3. BettingStake can efficiently match capital to risk

## Contract Structure

```
src/contract/roulette/
├── Cargo.toml
├── README.md
├── proof/
│   ├── place_bet_v1.zk    # Verify commitment binding + nullifier derivation
│   └── settle_bet_v1.zk   # Verify correct settlement + nullifier
└── src/
    ├── lib.rs              # RouletteFunction enum, BetType, constants
    ├── error.rs            # RouletteError enum
    ├── model/
    │   └── mod.rs          # RouletteTable, Bet, params/update structs
    └── entrypoint.rs       # All function implementations
```

## Function Opcodes

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | InitializeV1 | House creates table |
| 0x01 | PlaceBetV1 | Player places bet |
| 0x02 | SpinWheelV1 | House spins wheel |
| 0x03 | SettleBetsV1 | House settles bets |
| 0x04 | HouseCloseV1 | House closes table |

## Data Model

### RouletteTable

```rust
pub struct RouletteTable {
    pub table_id: pallas::Base,
    pub house_pub: PublicKey,
    pub wheel_size: u8,              // 37 (European) or 38 (American)
    pub house_edge_bp: u32,           // 270 (EU) or 526 (US)
    pub house_capital: u64,           // Available for payouts
    pub max_straight_bet: u64,       // Max single number bet
    pub state: RouletteTableState,    // Active/WaitingForSpin/Spun/Closed
    pub spin_count: u64,
    pub winning_number: Option<u8>,
    pub bets_close_block: u64,
    pub spun_at_block: Option<u64>,
    pub created_at: u64,
}
```

### Bet

```rust
pub struct Bet {
    pub bet_id: pallas::Base,
    pub table_id: pallas::Base,
    pub player_pub: PublicKey,
    pub bet_type: BetType,
    pub numbers: Vec<u8>,
    pub amount: u64,
    pub payout: u64,                  // amount * payout_ratio
    pub won: Option<bool>,
    pub actual_payout: u64,
    pub spin_number: u64,
    pub placed_at: u64,
    pub nullifier: pallas::Base,
}
```

## BettingStake Integration

Roulette works **natively** with BettingStake:

```
Capital Flow:
1. Staker deposits capital into BettingStake against roulette table
2. House initializes roulette with capital from BettingStake
3. Players place bets, house reserves payout
4. If player wins: house pays from table capital, staker absorbs loss
5. If house wins: staker earns house edge share
6. Over time: staker earns positive EV of house edge minus risk premium
```

**Why it works**: Max payout is bounded (max_straight_bet × 35), so BettingStake can accurately price risk.

## Comparison: Roulette vs Lottery

| Aspect | Roulette | Lottery |
|--------|----------|---------|
| Odds | Fixed (35:1 for straight) | Variable (pool/matches) |
| Max payout | Known pre-spin | Unknown until draw |
| Capital needed | max_payout × margin | pool × odds |
| BettingStake | Native fit | Band-aid solution |
| Jackpot limit | N/A | pool × jackpot% |

## House Edge

The house edge comes from the difference between true odds and payout odds:

| Wheel | True Odds | Payout | House Edge |
|-------|-----------|--------|------------|
| European (37) | 36:1 | 35:1 | 2.7% |
| American (38) | 37:1 | 35:1 | 5.26% |

For a straight bet on European:
- True probability: 1/37
- Fair payout: 36:1
- Actual payout: 35:1
- House edge: (36-35)/37 × 100 = 2.7%

## Security Properties

1. **Block hash entropy**: Wheel spin uses blockchain randomness via [Entropy Module](./entropy.md)
2. **Nullifiers**: Prevent double-claiming of bets
3. **Signature verification**: Only house can spin/settle
4. **Capital reservation**: Payout reserved when bet placed
5. **Bets close block**: Bets cannot be placed after spin starts

## Future Improvements

- [ ] ZK circuits for place_bet and settle_bet
- [ ] En prison rule for European even-money bets (reduces HE to 1.35%)
- [ ] Multi-spin tracking
- [ ] Progressive jackpot option (separate pool)

## Related Contracts

- [Entropy Module](./entropy.md) - Provably fair randomness for all betting contracts
- [BettingStake](./betting_stake.md) - Capital staking for betting games (native fit)
- [Lottery](./lottery.md) - Parimutuel betting (bridge to insurance)
- [DarkToshi Dice](./darktoshi_dice.md) - Commit-reveal gambling
- [Baccarat](./baccarat.md) - Multi-round betting