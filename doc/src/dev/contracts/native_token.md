# NativeToken: Z-Cash Style Burn-Mint Native Token

## Overview

NativeToken is DarkWow's native token contract for consensus (block rewards, fees), implementing a **Z-cash style burn-mint privacy model**. Unlike MoneyV2 (which has token freezing), NativeToken has **no token freezing** capability, eliminating freeze-key attack vectors while providing full privacy for transfers.

**Key Principle**: The native token must be reliable for consensus before anything else. Privacy is layered on top, never compromising block rewards or fee payment.

> [!NOTE]
> **Design Philosophy: Minimal Viable Circuits, Maximum Reliability**
>
> NativeToken follows a "do one thing well" philosophy:
>
> - **Minimum viable ZK circuits** - Only what's strictly necessary for consensus
> - **Tokens are infrastructure** - Simple value movement, no business logic
> - **Smart contracts own complexity** - DEX, stablecoin, etc. handle business logic
> - **Permissionless deployment** - Anyone can deploy their own token contracts with custom logic
>
> This is a process safety principle: isolate complexity where it's required, not in the most frequently-called code.

> [!IMPORTANT]
> **For custom token logic**: NativeToken provides the consensus layer. For custom token contracts with different business logic, deployment is permissionless - deploy your own token contract. DarkWow's architecture encourages innovation at the smart contract layer while keeping the base token layer stable and minimal.

## Why NativeToken?

The original MoneyV2 had significant issues:
- Tight coupling to DAO via ACL-based authorization
- Complex multi-step token authorization (AuthTokenMint + TokenMint)
- EC operations in circuits led to heap bugs
- Genesis minting tied to governance parameters
- Token freezing capabilities introduced attack vectors

NativeToken solves these by being:
1. **DAO-Decoupled**: No ACL, no governance coupling
2. **Simple Genesis**: Single GenesisMintV1 call at startup
3. **Minimal Circuits**: Only essential ZK operations
4. **Consensus-First**: Block rewards and fees are paramount
5. **No Token Freezing**: Eliminated freeze-key attack vectors entirely

> [!NOTE]
> **For DeFi functionality (tokens, stablecoins, wrapped assets), see [MoneyV3](./money_v3.md)** - the privacy-first ERC-20 style contract with zero EC operations and 100% fungible tokens.

## Design Philosophy: CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD

This design philosophy prioritizes the core functions of a blockchain native token:

1. **Consensus Reward** - Block rewards for PoW mining must be reliable
2. **Network Fees** - Transaction fee payment must be deterministic
3. **Privacy Layer** - Privacy on top, never compromising consensus

## Z-Cash Style Burn-Mint Model

NativeToken implements a Z-cash style privacy model:

| Operation | Z-Cash Analogy | Description |
|-----------|----------------|-------------|
| **MintV1** | transparent-to-private | Creates new coins with Pedersen commitments |
| **BurnV1** | 0x02 | Destroy coins with nullifier |
| **TransferV1** | private-to-private | Private transfers between parties |

### Key Privacy Properties

- **Mint**: Creates coins privately - value hidden via Pedersen commitment
- **Burn**: Destroys coins with nullifier - enables private coin destruction
- **Transfer**: Private sends between parties

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

Nullifiers prevent double-spending by hashing the spending key with the coin hash.

## Contract Functions

| Function | Opcode | Purpose | Priority |
|----------|--------|---------|----------|
| FeeV1 | 0x00 | Pay network fees | CONSENSUS |
| MintV1 | 0x01 | Create new coins | PRIVACY |
| BurnV1 | 0x02 | Destroy coins with nullifier | PRIVACY |
| TransferV1 | 0x03 | Private transfers | PRIVACY |
| SpendV1 | 0x04 | Spend coins with change output | PRIVACY |
| PoWRewardV1 | 0x05 | Block rewards | CONSENSUS |

> [!NOTE]
> **NativeToken vs MoneyV3**: NativeToken handles **consensus functions** (PoW rewards, network fees). For **DeFi functions** (ERC-20 tokens, stablecoins, wrapped assets), use [MoneyV3](./money_v3.md).

### Function Demarcation

| Use Case | Contract | Functions |
|----------|----------|-----------|
| PoW Mining Rewards | NativeToken | PoWRewardV1 |
| Network Fees | NativeToken | FeeV1 |
| Genesis Minting | NativeToken | MintV1 |
| Token Transfers (native) | NativeToken | TransferV1 |
| **Create Token Types** | **MoneyV3** | **TokenMintV1** |
| **Authorize Minting** | **MoneyV3** | **AuthTokenMintV1** |
| **Mint Tokens (ERC-20)** | **MoneyV3** | **MintV1** |
| **Burn Tokens** | **MoneyV3** | **BurnV1** |
| **Token Transfers (DeFi)** | **MoneyV3** | **TransferV1** |

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

### MintV1 (0x01)

Creates new coins with Pedersen commitments. Used for genesis minting and general coin creation.

**Parameters:**
```rust
struct MintParamsV1 {
    coin: Coin,      // The newly minted coin
}
```

**ZK Circuit:** `mint_v1.zk`

### BurnV1 (0x02)

Destroys coins with nullifier generation for double-spend prevention.

**Parameters:**
```rust
struct BurnParamsV1 {
    inputs: Vec<Input>,     // Coins being burned
}
```

**Validation:**
- Nullifier not already spent
- Merkle proof verification for each input
- ZK proof of burn

**ZK Circuit:** `burn_v1.zk`

**Current Status:** Contract-side logic and client API (`BurnCallBuilder` in `src/contract/native_token/src/client/burn_v1.rs`) fully implemented.

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

### PoWRewardV1 (0x05)

Distributes block rewards to miners. CONSENSUS CRITICAL - this is how mining is incentivized.

**Parameters:**
```rust
struct PoWRewardParamsV1 {
    input: ClearInput,      // Clear input for reward amount
    output: Output,        // Coin to reward
}
```

**ZK Circuit:** `mint_v1.zk` (reused for mint operation)

## Key Differences from MoneyV2

| Feature | MoneyV2 | NativeToken |
|---------|---------|-------------|
| Design | Mixed priorities | Consensus-first |
| DAO Coupling | Tight via ACL | Fully decoupled |
| Genesis Mint | Complex multi-step | Single GenesisMintV1 |
| EC Operations | Required in circuits | Minimal |
| Authorization | ACL Merkle proofs | ZK predicates |
| Token Freezing | Yes | **No** (eliminated) |

## No Token Freezing

Unlike MoneyV2, NativeToken has **no token freezing capability**. This was an intentional design decision to:

- **Simplify the security model** - Fewer attack vectors
- **Remove freeze-key complexity** - No freeze authority management
- **Enable true decentralization** - No single party can freeze tokens

### Sacrifices

By removing token freezing:
- No on-chain regulatory controls
- No freeze authority to manage
- Enables true permissionless operation

## The EC Heap Bug in MoneyV2

**Critical Issue**: MoneyV2 circuits contain EC operations that caused heap memory corruption bugs.

### The Bug

MoneyV2's circuits use elliptic curve operations:
- `ec_mul_base(secret, NULLIFIER_K)` - Deriving public keys
- `ec_mul_short(value, VALUE_COMMIT_VALUE)` - Pedersen commitments
- `ec_mul(blind, VALUE_COMMIT_RANDOM)` - Commitment blinding
- `ec_add(vcv, vcr)` - Combining commitment parts

These EC operations were implemented incorrectly in the halo2 stack, leading to heap corruption when processing certain inputs. The bug manifested as:

```
heap buffer overflow
ec_mul_base operation corrupted heap state
```

### Affected Circuits in MoneyV2

| Circuit | EC Operations | Status |
|---------|---------------|--------|
| `fee_v1.zk` (Fee_V2) | ec_mul_base, ec_mul_short, ec_mul, ec_add | **BUGGY** |
| `mint_v1.zk` (Mint_V2) | ec_mul_short, ec_mul, ec_add | **BUGGY** |
| `burn_v1.zk` (Burn_V2) | ec_mul_base, ec_mul_short, ec_mul, ec_add | **BUGGY** |
| `auth_token_mint_v1.zk` | ec_mul_base | **BUGGY** |
| `token_mint_v1.zk` | None (uses Poseidon only) | **SAFE** |

### Why NativeToken Still Uses EC

Unfortunately, NativeToken's circuits (`mint_v1.zk`, `burn_v1.zk`, `fee_v1.zk`) also use the same EC operations - they have the same heap bug vulnerability.

The difference is that NativeToken prioritizes consensus reliability over privacy circuit safety. The EC operations are consensus-critical for value commitments, and NativeToken's simpler design makes it easier to work around known issues.

### The Solution: MoneyV3 (Poseidon-Only)

MoneyV3 (see `src/contract/money_v3/`) implements a **complete Poseidon-only** design:

- **Zero EC operations** - No heap bug possible
- **Value commitment**: `poseidon_hash(value, blind)` instead of Pedersen
- **Public key**: `poseidon_hash(secret)` instead of `ec_mul_base`
- **100% fungible** - Token ID is a hidden commitment

The trade-off is losing the homomorphic property of Pedersen commitments, but DarkWow's transaction validation doesn't actually use homomorphic addition of commitments, so this is acceptable.

### Security Comparison

| Aspect | Money V2 | NativeToken | Money V3 |
|--------|----------|-------------|----------|
| EC operations | 4 circuits | 3 circuits | 0 (none!) |
| Heap bug risk | YES | YES | NO |
| Value commitment | Pedersen (EC) | Pedersen (EC) | Poseidon hash |
| Public key | ec_mul_base | ec_mul_base | poseidon_hash |
| Token ID privacy | Revealed | Revealed | Hidden commitment |

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
| mint_v1.zk | `Mint_V1` | Mint and PoW rewards |
| burn_v1.zk | `Burn_V1` | Burning with nullifier |
| fee_v1.zk | `Fee_V1` | Fee payment |

### Circuit Design Principles

The ZK circuits follow strict security principles:

1. **constrain_equal_base binding**: Public key derivation is bound to public inputs
2. **Range proofs**: Value overflow prevention
3. **Merkle proofs**: Coin existence verification
4. **Nullifier proofs**: Double-spend prevention
5. **No EC heap operations**: Avoiding the heap bugs in MoneyV2

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
  - `src/client/` - Client API (burn_v1, pow_reward_v1, transfer_v1)
  - `proof/*.zk` - ZK circuit source
  - `proof/*.zk.bin` - Compiled circuit binaries
  - `docs/engineering_spec.md` - Detailed engineering specification

## See Also

- [MoneyV3](./money_v3.md) - Privacy-first DeFi token contract for ERC-20 style tokens, stablecoins, and wrapped assets
- [Money V2 Deprecation Notice](./money_v2.md) - Migration path from legacy MoneyV2

## Testing

```bash
# Build native_token contract
cargo build -p darkfi_native_token_contract

# Run test harness
cargo run -p dwow-contract-test-harness --bin test_native_token
```

**Test Status:**
- [x] MintV1 test passes
- [x] PoWRewardCallBuilder generates real ZK proofs
- [ ] BurnV1 client API (stubbed - pending Merkle infrastructure)

## Success Criteria

- [x] Consensus-first design (fees, rewards work reliably)
- [x] DAO-decoupled (no ACL dependencies)
- [x] Simple genesis mint
- [x] Private transfers via TransferV1
- [x] Block rewards via PoWRewardV1
- [x] Fee payment via FeeV1
- [x] ZK circuits with constrain_equal_base binding
- [x] No token freezing capability
