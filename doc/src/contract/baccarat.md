# Baccarat Contract

A privacy-preserving Baccarat (Punto Banco) contract implementing provably fair gambling with commit-reveal pattern. Famous from James Bond films, Baccarat is a casino classic that offers three possible outcomes: Player wins, Banker wins, or Tie.

## Overview

Baccarat is particularly well-suited for blockchain gambling because:

- **No player decisions**: Drawing rules are completely fixed (no discretion)
- **Deterministic outcomes**: No disputes possible—rules determine everything
- **Fast gameplay**: Simple 3-outcome betting
- **Perfect for commit-reveal**: Block hash provides card shuffle entropy

## State Machine

```
COMMITTED ──[DrawCards]──> CARDS_DRAWN ──[SettleBet]──> SETTLED
     │                                                    │
     └──[HouseClose after timeout]──> CANCELLED <─────────┘
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize house settings |
| CommitBetV1 | 0x01 | Player commits to a bet (Player/Banker/Tie) |
| DrawCardsV1 | 0x02 | Reveal cards using PoW block hash entropy |
| SettleBetV1 | 0x03 | Settle and claim payout |
| HouseCloseV1 | 0x04 | House closes stale bets after timeout |

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

## Betting Odds

| Bet Type | Payout | House Edge | Probability |
|----------|--------|------------|-------------|
| Player | 1:1 | ~1.24% | 44.62% |
| Banker | 0.95:1 | ~1.06% | 45.85% |
| Tie | 8:1 | ~14.36% | 9.52% |

## Payout Formula

```
Player bet:     payout = bet_value * 100 / 100        // 1:1
Banker bet:     payout = bet_value * 95 / 100         // 0.95:1 (house takes 5%)
Tie bet:        payout = bet_value * 800 / 100        // 8:1
```

## Card Dealing via Block Hash

Cards are derived using cumulative PoW entropy from K consecutive block hashes (where K = confirmation_depth):

```rust
entropy = bet_id
for (i, block_hash) in recent_blocks.iter().enumerate() {
    // Convert 32-byte block hash to 4 x u64
    let a = u64::from_le_bytes(block_hash[0..8]);
    let b = u64::from_le_bytes(block_hash[8..16]);
    let c = u64::from_le_bytes(block_hash[16..24]);
    let d = u64::from_le_bytes(block_hash[24..32]);

    // Poseidon hash of the block entropy
    block_entropy = poseidon_hash([Base::from(a), Base::from(b), Base::from(c), Base::from(d)]);

    // Cumulative entropy with index
    entropy = poseidon_hash([entropy, block_entropy, Base::from(i)]);
}

// Derive 4 cards from final entropy
player_card1 = Card(entropy[0:8] % 52)
player_card2 = Card(entropy[8:16] % 52)
banker_card1 = Card(entropy[16:24] % 52)
banker_card2 = Card(entropy[24:32] % 52)
```

## Promissory Note Lifecycle Integration

The Baccarat contract is a **token mover** in the Promissory Note ecosystem — it holds
bets in escrow until settlement and distributes payouts via TransferV1.

### Why Baccarat Uses TransferV1

All Baccarat PN child calls use **TransferV1 (0x04)** exclusively:

| Operation | PN Child Call | What Actually Happens |
|-----------|--------------|----------------------|
| CommitBetV1 | TransferV1 | Player transfers bet to contract escrow |
| SettleBetV1 | TransferV1 | Contract pays winner (player or house) |
| HouseCloseV1 | TransferV1 | House reclaims stale bet amounts |

This is architecturally correct: Baccarat is not a token issuer. It manages existing
tokens on behalf of players and the house. Tokens are created and destroyed by the
[stablecoin](stablecoin.md) contract.

### Custody Model

Baccarat acts as a temporary custodian: bets are locked at commit time and released
at settlement. The house must maintain sufficient balance in `BACCARAT_CONTRACT_HOUSE_TREE`
to cover winning payouts. House capital is tracked via the `b"balance"` key and updated
on SettleBetV1 and HouseCloseV1.

### Cross-Contract Validation

Child calls validate both `contract_id` and `value_commit` to prevent routing attacks
and ensure the correct bet amount is transferred.

## Key Implementation Details

### Confirmation Depth and Security

The `confirmation_depth` parameter allows players to customize the security vs. speed tradeoff:

```
Security scaling with depth:
- K=1: Fastest, uses only current block hash
- K=3: Recommended for most bets
- K=10: Maximum security for high-stakes
```

Higher confirmation depths exponentially increase the cost of manipulation but require waiting for more blocks before cards can be revealed.

The `settle_block` is calculated as:
```
settle_block = current_block + confirmation_depth
```

### Card Representation

Cards are represented as u8 values 0-51:
- 0-12: Clubs (2, 3, 4, 5, 6, 7, 8, 9, 10, J, Q, K, A)
- 13-25: Diamonds
- 26-38: Hearts
- 39-51: Spades

### Constants

- `DEFAULT_HOUSE_EDGE`: 150 basis points (~1.5%)
- `MAX_CONFIRMATION_DEPTH`: 10 blocks
- `BANKER_PAYOUT_NUM/DEN`: 95/100 (0.95:1)
- `PLAYER_PAYOUT_NUM/DEN`: 100/100 (1:1)
- `TIE_PAYOUT_NUM/DEN`: 800/100 (8:1)

## Randomness Analysis

Baccarat demonstrates cumulative PoW entropy for randomness:

| Pattern | Current Use | Security Level |
|---------|-------------|----------------|
| Commit-Reveal | Secret nonce committed at bet time | High |
| Block Hash | Cumulative K blocks via `get_block_hash()` | High |
| Poseidon Hash | Combine entropy sources | High |

The cumulative approach (using K consecutive blocks) is more resistant to manipulation than single-block hashes because an attacker would need to control the mining of K consecutive blocks.

**Key advantage over DarkToshi Dice**: Baccarat uses block hash entropy directly (not tx hash), leveraging PoW more directly. The `get_block_hash()` function provides access to canonical block hashes that miners have already committed to via RandomX.

See [Provable Randomness](provable_randomness.md) for full analysis including:
- Leveraging DarkWow's RandomX PoW for randomness
- ECVRF-based verifiable randomness
- Hybrid approaches (PoW + VRF + Commit-Reveal)
- **Case study: Baccarat** with detailed card dealing entropy analysis

## Comparison with DarkToshi Dice

| Aspect | DarkToshi Dice | Baccarat |
|--------|----------------|----------|
| Outcomes | 2 (win/lose) | 3 (Player/Banker/Tie) |
| Player decisions | Target selection | None |
| Randomness | Single tx hash | Cumulative K blocks |
| Complexity | Simple | Moderate |
| Popularity | Moderate | Very high (casino staple) |
| House edge | 2% (configurable) | 1.06-14.36% (varies by bet) |

## Why Baccarat Over Blackjack?

1. **Fixed rules**: No player decisions mean no disputes
2. **Simpler implementation**: No strategy variation
3. **Higher popularity**: Casino staple, known in popular culture
4. **Better for privacy**: No revealing of player strategy

## Primitives Provided

This contract establishes useful primitives for other games:

- Commit-reveal schemes via Poseidon hash
- Cumulative block hash entropy via `get_block_hash()`
- Fixed-odds betting with deterministic outcomes
- Multi-outcome betting (3 outcomes vs 2)
- Time-locked state transitions

## See Also
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis


- [Provable Randomness](provable_randomness.md) - Deep dive into randomness sources and security
- [DarkToshi Dice](darktoshi_dice.md) - Commit-reveal pattern reference
- [PromissoryNote Contract](promissory_note.md) - Value transfer integration
