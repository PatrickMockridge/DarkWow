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
use darkfi_sdk::crypto::entropy::{tx_hash_to_base, mix_entropy};

// Simple roll from single block
let block_hash = tx_hash_to_base(&tx_hash_bytes);
let roll_entropy = mix_entropy(block_hash, &[bet_id, secret_nonce]);
let roll = (roll_entropy % 100) as u8;

// High-security roll with multiple block confirmations
use darkfi_sdk::crypto::entropy::{combine_block_hashes, draw_with_depth};
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
# Start darkfid with localnet config
./target/release/darkfid -c contrib/localnet/darkfid-single-node/darkfid.toml

# Mine blocks to fund wallet
./target/release/drk -c bin/drk/drk_config.toml -n localnet mine
# Press Ctrl+C when sufficient DARK accumulated
```

### Deployment

```bash
# Generate deploy authority
drk -c bin/drk/drk_config.toml -n localnet contract generate-deploy
# Output: Contract ID: BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1

# Deploy contract (pipe to broadcast)
drk -c bin/drk/drk_config.toml -n localnet contract deploy \
  BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1 \
  target/wasm32-unknown-unknown/release/darkfi_darktoshi_dice_contract.wasm \
  | drk -c bin/drk/drk_config.toml -n localnet broadcast
```

### Verification

```bash
# Check balance
drk -c bin/drk/drk_config.toml -n localnet wallet balance

# List coins
drk -c bin/drk/drk_config.toml -n localnet wallet coins

# Scan blockchain
drk -c bin/drk/drk_config.toml -n localnet scan
# Or full rescan: drk -c bin/drk/drk_config.toml -n localnet scan --reset 0

# List deployed contracts
drk -c bin/drk/drk_config.toml -n localnet contract list
```

### Test Results (2026-04-07)

- **Deployment TX**: `e15a50bae7940593057ca9674f774aaf7f50e107bd4b3483d6e65130e55d8e2f`
- **Contract ID**: `BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1`
- **Initial balance**: 120 DARK (from mining)

## Building

```bash
# Compile ZK circuits
./target/debug/zkas proof/commit_bet_v1.zk -o proof/commit_bet_v1.zk.bin
./target/debug/zkas proof/settle_bet_v1.zk -o proof/settle_bet_v1.zk.bin

# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_darktoshi_dice_contract
```

## Usage Example

```rust
use darkfi_darktoshi_dice_contract::client::CommitBetV1Builder;

// Create a bet: 100 tokens, target 50 (50% chance to win)
let (params, own_bet) = CommitBetV1Builder::new(
    player_pubkey,
    100,  // bet_value
    50,   // target (1-99)
)
.token_id(DRKW_TOKEN_ID)
.build();
```

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

### Client Integration

```rust
// 1. Create Money::Burn to lock bet value
let burn_params = MoneyBurnParamsV1 {
    inputs: vec![Input {
        value_commit,
        token_commit: token_id,
        nullifier,
        merkle_root,
        user_data_enc: encode_bet_metadata(&bet_metadata),
        signature_public: player_pubkey,
    }],
};

// 2. Create Dice::CommitBet as child of Money::Burn
let commit_params = CommitBetParamsV1 {
    player_pub: player_pubkey,
    bet_value: 100,
    target: 50,
    secret_nonce,
    blind,
    token_id,
    value_commit,
    signature,
    house_edge: 200, // 2%
};

// 3. After reveal and settle (if won), create Money::TokenMint
let mint_params = MoneyTokenMintParamsV1 {
    coin: create_winning_coin(player_pubkey, payout, token_id),
};
```

### House Balance Tracking

The contract tracks house funds in `DICE_CONTRACT_HOUSE_TREE`:
- Key: `b"balance"` - accumulated house balance
- Updated on:
  - `SettleBetV1` when house wins: +house_take
  - `HouseCloseV1` when bet cancelled: +house_take

The house must maintain sufficient balance to cover winning payouts.

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
cargo test --release -p darkfid test_darktoshi_dice_heavyweight
```

**Test Coverage**:
| Function | Opcode | Status |
|----------|--------|--------|
| CommitBetV1 | 0x01 | ✅ Tested with ZK proof |
| RevealRollV1 | 0x02 | ✅ Tested (no ZK proof) |
| SettleBetV1 | 0x03 | ⚠️ Requires money_v3::transfer_v1 child call |
| HouseCloseV1 | 0x04 | ⚠️ Requires money_v3::transfer_v1 child call |

**Note**: SettleBetV1 and HouseCloseV1 require money_v3::transfer_v1 child calls for locking/unlocking bet value. These are exercised in isolated heavyweight testing but may fail without full money contract integration.

## See Also

- [Entropy Module](../entropy/) - Provably fair randomness for all betting contracts
- [Money Contract](../money_v2/) - Value transfer integration
- [Atomic Swap](../atomic_swap/) - Commit-reveal pattern reference
- [Tender Contract](../tender/) - Sealed bid pattern reference
- [Betting Capital Staking](../betting_stake/) - Capital provider infrastructure
- [Baccarat Contract](../baccarat/) - Multi-round betting game
- [Roulette Contract](../roulette/) - Fixed-odds betting
