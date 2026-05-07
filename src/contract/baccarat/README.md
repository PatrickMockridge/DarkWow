# Baccarat Contract

A privacy-preserving Baccarat (Punto Banco) contract for DarkWow. Famous from James Bond films, Baccarat is a casino classic that offers three possible outcomes: Player wins, Banker wins, or Tie.

## Overview

Baccarat is particularly well-suited for blockchain gambling because:
- **No player decisions**: Drawing rules are completely fixed (no discretion)
- **Deterministic outcomes**: No disputes possible—rules determine everything
- **Fast gameplay**: Simple 3-outcome betting
- **Perfect for commit-reveal**: Block hash provides card shuffle entropy

## Capital Requirements

**Critical**: A betting contract can only pay out what it has in capital. This means:

1. **House must maintain reserves**: Sufficient capital to cover maximum potential payouts
2. **Bet sizing limits**: Maximum bet size constrained by available capital
3. **No fractional reserves**: Every winning bet must be fully backed

For example, if a Baccarat table has $1M in capital and a player bets $2M on Player:
- The bet exceeds available capital
- The house **cannot** accept this bet (would be insolvency risk)

This creates an opportunity for capital providers to stake against the house and earn yield.

## Key Features

- **Privacy-preserving**: Bet details committed via Poseidon hash
- **Provably fair**: Cards derived from cumulative PoW block hashes
- **Configurable confirmation depth**: Player can wait for N blocks for extra security
- **Fixed house edge**: ~1.5% average (built into Banker payout)
- **Timeout protection**: Stale bets can be closed by house

## Betting Odds

| Bet Type | Payout | House Edge | Probability |
|----------|--------|------------|-------------|
| Player | 1:1 | ~1.24% | 44.62% |
| Banker | 0.95:1 | ~1.06% | 45.85% |
| Tie | 8:1 | ~14.36% | 9.52% |

## Baccarat Rules

### Hand Values

- Cards 2-9 = face value
- 10, J, Q, K = 0
- A = 1
- Hand value = sum of cards % 10 (displayed as 0-9)

### Drawing Rules (Fixed—No Discretion)

```
Initial Deal: 2 cards each, evaluated immediately

Natural 8 or 9:
  - Higher natural wins
  - Equal naturals = Tie

Player Drawing:
  - 0-5: Draw third card
  - 6-7: Stand

Banker Drawing (after Player):
  - 0-2: Always draw
  - 3: Draw if Player drew (any third card)
  - 4: Draw if Player's third card is 2-7
  - 5: Draw if Player's third card is 4-7
  - 6-7: Stand
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize house settings |
| CommitBetV1 | 0x01 | Player commits to a bet (Player/Banker/Tie) |
| DrawCardsV1 | 0x02 | Reveal cards using PoW block hash entropy |
| SettleBetV1 | 0x03 | Settle and claim payout |
| HouseCloseV1 | 0x04 | House closes stale bets after timeout |

## State Machine

```
COMMITTED ──[DrawCards]──> CARDS_DRAWN ──[SettleBet]──> SETTLED
     │                                                    │
     └──[HouseClose after timeout]──> CANCELLED <─────────┘
```

## Card Dealing via Block Hash

Cards are derived using cumulative PoW entropy from K consecutive block hashes (where K = confirmation_depth) via the [Entropy Module](../entropy/):

```rust
use darkfi_sdk::crypto::entropy::{tx_hash_to_base, mix_entropy};

// Combine entropy from multiple block hashes
let mut entropy = bet_id;
for (i, hash) in block_hashes.iter().enumerate() {
    let block_entropy = tx_hash_to_base(&hash.0);
    entropy = mix_entropy(entropy, &[block_entropy, pallas::Base::from(i as u64)]);
}

// Extract cards from entropy bytes
player_card1 = Card(entropy[0:8] % 52)
player_card2 = Card(entropy[8:16] % 52)
banker_card1 = Card(entropy[16:24] % 52)
banker_card2 = Card(entropy[24:32] % 52)
```

See [Entropy Module](../entropy/) for cumulative PoW entropy combining (`combine_block_hashes`, `draw_with_depth`).

The player specifies `confirmation_depth` when placing the bet—more blocks means more entropy security but longer wait time.

## Building

```bash
# Compile ZK circuits
./target/debug/zkas proof/commit_bet_v1.zk -o proof/commit_bet_v1.zk.bin
./target/debug/zkas proof/settle_bet_v1.zk -o proof/settle_bet_v1.zk.bin

# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_baccarat_contract
```

## Usage Example

```rust
use darkfi_baccarat_contract::client::CommitBetV1Builder;
use darkfi_baccarat_contract::model::BetType;

// Create a bet: 100 tokens, bet on Player
let (params, own_bet) = CommitBetV1Builder::new(
    player_pubkey,
    100,  // bet_value
    BetType::Player,
)
.token_id(DRKW_TOKEN_ID)
.confirmation_depth(3)  // Wait for 3 blocks
.build();
```

## Integration with Money Contract

The Baccarat contract integrates with the Money contract for value transfers:

### Transaction Structure

A complete Baccarat bet transaction should include:

1. **Money::Burn** (parent call)
   - Burns the player's bet value
   - Sets `spend_hook` to authorize Baccarat::CommitBet
   - Use `user_data_enc` to pass bet metadata

2. **Baccarat::CommitBetV1** (child call, `parent_index=0`)
   - Receives burn authorization via spend_hook
   - Stores bet in Committed state
   - Validates burn value matches bet_value

3. **Baccarat::DrawCardsV1**
   - Calculates cards from block hash entropy
   - Evaluates drawing rules
   - Transitions bet to CardsDrawn state

4. **Baccarat::SettleBetV1** + **Money::TokenMint** (if player won)
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

// 2. Create Baccarat::CommitBet as child of Money::Burn
let commit_params = CommitBetParamsV1 {
    player_pub: player_pubkey,
    bet_type: BetType::Player as u8,
    bet_value: 100,
    secret_nonce,
    blind,
    token_id,
    value_commit,
    house_edge: 150,  // 1.5%
    confirmation_depth: 3,
};

// 3. After draw and settle (if won), create Money::TokenMint
let mint_params = MoneyTokenMintParamsV1 {
    coin: create_winning_coin(player_pubkey, payout, token_id),
};
```

## Confirmation Depth and Security

The `confirmation_depth` parameter allows players to customize the security vs. speed tradeoff:

- **depth=1**: Fastest, uses only current block hash
- **depth=K**: More entropy, requires waiting K blocks

This allows "time + PoW" security—the player and house can agree on an acceptable delay before cards are revealed.

## Comparison with DarkToshi Dice

| Aspect | DarkToshi Dice | Baccarat |
|--------|----------------|----------|
| Outcomes | 2 (win/lose) | 3 (Player/Banker/Tie) |
| Player decisions | Target selection | None |
| Randomness | Single block hash | Cumulative K blocks |
| Complexity | Simple | Moderate |
| Popularity | Moderate | Very high (casino staple) |

## Primitives Provided

This contract establishes useful primitives for other games:

- Commit-reveal schemes via Poseidon hash
- Cumulative block hash entropy via `get_block_hash()`
- Fixed-odds betting with deterministic outcomes
- Time-locked state transitions

## Capital Staking

Baccarat presents a clear yield opportunity for capital providers. See [Betting Capital Staking](../betting_stake/) for infrastructure that allows:

- Staking capital against Baccarat (or Dice) tables
- Earning a share of the house edge over time
- Bearing risk of large payouts (but compensated for this risk)

## See Also

- [Entropy Module](../entropy/) - Provably fair randomness for all betting contracts
- [Money Contract](../money_v2/) - Value transfer integration
- [DarkToshi Dice Contract](../darktoshi_dice/) - Commit-reveal pattern reference
- [Betting Capital Staking](../betting_stake/) - Capital provider infrastructure
- [Roulette Contract](../roulette/) - Fixed-odds betting
