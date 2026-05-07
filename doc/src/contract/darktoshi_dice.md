# DarkToshi Dice Contract

A privacy-preserving Satoshi Dice clone implementing provably fair gambling with commit-reveal pattern.

## Overview

DarkToshi Dice allows players to bet on random rolls with the following mechanics:

1. **Commit Phase**: Player commits to a bet (value + target + secret nonce) without revealing
2. **Roll Phase**: Random roll is derived from block hash + commitment
3. **Settlement Phase**: Winners receive payouts; losers forfeit to house

## State Machine

```
COMMITTED ──[RevealRoll]──> REVEALED ──[SettleBet]──> SETTLED
     │                                                   │
     └──[HouseClose after timeout]──> CANCELLED <───────┘
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize house settings |
| CommitBetV1 | 0x01 | Player commits to a bet |
| RevealRollV1 | 0x02 | Reveal roll using block hash |
| SettleBetV1 | 0x03 | Settle and claim payout |
| HouseCloseV1 | 0x04 | House closes stale bets |

## Betting Odds

| Target | Probability | Payout (excl. house edge) | House Edge |
|--------|-------------|---------------------------|------------|
| 10     | 10%         | 10x                       | 2%         |
| 25     | 25%         | 4x                        | 2%         |
| 50     | 50%         | 2x                        | 2%         |
| 75     | 75%         | 1.33x                     | 2%         |
| 90     | 90%         | 1.11x                     | 2%         |

## Roll Calculation

```
roll = hash(block_hash, bet_id, secret_nonce) % 100
```

- Player wins if `roll < target`
- House wins if `roll >= target`

## Payout Formula

```
payout = bet_value * (10000 - house_edge) / (target * 100)
```

Where `house_edge` is in basis points (e.g., 200 = 2%).

## Integration with Money Contract

The Dice contract integrates with the Money contract for value transfers:

### Transaction Structure

A complete Dice bet transaction should include:

1. **Money::Burn** (parent call)
   - Burns the player's bet value
   - Sets `spend_hook` to authorize Dice::CommitBet
   - Use `user_data_enc` to pass bet metadata

2. **Dice::CommitBetV1** (child call, `parent_index=0`)
   - Receives burn authorization via spend_hook
   - Stores bet in committed state
   - Validates burn value matches bet_value

3. **Dice::RevealRollV1**
   - Calculates roll from block hash + commitment
   - Transitions bet to REVEALED state

4. **Dice::SettleBetV1** + **Money::TokenMint** (if player won)
   - Settles bet and determines payout
   - Player-winning bets: client creates Money::TokenMint call to mint payout
   - House-winning bets: house balance credited automatically

### House Balance Tracking

The contract tracks house funds in `DICE_CONTRACT_HOUSE_TREE`:
- Key: `b"balance"` - accumulated house balance
- Updated on:
  - `SettleBetV1` when house wins: +house_take
  - `HouseCloseV1` when bet cancelled: +house_take

The house must maintain sufficient balance to cover winning payouts.

## Key Implementation Details

### Randomness Source

Uses full 32-byte tx hash for randomness via Poseidon hashing:
```rust
let a = u64::from_le_bytes(tx_hash[0..8]);
let b = u64::from_le_bytes(tx_hash[8..16]);
let c = u64::from_le_bytes(tx_hash[16..24]);
let d = u64::from_le_bytes(tx_hash[24..32]);
let block_hash = poseidon_hash([...]);
let roll = poseidon_hash([block_hash, bet_id, secret_nonce]);
```

**Security Note**: The current implementation uses `wasm::util::get_tx_hash()` which
provides tx-level randomness. For higher-stakes applications, see [Provable Randomness](provable_randomness.md)
for analysis of leveraging DarkWow's PoW mechanism (RandomX) directly.

### Adjustable Confirmation Depth

Players can specify a `confirmation_depth` when placing bets, determining how many blocks
to wait before settlement is allowed. This accumulates PoW entropy for stronger randomness.

```
Security scaling with depth:
- K=1: 33% manipulation chance (with 33% hash power)
- K=6: ~0.14% (Bitcoin "6 confirmations" standard)
- K=10: ~0.005%
```

The `settle_block` is calculated as:
```
settle_block = current_block + confirmation_depth
```

Higher confirmation depths exponentially increase the cost of manipulation but require
waiting for more blocks before the bet can be settled.

### Constants

- `DEFAULT_HOUSE_EDGE`: 200 basis points (2%)
- `MIN_HOUSE_EDGE`: 100 basis points (1%)
- `MAX_HOUSE_EDGE`: 500 basis points (5%)
- `DEFAULT_ROLL_TIMEOUT`: 10 blocks
- `MAX_TARGET`: 99

## Randomness Analysis

The dice contract demonstrates several randomness patterns:

| Pattern | Current Use | Security Level |
|---------|-------------|----------------|
| Commit-Reveal | Secret nonce committed at bet time | High |
| Block Hash | tx_hash at block inclusion | Medium |
| Poseidon Hash | Combine multiple sources | High |

**Weakness**: Current implementation relies on tx_hash which could be influenced by transaction ordering. For production gambling use, leverage PoW block hash directly.

See [Provable Randomness](provable_randomness.md) for full analysis including:
- Leveraging DarkWow's RandomX PoW for randomness
- ECVRF-based verifiable randomness
- Hybrid approaches (PoW + VRF + Commit-Reveal)
- **Case study: Block Height Prediction Market** with detailed implementation design

## Primitives Provided

This contract establishes useful primitives for other games:

- Commit-reveal schemes via Poseidon hash
- Block hash randomness via `get_tx_hash()`
- Conditional value transfer (win/lose outcomes)
- Time-locked state transitions
- House edge economics

## See Also

- [Provable Randomness](provable_randomness.md) - Deep dive into randomness sources and security
- [Money Contract](money.md) - Value transfer integration
- [Atomic Swap](../contract/atomic_swap.md) - Commit-reveal pattern reference
- [Tender Contract](tender.md) - Sealed bid pattern reference

## Localnet Testing (2026-04-07)

The DarkToshi Dice contract was successfully deployed and tested on localnet.

### Test Configuration

- **Network**: localnet with `pow_fixed_difficulty=1`
- **Mining**: `drk mine` against dwowd stratum server (port 48347)
- **Block reward**: 20 DRKW per block
- **Wallet**: Initialized and funded via mining

### Deployment Details

```bash
# Deploy contract
drk contract deploy BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1 \
  target/wasm32-unknown-unknown/release/darkfi_darktoshi_dice_contract.wasm \
  | dww broadcast

# Transaction ID: e15a50bae7940593057ca9674f774aaf7f50e107bd4b3483d6e65130e55d8e2f
# Contract ID: BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1
```

### Verified Workflow

1. `drk wallet balance` - Check DRKW tokens
2. `drk wallet coins` - View unspent coins
3. `drk contract list` - List deploy authorities
4. `drk scan` - Discover blockchain updates
5. `drk contract deploy | dww broadcast` - Deploy contract

### CLI Notes

- Config file required: `-c bin/drk/drk_config.toml`
- Network flag: `-n localnet`
- `scan` is a top-level subcommand (not `wallet scan`)
- Values displayed in raw units (8 decimal places)
