# Lottery Contract

A provably fair, privacy-preserving lottery contract for DarkFi. Supports both standard lottery types (UK National Lottery, Powerball, etc.) and custom configurations with tiered prize structures.

## Overview

The Lottery contract enables:

1. **Configurable Lotteries**: Deploy lotteries with custom number ranges, pick counts, and prize tiers
2. **Standard Presets**: Pre-configured lotteries (UK 6/59, Powerball 5/69, Superenalotto 6/90)
3. **Provably Fair Drawing**: Winning numbers derived from block hash entropy
4. **Privacy-Preserving**: Ticket commitments hide numbers until reveal
5. **Tiered Prizes**: Multiple prize tiers based on number of matches

## Key Features

- **Commit-Reveal Pattern**: Players commit to tickets before numbers are drawn
- **Block Hash Entropy**: Drawing uses blockchain randomness for fairness
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

## Insurance as Underwriter

Lotteries present a clear risk profile ideal for insurance underwriters:

| Risk | Description | Mitigation |
|------|-------------|------------|
| Jackpot hit early | Large payout before pool grows | Reinsurance |
| Volatility | Prize pool variance | Capital reserves |
| Fraud | Fake tickets | ZK proof verification |

Insurance companies can:
1. **Stake against the pool**: Provide liquidity for large jackpots
2. **Yield farming**: Earn house edge share for risk-bearing
3. **Reinsurance**: Cross-lottery risk distribution

See [Insurance Market Contract](../insurance_market/) for underwriter infrastructure.

## Drawing Algorithm

Winning numbers are drawn using block hash entropy:

```rust
fn draw_winning_numbers(
    block_hash: BlockHash,
    seed_nonce: u64,
    num_picks: u8,
    number_range: u8,
) -> Vec<u8> {
    // Use block hash + nonce as entropy
    let entropy = poseidon_hash([block_hash, Base::from(seed_nonce)]);

    let mut rng_seed = u64::from_le_bytes(entropy.to_repr()[0..8]);
    let mut numbers = Vec::new();

    while numbers.len() < num_picks {
        let num = ((rng_seed % (number_range as u64)) + 1) as u8;
        if !numbers.contains(&num) {
            numbers.push(num);
        }
        rng_seed = rng_seed.wrapping_mul(31).wrapping_add(17);
    }
    numbers
}
```

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
- Block hash randomness for drawing

## See Also

- [Insurance Market Contract](../insurance_market/) - Underwriter infrastructure
- [DarkToshi Dice Contract](../darktoshi_dice/) - Commit-reveal gambling pattern
- [Baccarat Contract](../baccarat/) - Multi-round game reference
- [Money V2 Contract](../money_v2/) - Value transfer integration
