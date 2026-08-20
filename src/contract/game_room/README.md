# Game Room Contract

A generalized betting and pot management contract for privacy-preserving staked games.

## Overview

The Game Room contract provides on-chain infrastructure for games with betting, without implementing game-specific logic. Game rules, win conditions, and dispute resolution are handled at the **app layer** by the room owner (escrow-DAO).

## Key Features

- **Stake Management**: Players bring cash in/out of rooms
- **Pot Management**: Bets, raises, calls, and collective pot tracking
- **On-chain Bet Proofs**: Verify bets externally via DarkIRC
- **Trusted Entropy Setup**: Optional multi-party entropy for fair randomness
- **DAO Governance**: Room owner (escrow-DAO) organizes game rules and disputes

## Two-Layer Design

```
┌─────────────────────────────────────────────────────────────┐
│                     App Layer (Outside Repo)                  │
│                                                              │
│  Room Owner (Escrow-DAO):                                   │
│  - Game rules (poker hands, backgammon moves)               │
│  - Turn sequencing                                           │
│  - Win condition determination                                │
│  - Dispute resolution                                        │
│                                                              │
│  DarkIRC integration (channel secret = room access)          │
└─────────────────────────────────────────────────────────────┘
                              │
                              │ SDK calls
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   Contract Layer (This Contract)             │
│                                                              │
│  - Stake management (bring cash in/out)                     │
│  - Pot management (bets, raises, calls)                     │
│  - Entropy source configuration (PoW or trusted setup)      │
│  - On-chain proof generation for bet verification            │
│  - Dispute escalation path (room owner DAO)                 │
└─────────────────────────────────────────────────────────────┘
```

## Contract Functions

| Function | ID | Purpose |
|----------|-----|---------|
| CreateRoomV1 | 0x00 | Create room (owner DAO) |
| DepositV1 | 0x01 | Bring cash into room (stake) |
| WithdrawV1 | 0x02 | Take cash out (if not locked) |
| PlaceBetV1 | 0x03 | Place ante/blind/bet |
| RaiseV1 | 0x04 | Raise current bet |
| CallV1 | 0x05 | Call current bet |
| FoldV1 | 0x06 | Fold (forfeit this hand) |
| ClosePotV1 | 0x07 | Close pot to new bets (round end) |
| SettlePotV1 | 0x08 | Settle pot to winners (owner DAO) |
| ContributeEntropyV1 | 0x09 | TrustedSetup entropy commit/reveal |
| ClaimV1 | 0x0A | Claim winnings from pot |

## Data Structures

### PlayerAccount
```rust
pub struct PlayerAccount {
    pub pubkey: PublicKey,
    pub balance: u64,      // Cash available
    pub locked: u64,       // Currently in pot (not withdrawable)
    pub last_action_block: u64,
    pub has_folded: bool,
    pub entropy_contribution: Option<EntropyContribution>,
}
```

### Pot
```rust
pub struct Pot {
    pub pot_id: PotId,
    pub room_id: RoomId,
    pub total: u64,
    pub contributions: Vec<PotContribution>,
    pub state: PotState,   // Open, Closed, Settled
    pub betting_round: u8,
}
```

### RoomConfig
```rust
pub struct RoomConfig {
    pub owner_dao: ContractId,
    pub asset_id: pallas::Base,
    pub min_stake: u64,
    pub max_stake: u64,
    pub entropy_mode: EntropyMode,   // BlockHash or TrustedSetup
    pub confirmation_depth: u8,
    pub required_entropy_contributions: u8,
    pub entropy_contribution_deadline: u64,
    pub max_players: u8,
}
```

## Entropy Modes

### BlockHash (Default)
Uses PoW block hash entropy - fast, no coordination needed.

### TrustedSetup
Multi-party commit-reveal entropy:
1. Room creator sets required contributors and deadline
2. Players commit `H(secret_nonce)` during contribution period
3. Players reveal actual nonce
4. Combined entropy = `poseidon_hash([n1, n2, ...])`

Requires at least 1 honest participant.

## Usage Flow

1. **Room owner** creates room via `CreateRoomV1`
2. **Players** deposit stake via `DepositV1`
3. **Players** place bets via `PlaceBetV1`, `RaiseV1`, `CallV1`
4. **Players** fold via `FoldV1`
5. **Owner DAO** closes pot via `ClosePotV1`
6. **Owner DAO** settles pot via `SettlePotV1`
7. **Players** claim winnings via `ClaimV1`

## Escrow-DAO Integration

The room owner is an escrow-DAO contract that:
- Organizes game rules (poker rounds, backgammon turns, etc.)
- Determines win conditions
- Resolves disputes
- Calls `SettlePotV1` to distribute pot

```rust
pub struct SettleParamsV1 {
    pub caller: PublicKey,           // DAO operator
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub winners: Vec<(PublicKey, u64)>,  // winner -> amount
    pub signature: Vec<u8>,           // DAO multi-sig
}
```

## Database Trees

| Tree | Key | Value |
|------|-----|-------|
| `game_room_rooms` | RoomId | GameRoom |
| `game_room_accounts` | (RoomId, Pubkey) | PlayerAccount |
| `game_room_pots` | PotId | Pot |
| `game_room_bets` | BetId | Bet |
| `game_room_nullifiers` | (PotId, Pubkey) | [] (prevent double-claim) |
| `game_room_entropy` | (RoomId, Pubkey) | EntropyContribution |

## SDK

App developers use the Game Room SDK to integrate:

```rust
use darkfi_sdk::game_room::{GameRoomClient, RoomConfig, BetType, EntropyMode};
use darkfi_sdk::crypto::{ContractId, Keypair};

// Create a client
let keypair = Keypair::random();
let client = GameRoomClient::new("http://localhost:8080", contract_id, keypair);

// Deposit stake
let deposit_tx = client.deposit(room_id, 500);

// Place a bet
let nonce = client.generate_nonce();
let bet_tx = client.place_bet(room_id, 100, BetType::Ante, nonce);

// Broadcast via DarkIRC, receive confirmations off-chain
```

The SDK provides:
- **Transaction builders**: `deposit()`, `withdraw()`, `place_bet()`, `raise()`, `call()`, `fold()`, `close_pot()`, `settle_pot()`, `contribute_entropy()`, `claim()`
- **Raw builders**: `build_deposit_tx()`, `build_place_bet_tx()`, etc. for custom flows
- **Helpers**: `generate_nonce()`, `derive_room_id()`

See [`src/sdk/src/game_room/`](src/sdk/src/game_room/) for full SDK API.

## See Also

- [Game Room Contract Specification](../../doc/src/arch/game_room.md)
- [Provable Randomness](../../doc/src/arch/provable_randomness.md) - Entropy analysis
- [DarkIRC](../../doc/src/misc/darkirc/specification.md) - Private channel messaging
