# Game Room Contract Specification

A generalized betting and pot management contract for privacy-preserving staked games.

## Context

Private chat groups on DarkIRC can organize staked games with partial and revealed information, turn-based betting, and trusted entropy setups. The Game Room contract provides on-chain stake and pot management while game logic resides at the app layer.

## Two-Layer Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     App Layer (Outside Repo)                │
│                                                              │
│  Room Owner (Escrow-DAO):                                    │
│  - Game rules (poker hands, backgammon moves)               │
│  - Turn sequencing                                           │
│  - Win condition determination                               │
│  - Dispute resolution                                       │
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
│  - Pot management (bets, raises, calls)                      │
│  - Entropy source configuration (PoW or trusted setup)       │
│  - On-chain proof generation for bet verification           │
│  - Dispute escalation path (room owner DAO)                  │
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

## State Machines

### Room State
```
Open → Active → Concluded
```
- **Open**: Players can deposit, room accepts initial bets
- **Active**: Game in progress, betting rounds occur
- **Concluded**: Room closed, no further actions

### Pot State
```
Open → Closed → Settled
```
- **Open**: Can accept bets
- **Closed**: No more bets accepted
- **Settled**: Paid out to winners

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
    pub state: PotState,
    pub betting_round: u8,
}

pub enum PotState {
    Open,      // Can still add to pot
    Closed,    // No more bets accepted
    Settled,   // Paid out
}
```

### Bet
```rust
pub struct Bet {
    pub bet_id: BetId,
    pub room_id: RoomId,
    pub pot_id: PotId,
    pub player: PublicKey,
    pub amount: u64,
    pub bet_type: BetType,
    pub round: u8,
    pub commitment: pallas::Base,
    pub block: u64,
}

pub enum BetType {
    Ante,    // Initial bet to join hand
    Blind,   // Forced blind bet
    Bet,     // Initial bet in a round
    Raise,   // Raise existing bet
    Call,    // Match existing bet
    AllIn,   // All remaining balance
    Fold,    // Forfeit hand (no pot claim)
}
```

### RoomConfig
```rust
pub struct RoomConfig {
    pub owner_dao: ContractId,   // Escrow-DAO contract ID
    pub token_id: pallas::Base,
    pub min_stake: u64,
    pub max_stake: u64,
    pub entropy_mode: EntropyMode,
    pub confirmation_depth: u8,
    pub required_entropy_contributions: u8,
    pub entropy_contribution_deadline: u64,
    pub max_players: u8,
}

pub enum EntropyMode {
    BlockHash,      // Pure PoW block hash
    TrustedSetup,   // Multi-party entropy
}
```

## Entropy Modes

### BlockHash Mode (Default)
- Uses `combine_block_hashes()` from entropy module
- Configurable confirmation depth (1, 6, 10 blocks)
- Fast, no coordination needed

### TrustedSetup Mode
- Room creator defines required contributors
- Players commit `H(secret_nonce)` during contribution period
- Then reveal actual nonce
- Combined entropy = `poseidon_hash([n1, n2, ...])`
- Requires at least 1 honest participant

Security: 1 block = 33%, 6 blocks = 0.14%, 10 blocks = 0.005%

## Stake Lifecycle

```
User Deposits (bring cash in):
  DepositV1 → balance += amount (via Money::Burn)

User Places Bet:
  PlaceBetV1 → balance -= amount, locked += amount, pot.total += amount

User Raises:
  RaiseV1 → same transfer into pot

User Calls:
  CallV1 → same transfer into pot

User Folds:
  FoldV1 → locked stays (goes to eventual winner)

User Withdraws (cash out):
  WithdrawV1 → balance -= amount, mint (via Money::TokenMint)
  (Only available for balance > locked)

Owner DAO Settles Pot:
  SettlePotV1 → pot.closed, winners determined

User Claims Winnings:
  ClaimV1 → winner.balance += amount
```

## Escrow-DAO Integration

The room owner is an escrow-DAO contract that:
1. Organizes game rules (poker rounds, backgammon turns, etc.)
2. Determines win conditions
3. Resolves disputes
4. Calls `SettlePotV1` to distribute pot

## Database Trees

```rust
pub const GAME_ROOM_ROOMS_TREE: &str = "game_room_rooms";
pub const GAME_ROOM_ACCOUNTS_TREE: &str = "game_room_accounts";   // (room_id, pubkey) → PlayerAccount
pub const GAME_ROOM_POTS_TREE: &str = "game_room_pots";          // pot_id → Pot
pub const GAME_ROOM_BETS_TREE: &str = "game_room_bets";          // bet_id → Bet
pub const GAME_ROOM_NULLIFIERS_TREE: &str = "game_room_nullifiers";
pub const GAME_ROOM_ENTROPY_TREE: &str = "game_room_entropy";     // trusted setup
```

## Security Considerations

1. **Player identification**: All functions take `player: PublicKey` in params (verified by ZK proof/signature at app layer)
2. **Owner verification**: Uses `ContractId::derive_public(caller) == owner_dao` pattern
3. **Double-claim prevention**: Nullifiers track claimed pots per player
4. **Balance validation**: Available balance = balance - locked

## See Also

- [Game Room SDK](../../src/contract/game_room/README.md)
- [Provable Randomness](./provable_randomness.md)
- [DarkIRC Specification](../misc/darkirc/specification.md)
