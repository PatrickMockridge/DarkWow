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
- [Identity Contract](../src/contract/identity/)
- [Bridge Contract](../src/contract/bridge/)
- [DEX Contract](../src/contract/dex/)
- [Stablecoin Contract](../src/contract/stablecoin/)
