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
| WithdrawV1 | 0x02 | Withdraw tokens (one PN transfer child) |
| PlaceBetV1 | 0x03 | Place ante/blind/bet |
| RaiseV1 | 0x04 | Raise current bet |
| CallV1 | 0x05 | Call current bet |
| FoldV1 | 0x06 | Fold (forfeit this hand) |
| ClosePotV1 | 0x07 | Close pot to new bets (round end) |
| SettlePotV1 | 0x08 | Settle pot to winners (owner DAO) |
| ContributeEntropyV1 | 0x09 | TrustedSetup entropy commit/reveal |
| ClaimV1 | 0x0A | Claim winnings from pot |
| CreatePotV1 | 0x0b | Create a pot in an open/active room |

## State Machines

### Room State
```
Open → Active        (Concluded exists in the enum; nothing assigns it)
```
- **Open**: Players can deposit, room accepts initial bets
- **Active**: Game in progress, betting rounds occur
- **Concluded**: declared (`= 2`) and **unreachable** — `ClosePot` closes the *pot*
  (`pot.state = PotState::Closed`) and clears the room's current-pot fields, but never writes
  `room.state`. The only reads of `Concluded` are a `withdraw.rs` guard and an error name, so a
  room stays `Active` after its pot closes.

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
    pub version: u8,
    pub pubkey: PublicKey,
    pub last_action_block: u64,
    pub has_folded: bool,
    pub entropy_contribution: Option<EntropyContribution>,
    pub instance_seed: [u8; 32],
}
```

There is **no `balance` and no `locked` field**. This snippet carried both until 2026-10-12
(`balance: u64 // Cash available`, `locked: u64 // Currently in pot`), and `version`/`instance_seed`
were missing from it and from `Pot` below. Neither the record nor the room holds a per-player
balance: value lives in promissory-note commitments and the room tracks participation, folding and
entropy. A reader who reconstructed the layout from the old snippet read every field after
`pubkey` at the wrong offset.

### Pot
```rust
pub struct Pot {
    pub version: u8,
    pub pot_id: PotId,
    pub room_id: RoomId,
    pub total: u64,
    pub contributions: Vec<PotContribution>,
    pub state: PotState,
    pub betting_round: u8,
    pub created_at: u64,
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
    AllIn,   // All remaining stake
    Fold,    // Forfeit hand (no pot claim)
}
```

### RoomConfig
```rust
pub struct RoomConfig {
    pub owner_dao: ContractId,   // Escrow-DAO contract ID
    pub asset_id: pallas::Base,
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
  DepositV1 → one promissory_note::transfer_v1 child (0x04)
  (the room records no per-player balance)

User Places Bet:
  PlaceBetV1 → one PN transfer child (0x04);
  Pot.total += amount and a PotContribution(player, amount, bet_type, block) is appended

User Raises / Calls:
  RaiseV1 / CallV1 → one PN transfer child (0x04), recorded the same way

User Folds:
  FoldV1 → has_folded = true; the contribution stays in the pot

User Withdraws (cash out):
  WithdrawV1 → one PN transfer child (0x04)
  (there is no stored balance to debit)

Owner DAO Settles Pot:
  SettlePotV1 → pot state closed, winners and payout determined

User Claims Winnings:
  ClaimV1 → PN transfer of the settled payout_amount;
  a nullifier prevents a second claim
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

## ZK Circuits

All **12** circuits compiled to `.zk.bin`:

| Circuit | Purpose |
|---------|---------|
| `create_room.zk` | Prove room creation with owner authorization |
| `create_pot.zk` | Prove pot creation for a room |
| `deposit.zk` | Prove stake deposit with account derivation |
| `place_bet.zk` | Prove bet placement with commitment binding |
| `raise.zk` | Prove a raise into the pot |
| `call.zk` | Prove a call for the current bet |
| `fold.zk` | Prove the folder's account update |
| `close_pot.zk` | Prove the pot closes and the room's current-pot fields clear |
| `settle_pot.zk` | Prove DAO-authorized pot settlement |
| `contribute_entropy.zk` | Prove an entropy commitment or reveal (TrustedSetup mode) |
| `claim.zk` | Prove winner claim with payout verification |
| `withdraw.zk` | Prove an amount withdrawal from the room |

**This table listed five and said "All 5 circuits"** until 2026-10-12 (`F-003`); the tree holds
twelve, and the seven added here — `create_pot`, `raise`, `call`, `fold`, `close_pot`,
`contribute_entropy`, `withdraw` — are the ones whose endpoints the state machine above
documents. A reader checking which operations are proven against this table would have concluded
that raising, calling and folding have no circuit at all.

## Promissory Note Lifecycle Integration

The Game Room contract is a **token mover** in the Promissory Note ecosystem — it manages
player stakes and pot funds during games via TransferV1.

### Why Game Room Uses TransferV1

All Game Room PN child calls use **TransferV1 (0x04)** exclusively:

| Operation | PN Child Call | What Actually Happens |
|-----------|--------------|----------------------|
| DepositV1 | TransferV1 | Player deposits tokens via a PN transfer; no room balance is written |
| WithdrawV1 | TransferV1 | Player withdraws tokens via a PN transfer |
| PlaceBetV1 / RaiseV1 / CallV1 | TransferV1 | The transfer is recorded as a `PotContribution`; `Pot.total` increases |
| ClaimV1 | TransferV1 | Winner claims pot payout |

This is architecturally correct: the Game Room manages existing tokens within a game
session. It does not mint or burn — tokens are promissory-note commitments moved by PN transfers;
the stablecoin's mint/redeem arms are currently refused ([stablecoin](stablecoin.md) §Status).

### Custody Model

The Game Room does **not** hold a per-player balance. Value stays in promissory-note
commitments; the room tracks `Pot.total` and `Pot.contributions`
(`player`, `amount`, `bet_type`, `block`). Every value-moving endpoint —
DepositV1, WithdrawV1, PlaceBetV1, RaiseV1, CallV1 and ClaimV1 — carries one
`promissory_note::transfer_v1` (0x04) child. Claim pays the settled `payout_amount`
and a nullifier prevents a second claim.

### Cross-Contract Validation

Child calls validate both `contract_id` and `value_commit` to prevent routing attacks
and ensure the correct deposit, bet, or payout amount is transferred.

## Security Considerations

1. **Player identification**: All functions take `player: PublicKey` in params (verified by ZK proof/signature at app layer)
2. **Owner verification**: Uses `ContractId::derive_public(caller) == owner_dao` pattern
3. **Double-claim prevention**: Nullifiers track claimed pots per player
4. **Payout validation**: the settled `payout_amount` is ZK-proven for the winner; nullifiers prevent a second claim

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](../dev/contracts/safety.md) — Capability safety analysis


- [Game Room SDK](../../../src/contract/game_room/README.md)
- [Provable Randomness](./provable_randomness.md)
- [DarkIRC Specification](../misc/darkirc/specification.md)
