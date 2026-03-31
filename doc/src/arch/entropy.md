# Entropy Module

A composable module providing provably fair randomness generation using block hash entropy. Used by all DarkFi betting/gambling contracts for drawing outcomes.

## Overview

The entropy module (`darkfi_sdk::crypto::entropy`) provides a unified API for randomness that was previously copy-pasted across multiple contracts:

- **Roulette**: Single number drawing (0 to wheel_size-1)
- **Lottery**: Multiple unique numbers from a range
- **DarkToshi Dice**: Cumulative PoW entropy with confirmation depth
- **Baccarat**: Card dealing from multiple block hashes
- **Block Height Prediction**: Resolution from cumulative entropy

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
use darkfi_sdk::crypto::entropy::draw_single;

// Draw winning number for European roulette (0-36)
let winning = draw_single(block_hash, nonce, 37);
```

#### `draw_unique_range(block_hash, seed_nonce, count, range)`
Draw `count` unique numbers in range `[1, range]`.

```rust
use darkfi_sdk::crypto::entropy::draw_unique_range;

// Draw 6 unique numbers from 1-59 (UK National Lottery)
let numbers = draw_unique_range(block_hash, seed_nonce, 6, 59);
assert_eq!(numbers.len(), 6);
```

#### `combine_block_hashes(block_hashes)`
Combine multiple block hashes into single entropy source using cumulative Poseidon hashing.

```rust
use darkfi_sdk::crypto::entropy::combine_block_hashes;

// Combine 6 blocks for high-security bet resolution
let entropy = combine_block_hashes(&[hash1, hash2, hash3, hash4, hash5, hash6]);
```

#### `draw_with_depth(block_hashes, nonce, range)`
Draw using cumulative PoW entropy with confirmation depth.

```rust
use darkfi_sdk::crypto::entropy::draw_with_depth;

// Draw with 6-block confirmation depth
let roll = draw_with_depth(&[hash1, hash2, hash3, hash4, hash5, hash6], bet_id, 100);
```

#### `mix_entropy(base, additional)`
Mix additional entropy sources with a base value.

```rust
use darkfi_sdk::crypto::entropy::mix_entropy;

// Combine block entropy with bet-specific data
let entropy = combine_block_hashes(block_hashes);
let final_entropy = mix_entropy(entropy, &[bet_id, secret_nonce]);
```

#### `tx_hash_to_base(tx_hash)`
Convert TransactionHash to pallas::Base for entropy use.

```rust
use darkfi_sdk::crypto::entropy::tx_hash_to_base;

let base = tx_hash_to_base(&tx_hash_bytes);
```

## Usage in Contracts

### Roulette
```rust
use darkfi_sdk::crypto::entropy::draw_single;

// In spin_wheel instruction
let winning_number = draw_single(block_hash, nonce, table.wheel_size);
```

### Lottery
```rust
use darkfi_sdk::crypto::entropy::draw_unique_range;

// In draw_winners instruction
let numbers = draw_unique_range(block_hash, seed_nonce, config.num_picks, config.number_range);
```

### DarkToshi Dice
```rust
use darkfi_sdk::crypto::{combine_block_hashes, mix_entropy};

// In reveal_roll instruction
let block_entropy = combine_block_hashes(&block_hashes);
let roll_entropy = mix_entropy(block_entropy, &[bet_id, secret_nonce]);
```

### Baccarat
```rust
use darkfi_sdk::crypto::{tx_hash_to_base, mix_entropy};

// In draw_cards instruction
let mut entropy = bet_id;
for (i, hash) in block_hashes.iter().enumerate() {
    let block_entropy = tx_hash_to_base(&hash.0);
    entropy = mix_entropy(entropy, &[block_entropy, pallas::Base::from(i as u64)]);
}
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