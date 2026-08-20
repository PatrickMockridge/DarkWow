# Game Room App Layer Implementation Guide

Guide for implementing app layer integrations with the Game Room contract.

## Overview

App developers build game logic on top of the Game Room contract. The contract handles stake/pot management; the app layer handles game rules, turn sequencing, and win determination.

## Integration Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     App Layer (Your Implementation)          │
│                                                              │
│  1. DarkIRC private channel (channel secret = room access)  │
│  2. Game logic (poker hands, backgammon moves, etc.)         │
│  3. Turn sequencing and action validation                     │
│  4. Win condition determination                               │
│  5. Dispute resolution (via escrow-DAO)                    │
│                                                              │
│  Uses GameRoomClient SDK to call contract                    │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ SDK calls
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   Game Room Contract (On-chain)                │
│                                                              │
│  - Stake management (Deposit/Withdraw)                       │
│  - Pot operations (PlaceBet/Raise/Call/Fold)               │
│  - Entropy source (if using TrustedSetup)                  │
│  - Settlement (via owner DAO)                               │
└─────────────────────────────────────────────────────────────┘
```

## SDK Usage

### Initialize Client

```rust
use dwow_sdk::game_room::{GameRoomClient, RoomConfig, EntropyMode};
use dwow_sdk::crypto::{ContractId, Keypair};

// Connect to existing room
let keypair = Keypair::random();
let client = GameRoomClient::new(
    "http://localhost:8080",  // RPC URL
    contract_id,              // Game Room contract ID
    keypair,                  // User's keypair
);

// Room configuration (builder pattern)
let config = RoomConfig::new(
    owner_dao,
    asset_id,
    100,                      // min_stake
    10000,                    // max_stake
    EntropyMode::TrustedSetup,
)
.with_confirmation_depth(6)
.with_entropy_contributions(3, current_block + 100)
.with_max_players(6);
```

### Create Room

```rust
// Room owner creates room
let tx = client.create_room(config)?;
send_to_channel(channel_id, tx)?;
```

### Join Room (Deposit Stake)

```rust
// Player deposits stake
let tx = client.deposit(room_id, amount)?;
send_to_channel(channel_id, tx)?;

// Wait for confirmation, check balance
let account = client.get_account(room_id, user_pubkey).await?;
println!("Balance: {}, Locked: {}", account.balance, account.locked);
```

### Place Bet

```rust
// Player places bet
let bet_id = client.place_bet(
    room_id,
    amount,
    BetType::Ante,  // or Bet, Blind, etc.
    nonce,          // for commitment
)?;
send_to_channel(channel_id, bet_id)?;
```

### Respond to Bet (Raise/Call)

```rust
// Opponent raises
let raise_id = client.raise(room_id, amount, nonce)?;
send_to_channel(channel_id, raise_id)?;

// Opponent calls
let call_id = client.call(room_id, nonce)?;
send_to_channel(channel_id, call_id)?;
```

### Fold

```rust
// Player folds
let tx = client.fold(room_id)?;
send_to_channel(channel_id, tx)?;
```

### Entropy Contribution (TrustedSetup Mode)

```rust
// Commit phase
let commitment = client.entropy_commit(room_id, secret_nonce)?;
send_to_channel(channel_id, commitment)?;

// Reveal phase (after deadline)
let tx = client.entropy_reveal(room_id, secret_nonce)?;
send_to_channel(channel_id, tx)?;
```

### Close Pot (Room Owner)

```rust
// After betting round ends
let tx = client.close_pot(room_id, pot_id)?;
send_to_channel(channel_id, tx)?;
```

### Settle Pot (Room Owner/DAO)

```rust
// Determine winners off-chain (poker hand evaluation, etc.)
let winners = vec![
    (winner1_pubkey, payout1),
    (winner2_pubkey, payout2),
];

// Owner settles pot
let tx = client.settle_pot(room_id, pot_id, winners, dao_signature)?;
send_to_channel(channel_id, tx)?;
```

### Claim Winnings

```rust
// Winner claims their share
let tx = client.claim(room_id, pot_id, winner_pubkey)?;
send_to_channel(channel_id, tx)?;

// Check new balance
let account = client.get_account(room_id, winner_pubkey).await?;
println!("New balance: {}", account.balance);
```

## DarkIRC Integration

### Channel Setup

```rust
// Create private channel with secret
let channel_secret = generate_random_secret();
let channel_id = create_private_channel(channel_secret)?;

// Share channel secret with players (off-chain)
send_invite(player_list, channel_secret)?;
```

### Message Types

```rust
enum GameMessage {
    // Contract calls (broadcast for confirmation)
    Deposit(DepositParams),
    PlaceBet(PlaceBetParams),
    Raise(RaiseParams),
    Call(CallParams),
    Fold(FoldParams),

    // Game state (for coordination)
    GameState {
        round: u8,
        current_turn: Pubkey,
        action_history: Vec<Action>,
    },

    // Reveal phase for entropy
    EntropyCommitment {
        commitment: pallas::Base,
    },
    EntropyReveal {
        nonce: pallas::Base,
    },

    // Dispute
    Dispute {
        player: Pubkey,
        evidence: Vec<u8>,
    },
}
```

### Turn Sequencing

```rust
struct GameState {
    room_id: RoomId,
    current_player: usize,
    players: Vec<Pubkey>,
    phase: GamePhase,
    current_bet: u64,
    pot_id: PotId,
}

enum GamePhase {
    Waiting,      // Waiting for players
    BettingRound, // Active betting
    Showdown,     // Reveal hands
    Settled,      // Pot distributed
}

fn next_turn(&mut self) {
    self.current_player = (self.current_player + 1) % self.players.len();
}

fn validate_action(&self, action: &Action, player: Pubkey) -> bool {
    if self.players[self.current_player] != player {
        return false; // Not your turn
    }
    if self.phase != GamePhase::BettingRound {
        return false;
    }
    // Validate action is legal given game rules
    true
}
```

## Escrow-DAO for Dispute Resolution

### DAO Actions

```rust
enum DAOAction {
    // Resolve dispute
    ResolveDispute {
        room_id: RoomId,
        pot_id: PotId,
        ruling: DisputeRuling,  // Refund, Slash, PayWinner
        evidence: Vec<Evidence>,
    },

    // Cancel room
    CancelRoom {
        room_id: RoomId,
        refund_amounts: Vec<(Pubkey, u64)>,
    },
}

enum DisputeRuling {
    RefundAll,      // All players get locked funds back
    PayWinner,      // Pot to specified winner
    SlashMalicious,  // Slash malicious player's stake
}
```

## Complete Example: Poker Room

```rust
struct PokerRoom {
    client: GameRoomClient,
    channel_id: ChannelId,
    game_state: GameState,
}

impl PokerRoom {
    pub async fn new(room_id: RoomId, player: Keypair) -> Result<Self> {
        let client = GameRoomClient::new(rpc.clone(), room_id, player, contract_id);

        Ok(Self {
            client,
            channel_id,
            game_state: GameState::default(),
        })
    }

    pub async fn handle_message(&mut self, msg: GameMessage) -> Result<()> {
        match msg {
            GameMessage::Deposit(params) => {
                self.client.deposit(params.room_id, params.amount).await?;
            }
            GameMessage::PlaceBet(params) => {
                // Validate bet is legal
                self.validate_bet(&params)?;
                self.client.place_bet(
                    params.room_id,
                    params.amount,
                    params.bet_type,
                    params.nonce,
                ).await?;
                self.game_state.current_bet = params.amount;
            }
            GameMessage::Raise(params) => {
                self.validate_raise(&params)?;
                self.client.raise(params.room_id, params.amount, params.nonce).await?;
                self.game_state.current_bet += params.amount;
            }
            GameMessage::Call(params) => {
                self.client.call(params.room_id, params.nonce).await?;
            }
            GameMessage::Fold(params) => {
                self.client.fold(params.room_id).await?;
                self.game_state.remove_player(params.player);
            }
            // ... handle other messages
        }
        Ok(())
    }

    fn validate_bet(&self, params: &PlaceBetParams) -> Result<()> {
        // Poker-specific validation
        if self.game_state.phase != GamePhase::BettingRound {
            return Err(GameError::NotBettingRound);
        }
        if params.amount < self.game_state.big_blind {
            return Err(GameError::BetTooSmall);
        }
        Ok(())
    }
}
```

## Security Considerations

1. **Player verification**: The contract expects `player: PublicKey` in params - your app layer must verify signatures
2. **Turn order**: Enforce turn order off-chain, contract only validates balances
3. **Entropy**: For TrustedSetup, ensure all commitments are broadcast before reveal phase
4. **Disputes**: Escalate to escrow-DAO with evidence (DarkIRC messages as proof)
5. **Channel secret**: Treat as shared secret - anyone with it can join the room

## Testing

The game room is tested at Level 1 (unit tests for bet lifecycle) and Level 2
(full ZK contract execution). See [Testing Overview](../dev/testing/overview.md)
for the four-level taxonomy and command reference.

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [Game Room Contract Specification](./game_room.md)
- [Game Room SDK Reference](../../../src/contract/game_room/README.md)
- [DarkIRC Specification](../misc/darkirc/specification.md)
- [Entropy Module](./provable_randomness.md)
