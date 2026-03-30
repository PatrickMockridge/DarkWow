# Prediction Market Contract

A decentralized prediction market using constant-product AMM pricing for position liquidity.

## Overview

Prediction markets allow participants to trade positions on the outcomes of future events. The market prices positions using an AMM-style mechanism where:

- **YES positions** on an outcome increase in value if that outcome occurs
- **NO positions** increase in value if that outcome does not occur
- Liquidity providers earn fees from the spread

## Core Pricing Formula

```
position_price = (other_pools * amount) / (pool_for_outcome + amount)
```

This ensures:
- Early bets have even odds (50/50)
- As bets accumulate, odds approach true probability
- LP fees create a spread that LP providers earn

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| CreateMarketV1 | 0x00 | Create a new prediction market |
| CreatePositionV1 | 0x01 | Bet on an outcome |
| AddLiquidityV1 | 0x02 | Provide liquidity to a market |
| RemoveLiquidityV1 | 0x03 | Withdraw liquidity + fees |
| ResolveMarketV1 | 0x04 | Oracle resolves market with outcome |
| ClaimWinningsV1 | 0x05 | Winners claim their payouts |
| CancelMarketV1 | 0x06 | Cancel market and refund bettors |

## Market Lifecycle

```
CREATED ──[CreatePosition]──> ACTIVE ──[ResolveMarket]──> RESOLVED
                                  │
                                  └──[CancelMarket]──> CANCELLED
```

## Key Data Structures

### Market

```rust
struct Market {
    id: MarketId,                    // Poseidon hash of market parameters
    creator: PublicKey,               // Market creator
    question: Vec<u8>,                // "Will BTC be > $100k on 2025-01-01?"
    resolve_time: u64,                // When market resolves
    num_outcomes: u8,                // 2 for YES/NO, N for discrete
    total_pool: u64,                 // Sum of all bets + LP fees
    total_lp_shares: u64,            // Total LP shares outstanding
    outcome_pools: Vec<u64>,         // Pool per outcome
    state: MarketState,              // Active/Frozen/Resolved/Cancelled
    protocol_fee: u32,                // Fee in basis points
    lp_fee: u32,                     // LP fee in basis points
    oracle_pubkey: PublicKey,         // Oracle that can resolve
}
```

### Position

```rust
struct Position {
    id: PositionId,                  // Unique position ID
    market_id: MarketId,             // Which market
    owner: PublicKey,                 // Position owner
    outcome: u8,                     // 0 = first outcome, 1 = second, etc.
    amount: u64,                     // Tokens wagered
    claimed: bool,                    // Whether winnings were claimed
}
```

## Integration with Other Contracts

### Money Contract

- **CreatePosition**: User burns tokens via Money::BurnV2 with spend_hook
- **ClaimWinnings**: User mints tokens via Money::MintV2

### Oracle Contract

- Markets specify an oracle_pubkey that can resolve outcomes
- ResolveMarket verifies oracle signature over the resolution

### Insurance Market

- Prediction market prices inform insurance premium calculations
- P(event) × impact = expected loss = insurance premium baseline

## Known Limitations

### Oracle Signature Verification (HIGH)

The `ResolveMarketV1` function accepts an oracle signature but does not cryptographically verify it using the oracle's public key. Currently:

- Only checks that `oracle_signature != pallas::Base::zero()` and `attestation.is_empty()`
- Full Schnorr signature verification requires ZK infrastructure integration

**TODO**: Implement proper Schnorr signature verification:
```rust
// In process_instruction_v1:
message = poseidon_hash([market.id, pallas::Base::from(params.outcome as u64)]);
market.oracle_pubkey.verify(&message_bytes, &signature)?;
```

### ZK Proof Verification (HIGH)

The `ClaimWinningsV1` function accepts a ZK proof but does not verify it. Currently:

- Checks that `proof.is_empty()` returns false
- Verifies `position.owner == params.owner` as access control
- ZK proof should be verified in a verifier circuit before payout

**TODO**: Integrate with zkas verifier to verify the proof demonstrates:
- Knowledge of position owner's secret key
- Position is in the Merkle tree of positions
- Market resolution outcome is correct

### Overflow Protection

All arithmetic functions use `checked_mul` and return `ArithmeticOverflow` error on integer overflow, protecting against manipulation attacks.

## File Structure

```
src/contract/prediction_market/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Function enum, constants
│   ├── error.rs                  # Error types
│   ├── entrypoint.rs             # init, metadata, exec, update
│   ├── entrypoint/
│   │   ├── create_market_v1.rs
│   │   ├── create_position_v1.rs
│   │   ├── add_liquidity_v1.rs
│   │   ├── remove_liquidity_v1.rs
│   │   ├── resolve_market_v1.rs
│   │   ├── claim_winnings_v1.rs
│   │   └── cancel_market_v1.rs
│   ├── model/
│   │   └── mod.rs               # Market, Position, Params, Updates
│   └── client/
│       └── mod.rs               # Client-side builders
└── proof/                        # ZK circuits (TODO)
```

## See Also

- [Risk Market Ecosystem](../contract/risk_market_ecosystem.md) — How prediction markets combine with insurance
- [Oracle Contract](oracle.md) — External data integration
- [Attestation Contract](attestation.md) — Claims verification
