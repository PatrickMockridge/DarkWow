# Entropy Module

A composable module providing provably fair randomness generation using block hash entropy. Used by all DarkWow betting/gambling contracts for drawing outcomes.

## Overview

The entropy module (`dwow_sdk::crypto::entropy`) provides a unified API for randomness that was previously copy-pasted across multiple contracts:

- **DarkToshi Dice**: Cumulative PoW entropy with confirmation depth
- **Baccarat**: Card dealing from multiple block hashes
- **Lottery**: Multiple unique numbers from a range
- **Slot**: Manual extraction from 32-byte block hash into 4 × u64
- **Roulette**: Uses own `draw_winning_number()` — does NOT use SDK entropy module

## Security Model

Block hash entropy is sourced from Proof-of-Work mining. An attacker with less than 33% hash power has negligible chance to manipulate a single block hash.

### Confirmation Depth Security

| Depth | Blocks | Manipulation Chance (33% attacker) |
|-------|--------|-------------------------------------|
| Low | 1 | ~33% |
| Medium | 6 | ~0.14% (Bitcoin standard) |
| High | 10 | ~0.005% |

Higher depth means exponentially harder to manipulate, but requires waiting for more block confirmations.

## API

### Core Functions

#### `draw_single(block_hash, nonce, range)`
Draw a single random number in range `[0, range)`.

```rust
use dwow_sdk::crypto::entropy::draw_single;

// Draw winning number for European roulette (0-36)
let winning = draw_single(block_hash, nonce, 37);
```

#### `draw_unique_range(block_hash, seed_nonce, count, range)`
Draw `count` unique numbers in range `[1, range]`.

```rust
use dwow_sdk::crypto::entropy::draw_unique_range;

// Draw 6 unique numbers from 1-59 (UK National Lottery)
let numbers = draw_unique_range(block_hash, seed_nonce, 6, 59);
assert_eq!(numbers.len(), 6);
```

#### `combine_block_hashes(block_hashes)`
Combine multiple block hashes into single entropy source using cumulative Poseidon hashing.

```rust
use dwow_sdk::crypto::entropy::combine_block_hashes;

// Combine 6 blocks for high-security bet resolution
let entropy = combine_block_hashes(&[hash1, hash2, hash3, hash4, hash5, hash6]);
```

#### `draw_with_depth(block_hashes, nonce, range)`
Draw using cumulative PoW entropy with confirmation depth.

```rust
use dwow_sdk::crypto::entropy::draw_with_depth;

// Draw with 6-block confirmation depth
let roll = draw_with_depth(&[hash1, hash2, hash3, hash4, hash5, hash6], bet_id, 100);
```

#### `mix_entropy(base, additional)`
Mix additional entropy sources with a base value.

```rust
use dwow_sdk::crypto::entropy::mix_entropy;

// Combine block entropy with bet-specific data
let entropy = combine_block_hashes(block_hashes);
let final_entropy = mix_entropy(entropy, &[bet_id, secret_nonce]);
```

#### `tx_hash_to_base(tx_hash)`
Convert TransactionHash to pallas::Base for entropy use.

```rust
use dwow_sdk::crypto::entropy::tx_hash_to_base;

let base = tx_hash_to_base(&tx_hash_bytes);
```

## Usage in Contracts

### Roulette

Roulette uses its own `draw_winning_number()` implementation in
`src/contract/roulette/src/model/mod.rs`. It extracts entropy manually
from block hash via `wasm::util::get_block_hash()` rather than using the
SDK entropy module.

```rust
// In spin_wheel instruction (actual implementation):
let block_hash = wasm::util::get_block_hash(current_block as u32)?;
let winning_number = draw_winning_number(
    &block_hash.0,
    table.spin_count,
    nonce,
    table.wheel_size as u64,
);
```

### Lottery
```rust
use dwow_sdk::crypto::entropy::draw_unique_range;

// In draw_winners instruction
let numbers = draw_unique_range(block_hash, seed_nonce, config.num_picks, config.number_range);
```

### DarkToshi Dice
```rust
use dwow_sdk::crypto::{combine_block_hashes, mix_entropy};

// In reveal_roll instruction
let block_entropy = combine_block_hashes(&block_hashes);
let roll_entropy = mix_entropy(block_entropy, &[bet_id, secret_nonce]);
```

### Baccarat
```rust
// In draw_cards instruction (deal_cards function in model/mod.rs)
// Uses wasm::util::get_block_hash() via WASM runtime, not SDK entropy module
fn deal_cards(block_hashes: &[TransactionHash], bet_id: BetId)
    -> (Hand, Hand, Option<Card>, Option<Card>) { ... }
```

### Slot
```rust
// In spin instruction (entrypoint.rs)
// Extracts from block hash manually via wasm::util::get_block_hash()
let block_hash = wasm::util::get_block_hash(wasm::util::get_verifying_block_height()?)?.0;
let a = u64::from_le_bytes(block_hash[0..8].try_into().unwrap());
let b = u64::from_le_bytes(block_hash[8..16].try_into().unwrap());
let c = u64::from_le_bytes(block_hash[16..24].try_into().unwrap());
let d = u64::from_le_bytes(block_hash[24..32].try_into().unwrap());
```

## Design Principles

1. **Composability**: Single module, reusable functions
2. **Security by default**: Cumulative PoW entropy available for high-value bets
3. **Flexibility**: Multiple API levels from simple to advanced
4. **No external dependencies**: Pure Poseidon hashing

## Future Improvements

- [ ] VRF (Verifiable Random Function) integration for publicly verifiable randomness
- [ ] BLS threshold signatures for distributed randomness
- [ ]-commit-reveal with zkSNARK proofs for committed randomness

## See Also

- [Contract Manifest](../arch/manifest.md) — On-chain ABI for this contract
- [Contract Trust Model](../arch/contract-trust-model.md) — Don't trust, verify
- [Contract Safety](safety.md) — Capability safety analysis
