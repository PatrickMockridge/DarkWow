# Composability & General Primitives

*This document describes common patterns and primitives that appear across DarkFi smart contracts, enabling composition and interoperability.*

## The Problem: Contract-Specific Reasoning

Most smart contract designs reason about each contract in isolation:

```
Contract A: "handles token transfers"
Contract B: "handles identity"
Contract C: "handles DEX swaps"
```

**This approach fails** when contracts need to compose with each other. A DAO might need to verify identity credentials before allowing governance participation. A DEX might need to verify token balances from the Money contract. A bridge might need to interact with multiple token standards.

Without a common framework for reasoning about composability, each contract reinvents the same patterns.

## The Solution: General Primitive Categories

DarkFi contracts share three categories of general primitives:

| Category | Purpose | Appears In |
|----------|---------|------------|
| **State Primitives** | How contracts represent and store data | All contracts |
| **Authorization Primitives** | How contracts verify authority | All contracts |
| **Interaction Primitives** | How contracts communicate and compose | Cross-contract calls |

### State Primitives

All DarkFi contracts must represent state. The common patterns are:

```
┌─────────────────────────────────────────────────────────────────┐
│                    State Primitive Patterns                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. BINARY ACCUMULATOR                                           │
│     Represents: "element exists in set"                           │
│     Used in: Merkle trees, Bloom filters, RSA accumulators       │
│                                                                   │
│     ┌─────────┐         ┌─────────────┐                          │
│     │ Element │ ──────→ │   Merkle    │                          │
│     └─────────┘         │     Root    │                          │
│                         └─────────────┘                          │
│     Proof: "element is in set" without revealing element         │
│                                                                   │
│  2. INTERVAL TREE                                                │
│     Represents: "value exists in range"                           │
│     Used in: Balance ranges, time windows, credential expiration │
│                                                                   │
│     ┌─────────┐         ┌─────────────┐                          │
│     │  Value  │ ──────→ │ Interval    │                          │
│     └─────────┘         │    Tree     │                          │
│                         └─────────────┘                          │
│     Proof: "value is within bounds" without revealing value      │
│                                                                   │
│  3. HASH CHAIN                                                   │
│     Represents: "sequence of events in order"                    │
│     Used in: Transaction history, credential issuance order       │
│                                                                   │
│     ┌─────┐ → ┌─────┐ → ┌─────┐ → ┌─────┐                       │
│     │Event│   │Event│   │Event│   │Event│                       │
│     └─────┘   └─────┘   └─────┘   └─────┘                       │
│       │         │         │         │                            │
│       ▼         ▼         ▼         ▼                            │
│     H(0)      H(1)      H(2)      H(3)                           │
│                                                                   │
│     Proof: "event i happened before event j"                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Authorization Primitives

As detailed in [Private Authorization Layer](privauth.md), all DarkFi contracts share the same authorization pattern:

```
┌─────────────────────────────────────────────────────────────────┐
│              Shared Authorization Pattern                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  COMMITMENT          NULLIFIER           PROOF                   │
│  H(secret, params)  H(secret, ...)      ZK(prover knows secret) │
│       │                  │                    │                   │
│       │                  │                    │                   │
│       ▼                  ▼                    ▼                   │
│  ┌─────────────────────────────────────────────────────┐         │
│  │              Authorization Check                       │         │
│  │  • Commitment exists and is valid                     │         │
│  │  • Nullifier has not been spent                       │         │
│  │  • Proof verifies predicate without revealing secret   │         │
│  └─────────────────────────────────────────────────────┘         │
│                                                                   │
│  KEY INSIGHT: Every contract uses the same pattern.              │
│               Only the predicate changes.                         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Interaction Primitives

Cross-contract composition follows specific patterns:

```
┌─────────────────────────────────────────────────────────────────┐
│              Cross-Contract Interaction Patterns                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. CALL-CHAIN                                                   │
│     Contract A → Contract B → Contract C                          │
│                                                                   │
│     Problem: How does C verify A's authorization?                │
│                                                                   │
│     Solution: Pass proof along call chain                         │
│     A produces proof, B validates and forwards, C trusts B        │
│                                                                   │
│  2. STATE DEPENDENCY                                             │
│     Contract A reads state from Contract B                        │
│                                                                   │
│     Problem: How does A know B's state is valid?                  │
│                                                                   │
│     Solution: Merkle proofs + consensus verification              │
│     A verifies B's state hash is in consensus                    │
│                                                                   │
│  3. TOKEN TRANSFER                                               │
│     Contract A sends tokens to Contract B                        │
│                                                                   │
│     Problem: How do we prevent double-spending?                   │
│                                                                   │
│     Solution: Atomic transactions with dependent operations      │
│     Both operations in same transaction, all or nothing          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Cross-Contract Composability Matrix

| Caller → | Money | DAO | Bridge | DEX | Identity | Stablecoin |
|----------|-------|-----|--------|-----|----------|------------|
| **Money** | - | Token transfers | Token escrow | Swap settlement | Token-gated access | Collateral deposits |
| **DAO** | Treasury management | - | Governance of bridge | Governance of DEX | Credential-gated voting | Governance of stability |
| **Bridge** | Cross-chain transfers | Relayer rewards | - | Liquidity provision | Identity verification | Liquidity provision |
| **DEX** | Swap execution | Fee distribution | - | - | Credential-gated pools | Liquidity pools |
| **Identity** | Token-gated access | Credential-gated voting | Identity attestation | Credential-gated trading | - | Credit scoring |
| **Stablecoin** | Collateral management | Stability pool governance | Collateral rebalancing | - | - | - |

## General Primitive Composition Patterns

### Pattern 1: Token-Gated Access

```
┌─────────────────────────────────────────────────────────────────┐
│                 Token-Gated Access                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: User holds >= N tokens                            │
│                                                                   │
│  Implementation:                                                 │
│  1. User creates commitment: H(balance_secret, token, amount)    │
│  2. User generates ZK proof: "I know secret such that             │
│     commitment = H(secret, token, amount) AND amount >= N"      │
│  3. Contract verifies: commitment exists, proof valid             │
│                                                                   │
│  Privacy: Only reveals "amount >= N", not actual balance         │
│                                                                   │
│  Used in: DAO voting, premium features, liquidity pools           │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern 2: Credential-Gated Access

```
┌─────────────────────────────────────────────────────────────────┐
│               Credential-Gated Access                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: User holds valid credential from trusted issuer   │
│                                                                   │
│  Implementation:                                                 │
│  1. Issuer creates: credential = H(issuer_key, holder, schema)   │
│  2. User creates claim: ZK proof of credential ownership        │
│  3. Contract verifies: credential exists, not revoked,           │
│     issuer is trusted                                            │
│                                                                   │
│  Privacy: Only reveals "valid credential", not holder identity   │
│                                                                   │
│  Used in: Governance rights, age-restricted pools, accredited    │
│           investor gates                                         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern 3: Time-Locked Actions

```
┌─────────────────────────────────────────────────────────────────┐
│                  Time-Locked Actions                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: Action can only happen after timestamp T         │
│                                                                   │
│  Implementation:                                                  │
│  1. State includes: time_lock = H(T, action_description)        │
│  2. Consensus ensures: block.timestamp >= T                      │
│  3. Contract verifies: current time >= lock time                │
│                                                                   │
│  Privacy: Locked action description hidden until unlock          │
│                                                                   │
│  Used in: Vesting schedules, delayed withdrawals, expiration     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern 4: Multi-Signature Authorization

```
┌─────────────────────────────────────────────────────────────────┐
│              Multi-Signature Authorization                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: N of M parties must sign                           │
│                                                                   │
│  Implementation:                                                 │
│  1. Each party creates: partial_sig_i = sign(secret_i, msg)     │
│  2. Aggregator combines: full_sig = combine(partial_sigs)         │
│  3. Contract verifies: threshold met, all signers authorized    │
│                                                                   │
│  Privacy: Individual signers revealed only if needed             │
│                                                                   │
│  Used in: DAO proposals, bridge admin keys, upgrade gates        │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## SDK Primitives

The DarkFi SDK provides reusable primitives for contracts:

### Generic Intent Primitives (`src/sdk/src/crypto/intent.rs`)

The `PrivateIntent` struct provides a reusable authorization pattern:

```rust
use darkfi_sdk::crypto::{PrivateIntent, IntentCommitment, IntentNullifier};

// Create an intent
let intent = PrivateIntent::new(
    owner_pubkey,
    namespace,        // Scopes to identity/bridge/DEX/etc.
    payload_hash,   // H(application-specific data)
    expiry,         // Block height expiration
    nonce,          // Prevents replay
    blind,          // Additional blinding
);

// Get commitment for on-chain storage
let commitment = intent.commitment();  // IntentCommitment

// Derive nullifier when consuming
let nullifier = intent.derive_nullifier(owner_secret)?;  // IntentNullifier
```

### Intent-Set State Machine (`src/sdk/src/crypto/intent_set.rs`)

The `IntentSetIndexV1` provides a generic state machine:

```rust
use darkfi_sdk::crypto::{IntentSetIndexV1, IntentPostTransitionV1, IntentConsumeTransitionV1};

let mut index = IntentSetIndexV1::new();

// Post new intent
let post = IntentPostTransitionV1 { ... };
index.validate_post(&post)?;
index.apply_post(&post)?;

// Consume intent (fill/cancel)
let consume = IntentConsumeTransitionV1 { ... };
index.validate_consume(&consume)?;
index.apply_consume(&consume)?;
```

### Contract Function Macro (`src/sdk/src/primitives.rs`)

Use `define_contract_function!` to define contract functions:

```rust
use darkfi_sdk::define_contract_function;

define_contract_function!(MyContract {
    InitializeV1 = 0x00,
    DoActionV1 = 0x01,
});
```

### Commitment/Nullifier Helpers (`src/sdk/src/primitives.rs`)

Low-level commitment and nullifier computation:

```rust
use darkfi_sdk::primitives::{compute_commitment, compute_nullifier};

let commitment = compute_commitment::<2>([secret, param1]);
let nullifier = compute_nullifier(secret, commitment);
```

### Transition Payload Encoding (`src/sdk/src/crypto/transition_payload.rs`)

Helper functions for encoding/decoding transition payloads:

```rust
use darkfi_sdk::crypto::{encode_intent_set_post_v1, decode_intent_set_post_v1};

let payload = encode_intent_set_post_v1(&transition)?;
let decoded = decode_intent_set_post_v1(&payload)?;
```

### Tree Name Helper (`src/sdk/src/primitives.rs`)

Generate consistent tree names:

```rust
use darkfi_sdk::primitives::tree_name;

pub const MY_STATE_TREE: &str = tree_name!("mycontract", "state");
// Results in: "mycontract_state"
```

## Relationship to Existing DarkFi Work

This framework builds on and integrates with existing DarkFi patterns:

### Existing Groundwork in DarkFi

DarkFi `master` already contains relevant foundational work:

| Component | Location | Purpose |
|-----------|----------|---------|
| Anonymous bridge draft | `doc/src/arch/bridge.md` | Early design for cross-chain privacy |
| DEX direction | `doc/src/arch/dex.md` | Uses `spend_hook` and `auth_otc` |
| OTC swap settlement | `src/contract/money/src/entrypoint/swap_v1.rs` | Real swap plumbing in money contract |

### Broader Framing: Private Authorization Layer

The intent primitives in this SDK are **not** specific to AMM or DEX. They implement a **private authorization layer** that appears across all DarkFi privacy-heavy contracts:

```
┌─────────────────────────────────────────────────────────────────┐
│        The Shared Pattern Across All Privacy Contracts                │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Bridge:                                                          │
│    DepositParams.commitment → WithdrawParams.nullifier            │
│    "I know the secret for this deposit"                          │
│                                                                   │
│  Identity:                                                        │
│    IssueCredentialParams.commitment → Claim.nullifier             │
│    "I hold a valid credential from this issuer"                  │
│                                                                   │
│  DEX:                                                             │
│    CreateSwapParams.lock_commitment → AcceptSwapParams.lock_commitment │
│    "I have locked funds for this swap"                           │
│                                                                   │
│  Stablecoin:                                                      │
│    OpenPositionParams.commitment → LiquidateParams.nullifier       │
│    "I own this CDP"                                              │
│                                                                   │
│  KEY INSIGHT: Same lifecycle, different predicates.                │
│               The machinery is reusable; the proof is not.         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

This suggests DarkFi's privacy architecture benefits from **reusable private authorization / claim machinery** rather than ad-hoc commitment/nullifier patterns per contract.

### Namespace Scoping

Each application domain uses a namespace constant to scope intents:

| Contract | Namespace | Purpose |
|----------|-----------|---------|
| Identity | `0x0001` | Credential claims and verifications |
| Bridge | `0x0002` | Cross-chain deposit/withdrawal |
| DEX | `0x0003` | Atomic swaps and exchange |
| Stablecoin | `0x0004` | CDP positions and liquidation |

Namespace separation allows the same `PrivateIntent` primitives to work across all privacy-preserving contracts without collision.

### Predicate Expressiveness and the Opcode Layer

The authorization primitives above define the **structure** of how contracts authorize actions.
The **expressiveness** of what predicates can be verified is determined by the zkVM opcode layer.

Current ZK circuits can verify:
- `amount > 0` (via `BoolCheck` and `RangeCheck`)
- Commitment validity (via `ConstrainEqualBase`)
- Merkle membership (via `MerkleRoot`)

But predicates like `attribute >= threshold` or `collateral >= 2 * debt` require comparison
opcodes that return values — currently missing from the zkVM.

See [zkVM Primitive Layer](zkvm_primitives.md) for the full analysis of:
- Why `LessThanOrEqual` and `IsEqualBase` are systematically needed
- How they compose with existing opcodes
- What each opcode unlocks across identity, stablecoin, DEX, and AMM use cases

This is **not blocking** current contracts from functioning — they use placeholder constraints
that always pass. But unlocking full predicate expressiveness requires implementing these
opcodes in the zkVM.

### Relationship to AMM/DEX Work

The DEX contract uses these primitives but is **not limited to AMM-style exchange**:

- The intent-set lifecycle supports: atomic swaps, intent-based matching, order book styles
- The `IntentSetIndexV1` state machine validates: post/consume transitions generically
- Actual AMM semantics (constant-product, TWAP pricing) are **application-specific**, built on top of these primitives

The primitives solve the **authorization and lifecycle problem**; the application layer solves the **pricing and matching problem**.

## Designing New Contracts: General Primitive Checklist

When designing a new DarkFi contract:

```
┌─────────────────────────────────────────────────────────────────┐
│         New Contract Design Checklist                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  State Primitives:                                                │
│  □ What state does this contract hold?                           │
│  □ Can state be expressed as Merkle tree / accumulator?           │
│  □ What are the state transition rules?                           │
│                                                                   │
│  Authorization Primitives:                                         │
│  □ What actions require authorization?                            │
│  □ Can all authorization be expressed as commitment/nullifier?    │
│  □ What predicates must be satisfied?                             │
│  □ Is revocation needed?                                          │
│                                                                   │
│  Interaction Primitives:                                          │
│  □ What other contracts does this call?                           │
│  □ What state does this read from other contracts?                │
│  □ What tokens does this contract manage?                         │
│  □ How are atomic transactions handled?                           │
│                                                                   │
│  Privacy Analysis:                                                │
│  □ What information is revealed?                                  │
│  □ Can we use ZK proofs to hide more?                             │
│  □ What is the minimal disclosure?                                │
│  □ Can different users share the same proof?                      │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## The Incremental Transparency Framework

All DarkFi contracts should support incremental transparency (see [Identity](identity.md)):

| Level | Name | What Is Revealed | Use Case |
|-------|------|-----------------|----------|
| **0** | `zk_only` | Nothing | Maximum privacy |
| **1** | `predicate` | Predicate result only | Basic verification |
| **2** | `attested` | Issuer attestation | Trusted interactions |
| **3** | `public` | Full disclosure | Regulatory compliance |

```
┌─────────────────────────────────────────────────────────────────┐
│            Incremental Transparency Implementation                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Contract State:                                                  │
│  ┌─────────────────────────────────────────────────────┐         │
│  │ visibility_level: u8  // 0, 1, 2, or 3              │         │
│  │ commitment: H(data)  // Always stored               │         │
│  │ nullifier: H(secret)  // For spent state            │         │
│  │ plaintext: Option<data>  // Only if level >= 3      │         │
│  └─────────────────────────────────────────────────────┘         │
│                                                                   │
│  ZK Circuit:                                                      │
│  assert(visibility_level >= required_level)                      │
│  if level >= 1: assert(predicate(data) == true)                   │
│  if level >= 2: assert(issuer_signature.valid)                   │
│  if level >= 3: reveal(data)                                      │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## References

- [Private Authorization Layer](privauth.md)
- [zkVM Primitive Layer](zkvm_primitives.md) — opcode-level reasoning for contract expressiveness
- [Contract MVP Status](mvp_status.md) — blockers for each contract in the contracts folder
- [Identity Contract](../src/contract/identity/)
- [Bridge Contract](../src/contract/bridge/)
- [DEX Contract](../src/contract/dex/)
- [Stablecoin Contract](../src/contract/stablecoin/)
- [Intent AMM Proposal](https://codeberg.org/rusticml/darkfi-intent-amm-proposal)
- [Response to PatrickM123](https://codeberg.org/rusticml/darkfi-intent-amm-proposal/src/branch/main/docs/response-to-patrickm123.md)
