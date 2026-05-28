# DarkBet Exchange Contract

A decentralized betting exchange supporting **two modes** via a single unified contract:

| Mode | `market_type` | Description |
|------|---------------|-------------|
| **Order-Book** | `0` | Peer-to-peer back/lay matching (DEX-style) |
| **AMM Pool** | `1` | Constant-product AMM for positions |

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                       DarkBet Exchange                               │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                      MARKET CREATION                         │   │
│   │                                                                  │   │
│   │   CreateMarketV1(market_type=0) ──► Order-Book Mode           │   │
│   │   CreateMarketV1(market_type=1) ──► AMM Pool Mode             │   │
│   │                                                                  │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                              │                                        │
│          ┌───────────────────┴───────────────────┐                   │
│          ▼                                       ▼                   │
│   ┌─────────────────┐                 ┌─────────────────┐           │
│   │  ORDER-BOOK     │                 │     AMM POOL    │           │
│   │                 │                 │                 │           │
│   │ PlaceBackV1     │                 │ BuyPositionV1   │           │
│   │ PlaceLayV1      │                 │ AddLiquidityV1  │           │
│   │ MatchOrdersV1   │                 │ RemoveLiquidity │           │
│   │ CancelOrderV1   │                 │                 │           │
│   │                 │                 │                 │           │
│   │ Matches:        │                 │ Positions:      │           │
│   │   Back/Lay      │                 │   Outcome shares│           │
│   │   peer-to-peer  │                 │   priced by AMM │           │
│   └─────────────────┘                 └─────────────────┘           │
│          │                                       │                   │
│          └───────────────────┬───────────────────┘                   │
│                              ▼                                        │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │                     COMMON OPERATIONS                        │   │
│   │                                                                  │   │
│   │   ResolveMarketV1 ──► Oracle attests outcome                 │   │
│   │   SettleMarketV1  ──► Winners paid from pool                 │   │
│   │   ClaimWinningsV1 ──► Position holders claim                 │   │
│   │                                                                  │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
└──────────────────────────────────────────────────────────────────────┘
```

## Order-Book Mode (market_type = 0)

Peer-to-peer betting where users place **back** and **lay** orders that get matched.

### Flow

1. **Create Market**: House creates market with `CreateMarketV1(market_type=0)`
2. **Place Back**: User A places back order for "Team A Wins" @ 2.5 odds
3. **Place Lay**: User B places lay order for "Team A Wins" @ 2.4 odds
4. **Match**: Orders matched when lay odds ≥ back odds (lay offers worse odds)
5. **Resolve**: Oracle attests the outcome
6. **Settle**: Winners paid, commission to treasury

### Key Concepts

- **Back**: Bet that an outcome WILL happen. Risk: stake. Reward: stake × odds
- **Lay**: Bet that an outcome will NOT happen. You become the bookie. Risk: liability. Reward: stake

## AMM Pool Mode (market_type = 1)

Automated market making where users buy **positions** (shares in outcomes) at AMM-calculated prices.

### Flow

1. **Create Market**: House creates AMM pool with `CreateMarketV1(market_type=1)`
2. **Add Liquidity**: LPs provide liquidity, receive LP shares
3. **Buy Position**: User buys shares in an outcome at AMM price
4. **Resolve**: Oracle attests the outcome
5. **Claim**: Winning position holders claim payouts

### AMM Pricing

Uses constant-product formula: `price = (other_pools × amount) / (pool_for_outcome + amount)`

```
Example (binary YES/NO market):

Initial state: 1000 tokens in YES pool, 1000 in NO pool
User buys 100 YES tokens:
  price = (1000 × 100) / (1000 + 100) = 90.9 tokens
  → User pays ~91 tokens for 100 YES shares

After purchase: 1100 YES, 1000 NO
New YES price = (1000 × 100) / (1100 + 100) = 83.3 tokens
```

## Database Trees

| Tree | Description |
|------|-------------|
| `darkbet_markets` | Market state indexed by market_id |
| `darkbet_back_orders` | Back orders (order-book mode) |
| `darkbet_lay_orders` | Lay orders (order-book mode) |
| `darkbet_matches` | Matched bets (order-book mode) |
| `darkbet_positions` | Positions (AMM mode) |
| `darkbet_lp_shares` | LP shares (AMM mode) |
| `darkbet_nullifiers` | Nullifiers for double-spend prevention |

## Functions

### Market Creation

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x00` | `CreateMarketV1` | Create market (order-book or AMM) |

### Order-Book Mode

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x01` | `PlaceBackV1` | Place back order |
| `0x02` | `PlaceLayV1` | Place lay order |
| `0x03` | `MatchOrdersV1` | Match back/lay orders |
| `0x06` | `CancelOrderV1` | Cancel unmatched order |

### AMM Mode

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x07` | `BuyPositionV1` | Buy outcome position |
| `0x08` | `AddLiquidityV1` | Add LP liquidity |
| `0x09` | `RemoveLiquidityV1` | Remove liquidity |
| `0x0A` | `ClaimWinningsV1` | Claim position winnings |

### Common

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x04` | `ResolveMarketV1` | Oracle resolves outcome |
| `0x05` | `SettleMarketV1` | Distribute winnings |

## Composability

DarkBet composes with:
- **BettingStake**: Liquidity pool for settlement
- **Oracle**: Event resolution
- **DAO-Escrow**: Commission treasury, governance

## Comparison with Prediction Market

The `prediction_market` contract is deprecated. DarkBet AMM mode (`market_type=1`) provides equivalent functionality.

| Aspect | Prediction Market | DarkBet Exchange |
|--------|------------------|------------------|
| AMM Mode | Yes (only) | Yes (`market_type=1`) |
| Order-Book | No | Yes (`market_type=0`) |
| Back/Lay | No | Yes |
| Liquidity | LP shares | LP shares or BettingStake |
| Fees | Protocol + LP | Configurable |

## Building

```bash
# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_darkbet_exchange_contract

# Run tests (library only)
cargo test -p darkfi_darkbet_exchange_contract --lib
```

## Implementation Status

### ZK Circuits ✅
- `create_market_v1.zk` - ✅ Working
- `buy_position_v1.zk` - ✅ Working
- `claim_winnings_v1.zk` - ✅ Working
- `add_liquidity_v1.zk` - ✅ Working

### Entrypoints
| Opcode | Function | Status | Child Call |
|--------|----------|--------|-----------|
| `0x00` | `CreateMarketV1` | ✅ Implemented | - |
| `0x01` | `PlaceBackV1` | ✅ Implemented | - |
| `0x02` | `PlaceLayV1` | ✅ Implemented | - |
| `0x03` | `MatchOrdersV1` | ✅ Implemented | - |
| `0x04` | `ResolveMarketV1` | ✅ Implemented | - |
| `0x05` | `SettleMarketV1` | ✅ Implemented | promissory_note::transfer_v1 |
| `0x06` | `CancelOrderV1` | ✅ Implemented | promissory_note::transfer_v1 |
| `0x07` | `BuyPositionV1` | ✅ Implemented | promissory_note::transfer_v1 |
| `0x08` | `AddLiquidityV1` | ✅ Implemented | promissory_note::transfer_v1 |
| `0x09` | `RemoveLiquidityV1` | ✅ Implemented | promissory_note::transfer_v1 |
| `0x0A` | `ClaimWinningsV1` | ✅ Implemented | promissory_note::transfer_v1 |

### Test Status
- Heavyweight pipeline test: ✅ PASSING (with promissory_note child calls)

## See Also

- [DarkBet Architecture](../../doc/src/arch/darkbet_exchange.md)
- [BettingStake Contract](../../doc/src/arch/betting_stake.md)
- [Oracle Contract](../../doc/src/arch/oracle.md)