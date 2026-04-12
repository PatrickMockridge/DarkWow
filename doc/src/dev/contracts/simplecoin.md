# SimpleCoin: Baseline UTXO Token Design

## Overview

SimpleCoin is a minimal Bitcoin-like UTXO token contract designed as a baseline for DarkFi. It prioritizes **simplicity**, **auditability**, and **correctness** over complexity.

**Key Principle**: A plain UTXO token should work without ZK proofs for basic operations. Privacy can be layered on top later.

## Why SimpleCoin?

The current money_v2 implementation has significant complexity:
- Token authorization via separate AuthTokenMint + TokenMint calls
- Pedersen commitments for hidden values
- Encrypted notes (AEAD)
- EC multiplication in circuits (`ec_mul_base`)

This complexity led to bugs (EcGetX heap indexing errors) and is hard to audit. SimpleCoin strips away all unnecessary complexity.

## Design Principles

1. **Simplicity**: No complex ZK circuits required for basic transfer
2. **Auditability**: Easy to understand the token logic
3. **Baseline**: Works as plain public token, privacy optional
4. **Compatibility**: Uses existing DarkFi primitives (Merkle proofs, signatures)

## Token Model

### Coin Structure

```rust
struct Coin {
    owner_x: pallas::Base,  // Owner public key X coordinate
    owner_y: pallas::Base,   // Owner public key Y coordinate
    value: u64,             // Clear value (no hidden amounts)
    token_id: pallas::Base, // Token type identifier
    nonce: u64,             // Uniqueness counter
}
```

**No encryption, no commitments**. Values are stored in clear.

### Coin ID and Nullifiers

```rust
// Coin ID for Merkle tree inclusion
coin_id = poseidon_hash(owner_x, owner_y, value, token_id, nonce)

// Nullifier for double-spend prevention
nullifier = poseidon_hash(coin_id)
```

## Contract Functions

| Function | Code | Description |
|----------|------|-------------|
| `GenesisV1` | 0x00 | Create initial coin supply |
| `TransferV1` | 0x01 | Send coins (signature-based, no ZK) |
| `SpendV1` | 0x02 | Consume coins, create change |
| `MeltV1` | 0x03 | Destroy coins (fees) |

### GenesisV1 (0x00)

Creates the initial coin supply at blockchain start.

**Parameters:**
```rust
struct GenesisParamsV1 {
    coins: Vec<Coin>,  // Initial coins to create
}
```

**Validation:**
- Genesis can only happen once (checked via `info_db`)
- All coins must have `value >= 1`
- Total supply tracked in `info_db`

### TransferV1 (0x01)

Send coins to another party using **signature-based verification** (no ZK required).

**Parameters:**
```rust
struct TransferParamsV1 {
    inputs: Vec<Input>,   // Coins being spent
    outputs: Vec<Output>,  // Coins being created
    signature: pallas::Base, // Proof of ownership
}
```

**Validation:**
- Input count: 1-16 coins
- Output count: 1-16 coins
- Value balance: `sum(inputs) == sum(outputs)` (no value created/destroyed)
- Double-spend check: nullifiers not already in database
- Merkle proof verification (optional for baseline)

**Key Insight**: We don't need ZK proofs for simple transfers. In Bitcoin, you just need a signature to prove you own a UTXO. The same applies here.

### SpendV1 (0x02)

Consume a single coin and create a change output.

**Parameters:**
```rust
struct SpendParamsV1 {
    input: Input,              // Coin being spent
    change_output: Output,     // Remaining value after fee
    fee: u64,                  // Fee being paid
    signature: pallas::Base,  // Proof of ownership
}
```

### MeltV1 (0x03)

Destroy coins (e.g., for fees or voluntary supply reduction).

**Parameters:**
```rust
struct MeltParamsV1 {
    inputs: Vec<Input>,      // Coins being melted
    melt_amount: u64,        // Amount to destroy
    signature: pallas::Base,
}
```

## Comparison with MoneyV2

| Feature | MoneyV2 | SimpleCoin |
|---------|---------|------------|
| Value privacy | Pedersen commitments | Public values |
| Note encryption | AEAD encrypted | Plaintext |
| Token creation | AuthTokenMint + TokenMint | Direct mint |
| EC operations | `ec_mul_base` in circuits | None in baseline |
| ZK proofs | Required for all ops | Optional, signature-based |
| Transfer complexity | High | Minimal |

## Why No EC Operations?

The `ec_mul_base` operation in `auth_token_mint_v1.zk` was causing heap indexing bugs. But more fundamentally:

1. **EC multiplication is redundant for simple token ownership** - Signature verification already proves knowledge of the private key
2. **Expensive in ZK** - EC operations are costly in circuits
3. **Over-engineered** - For a baseline token, simple hashing suffices

### What EC Operations Were Used For

In money_v2, `ec_mul_base(mint_secret, NULLIFIER_K)` was used to:
- Derive a public key from a secret
- Verify the derived public key matches a provided value

**But this is what signatures already do!** When you sign a transaction, you're proving you know the private key corresponding to a public key.

## Database Trees

```
SIMPLECOIN_CONTRACT_COINS_TREE     - coin_id -> Coin
SIMPLECOIN_CONTRACT_NULLIFIERS_TREE - nullifier -> spent
SIMPLECOIN_CONTRACT_MERKLE_TREE    - Merkle tree of all coins
SIMPLECOIN_CONTRACT_INFO_TREE      - contract metadata
```

## Future: Privacy Layer

Once SimpleCoin works as a baseline, privacy can be layered:

1. **Pedersen Commitments** - Hide amounts
2. **Range Proofs** - Prove value is positive without revealing it
3. **ZK Proofs** - Prove balance without revealing inputs
4. **Encrypted Notes** - Hide coin contents

But start simple. Get the baseline working first.

## Files

- `src/contract/simplecoin/` - Contract implementation
  - `src/lib.rs` - Function enum, constants
  - `src/error.rs` - Error types
  - `src/model/mod.rs` - Data models
  - `src/entrypoint/mod.rs` - WASM entrypoint

## Testing

```bash
# Build simplecoin contract
cd src/contract/simplecoin && make all

# Run simple transfer test
cargo test --release --test simple_transfer

# Verify all operations
cargo test --release --test simplecoin
```

## Success Criteria

- [x] Simple UTXO token works with transfer/spend/melt
- [x] No EC multiplication in baseline circuits
- [x] No Pedersen commitments (values in clear)
- [x] No encrypted notes (public coin data)
- [x] Transfer test passes without complex ZK proofs
- [x] Double-spend protection via nullifiers works
- [ ] Privacy can be added as optional layer on top