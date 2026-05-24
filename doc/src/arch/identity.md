# Identity Contract: O-Cap (Object Capability) Authorization

> **Note:** For the conceptual foundations of O-Cap — the ambient authority problem, privacy rules, authorization inversion, and the full opcode reference — see the definitive [O-Cap & Composable Privacy](ocap.md) chapter. This document covers the Identity contract implementation: data structures, ZK circuit design, composability patterns, and deployment roadmap.


## The Fundamental Shift: ACL to O-Cap

### Traditional ACL Model

Access Control Lists (ACLs) are the standard authorization model:

```
┌─────────────────────────────────────────────────────────────────┐
│                    ACL Authorization Model                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  System maintains a list:                                         │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │  alice@company     →  repo:read, repo:write, wiki:edit  │     │
│  │  bob@company       →  repo:read, wiki:read              │     │
│  │  charlie@company   →  repo:admin, wiki:admin, ...      │     │
│  └─────────────────────────────────────────────────────────┘     │
│                                                                   │
│  To authorize:                                                    │
│  1. Identify the user (who are you?)                             │
│  2. Look up their permissions                                      │
│  3. Check if permission exists for action                         │
│                                                                   │
│  PROBLEMS:                                                        │
│  - Identity always revealed to verifier                          │
│  - Every action traces back to identity                          │
│  - Can't prove attributes without revealing identity             │
│  - No way to delegate without sharing identity                   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### O-Cap Model

Object Capabilities (O-Caps) invert this model:

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Authorization Model                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Issuer creates capabilities (not tied to identity yet):           │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │  CAN_MERGE_PR     → requires: role >= senior_engineer    │     │
│  │  CAN_APPROVE_AUDIT → requires: certified_auditor         │     │
│  │  CAN_VOTE         → requires: token_balance >= 1000      │     │
│  └─────────────────────────────────────────────────────────┘     │
│                                                                   │
│  Holder proves capability (identity never revealed):              │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │  Alice proves: "I have can_merge_pr"                     │     │
│  │              = "I prove role >= senior_engineer"         │     │
│  │                                                       │     │
│  │  Verifier learns:                                       │     │
│  │  - can_merge_pr = VALID                                │     │
│  │  - predicate_result = 1 (passes threshold)            │     │
│  │                                                       │     │
│  │  Verifier does NOT learn:                              │     │
│  │  - Who Alice is                                        │     │
│  │  - What Alice's actual role is                         │     │
│  │  - Alice's other attributes                             │     │
│  └─────────────────────────────────────────────────────────┘     │
│                                                                   │
│  KEY INSIGHT: The verifier only learns WHAT capability is held,  │
│               not WHO holds it.                                   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Why O-Cap is a Paradigm Shift

1. **Authorization without identification**: Prove you have permission without revealing who you are
2. **Capability composition**: New capabilities can be derived from base capabilities
3. **Decentralized issuance**: Anyone with the right credential can obtain capabilities
4. **No identity tracking**: Actions cannot be linked back to identity
5. **Minimal disclosure by default**: Reveal only that you have the capability

## Core O-Cap Concepts

### Capability

A `Capability` is a registered authorization that defines:
- **What** it allows (capability name)
- **How** to obtain it (credential requirement)
- **Who** can issue it (issuer public key)
- **Limits** on how many can be held (max_holders)

```rust
struct Capability {
    capability_id: [u8; 32],           // Unique identifier
    name: Vec<u8>,                     // e.g., "can_merge_pr"
    credential_requirement: CredentialRequirement, // How to obtain
    issuer_pub: [u8; 32],              // Who issues this
    max_holders: Option<u64>,          // Limit (None = unlimited)
    issued_count: u64,                 // How many issued
}
```

### CredentialRequirement

Describes what a holder must prove to get a capability:

```rust
struct CredentialRequirement {
    schema_hash: [u8; 32],            // Required credential type
    issuer_pub: [u8; 32],             // Must be from this issuer
    min_threshold: u64,               // Minimum attribute value
    attribute_name: Vec<u8>,          // Which attribute to check
}
```

**Example:** To get `can_merge_pr`:
```
require: schema_hash = software_engineer_v1
require: issuer = ACME_Corp
require: attribute_name = "role_level"
require: min_threshold >= 5 (senior engineer)
```

### CapabilityProof

What the holder presents to prove they have a capability:

```rust
struct CapabilityProof {
    capability_id: [u8; 32],          // What capability
    nullifier: IntentNullifier,        // From underlying credential
    predicate_result: u8,             // 1 if requirements met
    issuer_pub: [u8; 32],             // Issuer of underlying credential
    schema_hash: [u8; 32],           // Schema of underlying credential
    proof: Vec<u8>,                   // ZK proof
    capability_secret: [u8; 32],     // Proves ownership of this cap
    created_at: u64,                  // Timestamp
}
```

### StoredCapability

Records who holds what capability:

```rust
struct StoredCapability {
    capability_id: [u8; 32],          // Which capability
    holder_pub: [u8; 32],            // Who holds it
    secret: [u8; 32],               // Proof of ownership
    revoked: bool,                   // Has it been revoked?
    issued_at: u64,                   // When issued
    expires_at: u64,                 // When it expires (0 = never)
}
```

## How O-Cap Works: Technical Deep Dive

### O-Cap Lifecycle

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Lifecycle                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. ISSUER REGISTERS CAPABILITY                                   │
│     └── Issuer calls RegisterCapabilityV1()                       │
│         └── Stores Capability definition in capabilities tree      │
│                                                                   │
│  2. ISSUER ISSUES CREDENTIAL TO HOLDER                            │
│     └── Issuer calls IssueCredentialV1()                          │
│         └── Holder receives credential with secret                │
│                                                                   │
│  3. HOLDER OBTAINS CAPABILITY                                     │
│     └── Issuer calls IssueCapabilityV1()                          │
│         └── Or: holder derives capability from credential          │
│         └── Stored in capability_issuances tree                   │
│                                                                   │
│  4. HOLDER CREATES PROOF                                          │
│     └── Off-chain: generates CapabilityProof                       │
│         └── Proves: credential_secret is valid                    │
│         └── Proves: predicate_result = 1                          │
│         └── Does NOT reveal: holder identity                       │
│                                                                   │
│  5. VERIFIER CHECKS PROOF                                         │
│     └── Verifier calls VerifyCapabilityV1()                       │
│         └── On-chain: verifies ZK proof                           │
│         └── Result: VALID/INVALID                                 │
│         └── Does NOT reveal: holder identity                       │
│                                                                   │
│  6. CAPABILITY CAN BE REVOKED                                     │
│     └── Issuer or holder calls RevokeCapabilityV1()               │
│         └── StoredCapability.revoked = true                       │
│         └── All future proofs for this cap fail                  │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### ZK Circuit: verify_capability_v1.zk

The `verify_capability_v1.zk` circuit proves:

```
PUBLIC INPUTS:
- capability_id: Which capability is being proven
- nullifier: From underlying credential (proves it exists)
- issuer_pub_x/y: Issuer of the credential
- schema_hash: Type of credential
- predicate_result: 1 if threshold satisfied

PRIVATE INPUTS (known only to holder):
- credential_secret: Proves ownership of credential
- commitment: Commitment to the credential
- attribute_value: Actual attribute value
- threshold: Required minimum
- capability_secret: Proves ownership of this capability

WHAT THE CIRCUIT VERIFIES:
1. credential_secret + commitment → nullifier (credential exists)
2. threshold <= attribute_value (predicate satisfied)
3. capability_secret is bound to capability_id

WHAT VERIFIER LEARNS:
- capability_id (what's being proven)
- predicate_result (1 = requirements met)
- nullifier (proof of credential existence)

WHAT VERIFIER DOES NOT LEARN:
- Who the holder is
- What the actual attribute_value is
- What the threshold is
- Full credential contents
```

### Database Trees

| Tree | Purpose | What's Stored |
|------|---------|---------------|
| `capabilities` | Capability definitions | `capability_id → Capability` |
| `capability_issuances` | Holder → capability mapping | `(capability_id, holder_pub) → StoredCapability` |
| `credentials` | Issued credentials | `nullifier → Credential` |
| `nullifiers` | Revocation tracking | `nullifier → reason` |

**Key design:** The `capability_issuances` tree maps `(capability_id, holder_pub)` to the capability record. This allows efficient lookup of all capabilities a holder has, or all holders of a capability.

## Capability Revocation Comparison

**ACL Revocation:**
```
Alice leaves company
→ Admin removes Alice from all ACLs
→ Must search for every entry mentioning Alice
→ If missed, Alice retains access
→ No automatic propagation to derived permissions
```

**O-Cap Revocation:**
```
Alice's credential is revoked
→ The underlying credential's nullifier is spent
→ ALL capabilities derived from this credential fail
→ Automatic, cascading revocation
→ No need to track individual capability grants
```

## Delegation Comparison

**ACL Delegation (problematic):**
```
Alice wants to delegate access to Bob
→ Admin must add Bob to ACL
→ Or: Alice shares her credentials (BAD)
→ No way to delegate without identity exposure
```

**O-Cap Delegation:**
```
Alice has can_manage_team capability
→ Derives can_manage_subteam capability
→ Issues to Bob (without revealing Alice's identity)
→ Bob can prove he has the capability
→ Alice's identity never appears in the delegation
```

## Private Authorization with O-Cap

### The Ambient Authority Problem

Traditional systems have **ambient authority** - your identity and permissions exist in the environment (OS, database, session) and ANY operation can potentially access them.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Ambient Authority Problem                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Alice logs into system                                           │
│  → Her identity and ALL permissions are ambient (in environment) │
│                                                                   │
│  Alice's code runs:                                              │
│  → Can it access resources it shouldn't?                         │
│  → Can it leak data to unauthorized parties?                     │
│  → Can it be tricked into acting for another user?              │
│                                                                   │
│  PROBLEM: Every operation runs in an environment                 │
│  filled with ambient authority that can be exploited.            │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### O-Caps Eliminate Ambient Authority

O-Caps make authority **EXPLICIT** and **BOUNDED**. The proof IS the authority - nothing more.

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Eliminates Ambient Authority               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Alice creates a proof:                                          │
│  → Only the SPECIFIC capability is in the proof                  │
│  → No other permissions are present in the environment           │
│  → The verifier only sees the capability, not Alice's identity   │
│                                                                   │
│  Alice's code cannot:                                            │
│  → Access resources beyond the specific capability                │
│  → Leak data because it has no ambient identity to leak         │
│  → Be tricked because there's nothing to impersonate              │
│                                                                   │
│  KEY INSIGHT: O-Caps make authority EXPLICIT and BOUNDED.       │
│               The proof IS the authority - nothing more.          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### The Intuitive Privacy Rule

```
┌─────────────────────────────────────────────────────────────────┐
│            O-Cap Makes Privacy Reasoning Intuitive                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ACL PRIVACY REASONING (hard):                                    │
│  "Does this operation reveal Alice's identity?"                   │
│  "Can this log be linked to other logs?"                         │
│  "What does the admin see?"                                       │
│  "If data breaches, what is exposed?"                             │
│  → Complex, contextual, hard to reason about                     │
│                                                                   │
│  O-Cap PRIVACY REASONING (simple):                               │
│  "What capability is being proven?"                               │
│  "That's all the verifier learns - nothing more."                 │
│  → SIMPLE, LOCAL, INTUITIVE                                      │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────┐      │
│  │  In O-Cap, you reveal ONLY what you prove.            │      │
│  │  If you prove "can_vote", verifier learns "can_vote"  │      │
│  │  Nothing else. Nothing more. Always.                    │      │
│  └─────────────────────────────────────────────────────────┘      │
│                                                                   │
│  WHY THIS IS INTUITIVE:                                           │
│  - Privacy is PROVABLE, not just policy                          │
│  - The ZK proof GUARANTEES what is/isn't revealed               │
│  - No trust in system admins or database security                 │
│  - Reasoning is LOCAL to the capability being proven              │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Privacy Properties

O-Cap provides strong privacy guarantees:

| What IS Revealed | What Is NOT Revealed |
|------------------|---------------------|
| Capability ID | Holder identity |
| Predicate result (1/0) | Actual attribute values |
| Nullifier (exists, not who) | Threshold values |
| Issuer public key | Full credential contents |
| Proof validity | When credential expires |

**The nullifier does NOT reveal identity:**
- Nullifier = poseidon_hash(credential_secret, commitment)
- credential_secret is known only to holder
- commitment does not contain identity
- Proof of existence, not of identity

## O-Cap Composability

### How O-Caps Compose with Each Other

> **See also:** [How O-Caps Compose](ocap.md#how-o-caps-compose) in the O-Cap chapter for the general paradigm.

Capabilities compose through **accumulation of requirements** - never amplification of authority:

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Composition                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  BASE CAPABILITY:                                                 │
│  ┌─────────────────────────────────────────────────────────┐      │
│  │ credential: "software_engineer_v1"                      │      │
│  │ proves: role >= 5 (senior engineer)                   │      │
│  └─────────────────────────────────────────────────────────┘      │
│                           │                                        │
│                           ▼                                        │
│  DERIVED CAPABILITY:                                             │
│  ┌─────────────────────────────────────────────────────────┐      │
│  │ can_merge_pr = credential.software_engineer_v1         │      │
│  │             + predicate(role >= 5)                      │      │
│  │             + issuer = ACME_Corp                       │      │
│  └─────────────────────────────────────────────────────────┘      │
│                           │                                        │
│                           ▼                                        │
│  FURTHER DERIVED:                                                 │
│  ┌─────────────────────────────────────────────────────────┐      │
│  │ can_approve_security_pr = can_merge_pr                 │      │
│  │                            + additional_training = yes  │      │
│  └─────────────────────────────────────────────────────────┘      │
│                                                                   │
│  COMPOSITION IS SIMPLE:                                           │
│  - Each capability declares its requirements                      │
│  - Deriving a new cap = adding more requirements                │
│  - NO amplification of authority - only accumulation of proof     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### How O-Caps Reduce Attack Surface

> **See also:** [How O-Caps Reduce Attack Surface](ocap.md#o-caps-reduce-attack-surface-dramatically) in the O-Cap chapter.

O-Caps dramatically reduce the attack surface by bounding authority:

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Reduces Attack Surface                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ACL Attack Surface:                                              │
│  1. Stolen credentials → full identity compromise                 │
│  2. SQL injection → ACL modification                              │
│  3. Privilege escalation → admin access                          │
│  4. Insider threat → ACL bypass                                  │
│  5. Cross-site scripting → session hijacking                     │
│  → ATTACK SURFACE: Every point where identity exists             │
│                                                                   │
│  O-Cap Attack Surface:                                            │
│  1. Stolen credential_secret → specific cap only                  │
│  2. No ACL to modify (capabilities are derived)                 │
│  3. No privilege escalation (authority is explicit)              │
│  4. No insider threat (identity never in system)                 │
│  5. No session to hijack (proof is transient)                   │
│  → ATTACK SURFACE: Only the specific capability being proven      │
│                                                                   │
│  WHY O-Caps REDUCE attack surface:                               │
│  - Authority is BOUNDED to what's being proven                   │
│  - Identity is NEVER in the system                               │
│  - Proofs are TRANSIENT (don't persist as ambient authority)      │
│  - Revocation is CASCADING (credential revoke → all caps fail)    │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Transaction Privacy with O-Caps

Transaction context is **SEPARATE** from authorization - these are independent concerns:

```
┌─────────────────────────────────────────────────────────────────┐
│                Transaction Privacy with O-Caps                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  TRADITIONAL TRANSACTION:                                         │
│  Alice (0x1234) sends 100 tokens to Bob (0x5678)                │
│  Revealed: Alice's identity, Bob's identity, amount, timing,    │
│            complete transaction history                           │
│  Privacy: ZERO                                                   │
│                                                                   │
│  O-Cap TRANSACTION:                                             │
│  Prover proves: "I have can_transfer capability"                 │
│                                                                   │
│  What verifier learns:                                           │
│  - can_transfer = VALID                                          │
│  - predicate_result = 1                                          │
│  - nullifier (exists, not who)                                   │
│                                                                   │
│  What verifier does NOT learn:                                    │
│  - Who sent the transaction                                      │
│  - Who received the transaction                                  │
│  - What amount was transferred                                   │
│  - When the transaction occurred                                  │
│  - What the transaction was for                                  │
│                                                                   │
│  KEY INSIGHT: Authorization (O-Cap) proves CAPABILITY             │
│  Transaction details are handled by the APPLICATION contract      │
│  These are INDEPENDENT concerns.                                 │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Integration Pattern

Any contract can call `VerifyCapabilityV1` to check if a caller has a required capability:

```rust
// In another contract's logic:
let params = VerifyCapabilityParams {
    capability_proof: proof_from_caller,
    verifier_pub: my_pubkey,
    fee: 0,
};

// This contract decides WHAT capability is required:
const REQUIRED_CAPABILITY: &[u8] = b"can_execute_proposal";

// Verify the caller has it:
identity::verify_capability(params, REQUIRED_CAPABILITY)?
```

### Example Integrations

| Contract | O-Cap Usage | Capability | Status |
|----------|-------------|------------|--------|
| **dao_escrow** | Propose claim | `member_vote` | Implemented |
| **dao_escrow** | Vote on proposal | `member_vote` | Implemented |
| **dao_escrow** | Treasury release | `board_treasury` | Implemented |
| **dao_escrow** | Endowment release | `board_endowment` | Implemented |
| **dao_escrow** | Dispute resolution | `dispute_arbitrator` | Implemented |
| **tender** | Submit sealed bid | `qualified_provider` | Implemented |
| Labor Market | Submit bid | `verified_contractor` | Designed |
| Insurance | Purchase coverage | `low_risk_profile` | Designed |
| Bridge | Cross-chain transfer | `authorized_signer` | Designed |

### Why O-Cap Works Across Contracts

1. **Same identity contract**: All O-Cap verification goes through Identity contract
2. **Standardized proof format**: `CapabilityProof` is universal
3. **Composable requirements**: Capabilities can require other credentials
4. **Decoupled from identity**: The holder's identity contract is irrelevant to verification

## O-Cap in Action: Use Cases

### Labor Markets

```
Alice proves:
- "I have a CS degree from an accredited university"
- "I have 5+ years of software engineering experience"
- "I have completed security training"

HR learns only: ✓ Alice meets requirements
                ✗ Alice's identity is NOT revealed

Why this matters:
- Alice can't be discriminated by age, gender, or name
- Alice's current employer doesn't know she's looking
- Competitors can't poach by offering more than she revealed
```

### Insurance

```
John proves:
- "I am over 25 years old"
- "I have no major violations in the last 3 years"
- "My annual mileage is under 15,000"

Insurer learns only: ✓ John meets risk criteria
                    ✗ John's identity is NOT revealed

Why this matters:
- Insurers can't discriminate based on protected characteristics
- No data breach exposes full profiles
- Customers prove characteristics, not surrender privacy
```

### Tendering

```
Bidders prove (sealed):
- "I have completed similar projects"
- "I have required certifications"
- "My team has relevant expertise"

Requester learns only: ✓ These bidders are qualified
                       ✗ Which bidder is which (until selection)

Why this matters:
- Prevents corruption (bidders don't know who's competing)
- Eliminates bias based on company size or reputation
- Enables fair competition on purely qualifications
```

## ZK Circuit Design

### Level 0: zk_only (Maximum Privacy)

Level 0 uses the safemath assertion pattern. The verifier learns **only** whether the proof is valid or invalid:

```zk
# From create_claim_v1.zk
# Proves: threshold <= attribute_value
# Verifier learns: only "proof valid/invalid"

range_check(64, attribute_value);
range_check(64, threshold);
ONE = witness_base(1);
attribute_plus_one = base_add(attribute_value, ONE);
less_than_strict(threshold, attribute_plus_one);
```

### Level 1: Selective Disclosure

Level 1 uses the **bounded equation construction** to return a public `predicate_result` bit:

```
threshold + delta = attribute_value + (1 - predicate_result) * 2^64
```

### Level 1 v2: LessThanOrEqual

The `create_claim_v1_l1_v2.zk` circuit uses the verified-sound `LessThanOrEqual` opcode:

```zk
range_check(64, attribute_value);
range_check(64, threshold);
bool_check(predicate_result);

is_lte = less_than_or_equal(threshold, attribute_value);
constrain_equal_base(is_lte, predicate_result);
```

### Multi-Credential Claims

The `create_claim_v1_multi.zk` circuit supports AND logic across multiple credentials:

```zk
is_lte_1 = less_than_or_equal(threshold_1, attribute_1);
is_lte_2 = less_than_or_equal(threshold_2, attribute_2);
is_lte_3 = less_than_or_equal(threshold_3, attribute_3);

# AND logic via multiplication
combined_predicates = base_mul(is_lte_1, is_lte_2);
combined_predicates = base_mul(combined_predicates, is_lte_3);
```

### Competency DAG Claims (0x0d)

The `create_claim_v1_dag.zk` circuit supports multiple credential paths with OR logic between paths:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Competency DAG Example                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PATH A:                          PATH B:                         │
│  High School → Associate's → Bachelor's (AND chain)               │
│                                    │                             │
│                                    ▼                             │
│                        Industry Certification                    │
│                                    │                             │
│                                    ▼                             │
│                    "Qualified Developer" (either path)           │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

```zk
# For each credential in the path (AND logic):
is_lte_1 = less_than_or_equal(threshold_1, attribute_1);
is_lte_2 = less_than_or_equal(threshold_2, attribute_2);
is_lte_3 = less_than_or_equal(threshold_3, attribute_3);

# Path satisfied if all credentials pass
path_satisfied = base_mul(is_lte_1, is_lte_2);
path_satisfied = base_mul(path_satisfied, is_lte_3);

constrain_equal_base(path_satisfied, ONE);
```

**Function:** `CreateClaimDAGV1` (0x0d)

**Data structures:**
```rust
/// A single credential in a DAG path
pub struct DAGCredential {
    pub nullifier: IntentNullifier,
    pub predicate_result: u8,
    pub claim_type: [u8; 32],
}

/// A single path (AND chain)
pub struct CredentialPath {
    pub credentials: Vec<DAGCredential>,
    pub path_hash: [u8; 32],
}

/// Full DAG structure
pub struct CompetencyDAG {
    pub dag_id: [u8; 32],
    pub name: Vec<u8>,
    pub paths: Vec<CredentialPath>,  // OR between paths
    pub dag_root: [u8; 32],
    pub issuer_pub: [u8; 32],
    pub created_at: u64,
    pub expires_at: u64,
}
```

### Ratio-Based Claims

The `create_claim_v1_ratio.zk` circuit enables supply-relative predicates:

```zk
ratio = base_div(my_value, total_supply);
is_lte = less_than_or_equal(threshold_ratio, ratio);
constrain_equal_base(is_lte, predicate_result);
```

## Roadmap

```
Level 0 (COMPLETE — May 2026)
├── O-Cap authorization (Register/Issue/Verify/Revoke)
├── Issuer-Holder-Verifier model
├── Single issuer credentials
├── Basic predicates (>=, ==, etc.)
├── On-chain verification
└── ZK proofs for attribute hiding

Level 1 (COMPLETE — May 2026)
├── Competency DAG structure
├── Derived competencies (prerequisite chains)
├── Multiple issuer support
├── Multi-credential AND claims (0x07)
├── Ratio-based predicates (0x08)
└── Credential chaining

Level 2 (Future)
├── Trust networks (Web of Trust + ZK)
├── Graduated disclosure based on trust
├── Anonymous reputation
└── Privacy-preserving referrals

Level 3 (Future)
├── K-Assets (knowledge assets)
├── Competency markets
├── Fractional ownership
└── Economic activation
```

## Privacy Analysis

### Attack: Credential Fishing

**Attack**: Attacker queries many credentials to find specific holders.

**Mitigation**:
- Nullifiers prevent credential enumeration
- ZK proofs don't reveal which credential is being used
- Different key per credential prevents linkage

### Attack: Credential Correlation

**Attack**: Same holder uses multiple credentials, linking them.

**Mitigation**:
- Different nullifier per credential
- ZK proofs don't reveal common attributes
- Holder secret is separate per credential

### Attack: Timing Analysis

**Attack**: Credential issuance timing reveals relationships.

**Mitigation**:
- Batched credential issuance
- Minimum delays between operations
- Time-stamped but not linkable

## Comparison

| Feature | Traditional KYC | ZK Email | DarkWow O-Cap |
|---------|-----------------|----------|--------------|
| Identity revealed | Everything | Email exists | Nothing (MVI) |
| Data minimization | None | Email only | Full |
| Revocability | Full | Limited | Full |
| O-Cap authorization | No | No | Yes |
| Offline verification | No | No | Yes |
| Competency DAG | No | No | Yes |
| Trust networks | No | No | Yes |
| K-Assets | No | No | Yes |

## References

- [DarkWow Identity Contract](../../src/contract/identity/)
- [DarkWow Identity README](../../src/contract/identity/README.md)
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)
- [Anonymous Credentials](https://en.wikipedia.org/wiki/Anonymous_credentials)
- [Differential Privacy](https://en.wikipedia.org/wiki/differential_privacy)
- [Web of Trust](https://en.wikipedia.org/wiki/web_of_trust)
