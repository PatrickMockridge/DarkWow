# DarkBet Exchange Architecture

A unified decentralized betting exchange supporting two modes:
- **Order-Book Mode**: Peer-to-peer back/lay matching
- **AMM Pool Mode**: Constant-product automated market making

## Overview

DarkBet demonstrates composability by combining:
- **DEX/BettingStake**: Order matching or liquidity provision
- **Oracle**: Event resolution
- **DAO-Escrow**: Commission treasury, governance

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         DarkBet Exchange Architecture                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                           MARKET CREATION                               │  │
│  │                                                                        │  │
│  │   CreateMarketV1(market_type=0) ──► Order-Book Mode                   │  │
│  │   CreateMarketV1(market_type=1) ──► AMM Pool Mode                     │  │
│  │                                                                        │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                               │                                              │
│          ┌───────────────────┴───────────────────┐                          │
│          ▼                                       ▼                          │
│  ┌─────────────────────┐             ┌─────────────────────┐              │
│  │    ORDER-BOOK        │             │      AMM POOL       │              │
│  │                      │             │                      │              │
│  │ PlaceBackV1 (0x01)  │             │ BuyPositionV1 (0x07) │              │
│  │ PlaceLayV1 (0x02)   │             │ AddLiquidityV1 (0x08)│              │
│  │ MatchOrdersV1 (0x03)│             │ RemoveLiquidity(0x09)│              │
│  │ CancelOrderV1 (0x06)│             │ ClaimWinningsV1(0x0A)│              │
│  │                      │             │                      │              │
│  │ Back/Lay orders      │             │ Positions:           │              │
│  │ matched peer-to-peer │             │ Outcome shares priced │              │
│  │                      │             │ by AMM formula       │              │
│  └─────────────────────┘             └─────────────────────┘              │
│          │                                       │                            │
│          └───────────────────┬───────────────────┘                            │
│                              ▼                                                │
│  ┌───────────────────────────────────────────────────────────────────────┐  │
│  │                      COMMON OPERATIONS                                  │  │
│  │                                                                        │  │
│  │   ResolveMarketV1 (0x04) ──► Oracle attests outcome                   │  │
│  │   SettleMarketV1 (0x05)  ──► Winners paid from pool                   │  │
│  │                                                                        │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Market Types

### Order-Book Mode (market_type = 0)

Peer-to-peer betting via DEX-style order matching:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Order-Book Mode Flow                                    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  1. House creates market with CreateMarketV1(market_type=0)            │
│                                                                          │
│  2. Alice places BACK order:                                            │
│     "Team A Wins @ 2.5" with 100 stake                                 │
│                                                                          │
│  3. Bob places LAY order:                                               │
│     "Team A Wins @ 2.4" with 104 liability                             │
│                                                                          │
│  4. Matcher matches orders at 2.4 (lay's worse odds)                  │
│     → Spread (0.1) incentivizes LP                                     │
│                                                                          │
│  5. Oracle resolves: Team A Wins                                       │
│                                                                          │
│  6. Settlement:                                                         │
│     → Alice wins: 100 × 2.4 = 240 (minus commission)                   │
│     → Bob loses: Forfeits liability (144) to Alice                    │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### AMM Pool Mode (market_type = 1)

Automated market making via constant-product formula:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    AMM Pool Mode Flow                                    │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  1. House creates AMM pool with CreateMarketV1(market_type=1)         │
│                                                                          │
│  2. LPs add liquidity:                                                  │
│     → Receive LP shares representing pool ownership                     │
│     → Earn protocol_fee + lp_fee on each trade                         │
│                                                                          │
│  3. Alice buys 100 YES shares @ 2.5:                                   │
│     Price = (other_pools × amount) / (pool_for_outcome + amount)       │
│           = (1000 × 100) / (1000 + 100) = 90.9 tokens                  │
│                                                                          │
│  4. Bob buys 100 NO shares @ 2.5:                                      │
│     Price = (1100 × 100) / (1000 + 100) = 95.2 tokens                │
│                                                                          │
│  5. Oracle resolves: YES wins                                          │
│                                                                          │
│  6. Claim: Alice claims payout, Bob's tokens redistributed             │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Contract Functions

| Opcode | Function | Mode | Description |
|--------|----------|------|-------------|
| 0x00 | CreateMarketV1 | Both | Create market (order-book or AMM) |
| 0x01 | PlaceBackV1 | Order-Book | Place back order |
| 0x02 | PlaceLayV1 | Order-Book | Place lay order |
| 0x03 | MatchOrdersV1 | Order-Book | Match back and lay |
| 0x04 | ResolveMarketV1 | Both | Oracle resolves outcome |
| 0x05 | SettleMarketV1 | Both | Distribute winnings |
| 0x06 | CancelOrderV1 | Order-Book | Cancel unmatched order |
| 0x07 | BuyPositionV1 | AMM | Buy outcome position |
| 0x08 | AddLiquidityV1 | AMM | Add LP liquidity |
| 0x09 | RemoveLiquidityV1 | AMM | Remove liquidity |
| 0x0A | ClaimWinningsV1 | AMM | Claim position winnings |

## Data Model

### Market

```rust
pub struct Market {
    pub market_id: pallas::Base,
    pub creator: PublicKey,
    pub description: String,
    pub outcomes: Vec<String>,
    pub oracle_id: pallas::Base,
    pub commission_bp: u32,
    pub market_type: MarketType,  // NEW: OrderBook or AmmPool
    pub state: MarketState,
    // Order-book fields
    pub back_volume: u64,
    pub lay_volume: u64,
    pub matched_volume: u64,
    // AMM fields
    pub total_pool: u64,
    pub total_lp_shares: u64,
    pub outcome_pools: Vec<u64>,
    pub protocol_fee: u32,
    pub lp_fee: u32,
    // Common
    pub close_block: u64,
    pub resolved_at: Option<u64>,
    pub winning_outcome: Option<u8>,
    pub created_at: u64,
}
```

### MarketType Enum

```rust
pub enum MarketType {
    OrderBook = 0,
    AmmPool = 1,
}
```

### Position (AMM Mode)

```rust
pub struct Position {
    pub position_id: pallas::Base,
    pub market_id: pallas::Base,
    pub owner: PublicKey,
    pub outcome: u8,
    pub amount: u64,
    pub potential_payout: u64,
    pub state: PositionState,
    pub created_at: u64,
}
```

### LpShare (AMM Mode)

```rust
pub struct LpShare {
    pub lp_share_id: pallas::Base,
    pub market_id: pallas::Base,
    pub provider: PublicKey,
    pub shares: u64,
    pub earned_fees: u64,
    pub state: LpShareState,
    pub created_at: u64,
}
```

## AMM Pricing Formula

The constant-product AMM formula:

```
price = (other_pools × amount) / (pool_for_outcome + amount)

where:
  - other_pools = sum of all outcome pools except the chosen one
  - amount = number of shares being purchased
  - pool_for_outcome = current tokens in the chosen outcome pool
```

### Example: Binary YES/NO Market

```
Initial State:
  - YES pool: 1000 tokens
  - NO pool: 1000 tokens
  - Total pool: 2000 tokens

Alice buys 100 YES shares:
  price = (1000 × 100) / (1000 + 100) = 90.9 tokens
  → Alice pays 90.9 tokens for 100 YES shares

After Alice's purchase:
  - YES pool: 1090.9 tokens
  - NO pool: 1000 tokens
  - Total pool: 2090.9 tokens

Bob buys 100 NO shares:
  price = (1090.9 × 100) / (1000 + 100) = 99.17 tokens
  → Bob pays 99.17 tokens for 100 NO shares

Final State:
  - YES pool: 1090.9 tokens
  - NO pool: 1099.17 tokens
  - Total pool: 2190.07 tokens
```

## State Machines

### Market Lifecycle

```
┌─────────┐   CreateMarket   ┌─────────┐   CloseBlock   ┌───────────┐
│  None   │ ───────────────▶│   Open   │ ─────────────▶│  Closed   │
└─────────┘                 └─────────┘                └───────────┘
                               │                            │
                               │ ResolveMarket              │ (if no resolution)
                               ▼                            ▼
                          ┌───────────┐              ┌────────────┐
                          │ Resolved  │              │ Cancelled  │
                          └───────────┘              └────────────┘
                               │
                               │ SettleMarket
                               ▼
                          ┌──────────┐
                          │ Settled  │
                          └──────────┘
```

### Order Lifecycle (Order-Book Mode)

```
┌─────────┐   PlaceBack/Lay   ┌─────────┐   MatchOrders   ┌──────────┐
│  None   │ ─────────────────▶│   Open   │ ─────────────▶│  Matched  │
└─────────┘                   └─────────┘                └──────────┘
                                   │
                                   │ CancelOrder
                                   ▼
                              ┌───────────┐
                              │ Cancelled │
                              └───────────┘
```

### Position Lifecycle (AMM Mode)

```
┌─────────┐   BuyPosition    ┌─────────┐   ResolveMarket   ┌──────────┐
│  None   │ ───────────────▶│  Active  │ ────────────────▶│  Claimed │
└─────────┘                 └─────────┘                   └──────────┘
                                   │
                                   │ (market cancelled)
                                   ▼
                              ┌───────────┐
                              │ Refunded  │
                              └───────────┘
```

## Composability Details

### BettingStake Integration

BettingStake provides:
- LP staking pool for settlement guarantee
- Commission distribution

```
DarkBet Settlement Flow:
1. Determine winner/loser
2. Calculate payout
3. Call BettingStake for settlement authorization
4. Transfer from loser to winner (minus commission)
5. Commission → DAO-Escrow treasury
```

### Oracle Integration

Oracle provides:
- Event outcome determination
- Attestation verification

```
Resolution Flow:
1. Oracle pushes value: PushValue(event_result)
2. Oracle creates attestation: AttestValue(predicate, threshold)
3. DarkBet calls: ResolveMarketV1(oracle_attestation)
4. DarkBet verifies oracle signature
5. Market resolved with outcome
```

### DAO-Escrow Integration

DAO-Escrow provides:
- Commission treasury
- Market governance

```
Commission Flow:
Protocol Fee (e.g., 1%) ──▶ DAO Treasury
LP Fee (e.g., 2%) ─────────▶ LP Providers

DAO Treasury Distribution:
├── 70% → DAO ops (grants, development)
└── 30% → Endowment (LP protection)
```

## Deprecating Prediction Market

The standalone `prediction_market` contract is deprecated in favor of DarkBet's AMM mode.

**Migration path:**
```rust
// Old prediction_market contract
CreateMarketV1 { /* ... */ }

// New darkbet_exchange with AMM mode
CreateMarketV1 {
    market_type: 1,       // Enable AMM mode
    protocol_fee: 100,   // 1%
    lp_fee: 200,         // 2%
    /* ... */
}
```

## Security Considerations

1. **Oracle trust**: Market relies on oracle for honest resolution
2. **AMM impermanent loss**: LPs should understand IL risks
3. **Slippage**: Position purchases should include min_payout protection
4. **Nullifier uniqueness**: Prevent double-claims on positions
5. **Settlement atomicity**: Ensure funds transfer is atomic

## See Also

- [DEX Contract](./dex.md) - Matching engine
- [BettingStake Contract](./betting_stake.md) - Liquidity provision
- [Oracle Contract](./oracle.md) - Event resolution
- [DAO-Escrow Contract](./dao_escrow.md) - Governance
- **Betfair-style exchange**: DarkBet supports an order-book mode with back/lay matching where users back outcomes (bet they happen) or lay outcomes (bet they don't), with the exchange matching counterparties. Combined with AMM pool mode for constant-product automated market making.