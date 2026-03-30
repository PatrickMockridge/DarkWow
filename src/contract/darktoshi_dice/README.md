# DarkToshi Dice Contract

A privacy-preserving Satoshi Dice clone for DarkFi. This contract implements provably fair gambling with privacy features.

## Overview

DarkToshi Dice allows players to bet on random rolls with the following mechanics:

1. **Commit Phase**: Player commits to a bet (value + target) without revealing
2. **Roll Phase**: Random roll is derived from block hash + commitment
3. **Settlement Phase**: Winners receive payouts; losers forfeit to house

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

```
roll = hash(block_hash, bet_id, secret_nonce) % 100
```

- Player wins if `roll < target`
- House wins if `roll >= target`

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
.token_id(DARK_TOKEN_ID)
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

## See Also

- [Money Contract](../money_v2/) - Value transfer integration
- [Atomic Swap](../atomic_swap/) - Commit-reveal pattern reference
- [Tender Contract](../tender/) - Sealed bid pattern reference
