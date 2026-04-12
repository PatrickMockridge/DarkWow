# NativeToken: Consensus-First Native Token

## Overview

NativeToken is the primary native token contract for DarkFi, designed with a **CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD** philosophy.

**Key Principle**: The native token must be reliable for consensus before anything else. Privacy is layered on top, never compromising block rewards or fee payment.

## Why NativeToken?

The original MoneyV2 had significant issues:
- Tight coupling to DAO via ACL-based authorization
- Complex multi-step token authorization (AuthTokenMint + TokenMint)
- EC operations in circuits led to heap bugs
- Genesis minting tied to governance parameters

NativeToken solves these by being:
1. **DAO-Decoupled**: No ACL, no governance coupling
2. **Simple Genesis**: Single GenesisMintV1 call at startup
3. **Minimal Circuits**: Only essential ZK operations
4. **Consensus-First**: Block rewards and fees are paramount

## Design Philosophy: CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD

This design philosophy prioritizes the core functions of a blockchain native token:

1. **Consensus Reward** - Block rewards for PoW mining must be reliable
2. **Network Fees** - Transaction fee payment must be deterministic
3. **Privacy Layer** - Privacy on top, never compromising consensus

## Token Model

### Coin Structure

```rust
struct Coin {
    inner: pallas::Base,  // Hash of coin attributes
}

struct CoinAttributes {
    public_key: PublicKey,
    value: u64,
    token_id: pallas::Base,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    blind: pallas::Base,
}
```

**Coin = poseidon_hash(pub_x, pub_y, value, token_id, spend_hook, user_data, blind)**

### Nullifier

```rust
// Nullifier for double-spend prevention
nullifier = poseidon_hash(coin_secret, coin)
```

## Contract Functions

| Function | Opcode | Purpose | Priority |
|----------|--------|---------|----------|
| FeeV1 | 0x00 | Pay network fees | CONSENSUS |
| GenesisMintV1 | 0x01 | Create initial supply | CONSENSUS |
| PoWRewardV1 | 0x02 | Block rewards | CONSENSUS |
| TransferV1 | 0x03 | Private transfers | PRIVACY |
| SpendV1 | 0x04 | Spend with change | PRIVACY |
| MeltV1 | 0x05 | Destroy coins | PRIVACY |

### FeeV1 (0x00)

Pays network fees using the native token. This is CONSENSUS CRITICAL - fee payment must always work.

**Parameters:**
```rust
struct FeeParamsV1 {
    input: Input,           // Coin being spent for fee
    output: Output,         // Change output
    fee_value_blind: Scalar,
    fee_token_blind: Base,
}
```

### GenesisMintV1 (0x01)

Creates the initial coin supply at blockchain start. CONSENSUS CRITICAL - happens exactly once.

**Parameters:**
```rust
struct GenesisMintParamsV1 {
    input: ClearInput,      // Clear input (no privacy needed for genesis)
    outputs: Vec<Output>,   // Anonymous outputs
}
```

**Validation:**
- Genesis can only happen once (checked via `info_db`)
- All outputs must have valid coin commitments
- Total supply tracked in `info_db`

### PoWRewardV1 (0x02)

Distributes block rewards to miners. CONSENSUS CRITICAL - this is how mining is incentivized.

**Parameters:**
```rust
struct PoWRewardParamsV1 {
    input: ClearInput,      // Clear input for reward amount
    output: Output,        // Coin to reward
}
```

### TransferV1 (0x03)

Private token transfer between parties. PRIVACY layer.

**Parameters:**
```rust
struct TransferParamsV1 {
    inputs: Vec<Input>,     // Coins being spent
    outputs: Vec<Output>,    // Coins being created
}
```

**Validation:**
- Input count: 1-16 coins
- Output count: 1-16 coins
- Double-spend check: nullifiers not already in database
- Merkle proof verification

### SpendV1 (0x04)

Consume a single coin and create a change output.

**Parameters:**
```rust
struct SpendParamsV1 {
    input: Input,           // Coin being spent
    output: Output,         // Change output
}
```

### MeltV1 (0x05)

Destroy coins (e.g., for fees or voluntary supply reduction).

**Parameters:**
```rust
struct MeltParamsV1 {
    inputs: Vec<Input>,     // Coins being melted
}
```

## Key Differences from MoneyV2

| Feature | MoneyV2 | NativeToken |
|---------|---------|-------------|
| Design | Mixed priorities | Consensus-first |
| DAO Coupling | Tight via ACL | Fully decoupled |
| Genesis Mint | Complex multi-step | Single GenesisMintV1 |
| EC Operations | Required in circuits | Minimal |
| Authorization | ACL Merkle proofs | ZK predicates |

## Decoupled from DAO

NativeToken does NOT require DAO for:
- Block reward distribution
- Fee payment
- Token transfers
- Genesis minting

DAO can optionally use NativeToken for governance token, but NativeToken is standalone.

## ZK Circuits

| Circuit | Namespace | Purpose |
|---------|-----------|---------|
| mint_v1.zk | `Mint_V1` | Genesis mint and PoW rewards |
| burn_v1.zk | `Burn_V1` | Spending with nullifier |
| fee_v1.zk | `Fee_V1` | Fee payment |

### Circuit Design Principles

The ZK circuits follow strict security principles:

1. **constrain_equal_base binding**: Public key derivation is bound to public inputs
2. **Range proofs**: Value overflow prevention
3. **Merkle proofs**: Coin existence verification
4. **Nullifier proofs**: Double-spend prevention

## Database Trees

```
NATIVE_TOKEN_CONTRACT_COINS_TREE          - coin -> ()
NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE     - nullifier -> spent
NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE     - historical Merkle roots
NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE - historical nullifier roots
NATIVE_TOKEN_CONTRACT_INFO_TREE           - contract metadata
```

## Files

- `src/contract/native_token/` - Contract implementation
  - `src/lib.rs` - Function enum, constants
  - `src/error.rs` - Error types
  - `src/model/mod.rs` - Data models (Coin, Input, Output, etc.)
  - `src/model/nullifier.rs` - Nullifier type
  - `src/entrypoint/mod.rs` - WASM entrypoint
  - `proof/*.zk` - ZK circuit source
  - `proof/*.zk.bin` - Compiled circuit binaries

## Testing

```bash
# Build native_token contract
cd src/contract/native_token && make all

# Run integration tests
cargo test -p darkfi_native_token_contract --test integration

# Run with test harness
cargo test -p darkfi_contract_test_harness -- native_token
```

## Success Criteria

- [x] Consensus-first design (fees, rewards work reliably)
- [x] DAO-decoupled (no ACL dependencies)
- [x] Simple genesis mint
- [x] Private transfers via TransferV1
- [x] Block rewards via PoWRewardV1
- [x] Fee payment via FeeV1
- [x] ZK circuits with constrain_equal_base binding