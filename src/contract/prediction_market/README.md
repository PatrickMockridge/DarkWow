# Prediction Market Contract

A privacy-preserving prediction market contract for DarkFi, enabling decentralized forecasting with liquidity pools and oracle resolution.

## Overview

Prediction markets allow users to bet on the outcome of future events. This contract implements:

1. **Market Creation**: Anyone can create a market with a question and resolution criteria
2. **Position/Bet Creation**: Users bet on outcomes by locking funds in a liquidity pool
3. **Liquidity Provision**: LPs provide liquidity and earn fees
4. **Oracle Resolution**: Authorized oracle determines the winning outcome
5. **Winnings Claim**: Winners claim payouts based on their share of the winning pool

## Key Features

- **Privacy-preserving**: Bet details are committed via Poseidon hash
- **Constant-product AMM pricing**: Inspired by Uniswap v2 for position pricing
- **Liquidity providers**: Earn fees by providing liquidity to outcome pools
- **Oracle-based resolution**: Authorized oracles resolve markets
- **Cancellation support**: Markets can be cancelled before resolution with refunds

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize market with settings |
| CreateMarketV1 | 0x01 | Create a new prediction market |
| CreatePositionV1 | 0x02 | Create a bet/position on an outcome |
| AddLiquidityV1 | 0x03 | Add liquidity to a market |
| RemoveLiquidityV1 | 0x04 | Remove liquidity and receive payout |
| ResolveMarketV1 | 0x05 | Oracle resolves the market |
| ClaimWinningsV1 | 0x06 | Claim payout after resolution |
| CancelMarketV1 | 0x07 | Cancel market before resolution |
| WithdrawFeesV1 | 0x08 | LP withdraws earned fees |

## Market States

```
ACTIVE ──[ResolveMarket]──> RESOLVED ──[ClaimWinnings]──> CLAIMED
   │                              │
   └──[CancelMarket]──> CANCELLED─┘
```

## Pricing Model

Position pricing uses a constant-product AMM inspired formula:

```
price = (other_pools * amount) / (pool_for_outcome + amount)
```

This ensures:
- Early bettors get favorable odds
- Price approaches 1 as bets approach even distribution
- Liquidity providers earn fees from price movement

## Payout Calculation

```
payout = total_pool * (position_amount / winning_pool) * (1 - fees)
```

Where fees = protocol_fee + lp_fee (in basis points).

## Building

```bash
# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_prediction_market_contract

# Run tests
cargo test -p darkfi_prediction_market_contract
```

## Usage Example

```rust
use darkfi_prediction_market_contract::client::CreateMarketV1Builder;

// Create a YES/NO market
let (params, market_id) = CreateMarketV1Builder::new(
    creator_pubkey,
    "Will BTC be > $100k on 2025-01-01?".to_string(),
    1704067200, // resolve_time (Unix timestamp)
)
.betting_closes(1704063600) // betting closes 1 hour before resolve
.num_outcomes(2) // YES/NO
.build();

// Create a position/bet on YES
use darkfi_prediction_market_contract::client::CreatePositionV1Builder;

let (params, position_id) = CreatePositionV1Builder::new(
    market_id,
    0, // outcome 0 = YES
    1000, // amount
    bettor_pubkey,
)
.build();
```

## Integration with Money Contract

The Prediction Market contract integrates with the Money contract for value transfers:

### Transaction Structure

A complete bet transaction should include:

1. **Money::Burn** (parent call)
   - Burns the player's bet value
   - Sets `spend_hook` to authorize PredictionMarket::CreatePositionV1
   - Use `user_data_enc` to pass position metadata

2. **PredictionMarket::CreatePositionV1** (child call, `parent_index=0`)
   - Receives burn authorization via spend_hook
   - Creates position in outcome pool
   - Updates market pool totals

### Winnings Claim

After market resolution:

1. **PredictionMarket::ClaimWinningsV1**
   - Verifies market is resolved
   - Verifies position matches winning outcome
   - Calculates payout

2. **Money::TokenMint** (separate transaction)
   - Mints payout to winner
   - Based on claimed winnings update

## Database Trees

| Tree | Key | Value |
|------|-----|-------|
| MARKETS_TREE | market_id (serialize) | Market struct |
| POSITIONS_TREE | position_id (serialize) | Position struct |
| LIQUIDITY_TREE | provider pubkey (serialize) | LpShare struct |
| CLAIMS_TREE | position_id (serialize) | ClaimWinningsUpdateV1 |
| RESOLUTIONS_TREE | market_id (serialize) | ResolveMarketUpdateV1 |

## Integration with Other Contracts

This contract is designed to compose with:

- **Money Contract**: Value transfer for bets and payouts
- **DaoEscrow Contract**: Potential for decentralized oracle escalation
- **DarkToshi Dice Contract**: Gambling primitives for additional market types

## See Also

- [Money Contract](../money_v2/) - Value transfer integration
- [DaoEscrow Contract](../dao_escrow/) - DAO escrow integration
- [DarkToshi Dice Contract](../darktoshi_dice/) - Gambling primitives