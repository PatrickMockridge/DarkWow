# DarkFi Smart Contract Standards

## Overview

This document defines the standards for building smart contracts on DarkFi. These standards emerged from analyzing the existing contract ecosystem and addressing critical security concerns.

## Part 1: ZK Circuit Primitives - EC vs Poseidon

### The Problem with EC Operations

Elliptic curve (EC) operations in ZK circuits have caused critical heap bugs in DarkFi:

| Circuit | EC Operations | Status |
|---------|-------------|--------|
| Fee_V2 | ec_mul_base, ec_mul_short, ec_mul, ec_add | **BUGGY** |
| Mint_V2 | ec_mul_short, ec_mul, ec_add | **BUGGY** |
| Burn_V2 | ec_mul_base, ec_mul_short, ec_mul, ec_add | **BUGGY** |
| AuthTokenMint_V2 | ec_mul_base | **BUGGY** |
| TokenMint_V2 | None (Poseidon only) | **SAFE** |

### EC Operations Used in DarkFi Circuits

```zk
ec_mul_base(secret, generator)     // Derive public key from secret
ec_mul_short(value, generator)      // Pedersen commitment (value part)
ec_mul(blind, generator)            // Pedersen commitment (blind part)
ec_add(point1, point2)             // Add EC points
ec_get_x(point)                    // Extract X coordinate
ec_get_y(point)                    // Extract Y coordinate
```

### Cost-Benefit Analysis

| Aspect | EC Operations | Poseidon Hash |
|--------|---------------|---------------|
| **Heap bugs** | CRITICAL - Memory corruption in zkVM | **NONE** - Pure arithmetic |
| **Implementation complexity** | HIGH - 4x more code paths | **LOW** - Simple hash |
| **Homomorphic commitments** | YES - Can add C1 + C2 | **NO** - Not needed |
| **DarkFi usage** | Unnecessary | **Sufficient** |
| **Audit surface** | Large | **Minimal** |

### Why DarkFi Doesn't Need Homomorphic Commitments

DarkFi uses **burn-mint**, not **transfer-with-change**:

```
TRADITIONAL (Pedersen):
  Spend coin A → Receive coin B
  Need: C_change = C_input - C_output
  Prove: C_input = C_output + C_change

DARKFI (burn-mint):
  Burn coin A (emit nullifier)
  Mint coin B (new commitment)
  Value balance checked at contract layer
  No addition of commitments needed
```

### Standard: Poseidon-Only

**All internal DarkFi ZK circuits MUST use Poseidon-only design.**

```zk
// CORRECT: Poseidon-only circuit
circuit "ExampleV1" {
    // Public key: poseidon_hash(secret)
    pub = poseidon_hash(secret);
    constrain_instance(pub);

    // Commitment: poseidon_hash(value, blind)
    commitment = poseidon_hash(value, blind);
    constrain_instance(commitment);

    // Nullifier: poseidon_hash(secret, commitment)
    nullifier = poseidon_hash(secret, commitment);
    constrain_instance(nullifier);

    // Range check
    range_check(64, value);
}
```

**Exception**: External chain verification (Bitcoin, Ethereum signatures) MAY use EC, but internal DarkFi circuits must remain Poseidon-only.

---

## Part 2: Token Layer Architecture

### The Three Token Contracts

| Contract | Purpose | Authorization | Usage |
|----------|---------|--------------|-------|
| **NativeToken** | Consensus | None (open) | PoW rewards, network fees |
| **MoneyV3** | DeFi | AuthTokenMint | ERC-20 tokens, stablecoins |
| **MoneyV2** | DEPRECATED | ACL DAO | DO NOT USE |

### Token Decision Tree

```
Is this for consensus (mining rewards, fees)?
  └── YES → Use NativeToken
  └── NO → Is this a token for DeFi (stablecoin, wrapped)?
            └── YES → Use MoneyV3
            └── NO → This contract doesn't need token layer
```

### Fee Payment Model

| Fee Type | Token | Why |
|----------|-------|-----|
| **Network/miner fees** | NativeToken | Consensus-critical |
| **Protocol fees (DEX)** | MoneyV3 | Traded token |
| **Cross-chain fees** | Varies | Per bridge design |

---

## Part 3: Security - The Freeze Problem

### Critical: Why Native Token Must Never Be Governable

The combination of **AuthTokenMint** + **Weighted Governance DAO** creates a catastrophic attack vector.

### Attack Scenario: Plutocratic Freeze

```
1. Setup:
   - Token uses AuthTokenMint for minting authorization
   - Weighted DAO controls who can call AuthTokenMint

2. Attack:
   - Whales form coalition
   - Vote to freeze native token minting
   - Miners can't get paid → Consensus fails

3. Result:
   - Miner starvation
   - Consensus degradation
   - Centralization pressure
```

### Why This Breaks PoW

The native token serves **consensus-critical functions**:

```
Native Token (DARK):
├── Block rewards → Pays miners for PoW
├── Transaction fees → Incentivizes validators
└── Store of value → Network security budget
```

If governance can freeze minting:
- Miners may not be paid → PoW security degrades
- Validators can't collect fees → Consensus weakens
- The network becomes extortable

### Contrast: Voluntary vs Mandatory

| Token Type | Freeze OK? | Governance OK? | Why |
|------------|-----------|---------------|-----|
| **Native Token** | **NEVER** | **NEVER** | Consensus must be permissionless |
| **DeFi Token** | OK | OK | No consensus dependency |
| **Governance** | OK | OK | No block reward dependency |

### The Correct Architecture

```
┌──────────────────────────────────────────────────────────────┐
│                    NATIVE TOKEN (NativeToken)                  │
│                                                                  │
│  - NO governance                                               │
│  - NO AuthTokenMint                                            │
│  - NO freeze capability                                       │
│  - Block rewards → miners directly                            │
│  - Fees → validators directly                                 │
│                                                                  │
│  Principle: Consensus is paramount, everything else is secondary │
└──────────────────────────────────────────────────────────────┘
                              │
                              │ (Native token = fee payment only)
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                    DEFI TOKENS (MoneyV3)                     │
│                                                                  │
│  - AuthTokenMint: OK (token-specific)                        │
│  - Weighted DAO: OK (voluntary membership)                   │
│  - Freeze: OK (contained within token)                        │
│                                                                  │
│  Principle: DeFi experiments must not compromise consensus     │
└──────────────────────────────────────────────────────────────┘
```

### Identity Leakage via ACL DAOs

Access Control List (ACL) DAOs have fundamental privacy problems:

| Problem | Impact |
|---------|--------|
| **Identity revelation** | Must reveal public key to join |
| **Holdings visible** | Token balance is on-chain |
| **Vote traceability** | All votes public |
| **Targeting risk** | Can be punished for votes |

**This is acceptable for voluntary DeFi governance. It is NOT acceptable for consensus.**

---

## Part 4: Compositional Framework

### Cross-Contract Calls via spend_hook

DarkFi uses **spend_hook** for atomic cross-contract composition:

```rust
// Burning tokens triggers cross-contract call
BurnV1 {
    coin: Coin,
    spend_hook: CONTRACT_ID,  // Which contract to invoke
    user_data: PARAMS,       // Data passed to contract
}

// Burn succeeds ONLY if spend_hook contract's exec() succeeds
// Creates atomic cross-contract transactions
```

### Standard Contract Structure

```
src/contract/{name}/
├── src/
│   ├── lib.rs              # Function enum, constants
│   ├── entrypoint.rs        # WASM entrypoint
│   ├── model/
│   │   └── mod.rs          # Data types
│   ├── client/
│   │   ├── mod.rs          # Client APIs
│   │   └── func_v1.rs      # Proof generation
│   └── error.rs
├── proof/
│   ├── func_v1.zk          # ZK circuits (Poseidon-only!)
│   └── func_v2.zk
├── tests/
│   └── integration.rs
└── Cargo.toml
```

### Database Tree Standards

| Tree | Purpose | Access |
|------|---------|--------|
| `{CONTRACT}_INFO_TREE` | Config, version | Read-only after init |
| `{CONTRACT}_{OBJECT}_TREE` | Main storage | Read/write |
| `{CONTRACT}_NULLIFIERS_TREE` | Spent proofs | Write-only |
| `{CONTRACT}_ROOTS_TREE` | Historical Merkle roots | Append-only |

---

## Part 5: Testing Standards

### Four Testing Layers

```
┌─────────────────────────────────────────┐
│  LAYER 4: Integration                    │
│  Full blockchain, multi-contract         │
└─────────────────────────────────────────┘
                    ↑
┌─────────────────────────────────────────┐
│  LAYER 3: Composition                   │
│  Cross-contract calls, mock externals    │
└─────────────────────────────────────────┘
                    ↑
┌─────────────────────────────────────────┐
│  LAYER 2: Contract                      │
│  Isolated, mock state                    │
└─────────────────────────────────────────┘
                    ↑
┌─────────────────────────────────────────┐
│  LAYER 1: Circuit                       │
│  Unit tests, deterministic               │
└─────────────────────────────────────────┘
```

### Deterministic Testing Requirement

```rust
// ALL tests MUST use fixed seeds
const TEST_SEED: [u8; 32] = [
    0xDE, 0xAD, 0xBE, 0xEF, /* ... */
];

#[test]
fn test_deterministic() {
    let mut rng = StdRng::from_seed(TEST_SEED);
    let value = pallas::Base::random(&mut rng);
    // Reproducible across runs
}
```

### Layer 1: Circuit Tests

```rust
#[test]
fn test_mint_public_inputs() {
    let public_inputs = MintCallData { ... }.compute_public_inputs();
    assert_eq!(public_inputs.len(), EXPECTED_COUNT);
    assert_eq!(public_inputs.coin, expected_hash);
}

#[test]
fn test_nullifier_deterministic() {
    let n1 = poseidon_hash([secret, coin]);
    let n2 = poseidon_hash([secret, coin]);
    assert_eq!(n1, n2);
}
```

### Layer 2: Contract Isolation

```rust
#[test]
fn test_mint_stablecoin() {
    let money_v3 = spawn_money_v3();
    let stablecoin = spawn_stablecoin(money_v3.root());

    let token_id = stablecoin.create_token(...).unwrap();
    let proof = stablecoin.mint(token_id, recipient, 1000).unwrap();

    assert!(verify_proof(&proof, stablecoin.vk()));
}
```

### Layer 3: Composition

```rust
#[test]
fn test_dex_swap_with_fee() {
    let money_v3 = spawn_money_v3();
    let dex = spawn_dex();

    // Atomic swap using MoneyV3 tokens
    let result = dex.execute_swap(swap_id, alice_coin, bob_coin);
    assert!(result.is_ok());
}
```

---

## Summary: Standards Checklist

| Standard | Requirement | Rationale |
|----------|-------------|----------|
| **ZK Primitive** | Poseidon-only internally | Zero heap bugs |
| **External Chain** | EC allowed | Different security model |
| **Token (consensus)** | NativeToken, no governance | Permissionless PoW |
| **Token (DeFi)** | MoneyV3, with governance OK | Voluntary experiments |
| **Fee (network)** | NativeToken | Consensus-critical |
| **Fee (protocol)** | MoneyV3 token | Tradeable |
| **Testing** | 4 layers, deterministic | Reproducibility |

---

## Security Principles

1. **Native token is sovereign** - No governance, no freeze, no ACL
2. **DeFi can experiment** - MoneyV3 allows governance within contained token
3. **Consensus first** - All design decisions prioritize PoW security
4. **Privacy by default** - ACL DAOs leak identity; avoid for consensus
5. **Poseidon everywhere** - Unless EC required for external chain integration