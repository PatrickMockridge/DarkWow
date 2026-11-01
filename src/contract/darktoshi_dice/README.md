# DarkToshi Dice Contract

A privacy-preserving Satoshi Dice clone for DarkWow. This contract implements provably fair gambling with privacy features.

## Overview

DarkToshi Dice allows players to bet on random rolls with the following mechanics:

1. **Commit Phase**: Player commits to a bet (value + target) without revealing
2. **Roll Phase**: Random roll is derived from block hash + commitment
3. **Settlement Phase**: Winners receive payouts; losers forfeit to house

## Capital Requirements

**Critical**: A betting contract can only pay out what it has in capital. This means:

1. **House must maintain reserves**: Sufficient capital to cover maximum potential payouts
2. **Bet sizing limits**: Maximum bet size constrained by available capital
3. **No fractional reserves**: Every winning bet must be fully backed

For example, with a target of 50 (50% win chance) and 10x payout:
- Maximum sustainable bet = house_capital / 10
- Any bet larger risks insolvency if player wins

This creates an opportunity for capital providers to stake against the house and earn yield.

## Key Features

- **Privacy-preserving**: Bet details are committed via Poseidon hash
- **Provably fair**: Roll derived from blockchain randomness
- **House edge**: Configurable (default 2%)
- **Timeout protection**: Stale bets can be closed after timeout

## Betting Odds

| Target | Probability | Payout (excl. house edge) | House Edge |
|--------|-------------|---------------------------|------------|
| 10     | 10%         | 10x                       | 2%         |
| 25     | 25%         | 4x                        | 2%         |
| 50     | 50%         | 2x                        | 2%         |
| 75     | 75%         | 1.33x                     | 2%         |
| 90     | 90%         | 1.11x                     | 2%         |

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize house settings |
| CommitBetV1 | 0x01 | Player commits to a bet |
| RevealRollV1 | 0x02 | Reveal roll using block hash |
| SettleBetV1 | 0x03 | Settle and claim payout |
| HouseCloseV1 | 0x04 | House closes stale bets |

## State Machine

```
COMMITTED ──[RevealRoll]──> REVEALED ──[SettleBet]──> SETTLED
     │                                                   │
     └──[HouseClose after timeout]──> CANCELLED <───────┘
```

## Roll Calculation

Rolls are derived using the [Entropy Module](../entropy/):

```rust
use dwow_sdk::crypto::entropy::{tx_hash_to_base, mix_entropy};

// Simple roll from single block
let block_hash = tx_hash_to_base(&tx_hash_bytes);
let roll_entropy = mix_entropy(block_hash, &[bet_id, secret_nonce]);
let roll = (roll_entropy % 100) as u8;

// High-security roll with multiple block confirmations
use dwow_sdk::crypto::entropy::{combine_block_hashes, draw_with_depth};
let entropy = combine_block_hashes(&block_hashes);
let roll = draw_with_depth(&block_hashes, bet_id, 100);
```

- Player wins if `roll < target`
- House wins if `roll >= target`

See [Entropy Module](../entropy/) for security levels and cumulative PoW entropy.

## Localnet Testing (2026-04-07)

The DarkToshi Dice contract was successfully deployed and tested on localnet.

### Prerequisites

```bash
# Start dwowd with localnet config
./target/release/dwowd -c contrib/localnet/dwowd-single-node/dwowd.toml

# Mine blocks to fund wallet
./target/release/dwow_wallet -c bin/dww/dww_config.toml -n localnet mine
# Press Ctrl+C when sufficient DARK accumulated
```

### Deployment

```bash
# Generate deploy authority
dwow_wallet -c bin/dww/dww_config.toml -n localnet contract generate-deploy
# Output: Contract ID: BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1

# Deploy contract (pipe to broadcast)
dwow_wallet -c bin/dww/dww_config.toml -n localnet contract deploy \
  BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1 \
  target/wasm32-unknown-unknown/release/dwow_darktoshi_dice_contract.wasm \
  | dwow_wallet -c bin/dww/dww_config.toml -n localnet broadcast
```

### Verification

```bash
# Check balance
dwow_wallet -c bin/dww/dww_config.toml -n localnet wallet balance

# List commitments
dwow_wallet -c bin/dww/dww_config.toml -n localnet wallet coins

# Scan blockchain
dwow_wallet -c bin/dww/dww_config.toml -n localnet scan
# Or full rescan: dwow_wallet -c bin/dww/dww_config.toml -n localnet scan --reset 0

# List deployed contracts
dwow_wallet -c bin/dww/dww_config.toml -n localnet contract list
```

### Test Results (2026-04-07)

- **Deployment TX**: `e15a50bae7940593057ca9674f774aaf7f50e107bd4b3483d6e65130e55d8e2f`
- **Contract ID**: `BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1`
- **Initial balance**: 120 DARK (from mining)

## Building

```bash
# Compile ZK circuits
./target/debug/zkas proof/commit_bet.zk -o proof/commit_bet.zk.bin
./target/debug/zkas proof/settle_bet.zk -o proof/settle_bet.zk.bin

# Build WASM
cargo build --target wasm32-unknown-unknown --release -p dwow_darktoshi_dice_contract
```

## Usage Example

```rust
use dwow_darktoshi_dice_contract::client::CommitBetV1Builder;

// Create a bet: 100 tokens, target 50 (50% chance to win)
let (params, own_bet) = CommitBetV1Builder::new(
    player_pubkey,
    100,  // bet_value
    50,   // target (1-99)
)
.asset_id(DRKW_ASSET_ID)
.build();
```

## Integration with PromissoryNote

The contract does not compose with the removed `money` contract and has no spend-hook
path. The manifest depends on `promissory_note` and `native_token_v1`; stakes and payouts
are `promissory_note::transfer_v1` (0x04) children. The older `Money::Burn`/`Money::TokenMint`
sketch described a contract that no longer exists and is removed rather than translated.

See [`doc/src/contract/darktoshi_dice.md`](../../../doc/src/contract/darktoshi_dice.md)
for the current function and circuit surface.

## Security Considerations

- Roll randomness depends on block hash unpredictability
- Secret nonce prevents front-running of commit phase
- Timeout prevents indefinite locking of funds
- House edge ensures house profitability

## Primitives Provided

This contract establishes useful primitives for other games:

- Commit-reveal schemes via Poseidon hash
- Block hash randomness via `get_tx_hash()`
- Conditional value transfer (win/lose outcomes)
- Time-locked state transitions

## Capital Staking

Dice presents a clear yield opportunity for capital providers. See [Betting Capital Staking](../betting_stake/) for infrastructure that allows:

- Staking capital against Dice tables
- Earning a share of the house edge over time
- Bearing risk of large payouts (but compensated for this risk)

## Heavyweight Test

The contract includes a heavyweight test that exercises two endpoints:

```bash
cargo test --release -p dwowd test_heavyweight_darktoshi_dice
```

**Test Coverage**:
| Function | Opcode | Status |
|----------|--------|--------|
| CommitBetV1 | 0x01 | ✅ Tested with ZK proof |
| RevealRollV1 | 0x02 | ✅ Tested (no ZK proof) |
| SettleBetV1 | 0x03 | ⚠️ Requires promissory_note::transfer_v1 child call |
| HouseCloseV1 | 0x04 | ⚠️ Requires promissory_note::transfer_v1 child call |

**Note**: SettleBetV1 and HouseCloseV1 require promissory_note::transfer_v1 child calls for locking/unlocking bet value. These are exercised in isolated heavyweight testing.

## See Also

- [Entropy Module](../entropy/) - Provably fair randomness for all betting contracts
- [Promissory Note](../promissory_note/) - Value transfer integration
- [Tender Contract](../tender/) - Sealed bid pattern reference
- [Betting Capital Staking](../betting_stake/) - Capital provider infrastructure
- [Baccarat Contract](../baccarat/) - Multi-round betting game
- [Roulette Contract](../roulette/) - Fixed-odds betting
