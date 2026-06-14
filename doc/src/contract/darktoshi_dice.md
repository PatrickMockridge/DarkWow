# DarkToshi Dice Contract

A privacy-preserving Satoshi Dice clone implementing provably fair gambling with commit-reveal pattern.

## Overview

DarkToshi Dice allows players to bet on random rolls with the following mechanics:

1. **Commit Phase**: Player commits to a bet (value + target + secret nonce) without revealing
2. **Roll Phase**: Random roll is derived from block hash + commitment
3. **Settlement Phase**: Winners receive payouts; losers forfeit to house

## State Machine

```
COMMITTED ──[RevealRoll]──> REVEALED ──[SettleBet]──> SETTLED
     │                                                   │
     └──[HouseClose after timeout]──> CANCELLED <───────┘
```

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize house settings |
| CommitBetV1 | 0x01 | Player commits to a bet |
| RevealRollV1 | 0x02 | Reveal roll using block hash |
| SettleBetV1 | 0x03 | Settle and claim payout |
| HouseCloseV1 | 0x04 | House closes stale bets |

## Betting Odds

| Target | Probability | Payout (excl. house edge) | House Edge |
|--------|-------------|---------------------------|------------|
| 10     | 10%         | 10x                       | 2%         |
| 25     | 25%         | 4x                        | 2%         |
| 50     | 50%         | 2x                        | 2%         |
| 75     | 75%         | 1.33x                     | 2%         |
| 90     | 90%         | 1.11x                     | 2%         |

## Roll Calculation

```
roll = hash(block_hash, bet_id, secret_nonce) % 100
```

- Player wins if `roll < target`
- House wins if `roll >= target`

## Payout Formula

```
payout = bet_value * (10000 - house_edge) / (target * 100)
```

Where `house_edge` is in basis points (e.g., 200 = 2%).

## Promissory Note Lifecycle Integration

The DarkToshi Dice contract is a **token mover** in the Promissory Note ecosystem — it
holds bets in escrow until the roll is revealed and distributes payouts via TransferV1.

### Why DarkToshi Dice Uses TransferV1

All Dice PN child calls use **TransferV1 (0x04)** exclusively:

| Operation | PN Child Call | What Actually Happens |
|-----------|--------------|----------------------|
| CommitBetV1 | TransferV1 | Player transfers bet to contract escrow |
| SettleBetV1 | TransferV1 | Contract pays winner (player or house) |
| HouseCloseV1 | TransferV1 | House reclaims stale bet amounts |

This is architecturally correct: DarkToshi Dice is not a token issuer. It manages
existing tokens on behalf of players and the house. Tokens are created and destroyed
by the [stablecoin](stablecoin.md) contract.

### Custody Model

DarkToshi Dice acts as a temporary custodian: bets are locked at commit time and released
at settlement. The house must maintain sufficient balance in `DICE_CONTRACT_HOUSE_TREE`
to cover winning payouts. House capital is tracked via the `b"balance"` key and updated
on SettleBetV1 and HouseCloseV1.

### Cross-Contract Validation

Child calls validate both `contract_id` and `value_commit` to prevent routing attacks
and ensure the correct bet amount is transferred.

## Key Implementation Details

### Randomness Source

Uses full 32-byte tx hash for randomness via Poseidon hashing:
```rust
let a = u64::from_le_bytes(tx_hash[0..8]);
let b = u64::from_le_bytes(tx_hash[8..16]);
let c = u64::from_le_bytes(tx_hash[16..24]);
let d = u64::from_le_bytes(tx_hash[24..32]);
let block_hash = poseidon_hash([...]);
let roll = poseidon_hash([block_hash, bet_id, secret_nonce]);
```

**Security Note**: The current implementation uses `wasm::util::get_tx_hash()` which
provides tx-level randomness. For higher-stakes applications, see [Provable Randomness](provable_randomness.md)
for analysis of leveraging DarkWow's PoW mechanism (RandomX) directly.

### Adjustable Confirmation Depth

Players can specify a `confirmation_depth` when placing bets, determining how many blocks
to wait before settlement is allowed. This accumulates PoW entropy for stronger randomness.

```
Security scaling with depth:
- K=1: 33% manipulation chance (with 33% hash power)
- K=6: ~0.14% (Bitcoin "6 confirmations" standard)
- K=10: ~0.005%
```

The `settle_block` is calculated as:
```
settle_block = current_block + confirmation_depth
```

Higher confirmation depths exponentially increase the cost of manipulation but require
waiting for more blocks before the bet can be settled.

### Constants

- `DEFAULT_HOUSE_EDGE`: 200 basis points (2%)
- `MIN_HOUSE_EDGE`: 100 basis points (1%)
- `MAX_HOUSE_EDGE`: 500 basis points (5%)
- `DEFAULT_ROLL_TIMEOUT`: 10 blocks
- `MAX_TARGET`: 99

## Randomness Analysis

The dice contract demonstrates several randomness patterns:

| Pattern | Current Use | Security Level |
|---------|-------------|----------------|
| Commit-Reveal | Secret nonce committed at bet time | High |
| Block Hash | tx_hash at block inclusion | Medium |
| Poseidon Hash | Combine multiple sources | High |

**Weakness**: Current implementation relies on tx_hash which could be influenced by transaction ordering. For production gambling use, leverage PoW block hash directly.

See [Provable Randomness](provable_randomness.md) for full analysis including:
- Leveraging DarkWow's RandomX PoW for randomness
- ECVRF-based verifiable randomness
- Hybrid approaches (PoW + VRF + Commit-Reveal)
- **Case study: Block Height Prediction Market** with detailed implementation design

## Primitives Provided

This contract establishes useful primitives for other games:

- Commit-reveal schemes via Poseidon hash
- Block hash randomness via `get_tx_hash()`
- Conditional value transfer (win/lose outcomes)
- Time-locked state transitions
- House edge economics

## See Also

- [Provable Randomness](provable_randomness.md) - Deep dive into randomness sources and security
- [PromissoryNote Contract](promissory_note.md) - Value transfer integration
- [Atomic Swap](../contract/atomic_swap.md) - Commit-reveal pattern reference
- [Tender Contract](tender.md) - Sealed bid pattern reference
- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis
## Localnet Testing (2026-04-07)

The DarkToshi Dice contract was successfully deployed and tested on localnet.

### Test Configuration

- **Network**: localnet with `pow_fixed_difficulty=1`
- **Mining**: `dwow_wallet mine` against dwowd stratum server (port 48347)
- **Block reward**: 20 DRKW per block
- **Wallet**: Initialized and funded via mining

### Deployment Details

```bash
# Deploy contract
dwow_wallet contract deploy BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1 \
  target/wasm32-unknown-unknown/release/darkfi_darktoshi_dice_contract.wasm \
  | dwow_wallet broadcast

# Transaction ID: e15a50bae7940593057ca9674f774aaf7f50e107bd4b3483d6e65130e55d8e2f
# Contract ID: BNLNkr1DrDLqVE3SovLkqHYukwvin1W93xwTpfsxmwh1
```

### Verified Workflow

1. `dwow_wallet wallet balance` - Check DRKW tokens
2. `dwow_wallet wallet coins` - View unspent coins
3. `dwow_wallet contract list` - List deploy authorities
4. `dwow_wallet scan` - Discover blockchain updates
5. `dwow_wallet contract deploy | dwow_wallet broadcast` - Deploy contract

### CLI Notes

- Config file required: `-c bin/drk/dww_config.toml`
- Network flag: `-n localnet`
- `scan` is a top-level subcommand (not `wallet scan`)
- Values displayed in raw units (8 decimal places)
