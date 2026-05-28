# O-Cap & Composable Privacy

*This document describes O-Cap (Object Capability) authorization as the central paradigm for privacy-preserving authorization in DarkWow smart contracts, enabling composable privacy for social reproduction.*

---

## O-Cap: The Central Paradigm

O-Cap is **not a feature** or an extension - it is the **central paradigm** for authorization in DarkWow.

**The fundamental question:**
- ACL: "WHO has access to X?"
- O-Cap: "Can you prove you have access to X?"

**The key insight:** Authorization should be based on **what you can prove**, not **who you are**.

> **Implementation note:** For the Identity contract's data structures (Capability, CredentialRequirement, CapabilityProof), ZK circuit design, deployment roadmap, and privacy analysis, see [Identity Contract](identity.md).

## The Ambient Authority Problem

Traditional systems have **ambient authority** — your identity and permissions exist
in the environment and any operation can potentially access them. O-Caps make authority
**explicit** and **bounded**: the proof IS the authority, nothing more. The verifier
learns only what capability is being proven, not who holds it.

Authorization follows a four-component pattern:

| Component | Purpose |
|-----------|---------|
| **Commitment** | `H(secret, params)` — private capability on-chain, bound to holder |
| **Nullifier** | `H(secret)` — consumes capability exactly once (replay protection) |
| **Proof** | ZK proof of secret knowledge + predicate satisfaction |
| **Revocation** | Issuer invalidates before use (optional) |

The lifecycle: Commit → Prove → Consume → (Revoke). O-Caps answer "WHAT can you
prove?" instead of "WHO are you?", eliminating identity-based privacy leakage.

---

## Authorization Inversion

ACLs have an inherent privacy gap: observing "ACCESS GRANTED" reveals the
principal came from the authorized set. O-Cap inverts the question from
"WHO has access?" to "Can you PROVE you have access?" — the verifier
learns capability_id, predicate result (1/0), and nullifier existence,
but nothing about the holder's identity or attribute values.

### The ZK Authorization Equivalence Theorem

The structural inversion from ACL to capability-based authorization is
stated as a formal equivalence. Define the ACL model:

**A(p, r, s)** — authorization function over principals, resources, and actions,
granting access iff (p, r, s) appears in a pre-authorized list L.

The capability-based replacement replaces identity-checking with witness-proving:

**A'(π, r, s) = ∃ w : P_{r,s}(w) = 1**

Where P_{r,s} is a predicate **independent of p** (the principal's identity).
The witness w is known only to the prover and unlinkable to any principal
identity. The proof π reveals only the predicate result — not w, not p.

**Theorem (Authorization Inversion).** An ACL-based authorization system
A(p, r, s) can be inverted to a privacy-preserving O-Cap scheme A'(π, r, s)
if and only if there exists a ZK proof system for the language:

**L_{r,s} = { w : P_{r,s}(w) = 1 }**

with proofs simulatable without knowledge of w. In other words:
**capability-based authorization is mathematically equivalent to having a
ZK proof system for the predicate defined by the capability.**

### The Return-Value Gate: Why LTE Completes the Primitive Set

The practical barrier was implementing comparison as a **return-value gate**
rather than a constraint-only assertion. Define:

**LTE_{F_p}(a, b) = { 1 if a ≤ b as integers in [0, 2^253), 0 otherwise }**

The constraint system that makes this sound:

```
a_offset = out * (b - a) + (1 - out) * (a - b - 1)
out * (1 - out) = 0          // out is binary
range_check(253, a_offset)    // no overflow
```

This produces a public output bit `is_authorized` that the verifier reads
**without learning a or b**. Without return-value comparison, circuits could
only assert constraints (binary accept/reject), not selectively disclose
predicate results. A capability like "balance ≥ threshold" would require
revealing the balance.

With return-value LTE, the holder proves `my_attribute ≥ threshold` in ZK,
the verifier sees only `is_authorized = 1`, and the attribute value stays
private. One circuit, one predicate, zero identity leakage.

The LessThanOrEqual (`0x55`) and IsNotEqual (`0x62`) opcodes — both
DarkWow additions to the zkVM with Lean4 formal verification of soundness —
complete the primitive set for full authorization inversion. For opcode-level
detail see [zkVM Primitives](./zk/zkvm_primitives.md).

### Architectural Consequence: Anonymous Identities First

This inverts the traditional blockchain architecture. Instead of building
from "money mint/burn first" and adding identity as an afterthought
(typically an ACL), DarkWow starts from anonymous identities and builds
value transfer on top of them. Every capability interaction — DAO vote,
tender bid, insurance claim, attestation verification — is a ZK predicate
evaluation. The user proves what they can do; the system never learns
who they are.

This radically reduces attack surface against the upstream ACL model:
- No identity to steal — there is no principal database
- No ACL to modify — there is no permissions table
- No session to hijack — proofs are transient and single-use
- No admin key to compromise — authority is bounded to exactly what is proven
- No cross-instance linking — `SecretKey::derive_instance` gives every
  contract instance a unique, cryptographically unlinkable key (see
  [Per-Capability Keys](../dev/contracts/safety.md))

## The Wallet: O-Cap Native Architecture

The DarkWow wallet ([bin/drk/src/capability.rs](../../bin/drk/src/capability.rs)) is
built natively on the o-cap model. It scans the local chain (full node) to
**derive the user's current capabilities and compute available actions** in a
single pass per contract. Each contract gets a resolver method that scans its
sled tree, derives capabilities from on-chain state, and builds per-instance
actions.

The wallet never stores or references a "user identity." It uses
`SecretKey::derive_instance` to create a unique, cryptographically unlinkable
key for every contract instance. The resolver dual-matches raw pubkeys and
derived keys for backward compatibility, then assembles a view of what the
user can do — vote, propose, claim, stake, withdraw — without the user ever
revealing who they are. See [capability.rs](../../bin/drk/src/capability.rs) for
the full resolver dispatch.

This means the wallet UI itself expresses o-cap: the user sees actions
(capabilities they can exercise), not accounts or balances. The wallet is a
**capability browser**, not an identity manager.

## O-Cap Opcodes

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x09` | `RegisterCapabilityV1` | Register a capability type |
| `0x0a` | `IssueCapabilityV1` | Issue a capability to a holder |
| `0x0b` | `VerifyCapabilityV1` | Verify a capability proof (cross-contract) |
| `0x0c` | `RevokeCapabilityV1` | Revoke a capability |

O-Caps compose through **capability chaining**: a base credential
("software_engineer_v1", role >= 5) derives `can_propose` (DAO), which further
derives `can_submit_bid` (Tender). Each contract adds requirements without
amplifying authority; no cross-contract identity linking occurs.

O-Caps reduce attack surface: stolen credential_secret compromises only one
capability; there is no ACL to modify, no identity to steal, no session to
hijack. Authority is bounded to what's being proven, and proofs are transient.

---

## Industries Enabled by O-Cap Privacy

| Industry | Capability | Privacy Guarantees |
|----------|-----------|-------------------|
| **Healthcare** | `can_consult_provider` | Provider/diagnosis hidden |
| **Domestic Labor** | `can_provide_childcare` | Worker/family identity hidden |
| **Education** | `can_enroll_in_program` | Student/grades hidden |
| **Freelance Work** | `can_work_on_freelance_jobs` | Identity/employer/salary hidden |
| **Mutual Insurance** | `can_purchase_coverage` | Medical history hidden |
| **Union Organization** | `can_participate_in_strike_vote` | Membership/vote hidden |

Each industry follows the same pattern: issuer registers capability with
credential requirements, holder proves capability via ZK proof, verifier
learns only that the predicate is satisfied.

## Resolution: Plain Contracts Deprecated

**COMPLETED**: `base_div` (0x58) and `LessThanOrEqual` (0x55) are now implemented and verified sound.

The `contract_plain/` directory has been **deleted**. ZK contracts now have full functionality via:
- O-Cap authorization (0x09-0x0d) for cross-contract capability verification
- `base_div` (0x58) for actuarial calculations
- `LessThanOrEqual` (0x55) for predicate evaluation

---

## Cross-Contract Composability

## State Primitives

All DarkWow contracts must represent state. The common patterns:

1. **Binary Accumulator**: "element exists in set" — Merkle trees, Bloom filters, RSA accumulators. Proves membership without revealing the element.
2. **Interval Tree**: "value exists in range" — Balance ranges, time windows, credential expiration. Proves bounds without revealing the value.
3. **Hash Chain**: "sequence of events in order" — Transaction history, credential issuance order. Proves event ordering.

## Authorization Primitives

O-Cap authorization is the **central paradigm** for all DarkWow contracts. Each contract uses three components:

| Component | Purpose |
|-----------|---------|
| **Commitment** | `H(secret, params)` — private capability on-chain |
| **Nullifier** | `H(secret)` — replay protection (consume exactly once) |
| **Proof** | ZK proof of secret knowledge + predicate satisfaction |

The proof IS the authority — nothing more. Commitment exists and is valid, nullifier hasn't been spent, and the proof verifies the predicate without revealing the secret.

**Identity Contract as O-Cap Baseline:**

The [Identity Contract](../../../src/contract/identity/) implements the canonical O-Cap pattern with full opcode support (0x09-0x0c):

| O-Cap Opcode | Function | Description |
|--------------|----------|-------------|
| `0x09` | `RegisterCapabilityV1` | Register capability types |
| `0x0a` | `IssueCapabilityV1` | Issue capabilities to holders |
| `0x0b` | `VerifyCapabilityV1` | Verify capability proofs (cross-contract) |
| `0x0c` | `RevokeCapabilityV1` | Revoke capabilities |
| Capability | `Capability` struct | Defines what capability allows and requires |
| Credential | `CredentialRequirement` | Specifies schema, issuer, threshold |
| Proof | `CapabilityProof` | ZK proof of capability satisfaction |
| Nullifier | `IntentNullifier` | Prevents capability replay |

## The Attestation Primitive

The [Attestation Contract](../contract/attestation.md) provides a **generalized claims and attestation system** that enables cross-contract composition through a common pattern:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Attestation Pattern                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ATTESTOR → ATTESTATION → CLAIMANT → CLAIM → VALIDATION          │
│                                                                   │
│  ┌─────────────┐    ┌──────────────┐    ┌─────────────┐         │
│  │ Attestor    │───→│ Attestation  │───→│ Claimant    │         │
│  │ (issuer)    │    │ (commitment) │    │ (holder)    │         │
│  └─────────────┘    └──────────────┘    └──────┬──────┘         │
│                                                 │                 │
│                                                 ▼                 │
│                                           ┌─────────────┐         │
│                                           │    Claim    │         │
│                                           │ (assertion) │         │
│                                           └──────┬──────┘         │
│                                                  │                 │
│                                                  ▼                 │
│                                           ┌─────────────┐         │
│                                           │   Verify    │         │
│                                           │ (predicate) │         │
│                                           └──────┬──────┘         │
│                                                  │                 │
│                                                  ▼                 │
│                                           ┌─────────────┐         │
│                                           │   Consume   │         │
│                                           │ (nullifier) │         │
│                                           └─────────────┘         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Attestation vs Identity

| Aspect | Identity Contract | Attestation Contract |
|--------|------------------|---------------------|
| **Purpose** | ZK credential proofs using competency DAGs | Generalized attestation and claims |
| **Pattern** | Issuer issues credentials, holder proves | Attestor commits to data, claimant creates claim |
| **Predicate** | Custom ZK circuits | Standard predicates: Matches, GreaterOrEqual, LessOrEqual, Contains |
| **Replay Prevention** | Nullifier per claim | Nullifier per claim consumption |
| **Use Cases** | Competency verification, age checks | Deliverable verification, price feeds, oracle data |

The Attestation contract generalizes the claims pattern that appeared in Identity, Labor Market, and Tender.

## Interaction Primitives

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
│     Solution: Atomic transactions with dependent operations       │
│     Both operations in same transaction, all or nothing          │
│                                                                   │
│  4. ATTESTATION REFERENCE                                       │
│     Contract A uses attestation from Contract B                  │
│                                                                   │
│     Problem: How does A verify B's attestation without direct?   │
│                                                                   │
│     Solution: Store attestation_id, verify claim via Attestation │
│     A reads attestation_id, calls Attestation.verify_claim()    │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Cross-Contract Verification Patterns

### When to Use O-Cap vs Trusted Setup

| Use Case | Solution |
|----------|---------|
| DEX ↔ Money lock verification | Use O-Cap: `can_swap` capability from Identity |
| Cross-contract token transfers | Use atomic transactions |
| Attestation references | Use Attestation contract directly |
| Oracle data verification | Use Oracle + Attestation |
| Cross-contract capability verification | Use Identity's `VerifyCapabilityV1` (0x0b) |

## Cross-Contract Composability Matrix

| Caller → | Identity (O-Cap) | Money | DAO | Bridge | DEX | Attestation | Oracle | Labor Market | Tender | DarkToshi Dice | Prediction Market | Insurance Market |
|----------|-------------------|-------|-----|--------|-----|--------------|--------|-------------|--------|----------------|-------------------|-----------------|
| **Identity (O-Cap)** | - | - | - | - | - | - | - | Capability verification | Capability verification | - | - | Capability verification |
| **Money** | - | - | Token transfers | Token escrow | Swap settlement | - | - | Job payment escrow | Bid deposit escrow | Bet value lock | Bet value lock | Premium payments, claim payouts |
| **DAO** | O-Cap governance | Treasury management | - | Governance of bridge | Governance of DEX | Attestation governance | - | Job approval governance | Tender authorization | House edge management | Market creation governance | Insurance governance |
| **Bridge** | - | Cross-chain transfers | Relayer rewards | - | Liquidity provision | - | - | External job funding | External tender integration | - | - | - |
| **DEX** | - | Swap execution | Fee distribution | - | - | - | - | - | - | - | - | - |
| **Attestation** | - | - | - | - | - | - | Oracle data attestation | Deliverable verification | Competency verification | - | - | Claim resolution |
| **Oracle** | - | Collateral pricing | - | - | Liquidity pricing | Creates attestations | - | - | - | Roll randomness | Outcome resolution | Claim validity |
| **Labor Market** | O-Cap capability check | Job payment settlement | Job DAO governance | External payment integration | - | Uses for delivery | - | - | Job creation from tender | - | - | Underwriter certification |
| **Tender** | O-Cap capability check | Bid deposit management | - | External tender integration | - | Uses for competency | - | Winner job creation | - | - | - | Insurance requirements |
| **DarkToshi Dice** | - | Token settlements | - | - | - | - | Block hash randomness | - | - | - | - | - |
| **Prediction Market** | - | Payout settlement | - | - | - | - | Oracle resolution | - | - | - | - | Risk probability pricing |
| **Insurance Market** | O-Cap capability check | Claim payouts | - | - | - | Claim verification | Oracle attestation | Underwriter bonding | Coverage requirements | - | Risk market integration | - |

## General Primitive Composition Patterns

### Pattern 1: Token-Gated Access

User proves `balance >= N` without revealing actual balance. Commitment: `H(balance_secret, token, amount)`. ZK proof verifies commitment exists and amount >= threshold. Privacy: only the inequality is revealed. Used in DAO voting, premium features, liquidity pools.

### Pattern 2: Attestation-Based Claims

Attestor creates `attestation = H(data, attestor_key)`. Claimant proves access via ZK proof. Contract verifies attestation exists, claim valid, predicate satisfied. Privacy: only "valid claim" revealed. Used in deliverable verification, competency claims, oracle data.

### Pattern 3: Time-Locked Actions

State includes `time_lock = H(T, action_description)`. Consensus ensures `block.timestamp >= T`. Contract verifies current time >= lock time. Privacy: action description hidden until unlock. Used in vesting, delayed withdrawals, expiration.

### Pattern 4: Multi-Signature Authorization

N of M parties sign: each creates `partial_sig_i = sign(secret_i, msg)`, aggregator combines, contract verifies threshold met. Privacy: individual signers revealed only if needed. Used in DAO proposals, bridge admin keys, upgrade gates.

## O-Cap Authorization: Now Fully Implemented

The Identity contract provides full O-Cap authorization via four opcodes:

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x09` | `RegisterCapabilityV1` | Register a new capability type (e.g., `can_merge_pr`) |
| `0x0a` | `IssueCapabilityV1` | Issue a capability to a holder based on their credential |
| `0x0b` | `VerifyCapabilityV1` | Verify a capability proof |
| `0x0c` | `RevokeCapabilityV1` | Revoke a capability |

The flow: Register defines the capability + credential requirements. Issue grants it to a holder after ZK proof of credential. Verify checks a capability proof from any contract. Revoke invalidates before use. Other contracts call `identity.verify_capability(params)` to check authorization without learning the holder's identity.

---

## Competency DAGs

Competency DAGs enable **multiple credential paths** where any path can be satisfied to achieve a competency. This is a generalization of the multi-credential AND logic into an OR structure across paths.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Competency DAG Example                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PATH A:                          PATH B:                         │
│  High School Diploma              Self-Taught + Portfolio         │
│       │                               │                           │
│       ▼                               ▼                           │
│  Associate's Degree ─────┐     Industry Certification            │
│       │                  │           │                           │
│       ▼                  │           ▼                           │
│  Bachelor's Degree ──────┼────► "Qualified Developer"           │
│                          │           │                           │
│       ┌──────────────────┘           │                           │
│       ▼                              ▼                           │
│  "Senior Developer" ◄────────────────┘                           │
│                                                                   │
│  OR LOGIC: Either PATH A OR PATH B leads to "Qualified"          │
│  AND LOGIC: PATH A requires ALL credentials in sequence           │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### DAG Implementation in Identity Contract (0x0d)

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x0d` | `CreateClaimDAGV1` | Create claim verifying multiple credential paths |

### DAG Data Structures

```rust
/// A single credential in a DAG path
pub struct DAGCredential {
    pub nullifier: IntentNullifier,     // Proves credential exists
    pub predicate_result: u8,            // 1 if predicate satisfied
    pub claim_type: [u8; 32],           // Type identifier
}

/// A single path (AND chain of credentials)
pub struct CredentialPath {
    pub credentials: Vec<DAGCredential>, // AND chain
    pub path_hash: [u8; 32],            // Merkle root of path
}

/// Full DAG structure
pub struct CompetencyDAG {
    pub dag_id: [u8; 32],
    pub name: Vec<u8>,
    pub paths: Vec<CredentialPath>,     // OR between paths
    pub dag_root: [u8; 32],
    pub issuer_pub: [u8; 32],
    pub created_at: u64,
    pub expires_at: u64,
}
```

### DAG ZK Circuit: `create_claim_v1_dag.zk`

The circuit verifies one path is satisfied using AND logic within the path:

```zk
# For each credential in path:
is_lte_1 = less_than_or_equal(threshold_1, attribute_1);
is_lte_2 = less_than_or_equal(threshold_2, attribute_2);
is_lte_3 = less_than_or_equal(threshold_3, attribute_3);

# AND logic (all must pass)
path_satisfied = base_mul(is_lte_1, is_lte_2);
path_satisfied = base_mul(path_satisfied, is_lte_3);

# Verify path result
constrain_equal_base(path_satisfied, ONE);
```

### O-Cap + DAG Composition

DAG claims compose with O-Cap capabilities:

```
┌─────────────────────────────────────────────────────────────────┐
│            O-Cap + Competency DAG Composition                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  DAG CLAIM:                                                      │
│  "I have PATH_A OR PATH_B leading to Senior Developer"          │
│  → Proves: Either credential chain satisfied                     │
│  → Hides: Which path, exact credentials, issuers               │
│                                                                   │
│  DERIVED CAPABILITY:                                            │
│  can_approve_architecture = Senior Developer DAG + predicate     │
│                                                                   │
│  O-Cap VERIFICATION:                                            │
│  VerifyCapability(can_approve_architecture)                     │
│  → Returns: VALID (DAG claim satisfied + predicate)            │
│  → Hides: Everything else                                      │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Use Cases for DAGs

| DAG Structure | Use Case |
|--------------|----------|
| Multiple education paths | "Qualified Developer" via degree OR certification |
| Experience alternatives | "Senior Engineer" via 10 years exp OR 5 years + certifications |
| Multi-jurisdiction | Medical license in any of US/EU/APAC |
| Skill equivalency | CPA OR CA OR ACCA for accounting |

---

## Case Studies

### Tender + Labor Market

Identity registers a capability (e.g., "qualified_contractor"), Tender accepts
sealed bids gated by capability proof, and Labor Market executes the awarded job
with attestation-based deliverable verification. The tender state machine flows
Created → Bidding → Revealed → Awarded (or Cancelled). At every step, identity
is hidden — only capabilities are proven.

Workers compete on capability, not identity: employers cannot discriminate by
age, gender, or ethnicity; current employers cannot see job-hunting activity;
salaries remain private.

### Case Study: The Complete O-Cap Pipeline — DAO Governance to Insurance

A full O-Cap lifecycle from DAO funding through insurance, every step capability-gated and identity-hidden:

| Stage | Contract | What Happens |
|-------|----------|-------------|
| **Governance** | DAO-Escrow | Members vote to fund 50,000 DRK security audit bounty via `member_vote` capability |
| **Credential** | Identity | Alice registers "verified_smart_contract_auditor" capability (requires auditor license, experience >= 3) and "senior_engineer" DAG (multiple qualification paths) |
| **Bidding** | Tender | Alice submits sealed bid with ZK capability proof — identity/employer never revealed |
| **Execution** | Labor Market | Job created from tender win; Alice accepts and delivers via capability |
| **Dispute** | DAO-Escrow | Multi-oracle attestation (3 of 5) + arbitrator resolves via `dispute_arbitrator` capability; payment released from escrow |
| **Insurance** | Insurance Market | Project owner purchases coverage; underwriters prove `auditor_bond` capability; claims resolved via `oracle_resolution` |

Key insights: DAO-Escrow bookends the entire lifecycle (funding → dispute). Alice's single capability works across Identity, Tender, Labor Market, and Insurance — she never re-proves her identity. Each step only calls `verify_capability()` — no complex cross-contract state sharing.

### Case Study: Subscription + DAO-Escrow + Atomic Swap

The Subscription contract demonstrates DarkWow's full composability stack: DAO-Escrow membership verification via Merkle proofs, block-based time locks, and cross-chain atomic swap payments.

```
┌─────────────────────────────────────────────────────────────────────────┐
│           Subscription + DAO-Escrow + Atomic Swap Composability              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌──────────────────────┐         ┌──────────────────────┐              │
│  │     DAO-Escrow       │         │    Subscription      │              │
│  │                      │         │                       │              │
│  │  ┌────────────────┐  │         │  ┌────────────────┐  │              │
│  │  │ pay_premium()  │──┼───┐     │  │ subscribe()    │  │              │
│  │  └────────────────┘  │   │     │  └───────┬────────┘  │              │
│  │                      │   │     │          │            │              │
│  │  State: Merklized    │   │     │  Verifies via:      │              │
│  │  Membership tree     │   │     │  ┌────────▼────────┐ │              │
│  │                      │   │     │  │ Merkle proof   │ │              │
│  │                      │   │     │  │ + expiry check │ │              │
│  │                      │   │     │  │ + pubkey link  │ │              │
│  └──────────────────────┘   │     │  └────────────────┘  │              │
│                             │     │                       │              │
│         ┌───────────────────┘     └───────────────────────┘              │
│         │                           │                                      │
│         │    Cross-Contract         │                                      │
│         │    ZK Verification        │                                      │
│         ▼                           ▼                                      │
│  ┌─────────────────────────────────────────────────────────────┐        │
│  │                   Composability                               │        │
│  │                                                             │        │
│  │  No direct state sharing!                                   │        │
│  │  Pure Merkle proof verification.                             │        │
│  │  Nullifiers prevent double-spending.                         │        │
│  └─────────────────────────────────────────────────────────────┘        │
│                                                                          │
│  ┌──────────────────────┐         ┌──────────────────────┐              │
│  │    Atomic Swap       │         │    Subscription      │              │
│  │                      │         │                       │              │
│  │  ┌────────────────┐  │         │  ┌────────────────┐  │              │
│  │  │ CreateSwap()   │──┼─────────┼──│ SubscribeV1()  │  │              │
│  │  │ + HTLC        │  │         │  │ + hash link   │  │              │
│  │  └────────────────┘  │         │  └───────┬────────┘  │              │
│  │                      │         │          │            │              │
│  │  External chain      │         │  Cross-chain            │              │
│  │  funding flow        │         │  payment settlement     │              │
│  └──────────────────────┘         └───────────────────────┘              │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## SDK Primitives

### Generic Intent Primitives (`src/sdk/src/crypto/intent.rs`)

The `PrivateIntent` struct provides a reusable authorization pattern:

```rust
use dwow_sdk::crypto::{PrivateIntent, IntentCommitment, IntentNullifier};

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
use dwow_sdk::crypto::{IntentSetIndexV1, IntentPostTransitionV1, IntentConsumeTransitionV1};

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

---

## Designing New Contracts: General Primitive Checklist

When designing a new DarkWow contract:

```
┌─────────────────────────────────────────────────────────────────┐
│         New Contract Design Checklist                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  State Primitives:                                                │
│  □ What state does this contract hold?                           │
│  □ Can state be expressed as Merkle tree / accumulator?          │
│  □ What are the state transition rules?                           │
│                                                                   │
│  Authorization Primitives:                                         │
│  □ What actions require authorization?                            │
│  □ Can all authorization be expressed as commitment/nullifier?     │
│  □ What predicates must be satisfied?                             │
│  □ Is revocation needed?                                          │
│  □ Can O-Cap capabilities from Identity be used instead of       │
│    building custom authorization?                                   │
│                                                                   │
│  Interaction Primitives:                                          │
│  □ What other contracts does this call?                           │
│  □ Does Attestation already provide what I need?                  │
│  □ What state does this read from other contracts?                │
│  □ What tokens does this contract manage?                         │
│  □ How are atomic transactions handled?                           │
│                                                                   │
│  Privacy Analysis:                                                │
│  □ What information is revealed?                                  │
│  □ Can we use ZK proofs to hide more?                           │
│  □ What is the minimal disclosure?                                │
│  □ Can different users share the same proof?                      │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

---


---

## See Also

- [Identity Contract](../../../src/contract/identity/) - O-Cap implementation
- [Attestation Contract](../contract/attestation.md) - Generalized attestation and claims
- [Oracle Contract](../contract/oracle.md) - Push-model oracle with attestation
- [DAO-Escrow Contract](../contract/dao_escrow.md) - DAO-governed endowment with voting
- [Wallet Capability Resolver](../../bin/drk/src/capability.rs) - O-Cap native wallet implementation
- [Contract Safety: Per-Capability Keys](../dev/contracts/safety.md) - `derive_instance` and cross-instance unlinkability
- [zkVM Primitive Layer](./zk/zkvm_primitives.md) — opcode-level reasoning for contract expressiveness
- [Zero-Knowledge Authorization (Authorization Inversion Theorem)](https://technologytruth.substack.com/p/the-zero-knowledge-authorization) - Mathematical foundation for O-Cap authorization
- [DarkFi Development Uncensored](https://technologytruth.substack.com/p/darkfi-development-uncensored-part-1a6) - O-Cap vs ACL architectural analysis
