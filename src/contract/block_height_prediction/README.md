# Block Height Prediction Market Contract

A **proof-of-concept** contract for betting on the canonical block height at a specific time. This contract demonstrates how DarkFi's proof-of-work blockchain can be leveraged as a trustless randomness source for prediction markets.

## Concept

Participants bet on what the "official" block height will be at a specific Unix timestamp. Instead of relying on an oracle to report the outcome, the contract uses DarkFi's PoW blockchain consensus mechanism to determine the result.

**Core Question**: "What will be the canonical block height at timestamp T?"

## How It Works

```
1. Create Market:
   - Creator sets a target timestamp for resolution
   - Market goes live, accepting position bets

2. Place Positions:
   - Participants bet BELOW, EXACT, or ABOVE a predicted height
   - Positions are tracked in on-chain pools

3. Wait for Confirmation:
   - After target_time + confirmation_depth blocks
   - Block hashes are retrieved for cumulative PoW entropy

4. Resolution:
   - Cumulative entropy derived from K consecutive PoW block hashes
   - Resolved height calculated from cumulative entropy
   - Winners claim proportional payouts
```

## Position Types

| Type | Description | Payout |
|------|-------------|--------|
| **BELOW** | Bet block height < predicted | Proportional |
| **EXACT** | Bet block height == predicted | 3x Jackpot |
| **ABOVE** | Bet block height > predicted | Proportional |

**Close Bonus**: Within tolerance range earns half payout

## Security Model

Unlike oracle-based prediction markets, this contract uses DarkFi's proof-of-work:

| Source | Security | Notes |
|--------|----------|-------|
| PoW Block Hash | High | Direct RandomX output access |
| PoW Confirmation | High | K blocks = p^K manipulation difficulty |
| Cumulative Entropy | Very High | K consecutive block hashes combined |

### BlockHashGet Opcode - Now Available!

The contract now uses `wasm::util::get_block_hash(block_height)` for stronger PoW-backed randomness:

```
wasm::util::get_block_hash(height)  // AVAILABLE
```

This enables:
- Direct use of PoW block hashes (RandomX output)
- Cumulative entropy from K consecutive blocks
- True PoW-backed randomness without tx dependency

**See**: [Provable Randomness](../../doc/src/arch/provable_randomness.md) for full analysis

## Contract Functions

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | InitializeV1 | Initialize contract settings |
| 0x01 | CreateMarketV1 | Create a new prediction market |
| 0x02 | CreatePositionV1 | Place a bet on a market |
| 0x03 | ResolveMarketV1 | Resolve market using PoW entropy |
| 0x04 | ClaimWinningsV1 | Claim payout after resolution |
| 0x05 | CancelMarketV1 | Cancel market and refund |

## Integration

Uses existing DarkFi primitives:
- **Money::Burn** for value lock (spend_hook to CreatePositionV1)
- **PoW** via wasm::util for tx_hash access

## Relationship to prediction_market Contract

This is a **specialized proof-of-concept** built alongside the general [prediction_market](../prediction_market/README.md) contract:

| Aspect | prediction_market | block_height_prediction |
|--------|-------------------|------------------------|
| **Use Case** | Generic markets | Block height only |
| **Resolution** | Oracle attestation | PoW-backed |
| **Outcomes** | Multiple discrete | Below/Exact/Above |
| **Complexity** | Full AMM + LP | Simplified |

## Architecture Notes

### Resolution Algorithm

```rust
// After target_time + confirmation_depth blocks:
let tx_hash = wasm::util::get_tx_hash()?;
let entropy = poseidon_hash(tx_hash[0..8], tx_hash[8..16], ...);
let expected = (target_time - created_at) / 120;
let resolved = base_height + (entropy % range) + expected;
```

### Why tx_hash Works (Mostly)

The tx_hash appears in a block that must satisfy the PoW target. While not as strong as block hash directly:
- Miner must find valid PoW to include tx
- Cannot choose arbitrary tx_hash value
- Multiple entropy sources combine well

### For Production

For high-stakes production use, implement `BlockHashGet` opcode:

```zk
# In zkVM circuit:
block_hash = BlockHashGet(block_height);
```

This would enable direct RandomX hash access for cumulative PoW entropy.

## Status

**Proof-of-Concept** - This contract demonstrates the concept but has limitations:

1. Uses tx_hash (not block hash directly)
2. No ZK proofs for position verification
3. Simplified payout model

For production gambling, use [DarkToshi Dice](../darktoshi_dice/README.md) which has more mature implementation.

## See Also

- [Provable Randomness](../../doc/src/arch/provable_randomness.md)
- [DarkToshi Dice Contract](../darktoshi_dice/README.md)
- [Consensus Mechanism](../../src/validator/consensus.rs)
- [PoW Module](../../src/validator/pow.rs)
