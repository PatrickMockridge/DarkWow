# Slot Contract

A **composable slot machine contract** designed like Baccarat — the core contract handles commitment and settlement, but the game logic (paytables, reel configurations) is modular and swappable.

## Architecture: The Composability Pattern

This contract follows the **same composability pattern as Baccarat**:

```
Baccarat:    CommitBet → DrawCards → SettleBet
Slot:         CommitSpin → RevealSpin → SettleSpin
```

### Why This Pattern?

1. **Commit phase** (ZK): Player commits to bet without revealing parameters
2. **Reveal phase**: Random outcome determined by block entropy
3. **Settle phase** (ZK): Outcome verified, payout calculated, constrained by circuit

The **COMPOSABILITY** comes from swapping game modules while keeping the core contract fixed:

| Component | Baccarat | Slot |
|----------|---------|------|
| Commitment | `commit_bet_v1.zk` | `commit_bet_v1.zk` |
| Game logic | Card dealing rules | Paytable + Reels |
| Settlement | `settle_bet_v1.zk` | `settle_bet_v1.zk` |

## Composability Points

### 1. Paytables (Swappable)

Different slot variants use different paytables:

```rust
// Classic 3-reel single-line (high RTP, simple)
mod classic_paytable {
    pub fn create() -> Paytable {
        Paytable::new(vec![
            PaytableEntry { symbol: BAR, count: 3, multiplier: 100 },
            PaytableEntry { symbol: Symbol(7), count: 3, multiplier: 50 },
            PaytableEntry { symbol: CHERRY, count: 3, multiplier: 20 },
            // ...
        ])
    }
}

// Video 5-reel multi-payline (more features)
mod video_paytable {
    pub fn create() -> Paytable {
        Paytable::new(vec![
            PaytableEntry { symbol: WILD, count: 5, multiplier: 1000 },
            PaytableEntry { symbol: SCATTER, count: 5, multiplier: 100 },
            // ... more combinations
        ])
    }
}
```

### 2. Reel Strip Configurations (Swappable)

Reel strips define symbol layouts:

```rust
// Classic slot: shorter strips, weighted toward wins
fn classic_reels() -> Vec<ReelStrip> {
    vec![
        ReelStrip::new(vec![BAR, CHERRY, CHERRY, ...]), // ~15 symbols
        ReelStrip::new(vec![...]), // ~15 symbols
        ReelStrip::new(vec![...]), // ~15 symbols
    ]
}

// Video slot: longer strips, more blanks
fn video_reels() -> Vec<ReelStrip> {
    vec![
        ReelStrip::new(vec![WILD, A, K, Q, J, 10, ..., BLANK, ...]), // ~50+ symbols
        // ...
    ]
}
```

### 3. Paylines (Configurable)

```rust
// Single line classic slot
vec![Payline::horizontal_middle(3)]

// 9-line video slot
vec![
    Payline::horizontal_top(5),
    Payline::horizontal_middle(5),
    Payline::horizontal_bottom(5),
    Payline::new(3, vec![0, 1, 2, 1, 0]),  // V shape
    Payline::new(4, vec![2, 1, 0, 1, 2]),  // inverted V
    // ... 5 more lines
]
```

### 4. Extension Circuits (Future)

Bonus rounds, progressive jackpots, and special features can be added as separate ZK circuits that integrate with the core settle flow.

## How It Works

### Flow: CommitSpin → RevealSpin → SettleSpin

```
1. Player commits to spin
   ├── Hides bet_value, paylines, secret_nonce in ZK proof
   └── Creates commitment (spin_id) on-chain

2. Block entropy reveals outcome
   ├── Positions derived from block hashes
   └── spin.result updated on-chain

3. Settlement calculates payout
   ├── ZK proof constrains payout calculation
   ├── Paytable lookup for winning combinations
   └── House edge applied
```

## ZK Proof Structure

### commit_bet_v1.zk
Commits to bet parameters without revealing them.

### settle_bet_v1.zk
Constrains payout calculation:
- Validates positions are within reel bounds
- Verifies paytable lookup results
- Applies house edge correctly

## Extending the Contract

### Creating a New Slot Variant

1. **Define paytable** in `model/mod.rs`:
```rust
pub mod my_slot_paytable {
    pub fn create() -> Paytable {
        Paytable::new(vec![
            // Your winning combinations
        ])
    }
}
```

2. **Define reel strips**:
```rust
pub fn my_reels() -> Vec<ReelStrip> {
    vec![
        ReelStrip::new(vec![...]), // Reel 1
        ReelStrip::new(vec![...]), // Reel 2
        // ...
    ]
}
```

3. **Add game type constant** in `lib.rs`:
```rust
pub const GAME_TYPE_MY_SLOT: u8 = 2;
```

4. **Update entrypoint.rs** to support your game type in initialization

### Adding Bonus Features

Bonus rounds can be implemented as separate ZK circuits that:
1. Take the base spin result as input
2. Derive bonus positions from additional entropy
3. Calculate bonus payouts
4. Integrate with the main settlement

## Money Contract Integration

- `CommitSpinV1` should be called as child of `Money::Burn` to lock bet
- `SettleSpinV1` updates state; winning spins require `Money::TokenMint` for payout
- `CancelSpinV1` lets house collect on abandoned/timeout spins

## Security Considerations

- **House edge** is applied in ZK during settle (cannot be manipulated)
- **Randomness** comes from block entropy (decentralized, unpredictable)
- **Payouts** are constrained by circuit (cannot overpay)

## File Structure

```
src/contract/slot/
├── Cargo.toml
├── proof/
│   ├── commit_bet_v1.zk      # ZK proof for committing
│   └── settle_bet_v1.zk      # ZK proof for settlement
└── src/
    ├── lib.rs                # Function enum, constants
    ├── error.rs              # Error types
    ├── model/
    │   └── mod.rs            # Core types, paytables, reels, game logic
    └── entrypoint.rs          # init, exec, update
```

## Related Documentation

- [Baccarat Contract](baccarat.md) — Same composability pattern for card games
- [Casino Architecture](baccarat.md) — Entropy module for provable randomness
- [Experimental Opcodes](experimental-opcodes.md) — Analysis of ZK circuit limitations