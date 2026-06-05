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

## Architecture Decision: Why Two Token Contracts

DarkWow deliberately splits token functionality across two contracts. This is a **key differentiator from upstream** (which bakes consensus, governance, and DeFi logic into a single monolithic token), and reflects a specific security philosophy:

### NativeToken: Security by Minimum Functionality

NativeToken is **deliberately dumb**. It does exactly three things: pay block rewards, collect fees, and transfer value. Nothing else.

| What it does | What it doesn't do |
|---|---|
| PoW block rewards (PoWRewardV1) | No token freezing |
| Network fee payment (FeeV1) | No governance coupling |
| Private transfers (Mint/Burn/Transfer) | No multi-token support |
| | No auth mint |
| | No token registry |
| | No DeFi business logic |

The principle: **every feature you don't add is a vulnerability you don't create**. NativeToken is the most frequently called contract in the system — a bug here cascades to every transaction. By keeping it minimal, we minimize the blast radius.

### PromissoryNote: Minimum Viable Business Logic for DeFi

PromissoryNote carries exactly the business logic that DeFi contracts need to compose: multi-token support, authorization, and cross-contract value verification.

| What it adds | Why |
|---|---|
| TokenMintV1 | Permissionless token creation for stablecoins, wrapped assets, LP tokens |
| Multi-token support (token_id) | DEX, lending, yield — all need multiple token types |
| BlindOutput_V1 ZK circuit | Proves all output coins are correctly formed (fully private) |
| validate_child_value_commit | Helper for parent contracts to verify child call amounts via commitment comparison |

PromissoryNote is still minimal by DeFi standards — no AMM logic, no lending pools, no governance. Those belong in their own contracts. PromissoryNote provides only the token-layer primitives that DeFi composition requires.

### Why Not One Contract?

Upstream projects typically merge these concerns into one token contract. DarkWow separates them because:

1. **Failure isolation**: A bug in DeFi token logic (PromissoryNote) cannot break consensus (NativeToken). Mining rewards and fees keep flowing regardless.
2. **Attack surface**: Consensus tokens need maximum security; DeFi tokens need flexibility. One contract can't optimize for both.
3. **Upgrade independence**: The consensus token can remain frozen while DeFi tokens evolve.
4. **Process safety**: Developers working on DeFi features don't touch consensus-critical code.

```
┌──────────────────────────────────────┐
│           NativeToken                 │
│  "Dumb money" — consensus only       │
│  PoW rewards, fees, transfers        │
│  MINIMAL by design                    │
│  No freezing, no auth, no registry   │
└──────────────────────────────────────┘
              ▲
              │ block rewards, fees
              │
┌─────────────┴────────────────────────┐
│           PromissoryNote                     │
│  "Smart money" — DeFi composition     │
│  Multi-token, auth mint, public vals │
│  MINIMAL VIABLE for DeFi              │
│  No AMM, no lending, no governance   │
└──────────────────────────────────────┘
              ▲
              │ token operations
              │
┌─────────────┴────────────────────────┐
│     DeFi Contracts (DEX, Bridge...)   │
│  Business logic lives here           │
└──────────────────────────────────────┘
```

## Why NativeToken?

The original MoneyV2 had significant issues:
- Tight coupling to DAO via ACL-based authorization
- Complex multi-step token authorization
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
> **For DeFi functionality (tokens, stablecoins, wrapped assets), see [PromissoryNote](./promissory_note.md)** - the privacy-first ERC-20 style contract with zero EC operations and 100% fungible tokens.

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
| MintV1 | 0x01 | ~~Create new coins~~ (DISABLED — unauthorized mint path, see safety.md Lesson 16) | REMOVED |
| BurnV1 | 0x02 | Destroy coins with nullifier | PRIVACY |
| TransferV1 | 0x03 | Private transfers | PRIVACY |
| SpendV1 | 0x04 | Spend coins with change output | PRIVACY |
| PoWRewardV1 | 0x05 | Block rewards | CONSENSUS |

> [!NOTE]
> **NativeToken vs PromissoryNote**: NativeToken handles **consensus functions** (PoW rewards, network fees). For **DeFi functions** (ERC-20 tokens, stablecoins, wrapped assets), use [PromissoryNote](./promissory_note.md).

### Function Demarcation

| Use Case | Contract | Functions |
|----------|----------|-----------|
| PoW Mining Rewards | NativeToken | PoWRewardV1 |
| Network Fees | NativeToken | FeeV1 |
| Genesis Minting | NativeToken | MintV1 |
| Token Transfers (native) | NativeToken | TransferV1 |
| **Create Token Types** | **PromissoryNote** | **TokenMintV1** |
| **Mint Tokens (ERC-20)** | **PromissoryNote** | **MintV1** |
| **Burn Tokens** | **PromissoryNote** | **BurnV1** |
| **Token Transfers (DeFi)** | **PromissoryNote** | **TransferV1** |

### FeeV1 (0x00)

Pays network fees using the native token. This is CONSENSUS CRITICAL - fee payment must always work.

**Circuit hardening (2026-06-05):** The ZK circuit now constrains `output_value + fee == input_value`,
preventing inflation via fee payments. The `fee` amount is exposed as a ZK public input and
verified against the transaction's declared fee. See [safety.md Lesson 17](../dev/contracts/safety.md#lesson-17-off-circuit-value-conservation--the-fee-inflation-vector).

**Parameters:**
```rust
struct FeeParamsV1 {
    input: Input,           // Coin being spent for fee
    output: Output,         // Change output
    fee_value_blind: Scalar,
    fee_token_blind: Base,
}
```

### MintV1 (0x01) — DISABLED

> [!WARNING]
> **MintV1 is disabled as of 2026-06-05.** This function accepted any valid ZK proof without
> authority check or supply tracking, creating an unbounded mint path parallel to PoWRewardV1's
> emission schedule enforcement. It has been removed from all three dispatch tables
> (metadata, exec, apply). The opcode 0x01 is reserved. Use PoWRewardV1 for block rewards.
> See [safety.md Lesson 16](../dev/contracts/safety.md#lesson-16-unconstrained-zk-witnesses--the-mint-authorization-bypass).

**Parameters (historical):**
```rust
struct MintParamsV1 {
    coin: Coin,      // The newly minted coin
}
```

**ZK Circuit:** `mint_v1.zk` (retained — used internally by PoWRewardV1)

### BurnV1 (0x02)

Destroys coins with nullifier generation for double-spend prevention.

**Circuit hardening (2026-06-05):** The independent `signature_secret` witness is now
cryptographically bound to `coin_secret` via in-circuit derivation:
`signature_secret = poseidon_hash(coin_secret, nullifier)`. This fixes the
coin-owner/transaction-signer separation attack while preserving privacy — each burn
has a different `nullifier`, producing a different `signature_secret` and therefore
a different `signature_public`, keeping burns unlinkable.
See [safety.md Lesson 18](../dev/contracts/safety.md#lesson-18-independent-witness-separation--the-coin-ownertransaction-signer-split).

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
- **Cross-proof value conservation**: sum(input Pedersen value_commits) == sum(output Pedersen value_commits) per token_commit (added 2026-06-05, see safety.md Lesson 17)

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
| `token_mint_v1.zk` | None (uses Poseidon only) | **SAFE** |

### Why NativeToken Still Uses EC

Unfortunately, NativeToken's circuits (`mint_v1.zk`, `burn_v1.zk`, `fee_v1.zk`) also use the same EC operations - they have the same heap bug vulnerability.

The difference is that NativeToken prioritizes consensus reliability over privacy circuit safety. The EC operations are consensus-critical for value commitments, and NativeToken's simpler design makes it easier to work around known issues.

### The Solution: PromissoryNote (Poseidon-Only)

PromissoryNote (see `src/contract/promissory_note/`) implements a **complete Poseidon-only** design:

- **Zero EC operations** - No heap bug possible
- **Value commitment**: `poseidon_hash(value, blind)` instead of Pedersen
- **Public key**: `poseidon_hash(secret)` instead of `ec_mul_base`
- **100% fungible** - Token ID is a hidden commitment

The trade-off is losing the homomorphic property of Pedersen commitments, but DarkWow's transaction validation doesn't actually use homomorphic addition of commitments, so this is acceptable.

### Security Comparison

| Aspect | Money V2 | NativeToken | Promissory Note |
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

- [PromissoryNote](./promissory_note.md) - Privacy-first DeFi token contract for ERC-20 style tokens, stablecoins, and wrapped assets
- Money V2 Deprecation Notice — Migration path from legacy MoneyV2 (planned)

## Testing

Native Token is tested via `cargo run -p dwow-contract-test-harness --bin test_native_token`.
See [Testing Overview](../testing/overview.md) for the four-level testing taxonomy
and command reference.

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
