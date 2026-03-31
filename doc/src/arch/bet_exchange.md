# Decentralized Betfair Exchange

A peer-to-peer betting exchange composed from existing DarkFi contracts: **DEX**, **BettingStake**, **Oracle**, and **DAO-Escrow**.

## Concept

A Betfair-style exchange where users **back** (bet for) or **lay** (bet against) outcomes, with bets matched peer-to-peer. The exchange earns commission, not house edge.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Decentralized Betfair Exchange                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                 │
│   │    BACK     │     │   MATCHING  │     │    LAY      │                 │
│   │  (Bet For)  │────▶│   ENGINE    │◀────│ (Bet Against)│                │
│   │             │     │    (DEX)    │     │             │                 │
│   └─────────────┘     └─────────────┘     └─────────────┘                 │
│         │                   │                       │                        │
│         │                   │                       │                        │
│         ▼                   ▼                       ▼                        │
│   ┌─────────────────────────────────────────────────────────────┐             │
│   │                    LIQUIDITY POOL                          │             │
│   │                  (BettingStake)                             │             │
│   │                                                              │             │
│   │   LP1 ──stake──▶ │         │◀──stake── LP2                 │             │
│   │                  │ Settlement│                              │             │
│   │                  │  Agent   │                              │             │
│   │                  │         │◀────fees──── Exchange          │             │
│   └──────────────────┴─────────┴──────────────────────────────┘             │
│                                    │                                          │
│                                    ▼                                          │
│   ┌─────────────┐     ┌─────────────┐     ┌─────────────┐                  │
│   │   ORACLE    │────▶│ RESOLUTION  │◀────│   PAYOUT    │                  │
│   │  (Events)   │     │   AGENT     │     │  DISTRIB    │                  │
│   └─────────────┘     └─────────────┘     └─────────────┘                  │
│                                                                              │
│   ┌─────────────┐     ┌─────────────┐                                       │
│   │  DAO-ESCROW│────▶│  GOVERNANCE │                                       │
│   │ (Commisson, │     │  Disputes   │                                       │
│   │  Upgrades)  │     │  Treasury   │                                       │
│   └─────────────┘     └─────────────┘                                       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Key Innovation: P2P vs House

| Traditional Sportsbook | Decentralized Exchange |
|------------------------|------------------------|
| House sets odds | Users set odds (back/lay) |
| House vs players | Players vs players |
| House takes house edge risk | Exchange takes commission only |
| Capital needed for payouts | LP provides liquidity, not house risk |
| Odds often manipulated | Market-driven odds |

**The exchange doesn't bet against users** - it matches them and takes a fee.

## Composition: Contract Roles

### 1. DEX (Matching Engine)

The DEX provides the core matching infrastructure:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              DEX: Bet Matching                            │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  CREATE ORDER:  BackBet {                                               │
│    outcome: "Team_A_Wins",                                              │
│    odds: 2.5,        // 2.5:1 payout                                     │
│    stake: 100,      // 100 tokens                                       │
│    max_liability: 250,  // If lay exists                                │
│  }                                                                        │
│                                                                          │
│  MATCHING ENGINE:                                                        │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐               │
│  │ Back Order   │    │    MATCH    │    │ Lay Order    │               │
│  │ Team_A @ 2.5 │ +  │   ENGINE    │ +  │ Team_A @ 2.4 │               │
│  └──────────────┘    └──────────────┘    └──────────────┘               │
│                                                                          │
│  Result: Back @ 2.5 matched with Lay @ 2.4                              │
│  Commission: 2% of winnings → Exchange treasury                         │
│                                                                          │
│  UNMATCHED ORDERS: Stored in order book for future matching              │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**DEX Functions Extended for Betting:**
- `CreateBetOrder` - Create back or lay order
- `MatchBetOrders` - Atomic match of back/lay at agreed odds
- `CancelOrder` - Cancel unmatched order
- `SettleBet` - Distribute winnings after resolution

### 2. BettingStake (Liquidity Pool)

Liquidity providers stake capital to enable settlement:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                        BettingStake: Liquidity                           │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                    Liquidity Pool                                │    │
│  │                                                                  │    │
│  │   LP1 Stake: 10,000 ──┐                                         │    │
│  │                       │                                          │    │
│  │   LP2 Stake: 25,000 ──┼──▶ Total Pool: 100,000                │    │
│  │                       │                                          │    │
│  │   LP3 Stake: 65,000 ──┘                                         │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                    │                                     │
│  What LPs Provide:                 │  What LPs Earn:                    │
│  - Settlement backing               │  - Commission share (e.g., 1.5%)   │
│  - Match guarantee                  │  - Volume-based incentives         │
│                                                                          │
│  How Settlement Works:                                                    │
│  1. Back wins: LP pool pays out (liability comes from layer's stake)    │
│  2. Lay wins: LP pool transfers layer's winnings                        │
│  3. LP's capital is NOT used for payouts (backer's stake covers lay)     │
│                                                                          │
│  LP Risk: Minimal! The pool guarantees settlement, not bets.          │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key insight**: Unlike betting games where house needs capital for payouts, here:
- Backer's stake covers potential winnings
- Layback's stake covers potential winnings
- LP pool ensures settlement even if edge cases occur

### 3. Oracle (Event Resolution)

Oracle resolves outcomes when events complete:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Oracle: Resolution                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  SPORTS ORACLE EXAMPLE:                                                 │
│                                                                          │
│  Oracle Operator: "ESPN_Data_Feed"                                     │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │ BEFORE MATCH:                                                     │   │
│  │                                                                   │   │
│  │ Back: "Team_A_Wins" @ 2.5  ←── User A believes Team A wins      │   │
│  │ Lay:  "Team_A_Wins" @ 2.4  ←── User B believes Team A loses      │   │
│  │                                                                   │   │
│  │ Match executes at 2.4 (lay@2.4 backs @ 2.5 = spread captured)  │   │
│  │ Exchange commission: 2% of 40 = 0.8 tokens                      │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                    │                                     │
│                                    ▼                                     │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │ AFTER MATCH:                                                      │   │
│  │                                                                   │   │
│  │ Oracle: PushValue(Team_A_Wins)  // Match result                 │   │
│  │ Oracle: AttestValue(Matches, Team_A_Wins)                        │   │
│  │                                                                   │   │
│  │ RESOLUTION:                                                       │   │
│  │ - User A (Back) wins: gets 2.4 × stake from User B (Lay)        │   │
│  │ - User B (Lay) loses: forfeits stake to User A                  │   │
│  │ - Exchange keeps commission                                       │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  CRITICAL: Oracle only determines OUTCOME, not payouts.                │
│  Payouts are determined by matched odds and stake amounts.             │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### 4. DAO-Escrow (Governance)

DAO manages exchange parameters, disputes, and treasury:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                      DAO-Escrow: Governance                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  MODE: TreasuryEndowment                                                 │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                        PREMIUM FLOWS                             │    │
│  │                                                                   │    │
│  │  Commission (2%) ──┬── treasury_share (70%) ──▶ DAO Treasury   │    │
│  │                   │                                           │    │
│  │                   └── endowment_share (30%) ──▶ Endowment     │    │
│  │                                               (LP protection)   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                    │                                     │
│  DAO Decisions:                                                         │
│  - Commission rate (e.g., 2% → 1.5% to compete)                       │
│  - LP protection fund allocation                                        │
│  - New market listings (what events can be bet on)                      │
│  - Oracle operator approvals                                          │
│  - Dispute resolution                                                  │
│                                                                          │
│  ┌─────────────────────────────────────────────────────────────────┐    │
│  │                      DISPUTE RESOLUTION                          │    │
│  │                                                                   │    │
│  │  Scenario: Oracle reports "Match Tied" but one user claims win  │    │
│  │                                                                   │    │
│  │  Process:                                                         │    │
│  │  1. User escalates dispute via DAO提案                            │    │
│  │  2. DAO votes on resolution (evidence from oracle)               │    │
│  │  3. If disputed: DAO can freeze settlement until resolved         │    │
│  │  4. DAO treasury pays out correct party                           │    │
│  │                                                                   │    │
│  └─────────────────────────────────────────────────────────────────┘    │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## The Complete Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    BETFAIR EXCHANGE: COMPLETE FLOW                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. MARKET CREATION (DAO Governance)                                       │
│     └── DAO creates market: "Premier League Match 247"                      │
│         - Available outcomes: Team_A_Wins, Team_B_Wins, Draw                 │
│         - Commission rate: 2%                                               │
│         - Oracle: ESPN_Sports_Feed                                          │
│                                                                              │
│  2. LIQUIDITY PROVISION (BettingStake)                                     │
│     ├── LP1 stakes 10,000 tokens                                            │
│     ├── LP2 stakes 25,000 tokens                                            │
│     └── LP3 stakes 15,000 tokens → Total: 50,000                          │
│                                                                              │
│  3. ORDER PLACEMENT (DEX)                                                  │
│     ├── Alice: Back Team_A @ 2.5, stake 100                               │
│     ├── Bob:   Lay Team_A @ 2.4, stake 103 (liability 144)                │
│     └── Orders stored in order book                                         │
│                                                                              │
│  4. ORDER MATCHING (DEX)                                                   │
│     ├── Matching engine finds: Back @ 2.5 ↔ Lay @ 2.4                      │
│     ├── Execution price: 2.4 (better lay price)                            │
│     ├── Alice's potential win: 240 (2.4 × 100)                             │
│     ├── Bob's potential loss: 144 (2.4 - 1 × 100)                          │
│     └── Commission: 2% × 40 = 0.8 tokens → Treasury                        │
│                                                                              │
│  5. EVENT RESOLUTION (Oracle)                                              │
│     ├── Match completes: Team_A wins 2-1                                   │
│     ├── Oracle: PushValue(Team_A_Wins)                                      │
│     └── Attestation created for resolution                                  │
│                                                                              │
│  6. SETTLEMENT (DEX + BettingStake)                                        │
│     ├── Alice (Back winner): receives 240 from Bob's lay stake              │
│     ├── Bob (Lay loser): forfeits 144 to Alice                             │
│     ├── Commission 0.8 → DAO treasury                                       │
│     └── BettingStake: LP pool unchanged (no house risk)                    │
│                                                                              │
│  7. LP EARNINGS (BettingStake)                                             │
│     ├── Volume: 240 (total matched)                                        │
│     ├── LP earns: 0.5% of volume = 1.2 tokens                             │
│     └── Distributed proportionally to LP stakes                             │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Comparison: Exchange vs Traditional Betting

| Aspect | Traditional Bookie | Betfair Exchange | DarkFi Exchange |
|--------|-------------------|------------------|-----------------|
| Odds source | House sets | Users/market | P2P via DEX |
| Counterparty | House | Other users | Other users |
| House edge | 5-10% | 2% commission | 2% commission |
| Capital need | For payouts | For settlement | For settlement |
| Risk profile | High (house edge risk) | Low (match only) | Minimal |

## Why This Works with Existing Contracts

### DEX (Matching Engine)
- Already has atomic swap infrastructure
- `CreateSwap` → `CreateBetOrder`
- `AcceptSwap` → `MatchBetOrders`
- `ExecuteSwap` → `SettleBet`
- Extends to support odds-based matching

### BettingStake (Liquidity)
- Already designed for staking against betting
- LP providers earn from betting volume
- Natural fit for settlement guarantee

### Oracle (Resolution)
- Already has attestation infrastructure
- Sports oracle example in existing docs
- Attestation → Event resolution for bets

### DAO-Escrow (Governance)
- Already has TreasuryEndowment mode
- Commission distribution already built
- Dispute resolution via governance

## Risk Model

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           RISK ANALYSIS                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  EXCHANGE RISKS:                                                         │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │ 1. Settlement Risk: LP pool must cover edge cases               │   │
│  │    Mitigation: Endowment from commission (30%)                    │   │
│  │    DAO can vote to use endowment for extraordinary cases         │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │ 2. Oracle Risk: Wrong/paused data                                │   │
│  │    Mitigation: DAO-approved oracle operators                     │   │
│  │    Multiple oracle sources for important markets                  │   │
│  │    Manual resolution via DAO governance as backup                │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │ 3. Dispute Risk: Users contest outcomes                          │   │
│  │    Mitigation: Oracle attestation is authoritative               │   │
│  │    DAO governance for edge cases                                 │   │
│  │    Clear market rules before trading begins                      │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  ┌──────────────────────────────────────────────────────────────────┐   │
│  │ 4. LP Impermanent Loss: Volume drops                              │   │
│  │    Mitigation: Volume-based LP rewards                           │   │
│  │    LP can unstake after cooldown period                           │   │
│  └──────────────────────────────────────────────────────────────────┘   │
│                                                                          │
│  WHAT EXCHANGE DOES NOT BEAR:                                            │
│  - Outcome risk (users bear this)                                        │
│  - Odds risk (market determines)                                         │
│  - Payout risk (covered by matched stakes)                               │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Implementation Phases

### Phase 1: Core Infrastructure
- [ ] Extend DEX for bet orders (back/lay)
- [ ] Add odds-based matching to DEX
- [ ] Commission tracking in DEX

### Phase 2: Integration
- [ ] Oracle integration for resolution
- [ ] BettingStake integration for LP
- [ ] DAO-Escrow for commission treasury

### Phase 3: Markets
- [ ] Sports betting markets (simple win/lose/draw)
- [ ] Financial markets (price above/below)
- [ ] Custom markets via DAO proposal

### Phase 4: Advanced
- [ ] In-play betting (odds change during event)
- [ ] Parlay bets (multiple outcomes)
- [ ] User-created markets

## See Also

- [DEX Contract](../dex/) - Matching engine
- [BettingStake Contract](../betting_stake/) - Liquidity provision
- [Oracle Contract](../oracle/) - Event resolution
- [DAO-Escrow Contract](../dao_escrow/) - Governance
- [BettingStake](./betting_stake.md) - Capital staking for betting games
- [Lottery](./lottery.md) - Parimutuel betting (bridge to insurance)