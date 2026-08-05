# Entropy Beacon

Verifiable randomness from DarkWow block hashes. Ported from the
[Mudra Arweave entropy beacon](https://codeberg.org/PatrickM123/mudra),
adapted from Arweave to DarkWow's own chain context.

For randomness theory and VRF-based approaches, see [Provable Randomness](provable_randomness.md).

No single party controls the seed. The anchor height is committed before
the entropy blocks exist — the seed is unpredictable at commitment time.

## Crate

`dwow_entropy_contract` at [src/contract/entropy/](../../../src/contract/entropy/).

This is a **pure library crate** (`rlib`), not a standalone contract. Betting
contracts import it directly — no cross-contract calls, no separate contract state.

```toml
[dependencies]
dwow_entropy_contract = { path = "../entropy" }
```

## Protocol

```
1. Record anchor height H = get_last_block_height()
       │
2. Wait for N blocks to be mined (H+1 through H+N)
       │
3. Collect block hashes for each block
       │
4. derive_seed(&blocks) → u64 seed
       │
5. Use seed for game outcome (modulo for dice roll, card draw, etc.)
```

The anchor is recorded *before* the entropy blocks exist. Block hashes depend on
all transactions in that block — collusion requires controlling the mining majority.

## API

### `derive_seed(blocks: &[EntropyBlock]) -> u64`

Derives a deterministic u64 seed from a list of block hashes via Blake3. For each
block, feeds `height.to_le_bytes() || block_hash` into the hasher. Takes the first
8 bytes of the final hash as a little-endian u64.

```rust
use dwow_entropy_contract::{derive_seed, EntropyBlock};

// Collect 3 blocks of entropy after the anchor height
let mut blocks = Vec::new();
for h in (anchor + 1)..=(anchor + 3) {
    blocks.push(EntropyBlock {
        height: h,
        block_hash: wasm::util::get_block_hash(BlockHeight::from(h))?.0,
    });
}

// Derive seed
let seed = derive_seed(&blocks);

// Use for game outcome
let dice_roll = (seed % 6) as u8 + 1;    // 1-6
let roulette = (seed % 37) as u8;         // 0-36
let card = (seed % 52) as u8;             // 0-51
```

### `EntropyBlock`

```rust
pub struct EntropyBlock {
    pub height: u64,        // block height
    pub block_hash: [u8; 32],  // 32-byte block hash
}
```

## Trust Model

- **Commitment before entropy**: The anchor height is recorded before the entropy
  blocks are mined. No party knows the future block hashes.
- **No single party controls blocks**: DarkWow block hashes depend on all
  transactions submitted in that block. Manipulation requires mining majority.
- **Verifiable provenance**: The block list (heights + hashes) can be stored
  alongside the seed. Any node can re-derive and confirm `derive_seed(&blocks)`
  matches the stored seed.
- **Blake3**: Standard cryptographic hash. No exotic primitives.

## Mudra → DarkWow Adaptation

| Mudra (Arweave) | DarkWow |
|-----------------|---------|
| Post intent to Arweave as timestamp proof | Record anchor height in contract state |
| Arweave block hashes via `arweave.net/block/height/{h}` | DarkWow block hashes via `wasm::util::get_block_hash(BlockHeight)` |
| Arweave block time ~2 minutes | DarkWow block time (configurable) |
| Blake3 derivation | Identical |
| Intent TXID for replay verification | Stored block list for replay verification |

## Integration with Betting Contracts

All betting contracts that need randomness import `dwow_entropy_contract` and call
`derive_seed` directly:

| Contract | Uses entropy? | Notes |
|----------|--------------|-------|
| Darktoshi Dice | Yes | Replaces ad-hoc block hash in `reveal_roll_v1` |
| Lottery | Yes | Replaces user-provided `instance_seed` |
| Roulette | Yes | Replaces manual `draw_winning_number` from block hash |
| Baccarat | Yes | Replaces manual `deal_cards` from block hash |
| Slot | Yes | Replaces manual u64 extraction from block hash bytes |
| Darkbet Exchange | Yes | Settlement randomness |
| Betting Stake | No | Staking only — no randomness needed |
| Game Room | No | Game lobby — randomness delegated to individual games |

## Test Vectors

The crate includes 7 test vectors (`cargo test -p dwow_entropy_contract`):

| Test | What it verifies |
|------|-----------------|
| `test_derive_seed_deterministic` | Same blocks → same seed |
| `test_derive_seed_different_blocks` | Different hashes → different seeds |
| `test_derive_seed_order_matters` | Block order affects seed |
| `test_derive_seed_single_block` | Single-block edge case |
| `test_derive_seed_ten_blocks` | 10-block case |
| `test_derive_seed_known_vector` | `blake3(42 || [0u8; 32])` → `14760227444319121995` |
| `test_derive_seed_nonzero` | Non-empty input → non-zero output |

## Security

- **Confirmation depth**: 1 block already provides strong entropy — the anchor was
  committed before that block's hash existed. Each additional block defends against
  deeper chain reorgs. 3 blocks is a practical default; 10 blocks is conservative.
- **No ZK proof needed**: The security is in the protocol (commitment before entropy
  blocks exist), not in zero-knowledge proofs. Anyone can re-derive and verify.
- **Independent verification**: Store the block list (heights + hashes). Any third
  party can call `derive_seed(&stored_blocks)` and confirm the outcome.

## See Also

- [Mudra Entropy Beacon](https://codeberg.org/PatrickM123/mudra/src/branch/master/docs/entropy.md) — reference implementation
- [Darktoshi Dice](../../../src/contract/darktoshi_dice/) — first betting contract to integrate
- [Baccarat](../../../src/contract/baccarat/) — multi-round game with capital efficiency
