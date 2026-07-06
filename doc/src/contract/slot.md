# Slot Contract

A **composable slot machine contract** designed like Baccarat — the core contract handles commitment and settlement, but the game logic (paytables, reel configurations) is modular and swappable.

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | `0x00` | Initialize the slot contract with house configuration |
| CommitSpinV1 | `0x01` | Player commits a bet (amount, reel selection) with entropy |
| RevealSpinV1 | `0x02` | House reveals the spin outcome from committed entropy |
| SettleSpinV1 | `0x03` | Settle the spin — pay winner or house based on paytable |
| CancelSpinV1 | `0x04` | Cancel a stale spin that was never revealed or settled |

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

## Promissory Note Lifecycle Integration

The Slot contract is a **token mover** in the Promissory Note ecosystem — it holds spin
bets in escrow until settlement and distributes payouts via TransferV1.

### Why Slot Uses TransferV1

All Slot PN child calls use **TransferV1 (0x04)** exclusively:

| Operation | PN Child Call | What Actually Happens |
|-----------|--------------|----------------------|
| CommitSpinV1 | TransferV1 | Player transfers bet to contract escrow |
| SettleSpinV1 | TransferV1 | Contract pays winner (player or house) |
| CancelSpinV1 | TransferV1 | House reclaims stale spin amounts |

This is architecturally correct: the Slot contract is not a token issuer. It manages
existing tokens on behalf of players and the house. Tokens are created and destroyed
by the [stablecoin](stablecoin.md) contract.

### Custody Model

The Slot contract acts as a temporary custodian: bets are locked at commit time and
released at settlement. Payouts are constrained by ZK circuits during settlement,
preventing overpayment.

### Cross-Contract Validation

Child calls validate both `contract_id` and `value_commit` to prevent routing attacks
and ensure the correct bet amount is transferred.

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

## Related
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis
 Documentation

- [Baccarat Contract](baccarat.md) — Same composability pattern for card games
- [Casino Architecture](baccarat.md) — Entropy module for provable randomness
- [Opcodes Reference](../arch/zk/opcodes.md) — Analysis of ZK circuit limitations