# Block Height Prediction Market Contract

A **proof-of-concept** contract for betting on the canonical DarkFi block height at a specific time.

## Overview

This contract demonstrates using DarkFi's proof-of-work blockchain as a trustless randomness source for prediction markets. Participants bet on what the canonical block height will be at a specific Unix timestamp.

## Key Innovation

Instead of relying on an oracle to report outcomes, the contract uses DarkFi's **PoW consensus mechanism** to determine results using cumulative entropy from K consecutive block hashes:

```
Randomness_Source = Cumulative PoW entropy from K consecutive blocks
```

The contract uses `wasm::util::get_block_hash(height)` to retrieve K consecutive block hashes (where K = confirmation_depth), combining them via poseidon for cumulative entropy.

## How It Works

### 1. Create Market

```rust
CreateMarketV1 {
    creator: PublicKey,
    target_time: u64,      // Unix timestamp for resolution
    confirmation_depth: u8, // PoW confirmations (K blocks)
    token_id: pallas::Base,
}
```

### 2. Place Positions

```rust
CreatePositionV1 {
    market_id: MarketId,
    predicted_height: u64,
    position_type: u8,    // 0=BELOW, 1=EXACT, 2=ABOVE
    tolerance: u8,         // +/- for "close" bonus
    amount: u64,
}
```

### 3. Resolution

```rust
// After target_time + confirmation_depth blocks:
// Use K consecutive block hashes for cumulative entropy
let confirmation_depth = market.confirmation_depth as usize;
let mut entropy = pallas::Base::zero();

for i in 0..confirmation_depth {
    let block_height = current_block.saturating_sub(i as u32);
    let block_hash = wasm::util::get_block_hash(block_height)?;
    // Combine block hash bytes into entropy
    let block_entropy = poseidon_hash(block_hash[0..8], block_hash[8..16], ...);
    entropy = poseidon_hash(entropy, block_entropy);
}

let resolved_height = derive_height_from_entropy(entropy, base_height, expected);
```

### 4. Claim Winnings

```rust
ClaimWinningsV1 {
    position_id: PositionId,
    market_id: MarketId,
    owner: PublicKey,
}
```

## Position Types

| Type | Condition | Payout |
|------|-----------|--------|
| **BELOW** | resolved < predicted | Proportional |
| **EXACT** | resolved == predicted | 3x Jackpot |
| **ABOVE** | resolved > predicted | Proportional |
| **CLOSE** | within tolerance | 0.5x |

## Security Analysis

### Randomness Source Comparison

| Source | Entropy | Trust Model | Suitable for Gambling? |
|--------|---------|------------|------------------------|
| tx_hash | ~64 bits | Single tx | Conditional |
| PoW block hash | ~256 bits | Distributed miners | Yes (with commit-reveal) |
| ECVRF | ~256 bits | Key holder | Conditional |

### Why tx_hash Works (Mostly)

The tx_hash appears in a block that must satisfy the PoW target:
1. Miner must find valid PoW to include the transaction
2. Cannot choose arbitrary tx_hash value
3. Multiple entropy sources combine well

**Limitation**: Miners have slight influence over tx ordering within blocks.

### Implemented Opcode: BlockHashGet

**UPDATE**: The `wasm::util::get_block_hash(block_height)` function is now implemented in the WASM runtime!

This enables:

```rust
// AVAILABLE in wasm runtime:
let block_hash = wasm::util::get_block_hash(block_height)?;
```

This enables:
- Direct RandomX output access
- Cumulative entropy from K consecutive blocks
- True PoW randomness without tx dependency

See [Provable Randomness](provable_randomness.md) for full analysis.

## Comparison to DarkToshi Dice

| Aspect | DarkToshi Dice | Block Height Prediction |
|--------|---------------|------------------------|
| **Randomness** | tx_hash (single) | Cumulative PoW block hashes (K blocks) |
| **Confirmation** | Player-selected K blocks | Market-determined K blocks |
| **Outcome** | Discrete (0-99) | Continuous (any height) |
| **Oracle Required** | No | No |
| **Complexity** | Mature | Proof-of-Concept |

## Relationship to Prediction Market

This is a **specialized proof-of-concept** that demonstrates:
- PoW-backed randomness without oracle
- Simplified AMM (single pool vs full AMM)
- Block-based resolution

The general [Prediction Market](prediction_market.md) contract handles multi-outcome markets with full liquidity provision.

## Contract State Machine

```
ACTIVE ──[ResolveMarket]──> RESOLVED
   │
   └──[CancelMarket]──> CANCELLED
```

## Implementation Status

| Component | Status | Notes |
|-----------|--------|-------|
| CreateMarket | ✅ Complete | |
| CreatePosition | ✅ Complete | |
| ResolveMarket | ✅ Complete | Uses PoW block hashes |
| ClaimWinnings | ✅ Complete | |
| CancelMarket | ✅ Complete | |
| ZK Circuits | ✅ Complete | create_market_v1.zk, create_position_v1.zk |

## See Also

- [Provable Randomness](provable_randomness.md) - Deep dive into randomness sources
- [DarkToshi Dice](darktoshi_dice.md) - Mature gambling contract
- [Prediction Market](prediction_market.md) - General prediction markets
- [Consensus Mechanism](../../src/validator/consensus.rs) - Fork resolution
- [PoW Module](../../src/validator/pow.rs) - RandomX implementation
