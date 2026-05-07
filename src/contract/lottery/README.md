# Lottery Contract

A provably fair, privacy-preserving lottery contract for DarkWow. Supports both standard lottery types (UK National Lottery, Powerball, etc.) and custom configurations with tiered prize structures.

## Overview

The Lottery contract enables:

1. **Configurable Lotteries**: Deploy lotteries with custom number ranges, pick counts, and prize tiers
2. **Standard Presets**: Pre-configured lotteries (UK 6/59, Powerball 5/69, Superenalotto 6/90)
3. **Provably Fair Drawing**: Winning numbers derived from block hash entropy via [Entropy Module](../entropy/)
4. **Privacy-Preserving**: Ticket commitments hide numbers until reveal
5. **Tiered Prizes**: Multiple prize tiers based on number of matches

## Key Features

- **Commit-Reveal Pattern**: Players commit to tickets before numbers are drawn
- **Block Hash Entropy**: Drawing uses blockchain randomness via `darkfi_sdk::crypto::entropy`
- **Configurable House Edge**: Set via deployment (default 2%)
- **Prize Rollover**: Unclaimed prizes can roll to next draw
- **ZK Proofs**: Placeholder circuits for commit and reveal verification

## Lottery Economics

**Important**: A lottery cannot pay out what it does not have. This contract implements **parimutuel** economics:

```
Prize Pool = Σ(ticket_sales) × (1 − house_edge_bp/10000)
Jackpot = Prize Pool × jackpot_percentage/10000
```

| Config | Picks | Range | Odds (1 in) | Typical Jackpot |
|--------|-------|-------|-------------|-----------------|
| UK National | 6 | 59 | 45,057,474 | Scales with sales |
| Powerball | 5 | 69 | 292,201,338 | Scales with sales |
| Neighborhood | 3 | 10 | 1,000 | Scales with sales |

The jackpot **always equals** the actual prize pool × jackpot percentage. It cannot exceed collected funds.

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | House creates new lottery round |
| BuyTicketV1 | 0x01 | Player commits to ticket (numbers hidden) |
| DrawWinnersV1 | 0x02 | House draws winning numbers |
| RevealTicketV1 | 0x03 | Player reveals numbers to claim |
| ClaimPrizeV1 | 0x04 | Winner claims prize share |
| ExpireLotteryV1 | 0x05 | House closes after claim deadline |

## State Machine

```
INITIALIZED ──[BuyTicket]──> TICKETS_SOLD ──[DrawWinners]──> WINNERS_DRAWN
                                                        │
                                                        ▼
                                                  EXPIRED <──[ExpireLottery]
                                                        │
                                                        ▼
                                                   CLAIMED (per player)
```

## Standard Configurations

| Config | num_picks | number_range | house_edge_bp | ticket_price |
|--------|------------|--------------|----------------|--------------|
| UK_LOTTERY_CONFIG | 6 | 59 | 2500 (25%) | 200 |
| NEIGHBORHOOD_CONFIG | 3 | 10 | 1000 (10%) | 10 |
| SIMPLE_690_CONFIG | 6 | 90 | 2000 (20%) | 100 |
| POWERBALL_CONFIG | 5 | 69 | 3000 (30%) | 200 |

## Prize Tiers

Each tier specifies:
- `matches_needed`: How many numbers must match to win
- `payout_percent`: Percentage of prize pool (in basis points)
- `roll_to_next`: Whether unclaimed prizes roll over

Example (UK National):
| Tier | Matches | Pool % | Notes |
|------|---------|--------|-------|
| Jackpot | 6 | 5000 (50%) | Rolls if no winner |
| 2nd | 5 | 2500 (25%) | Fixed |
| 3rd | 4 | 1000 (10%) | Fixed |
| 4th | 3 | 250 (2.5%) | Fixed |

## Building

```bash
# Compile ZK circuits
./target/debug/zkas proof/commit_ticket_v1.zk -o proof/commit_ticket_v1.zk.bin
./target/debug/zkas proof/reveal_ticket_v1.zk -o proof/reveal_ticket_v1.zk.bin

# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_lottery_contract

# Run tests (library only)
cargo test -p darkfi_lottery_contract --lib
```

## Usage Example

```rust
use darkfi_lottery_contract::{model::*, *};

// Create a custom lottery configuration
let config = LotteryConfig {
    num_picks: 6,
    number_range: 59,
    house_edge_bp: 2500,  // 25% house edge
    ticket_price: 200,
    prize_tiers: vec![
        PrizeTierConfig { matches_needed: 6, payout_percent: 5000, roll_to_next: true },
        PrizeTierConfig { matches_needed: 5, payout_percent: 2500, roll_to_next: false },
        PrizeTierConfig { matches_needed: 4, payout_percent: 1000, roll_to_next: false },
        PrizeTierConfig { matches_needed: 3, payout_percent: 250, roll_to_next: false },
    ],
};

// Or use a preset
let config = uk_lottery_config();

// Initialize as house
let params = InitializeParamsV1 {
    house_pub: house_pubkey,
    config,
    duration: 1000,        // 1000 blocks until draw
    claim_duration: 100,    // 100 blocks to claim
    rolled_over: 0,
};
```

## Integration with Money Contract

The Lottery contract integrates with Money for value transfers:

1. **BuyTicket**: Money::Burn locks ticket value (spend_hook to BuyTicketV1)
2. **ClaimPrize**: Money::MintV2 pays out winner's share
3. **ExpireLottery**: Money::MintV2 sends unclaimed to house

## The Lottery Problem Space

Lottery sits **between** BettingStake and Insurance in the DarkWow problem space:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    RISK CAPITAL SPECTRUM                              │
├─────────────────┬─────────────────────┬─────────────────────────────┤
│   BETTINGSTAKE  │       LOTTERY       │         INSURANCE            │
├─────────────────┼─────────────────────┼─────────────────────────────┤
│ Fixed odds      │ Parimutuel pool     │ Indemnity-based             │
│ Bounded max     │ Variable jackpot    │ Catastrophic risk           │
│ loss per bet    │ Can exceed pool     │ Pool can be insufficient     │
├─────────────────┼─────────────────────┼─────────────────────────────┤
│ Max payout      │ Jackpot may need    │ Major catastrophe           │
│ known before    │ capital buffer       │ requires reinsurance        │
│ bet placed      │ (BettingStake)      │                              │
├─────────────────┼─────────────────────┼─────────────────────────────┤
│ Works natively  │ Bridge solution:    │ True insurance for          │
│ with staking    │ BettingStake as     │ Powerball-scale jackpots    │
│                 │ capital backstop    │                              │
└─────────────────┴─────────────────────┴─────────────────────────────┘
```

### Why Lottery is the Bridge

| Aspect | BettingStake | Lottery | Insurance |
|--------|--------------|---------|----------|
| Odds | Fixed (e.g., 35:1) | Pool-based (variable) | Event-based |
| Max payout | Known pre-event | Exceeds pool possible |理论上无上限 |
| Capital need | Bounded (max × odds) | Bounded + buffer | Catastrophe modeling |
| Risk profile | Low-medium | Medium-high | High/catastrophic |

Lottery's parimutuel nature means:
- Small jackpots: Covered by ticket pool + BettingStake
- Large jackpots: BettingStake insufficient, needs insurance/reinsurance
- Powerball-scale: True insurance needed for catastrophic risk

## BettingStake as Capital Backstop

For small/neighborhood lotteries, BettingStake works well:
- Fixed ticket count limits max payout
- Pool accumulation is predictable

For large lotteries (Powerball-scale), BettingStake is a **band-aid**:
```
Problem: Large lottery needs $10M for jackpot but only has $5M in pool
Solution: BettingStake provides $5M capital buffer

Limitation: If multiple players hit jackpot simultaneously,
the pool may still not cover all payouts
```

See [BettingStake Contract](../betting_stake/) for the capital staking infrastructure.

## Insurance as Underwriter

For catastrophic lottery risks, true insurance is needed:

| Risk | Description | Mitigation |
|------|-------------|------------|
| Jackpot hit early | Large payout before pool grows | Reinsurance |
| Volatility | Prize pool variance | Capital reserves |
| Fraud | Fake tickets | ZK proof verification |

Insurance companies can:
1. **Reinsurance**: Cross-lottery risk distribution
2. **Catastrophe bonds**: Transfer risk to capital markets
3. **Yield farming**: Earn house edge share for risk-bearing

See [Insurance Market Contract](../insurance_market/) for underwriter infrastructure.

## Drawing Algorithm

Winning numbers are drawn using the [Entropy Module](../entropy/) (`darkfi_sdk::crypto::entropy`):

```rust
use darkfi_sdk::crypto::entropy::draw_unique_range;

// Draw N unique numbers from 1 to M using block hash entropy
let numbers = draw_unique_range(block_hash, seed_nonce, num_picks, number_range);
```

This provides:
- **Provably fair**: Block hash from PoW mining
- **Unique picks**: LCG-based without-replacement sampling
- **Verifiable**: Deterministic given same inputs

## Security Considerations

- **Block hash unpredictability**: Ensures fair drawing
- **Commit-reveal**: Prevents front-running of ticket purchases
- **ZK proofs**: Verify ticket validity without revealing numbers
- **Nullifiers**: Prevent double-claiming of prizes
- **Timelocks**: Claim deadlines prevent indefinite disputes

## Primitives Provided

This contract establishes useful primitives:

- Commit-reveal schemes via Poseidon hash
- Parimutuel prize pool calculations
- Multi-tier payout structures
- Block hash randomness via [Entropy Module](../entropy/)

## See Also

- [Entropy Module](../entropy/) - Provably fair randomness for all betting contracts
- [Insurance Market Contract](../insurance_market/) - Underwriter infrastructure
- [BettingStake Contract](../betting_stake/) - Capital staking for betting games
- [DarkToshi Dice Contract](../darktoshi_dice/) - Commit-reveal gambling pattern
- [Baccarat Contract](../baccarat/) - Multi-round game reference
- [Roulette Contract](../roulette/) - Fixed-odds betting (native BettingStake fit)
- [Money V2 Contract](../money_v2/) - Value transfer integration