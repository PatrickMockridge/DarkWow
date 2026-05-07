# Lottery Contract

A provably fair, privacy-preserving lottery contract supporting standard lottery types (UK National Lottery, Powerball, EuroMillions, Superenalotto) and custom configurations.

## Overview

The lottery contract implements a **parimutuel** betting system where:
- Players purchase tickets by committing to number selections
- A prize pool accumulates from ticket sales (minus house edge)
- Winning numbers are drawn using blockchain entropy
- Winners share the prize pool based on matched numbers

**Key Design Principle**: A lottery cannot pay out what it does not have. The jackpot is always a percentage of the actual collected prize pool, not a fixed amount.

## Economic Model

### Parimutuel Pool Structure

```
Gross Pool = Σ(ticket_price) for all sold tickets
House Share = Gross Pool × (house_edge_bp / 10000)
Prize Pool = Gross Pool − House Share
```

### Tiered Prize Distribution

Each prize tier receives a percentage of the prize pool:

| Tier | Matches | Pool % | Winner Share |
|------|---------|--------|--------------|
| Jackpot | All N | 50% | Prize Pool × 50% / num_winners |
| 2nd | N−1 | 25% | Prize Pool × 25% / num_winners |
| 3rd | N−2 | 10% | Prize Pool × 10% / num_winners |
| 4th | N−3 | 2.5% | Prize Pool × 2.5% / num_winners |

### Why Parimutuel?

Consider a lottery with $1M in ticket sales and 25% house edge:
- Gross Pool: $1,000,000
- House Share: $250,000
- Prize Pool: $750,000

If 4 players match 6 numbers:
- Each winner receives: $750,000 × 50% / 4 = $93,750

**This is mathematically sustainable** because payouts derive from actual collections.

### Alternative: Fixed Jackpots

Fixed jackpots (e.g., "always pay $10M regardless of sales") require:
1. **Pre-committed capital**: House or investors put up money upfront
2. **Reinsurance**: Third parties absorb excess risk
3. **Cross-subsidization**: Smaller prizes funded by jackpot losses

These mechanisms work but introduce counterparty risk and undermine the trustless premise.

## Contract Functions

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | InitializeV1 | House creates new lottery round |
| 0x01 | BuyTicketV1 | Player commits to ticket numbers |
| 0x02 | DrawWinnersV1 | House draws winning numbers |
| 0x03 | RevealTicketV1 | Player reveals to claim |
| 0x04 | ClaimPrizeV1 | Winner claims prize |
| 0x05 | ExpireLotteryV1 | House closes, claims unclaimed |

## State Machine

```
┌─────────────┐
│ INITIALIZED │
└──────┬──────┘
       │ BuyTicket
       ▼
┌─────────────┐
│ TICKETS_SOLD │
└──────┬──────┘
       │ DrawWinners
       ▼
┌─────────────────┐
│ WINNERS_DRAWN  │◄──────── RevealTicket
└──────┬─────────┘
       │ ExpireLottery
       ▼
┌─────────────┐
│   EXPIRED   │
└─────────────┘
```

## Data Structures

### LotteryConfig

```rust
struct LotteryConfig {
    num_picks: u8,           // N: how many numbers player picks
    number_range: u8,        // M: numbers from 1 to M
    house_edge_bp: u32,      // House edge in basis points
    ticket_price: u64,       // Cost per ticket
    prize_tiers: Vec<PrizeTierConfig>,
}

struct PrizeTierConfig {
    matches_needed: u8,      // e.g., 6 for jackpot
    payout_percent: u32,      // % of prize pool (basis points)
    roll_to_next: bool,      // Unclaimed rolls over
}
```

### Standard Configurations

| Name | Picks | Range | House Edge | Odds (1 in) |
|------|-------|-------|------------|-------------|
| UK National | 6 | 59 | 25% | 45,057,474 |
| US Powerball | 5 | 69 | 30% | 292,201,338 |
| EuroMillions | 5+2 | 50+12 | 30% | 139,838,160 |
| Superenalotto | 6 | 90 | 20% | 622,614,630 |
| Neighborhood | 3 | 10 | 10% | 1,000 |

## Drawing Algorithm

Winning numbers use block hash entropy via the [Entropy Module](./entropy.md) (`dwow_sdk::crypto::entropy`):

```rust
use dwow_sdk::crypto::entropy::draw_unique_range;

// Draw N unique numbers from 1 to M
let numbers = draw_unique_range(block_hash, seed_nonce, num_picks, number_range);
```

This uses LCG-based without-replacement sampling to ensure unique picks.

See [Entropy Module](./entropy.md) for security levels and cumulative PoW entropy.

## Privacy Model

### Commitment Scheme

Players commit to tickets without revealing numbers:

```
commitment = PoseidonHash(
    PoseidonHash(...PoseidonHash(lottery_id, n1), n2...), nN,
    nonce
)
```

This allows:
- **Hidden tickets**: Numbers are not public during sale phase
- **Reveal later**: Player can prove they held specific numbers
- **ZK verification**: (Future) ZK proofs can verify matches without revealing

### Commit-Reveal Flow

1. **Buy Ticket**: Player commits `H(numbers, nonce)` → ticket stored
2. **Draw Winners**: House draws numbers → stored on-chain
3. **Reveal**: Player reveals numbers + nonce → match count verified
4. **Claim**: Winner provides ZK proof of match → receives payout

## The Lottery Problem Space

Lottery sits **between** BettingStake and Insurance in the DarkWow risk capital spectrum:

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

For small/neighborhood lotteries, BettingStake works well:
- Fixed ticket count limits max payout
- Pool accumulation is predictable

For large lotteries (Powerball-scale), BettingStake is a **band-aid** and true insurance/reinsurance is needed.

## Insurance Integration

Lotteries present well-defined risks ideal for insurance underwriting:

### Risk Profile

| Risk Type | Description | Mitigation |
|-----------|-------------|-------------|
| Jackpot Hit Early | Large payout before pool grows | Reinsurance |
| Pool Volatility | Prize pool variance between draws | Capital reserves |
| Fraud | Fake/fabricated tickets | ZK proof verification |
| Operator Deficit | House can't pay winners | Bonded house |

### Insurance Company Use Cases

1. **Underwrite Large Jackpots**
   - Insurance stakes against pool for large potential payouts
   - Earns premium for bearing jackpot risk
   - Only pays if jackpot actually hits

2. **Yield Farming**
   - Provide liquidity for prize pool
   - Earn house edge share
   - Positive expected value with law of large numbers

3. **Cross-Lottery Reinsurance**
   - Distribute jackpot risk across multiple lotteries
   - Diversified risk portfolio
   - Lower capital requirements per lottery

See [Insurance Market Contract](./insurance_market.md) for underwriter infrastructure.

## Money Contract Integration

| Action | Money Contract | Description |
|--------|----------------|-------------|
| Buy Ticket | BurnV2 | Locks ticket value, spend_hook triggers BuyTicketV1 |
| Claim Prize | MintV2 | Mints winner's share from prize pool |
| Expire | MintV2 | Mints unclaimed prizes to house |

## Related Contracts

- [Entropy Module](./entropy.md) - Provably fair randomness for all betting contracts
- [Insurance Market](./insurance_market.md) - Underwriter infrastructure for risk markets
- [BettingStake](./betting_stake.md) - Capital staking for betting games
- [DarkToshi Dice](./darktoshi_dice.md) - Commit-reveal gambling with house edge
- [Roulette](./roulette.md) - Fixed-odds betting (native BettingStake fit)
- [Baccarat](./baccarat.md) - Multi-round card game contract
- [Native Token](../dev/contracts/native_token.md) - Consensus-first native token

## Status

- [x] Contract implementation (6 functions)
- [x] ZK circuits (placeholder, need full implementation)
- [x] Standard lottery presets
- [x] Custom configuration support
- [ ] Full ZK proof verification for reveal
- [ ] Merkle tree for ticket commitments
- [ ] Integration tests
