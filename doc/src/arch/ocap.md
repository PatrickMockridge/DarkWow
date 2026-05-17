# O-Cap & Composable Privacy

*This document describes O-Cap (Object Capability) authorization as the central paradigm for privacy-preserving authorization in DarkWow smart contracts, enabling composable privacy for social reproduction.*

---

## O-Cap: The Central Paradigm

O-Cap is **not a feature** or an extension - it is the **central paradigm** for authorization in DarkWow.

**The fundamental question:**
- ACL: "WHO has access to X?"
- O-Cap: "Can you prove you have access to X?"

**The key insight:** Authorization should be based on **what you can prove**, not **who you are**.

## The Ambient Authority Problem

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
│  → Can it be tricked into acting for another user?               │
│                                                                   │
│  PROBLEM: Every operation runs in an environment                 │
│  filled with ambient authority that can be exploited.             │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## O-Caps Eliminate Ambient Authority

O-Caps make authority **EXPLICIT** and **BOUNDED**. The proof IS the authority - nothing more.

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Eliminates Ambient Authority               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Alice creates a proof:                                           │
│  → Only the SPECIFIC capability is in the proof                  │
│  → No other permissions are present in the environment           │
│  → The verifier only sees the capability, not Alice's identity   │
│                                                                   │
│  Alice's code cannot:                                            │
│  → Access resources beyond the specific capability                │
│  → Leak data because it has no ambient identity to leak          │
│  → Be tricked because there's nothing to impersonate              │
│                                                                   │
│  KEY INSIGHT: O-Caps make authority EXPLICIT and BOUNDED.       │
│               The proof IS the authority - nothing more.           │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## The Intuitive Privacy Rule

```
┌─────────────────────────────────────────────────────────────────┐
│            O-Cap Makes Privacy Reasoning Intuitive                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ACL PRIVACY REASONING (hard):                                    │
│  "Does this operation reveal Alice's identity?"                   │
│  "Can this log be linked to other logs?"                          │
│  "What does the admin see?"                                       │
│  "If data breaches, what is exposed?"                            │
│  → Complex, contextual, hard to reason about                     │
│                                                                   │
│  O-Cap PRIVACY REASONING (simple):                               │
│  "What capability is being proven?"                               │
│  "That's all the verifier learns - nothing more."                 │
│  → SIMPLE, LOCAL, INTUITIVE                                      │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────┐      │
│  │  In O-Cap, you reveal ONLY what you prove.             │      │
│  │  If you prove "can_vote", verifier learns "can_vote"   │      │
│  │  Nothing else. Nothing more. Always.                    │      │
│  └─────────────────────────────────────────────────────────┘      │
│                                                                   │
│  WHY THIS IS INTUITIVE:                                          │
│  - Privacy is PROVABLE, not just policy                          │
│  - The ZK proof GUARANTEES what is/isn't revealed                │
│  - No trust in system admins or database security                 │
│  - Reasoning is LOCAL to the capability being proven               │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## The Pattern: How O-Caps Work

Every privacy-preserving DarkWow contract needs to solve the same fundamental problem:

**How do you authorize an action without revealing who you are or what you're doing?**

The solution is a reusable pattern with four components:

| Component | Purpose | Appears In |
|-----------|---------|------------|
| **Commitment** | Creates a private capability bound to a secret | All contracts |
| **Nullifier** | Consumes the capability exactly once | All contracts |
| **Proof** | Verifies authorization without revealing secret | All contracts |
| **Revocation** | Allows issuer to invalidate before use | Identity, some others |

```
┌─────────────────────────────────────────────────────────────────┐
│              Private Authorization Lifecycle                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. COMMIT                                                        │
│     User creates commitment = H(secret, params)                    │
│     → Private capability exists on-chain                          │
│     → No one knows the secret                                    │
│     → Capability is bound to user                                 │
│                                                                   │
│  2. PROVE (optional intermediate step)                            │
│     User generates ZK proof                                        │
│     → Proves they know the secret                                 │
│     → Proves commitment is valid                                 │
│     → Proves predicate is satisfied                               │
│     → Nothing revealed to observers                                │
│                                                                   │
│  3. CONSUME                                                       │
│     User provides nullifier = H(secret)                            │
│     → Capability consumed exactly once                             │
│     → Cannot be used again (replay protection)                   │
│     → Action executed atomically                                  │
│                                                                   │
│  4. REVOKE (optional)                                            │
│     Issuer marks nullifier as revoked                             │
│     → Commitment invalidated before use                            │
│     → Issuer can cancel before consumed                          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Why O-Caps

### The Problem with Traditional Authorization

Traditional blockchain authorization reveals too much:
- **Public keys** link transactions to identities
- **Signatures** prove ownership but don't hide the transaction
- **Balances** are visible to everyone
- **Transaction graphs** can be analyzed to deanonymize users
- **Ambient authority** exists in the environment, exploitable by code

### The O-Cap Solution

O-Caps achieve **authorization without revelation**:

1. **Capability bounds authority**: The proof IS the authority - nothing more
2. **Commitment hides the secret**: `H(secret, params)` means only the holder knows the secret
3. **Nullifier prevents reuse**: `H(secret)` can only be spent once
4. **Proof enables authorization without disclosure**: ZK proof shows the secret is known without revealing it
5. **Revocation provides control**: Issuer can invalidate before use

**The key insight**: O-Caps answer the question "WHAT can you prove?" instead of "WHO are you?"

---

## Authorization Inversion: Design Rationale

*O-Cap authorization addresses a fundamental property of identity-based access control. What follows describes why this fork chose O-Cap over ACLs for privacy-preserving authorization.*

### The ACL Privacy Gap

Access Control Lists (ACLs) have an inherent privacy property when the authorization decision itself reveals information:

```
┌─────────────────────────────────────────────────────────────────┐
│            ACL Privacy Gap                                       │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  OBSERVATION:                                                     │
│  When a verifier observes "ACCESS GRANTED", they learn the      │
│  principal came from the authorized set — reducing anonymity.   │
│                                                                   │
│  Let A = {principals authorized for action X}                    │
│  When access is granted, verifier learns: ∃p ∈ A : p performed X│
│  This reveals information about p from the set A                │
│                                                                   │
│  The O-Cap approach: don't condition on identity at all —       │
│  instead, prove possession of a capability without revealing    │
│  which identity holds it.                                        │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

**Note:** This is not a flaw in ACLs. ACLs are simpler to implement, easier to audit, and are the right choice for systems that don't require anonymous authorization. The privacy property is a consequence of the ACL abstraction — identity-based authorization naturally reveals identity-adjacent information. For privacy-preserving systems, O-Caps provide an alternative model that avoids this inherent tradeoff.

### Authorization Inversion

Authorization inversion resolves this by changing the fundamental question:

```
ACL:         "WHO has access to X?"
O-Cap:       "Can you prove you have access to X?"

ACL Response: alice@company → access granted (identity revealed)
O-Cap Response: prove(predicate) → VALID (identity hidden)
```

Instead of checking "is this identity authorized?", O-Cap checks "does this proof satisfy the predicate?" The verifier learns:
- **What** capability is being proven
- **Whether** the predicate is satisfied (1/0)
- **Nothing** about who holds the capability

### Bounded Authority in ZK

The O-Cap verification evaluates a zero-knowledge predicate:

```
∃ w : P_{r,s}(w) = 1
```

Where:
- `w` = witness (hidden credential data)
- `P_{r,s}` = predicate with public params r and secret params s
- Result = 1 if requirements are met

**Bounded Authority Guarantee:**
```
┌─────────────────────────────────────────────────────────────────┐
│         What the Verifier Learns (and doesn't learn)                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  LEARNS:                                                          │
│  ✓ capability_id (what's being proven)                          │
│  ✓ predicate_result (1 = pass, 0 = fail)                        │
│  ✓ nullifier (exists, not who)                                   │
│  ✓ proof validity                                                │
│                                                                   │
│  DOES NOT LEARN:                                                  │
│  ✗ holder identity                                               │
│  ✗ actual attribute values                                       │
│  ✗ threshold values                                              │
│  ✗ full credential contents                                      │
│  ✗ when credential expires                                       │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

This bounded disclosure is **guaranteed** by the ZK proof construction — not by trust in the system.

### LTE Gate: Formal Verification of Threshold Predicates

The LessThanOrEqual (LTE) operation is central to O-Cap predicates, enabling threshold comparisons like `role >= senior_engineer`. `LessThanOrEqual` is a **DarkWow addition to the zkVM** — it does not exist in upstream DarkWow. Both the implementation and the Lean4 formal verification were completed on this fork.

```
┌─────────────────────────────────────────────────────────────────┐
│                 LTE Gate: Formally Verified                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  IMPLEMENTATION: less_than_or_equal(threshold, attribute_value) │
│  (DarkWow addition — not in upstream)                             │
│                                                                   │
│  VERIFICATION: Lean 4 formal methods (completed on this fork)     │
│  - Proves circuit soundness for threshold comparisons             │
│  - No floating-point approximations                             │
│  - Mathematically correct boundary conditions                     │
│                                                                   │
│  WHY IT MATTERS:                                                  │
│  O-Cap predicates rely on LTE for role thresholds,               │
│  balance minimums, and credential requirements. If LTE          │
│  were buggy, O-Cap authorization could be bypassed.             │
│  Formal verification eliminates this entire class of risk.       │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

---

*This framework explains why O-Cap authorization is the chosen approach for DarkWow's privacy-preserving authorization model.*

## O-Cap Full Opcode Reference (0x09-0x0c)

The Identity contract implements O-Cap authorization via these opcodes:

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x09` | `RegisterCapabilityV1` | Register a new capability type (e.g., `can_merge_pr`, `can_work_on_freelance_jobs`) |
| `0x0a` | `IssueCapabilityV1` | Issue a capability to a holder based on their credential |
| `0x0b` | `VerifyCapabilityV1` | Verify a capability proof (cross-contract authorization) |
| `0x0c` | `RevokeCapabilityV1` | Revoke a capability (issuer action) |

### Why O-Cap Changes Everything

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap as the Composition Primitive                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ACL REASONING (contract-specific):                              │
│  "Alice has identity... Alice has credentials...                   │
│   ...therefore Alice can vote"                                   │
│  → Complex, cross-contract, identity-dependent                    │
│                                                                   │
│  O-Cap REASONING (capability-based):                            │
│  "Prove can_vote capability... therefore ACCESS GRANTED"         │
│  → Simple, local, identity-independent                            │
│                                                                   │
│  The key insight: Authorization (what you CAN do)               │
│  doesn't require Identity (who you ARE)                          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### How O-Caps Compose

O-Caps compose through **capability chaining** - derived capabilities build on base capabilities:

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Composition                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  IDENTITY CONTRACT (Base O-Cap):                                 │
│  ┌─────────────────────────────────────────────────────────┐      │
│  │ Credential: "software_engineer_v1"                       │      │
│  │ Proves: role_level >= 5                                  │      │
│  │ Issuer: ACME_Corp                                         │      │
│  └─────────────────────────────────────────────────────────┘      │
│                           │                                        │
│                           ▼                                        │
│  DERIVED CAPABILITY (DAO):                                       │
│  ┌─────────────────────────────────────────────────────────┐      │
│  │ can_propose = credential.software_engineer_v1           │      │
│  │             + predicate(role >= 5)                      │      │
│  └─────────────────────────────────────────────────────────┘      │
│                           │                                        │
│                           ▼                                        │
│  FURTHER DERIVED (Tender):                                       │
│  ┌─────────────────────────────────────────────────────────┐      │
│  │ can_submit_bid = can_propose + credential.other_requirements│  │
│  └─────────────────────────────────────────────────────────┘      │
│                                                                   │
│  COMPOSITION IS SIMPLE:                                           │
│  - Each contract adds requirements, not amplifying authority       │
│  - Each proof only reveals what's being proven                   │
│  - No cross-contract identity linking                             │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### O-Caps Reduce Attack Surface Dramatically

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Reduces Attack Surface                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ACL Attack Surface:                                              │
│  1. Stolen credentials → full identity compromise               │
│  2. SQL injection → ACL modification                             │
│  3. Privilege escalation → admin access                          │
│  4. Insider threat → ACL bypass                                  │
│  5. Cross-site scripting → session hijacking                     │
│  → ATTACK SURFACE: Every point where identity exists              │
│                                                                   │
│  O-Cap Attack Surface:                                            │
│  1. Stolen credential_secret → specific cap only                 │
│  2. No ACL to modify (capabilities are derived)                  │
│  3. No privilege escalation (authority is explicit)             │
│  4. No insider threat (identity never in system)                  │
│  5. No session to hijack (proof is transient)                    │
│  → ATTACK SURFACE: Only the specific capability being proven       │
│                                                                   │
│  WHY O-Caps REDUCE attack surface:                               │
│  - Authority is BOUNDED to what's being proven                   │
│  - Identity is NEVER in the system                               │
│  - Proofs are TRANSIENT (don't persist as ambient authority)     │
│  - Revocation is CASCADING (credential revoke → all caps fail)   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

---

## Industries Vital to Social Reproduction

These industries are essential for societal survival and NOW ENABLED with O-Cap privacy:

| Industry | Why Vital | O-Cap Capability | Privacy Guarantees |
|----------|-----------|-----------------|-------------------|
| **Healthcare** | Medical decisions should be private | `can_consult_provider` | Provider identity hidden, diagnosis hidden |
| **Domestic Labor** | Care work, cleaning, cooking | `can_provide_childcare` | Worker/family identity hidden |
| **Education** | Tutoring, skill training | `can_enroll_in_program` | Student identity hidden, grades hidden |
| **Freelance Work** | Programming, writing, design | `can_work_on_freelance_jobs` | Identity/employer/salary hidden |
| **Mutual Insurance** | Community risk pooling | `can_purchase_coverage` | Medical history hidden |
| **Union Organization** | Collective bargaining | `can_participate_in_strike_vote` | Membership/vote hidden |

### Healthcare O-Cap Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  Healthcare O-Cap Capabilities                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Provider registers:                                              │
│  - can_prescribe_controlled_substances                          │
│    requires: credential.medical_license                        │
│    requires: credential.board_certified                        │
│    requires: predicate(years_exp >= 5)                         │
│                                                                  │
│  Patient verifies:                                              │
│  - can_consult_this_provider(prove: can_prescribe...)           │
│  - Verifier learns: Provider is licensed                        │
│  - Verifier DOES NOT learn: Provider name, institution          │
│                                                                  │
│  Insurance claims via:                                          │
│  - can_file_insurance_claim(prove: verified_patient)           │
│  - Claim verified WITHOUT revealing diagnosis details             │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Domestic Labor O-Cap Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  Domestic Labor O-Cap Capabilities                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Care worker registers:                                         │
│  - can_provide_childcare_services                              │
│    requires: credential.cpr_certified                          │
│    requires: credential.background_check_passed                │
│    requires: predicate(experience_years >= 2)                  │
│                                                                  │
│  Family verifies:                                               │
│  - can_hire_caregiver(prove: can_provide_childcare...)         │
│  - Verifier learns: Worker is qualified                        │
│  - Verifier DOES NOT learn: Worker's real name, location       │
│                                                                  │
│  Payment via Money contract:                                    │
│  - Amount hidden via ZK commitment                             │
│  - No wage records visible                                      │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Education O-Cap Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  Education O-Cap Capabilities                                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  University registers:                                          │
│  - can_issue_degree(credential_hierarchy)                       │
│    requires: credential.high_school_diploma                     │
│    requires: credential.bachelors_degree                        │
│    requires: credential.masters_degree (for PhD)               │
│                                                                  │
│  Student proves:                                                │
│  - can_enroll_in_phd_program(prove: can_issue_degree)         │
│  - Verifier learns: Student meets degree requirements          │
│  - Verifier DOES NOT learn: Student name, grades, school       │
│                                                                  │
│  Employer verifies:                                             │
│  - has_professional_certification(prove: can_issue_degree)     │
│  - Verifier learns: Candidate is certified                      │
│  - Verifier DOES NOT learn: Which school, grades, when         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Freelance Work O-Cap Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  Freelance O-Cap Capabilities                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Worker registers:                                              │
│  - can_work_on_freelance_jobs                                   │
│    requires: credential.senior_engineer                          │
│    requires: predicate(experience >= 5)                         │
│    requires: credential.domain_expertise                         │
│                                                                  │
│  Client posts job requiring:                                    │
│  - required_capability: can_work_on_freelance_jobs             │
│                                                                  │
│  Worker submits bid:                                            │
│  - prove: can_work_on_freelance_jobs                           │
│  - Verifier learns: Worker meets job requirements               │
│  - Verifier DOES NOT learn: Worker's identity, employer,       │
│    current salary, age, gender, ethnicity                       │
│                                                                  │
│  Milestone payments via Money contract:                         │
│  - Amount hidden via ZK commitment                              │
│  - No salary history visible                                   │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Mutual Insurance O-Cap Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  Insurance O-Cap Capabilities                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Member registers:                                              │
│  - can_purchase_standard_coverage                              │
│    requires: credential.low_risk_profile                        │
│    requires: predicate(age >= 25)                              │
│    requires: credential.no_major_violations_3yrs               │
│                                                                  │
│  Insurer verifies:                                              │
│  - prove: can_purchase_standard_coverage                       │
│  - Verifier learns: Customer is low-risk                       │
│  - Verifier DOES NOT learn: Age, exact mileage, medical        │
│    history, lifestyle details                                  │
│                                                                  │
│  Claims via:                                                    │
│  - can_file_claim(prove: verified_policy_holder)              │
│  - Claim verified WITHOUT revealing accident details             │
│                                                                  │
│  Premiums calculated via base_div (0x58):                       │
│  - Complex actuarial math now possible in ZK                    │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Union Organization O-Cap Flow

```
┌─────────────────────────────────────────────────────────────────┐
│  Union O-Cap Capabilities                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Union registers:                                                │
│  - can_participate_in_strike_vote                              │
│    requires: credential.union_member                           │
│    requires: credential.currently_employed                     │
│                                                                  │
│  Worker proves:                                                 │
│  - can_vote_on_strike(prove: can_participate...)              │
│  - Verifier learns: Vote is legitimate                         │
│  - Verifier DOES NOT learn: Worker's identity, employer,       │
│    position, salary, location                                  │
│                                                                  │
│  Employer surveillance PREVENTED:                               │
│  - Employer cannot see who is union member                      │
│  - Employer cannot see strike vote participation               │
│  - Workers organize WITHOUT exposure                           │
│                                                                  │
│  Treasury via DAO-Escrow:                                       │
│  - Strike fund hidden via ZK                                    │
│  - No donation records visible                                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## Resolution: Plain Contracts Deprecated

**COMPLETED**: `base_div` (0x58) and `LessThanOrEqual` (0x55) are now implemented and verified sound.

The `contract_plain/` directory has been **deleted**. ZK contracts now have full functionality via:
- O-Cap authorization (0x09-0x0d) for cross-contract capability verification
- `base_div` (0x58) for actuarial calculations
- `LessThanOrEqual` (0x55) for predicate evaluation

---

## Cross-Contract Composability

## State Primitives

All DarkWow contracts must represent state. The common patterns are:

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
│     └─────────┘         │     Root     │                          │
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

## Authorization Primitives

O-Cap authorization is the **central paradigm** that all DarkWow contracts use for authorization:

```
┌─────────────────────────────────────────────────────────────────┐
│              O-Cap Authorization Pattern                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  COMMITMENT          NULLIFIER           PROOF                   │
│  H(secret, params)  H(secret, ...)      ZK(prover knows secret) │
│       │                  │                    │                   │
│       │                  │                    │                   │
│       ▼                  ▼                    ▼                   │
│  ┌─────────────────────────────────────────────────────┐         │
│  │  O-CAP: Authorization is BOUNDED and EXPLICIT        │         │
│  │  • The proof IS the authority - nothing more          │         │
│  │  • Commitment exists and is valid                     │         │
│  │  • Nullifier has not been spent                       │         │
│  │  • Proof verifies predicate without revealing secret   │         │
│  └─────────────────────────────────────────────────────┘         │
│                                                                   │
│  KEY INSIGHT: O-Cap makes authority EXPLICIT and BOUNDED.         │
│               Every DarkWow contract uses this pattern.            │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

**Identity Contract as O-Cap Baseline:**

The [Identity Contract](../../src/contract/identity/) implements the canonical O-Cap pattern with full opcode support (0x09-0x0c):

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

```
┌─────────────────────────────────────────────────────────────────┐
│                 Token-Gated Access                                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: User holds >= N tokens                            │
│                                                                   │
│  Implementation:                                                 │
│  1. User creates commitment: H(balance_secret, token, amount)   │
│  2. User generates ZK proof: "I know secret such that             │
│     commitment = H(secret, token, amount) AND amount >= N"      │
│  3. Contract verifies: commitment exists, proof valid            │
│                                                                   │
│  Privacy: Only reveals "amount >= N", not actual balance         │
│                                                                   │
│  Used in: DAO voting, premium features, liquidity pools           │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern 2: Attestation-Based Claims

```
┌─────────────────────────────────────────────────────────────────┐
│               Attestation-Based Claims                                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: Attestor has attested to a claim                   │
│                                                                   │
│  Implementation:                                                 │
│  1. Attestor creates: attestation = H(data, attestor_key)       │
│  2. Claimant creates: claim = ZK proof of attestation access   │
│  3. Contract verifies: attestation exists, claim valid,          │
│     predicate satisfied                                           │
│                                                                   │
│  Privacy: Only reveals "valid claim", not underlying data        │
│                                                                   │
│  Used in: Deliverable verification, competency claims,             │
│           oracle data consumption, event attestation               │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Pattern 3: Time-Locked Actions

```
┌─────────────────────────────────────────────────────────────────┐
│                  Time-Locked Actions                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  PRECONDITION: Action can only happen after timestamp T           │
│                                                                   │
│  Implementation:                                                  │
│  1. State includes: time_lock = H(T, action_description)         │
│  2. Consensus ensures: block.timestamp >= T                      │
│  3. Contract verifies: current time >= lock time                  │
│                                                                   │
│  Privacy: Locked action description hidden until unlock           │
│                                                                   │
│  Used in: Vesting schedules, delayed withdrawals, expiration      │
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
│  1. Each party creates: partial_sig_i = sign(secret_i, msg)      │
│  2. Aggregator combines: full_sig = combine(partial_sigs)         │
│  3. Contract verifies: threshold met, all signers authorized     │
│                                                                   │
│  Privacy: Individual signers revealed only if needed              │
│                                                                   │
│  Used in: DAO proposals, bridge admin keys, upgrade gates         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## O-Cap Authorization: Now Fully Implemented

The Identity contract provides full O-Cap authorization via these opcodes:

| Opcode | Function | Description |
|--------|----------|-------------|
| `0x09` | `RegisterCapabilityV1` | Register a new capability type (e.g., `can_merge_pr`) |
| `0x0a` | `IssueCapabilityV1` | Issue a capability to a holder based on their credential |
| `0x0b` | `VerifyCapabilityV1` | Verify a capability proof |
| `0x0c` | `RevokeCapabilityV1` | Revoke a capability |

```
┌─────────────────────────────────────────────────────────────────┐
│            O-Cap Authorization: Now Fully Implemented                 │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  IDENTITY CONTRACT (0x09-0x0c):                                 │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ RegisterCapabilityV1: Define "can_merge_pr"                    │ │
│  │   - credential_requirement: role >= senior_engineer          │ │
│  │   - issuer: ACME_Corp                                      │ │
│  │                                                              │ │
│  │ IssueCapabilityV1: Alice receives "can_merge_pr"            │ │
│  │   - Proves: Alice has credential with role >= 5             │ │
│  │   - Hides: Alice's actual role, employer, salary           │ │
│  │                                                              │ │
│  │ VerifyCapabilityV1: Verify Alice's capability proof         │ │
│  │   - Returns: can_merge_pr = VALID                           │ │
│  │   - Hides: Everything else                                 │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                           │                                       │
│                           ▼                                       │
│  OTHER CONTRACT (DAO, Labor Market, etc):                       │
│  ┌─────────────────────────────────────────────────────────────┐ │
│  │ call identity.verify_capability(params)                     │ │
│  │   - capability_id: can_merge_pr                            │ │
│  │   - proof: Alice's ZK proof                                │ │
│  │   - Returns: true/false                                     │ │
│  │                                                              │ │
│  │ Result: Alice can merge (without revealing identity)         │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

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

### Case Study: Tender + Labor Market + Attestation

The Tender and Labor Market contracts demonstrate DarkWow's integration of sealed-bid procurement with attestation-based competency verification and O-Cap capability authorization.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│              Tender + Labor Market + Attestation + O-Cap Composability                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌──────────────────────┐         ┌──────────────────────┐                  │
│  │    Identity (O-Cap)  │         │       Tender         │                  │
│  │                      │         │                       │                  │
│  │  ┌────────────────┐  │         │  ┌────────────────┐  │                  │
│  │  │ RegisterCap()   │──┼─────────┼──│ SubmitBidV1()  │  │                  │
│  │  │ + IssueCap()   │  │         │  │ + cap_id      │  │                  │
│  │  └────────────────┘  │         │  └───────┬────────┘  │                  │
│  │                       │         │          │            │                  │
│  │  O-Cap Capability     │         │  Sealed Bid          │                  │
│  │  Reference            │         │  + Capability ID       │                  │
│  └───────────────────────┘         └───────────────────────┘                  │
│                                    │                                           │
│                                    │    Winner Selected                       │
│                                    ▼                                           │
│  ┌────────────────────────────────────────────────────────────────┐           │
│  │                    Labor Market                                  │           │
│  │                                                                 │           │
│  │  ┌──────────────────────────────────────────────────────────┐  │           │
│  │  │ CreateJobV1()                                             │  │           │
│  │  │ - Creates job from tender winner                          │  │           │
│  │  │ - Sets required_capability from tender specification       │  │           │
│  │  │ - Sets payment_amount from winning bid                     │  │           │
│  │  │ - Sets deadline from tender delivery_deadline             │  │           │
│  │  └──────────────────────────────────────────────────────────┘  │           │
│  │                                                                 │           │
│  │  ┌──────────────────────────────────────────────────────────┐  │           │
│  │  │ SubmitDeliverableV1()                                     │  │           │
│  │  │ - Worker submits claim_id from attestation               │  │           │
│  │  │ - Labor market verifies claim via Attestation contract    │  │           │
│  │  │ - VerifyCapabilityV1 verifies worker's O-Cap capability  │  │           │
│  │  └──────────────────────────────────────────────────────────┘  │           │
│  └────────────────────────────────────────────────────────────────┘           │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Tender State Machine

The tender contract implements a sealed-bid workflow:

```
Created ──[SubmitBid]──> Bidding ──[Close]──> Revealed ──[Select]──> Awarded
                                               │
                                               └──[Cancel]──> Cancelled
```

### O-Cap Integration: Privacy for Workers

O-Cap capabilities provide privacy for workers by hiding identity during job applications:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│            O-Cap Tender + Labor Market Flow                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  IDENTITY CONTRACT (O-Cap 0x09-0x0c):                                        │
│     │                                                                       │
│     │  RegisterCapability("qualified_contractor"):                           │
│     │    - requires: credential.professional_license                      │
│     │    - requires: predicate(experience >= 5)                           │
│     │    - issuer: Industry Authority                                      │
│     │                                                                       │
│     │  IssueCapability(worker, "qualified_contractor"):                      │
│     │    - worker proves: role >= senior, license valid                   │
│     │    - Hides: name, employer, salary, exact experience               │
│     │                                                                       │
│     ▼                                                                       │
│  TENDER:                                                                    │
│     │                                                                       │
│     │  CreateJob(required_capability="qualified_contractor"):                │
│     │    - Job listing visible (capability required)                        │
│     │    - Prover identity HIDDEN                                          │
│     │                                                                       │
│     │  SubmitBid(prove: qualified_contractor):                              │
│     │    - Verifier learns: qualified_contractor = VALID                  │
│     │    - Verifier DOES NOT learn: Who, company, salary, experience      │
│     │                                                                       │
│     ▼                                                                       │
│  LABOR MARKET:                                                              │
│     │                                                                       │
│     │  CreateJob(required_capability="qualified_contractor"):                │
│     │    - Same pattern - capability required                               │
│     │    - Worker identity HIDDEN                                          │
│     │                                                                       │
│     │  AcceptJob(verify: worker.has("qualified_contractor")):               │
│     │    - Uses VerifyCapabilityV1 (0x0b)                                  │
│     │    - Verifier learns: worker has capability                          │
│     │    - Verifier DOES NOT learn: worker identity                       │
│     │                                                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Why O-Cap matters for labor**:
- Employer cannot discriminate by age, gender, ethnicity (not revealed)
- Worker's current employer cannot see they're job hunting (identity hidden)
- Salary negotiation is neutralized (actual salary not revealed)
- Workers compete on capability, not identity

### Case Study: The Complete O-Cap Pipeline - Workers to Executives

This case study demonstrates how O-Cap capabilities flow through the entire DarkWow ecosystem, from supply chain workers proving qualifications via Identity, to executives securing contracts via Tender, to underwriters providing coverage via Insurance Market.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│              COMPLETE O-CAP PIPELINE: IDENTITY → TENDER → LABOR MARKET → INSURANCE      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ╔═══════════════════════════════════════════════════════════════════════╗  │
│  ║                        SUPPLY CHAIN WORKERS                             ║  │
│  ║                   (Identity Contract - O-Cap 0x09-0x0d)                 ║  │
│  ╠═══════════════════════════════════════════════════════════════════════╣  │
│  ║                                                                       ║  │
│  ║  ISSUER (e.g., Professional Association)                             ║  │
│  ║     │                                                                  ║  │
│  ║     │  RegisterCapability("verified_smart_contract_auditor")         ║  │
│  ║     │    - requires: credential.auditor_license                      ║  │
│  ║     │    - requires: predicate(experience >= 3)                      ║  │
│  ║     │    - issuer: Industry Authority                                 ║  │
│  ║     │                                                                  ║  │
│  ║     │  IssueCapability(alice, "verified_smart_contract_auditor")    ║  │
│  ║     │    - alice proves: license valid, experience >= 3            ║  │
│  ║     │    - Hides: name, employer, exact salary, projects           ║  │
│  ║     │                                                                  ║  │
│  ║     │  CreateClaimDAG("senior_engineer")                            ║  │
│  ║     │    - PATH A: BSC + 5yr exp + senior lead                      ║  │
│  ║     │    - PATH B: Industry cert + 10yr exp                          ║  │
│  ║     │    - Alice satisfies PATH B (hidden)                          ║  │
│  ║     │                                                                  ║  │
│  ║  RESULT: Alice has "verified_smart_contract_auditor" capability      ║  │
│  ║          AND "senior_engineer" DAG competency                        ║  │
│  ║          WITHOUT revealing: identity, employer, exact credentials  ║  │
│  ╚═══════════════════════════════════════════════════════════════════════╝  │
│                                    │                                        │
│                                    ▼                                        │
│  ╔═══════════════════════════════════════════════════════════════════════╗  │
│  ║                         TENDER PROCESS                                  ║  │
│  ║                   (Tender Contract - O-Cap 0x07-0x08)                   ║  │
│  ╠═══════════════════════════════════════════════════════════════════════╣  │
│  ║                                                                       ║  │
│  ║  EXECUTIVE (Project Owner)                                            ║  │
│  ║     │                                                                  ║  │
│  ║     │  CreateTenderWithCapability(                                   ║  │
│  ║     │    title: "DeFi Protocol Security Audit"                       ║  │
│  ║     │    required_capability: "verified_smart_contract_auditor",    ║  │
│  ║     │    required_dag_id: Some("senior_engineer")                    ║  │
│  ║     │  )                                                              ║  │
│  ║     │                                                                  ║  │
│  ║  ALICE SUBMITS BID:                                                   ║  │
│  ║     │                                                                  ║  │
│  ║     │  SubmitBidWithCapability(                                      ║  │
│  ║     │    tender_id: X,                                                ║  │
│  ║     │    capability_proof: ZK(VerifyCapability(                      ║  │
│  ║     │      "verified_smart_contract_auditor")),                      ║  │
│  ║     │    dag_proof: ZK(CreateClaimDAG("senior_engineer")),           ║  │
│  ║     │    sealed_bid_amount: 50000                                    ║  │
│  ║     │  )                                                              ║  │
│  ║     │                                                                  ║  │
│  ║  TENDER VERIFIES:                                                    ║  │
│  ║     │                                                                  ║  │
│  ║     │  verify_capability("verified_smart_contract_auditor")         ║  │
│  ║     │  verify_dag_claim("senior_engineer")                           ║  │
│  ║     │                                                                  ║  │
│  ║  RESULT: ✓ Alice's bid accepted                                       ║  │
│  ║          ✗ Alice's identity NOT revealed (only capability proven)   ║  │
│  ║          ✗ Alice's employer NOT revealed                              ║  │
│  ║          ✗ Other bidders don't know who competed                     ║  │
│  ║     │                                                                  ║  │
│  ║  WINNER SELECTED:                                                     ║  │
│  ║     │                                                                  ║  │
│  ║     │  RevealBid() → Alice wins with 50000                          ║  │
│  ║     │                                                                  ║  │
│  ║     │  SelectWinner() → Job created in Labor Market                  ║  │
│  ╚═══════════════════════════════════════════════════════════════════════╝  │
│                                    │                                        │
│                                    ▼                                        │
│  ╔═══════════════════════════════════════════════════════════════════════╗  │
│  ║                      LABOR MARKET EXECUTION                            ║  │
│  ║                   (Labor Market - O-Cap 0x0d)                         ║  │
│  ╠═══════════════════════════════════════════════════════════════════════╣  │
│  ║                                                                       ║  │
│  ║  JOB CREATED FROM TENDER WIN:                                         ║  │
│  ║     │                                                                  ║  │
│  ║     │  CreateJob(                                                    ║  │
│  ║     │    client: project_owner,                                      ║  │
│  ║     │    worker: alice (via capability),                              ║  │
│  ║     │    required_capability: "verified_smart_contract_auditor",     ║  │
│  ║     │    payment: 50000                                               ║  │
│  ║     │  )                                                              ║  │
│  ║     │                                                                  ║  │
│  ║  ALICE ACCEPTS JOB:                                                   ║  │
│  ║     │                                                                  ║  │
│  ║     │  AcceptJobWithCapability(                                      ║  │
│  ║     │    job_id: Y,                                                   ║  │
│  ║     │    capability_proof: ZK(VerifyCapability(                      ║  │
│  ║     │      "verified_smart_contract_auditor")),                      ║  │
│  ║     │  )                                                              ║  │
│  ║     │                                                                  ║  │
│  ║  ALICE DELIVERS WORK:                                                 ║  │
│  ║     │                                                                  ║  │
│  ║     │  SubmitDeliverable(claim_id: Z)                                ║  │
│  ║     │    - Attestation verifies work completion                      ║  │
│  ║     │    - Payment released via Money contract                       ║  │
│  ╚═══════════════════════════════════════════════════════════════════════╝  │
│                                    │                                        │
│                                    ▼                                        │
│  ╔═══════════════════════════════════════════════════════════════════════╗  │
│  ║                      INSURANCE MARKET                                  ║  │
│  ║                (Insurance Market - O-Cap 0x09-0x0c)                  ║  │
│  ╠═══════════════════════════════════════════════════════════════════════╣  │
│  ║                                                                       ║  │
│  ║  PROJECT OWNER PURCHASES COVERAGE:                                    ║  │
│  ║     │                                                                  ║  │
│  ║     │  CreateMarket(                                                 ║  │
│  ║     │    risk_type: SmartContractHack,                                ║  │
│  ║     │    required_underwriter_capability: "auditor_bond",            ║  │
│  ║     │    required_buyer_capability: Some("institutional_inv"),      ║  │
│  ║     │  )                                                              ║  │
│  ║     │                                                                  ║  │
│  ║  UNDERWRITER PROVIDES COVERAGE:                                        ║  │
│  ║     │                                                                  ║  │
│  ║     │  UnderwriteWithCapability(                                     ║  │
│  ║     │    proof: ZK(VerifyCapability("auditor_bond")),               ║  │
│  ║     │  )                                                              ║  │
│  ║     │                                                                  ║  │
│  ║  PROJECT OWNER PURCHASES COVERAGE:                                    ║  │
│  ║     │                                                                  ║  │
│  ║     │  PurchaseCoverageWithCapability(                              ║  │
│  ║     │    proof: ZK(VerifyCapability("institutional_inv")),          ║  │
│  ║     │  )                                                              ║  │
│  ║     │                                                                  ║  │
│  ║  CLAIM FILED (if audit finds vulnerability):                          ║  │
│  ║     │                                                                  ║  │
│  ║     │  FileClaim(evidence: audit_report)                             ║  │
│  ║     │                                                                  ║  │
│  ║     │  ResolveClaimWithCapability(                                  ║  │
│  ║     │    proof: ZK(VerifyCapability("oracle_resolution"))           ║  │
│  ║     │  )                                                              ║  │
│  ║     │                                                                  ║  │
│  ║  RESULT: Payout released, coverage maintained                        ║  │
│  ╚═══════════════════════════════════════════════════════════════════════╝  │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐  │
│  │                    THE ELEGANT WEAVE                                      │  │
│  ├─────────────────────────────────────────────────────────────────────────┤  │
│  │                                                                         │  │
│  │  WORKER (alice) flows through entire system WITHOUT revealing identity: │  │
│  │                                                                         │  │
│  │  1. Proves "verified_smart_contract_auditor" via Identity              │  │
│  │  2. Proves "senior_engineer" DAG via Identity                          │  │
│  │  3. Submits bid to Tender WITHOUT revealing employer/salary            │  │
│  │  4. Wins tender, job created in Labor Market                           │  │
│  │  5. Accepts job via capability (same one from Identity)              │  │
│  │  6. Delivers work, receives payment via Money                          │  │
│  │  7. Later acts as underwriter via Insurance Market                      │  │
│  │     (proves "auditor_bond" capability)                                │  │
│  │                                                                         │  │
│  │  THE CAPABILITY FOLLOWS ALICE EVERYWHERE - IDENTITY NEVER REVEALED    │  │
│  │                                                                         │  │
│  └─────────────────────────────────────────────────────────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Key Insights from the Complete Pipeline:**

1. **Single Capability, Multiple Uses**: Alice's "verified_smart_contract_auditor" capability works across Identity, Tender, Labor Market, and Insurance - she never re-proves her identity, only reuses the same capability proof.

2. **Identity Hidden at Every Step**: From job application to contract execution to underwriter bonding, Alice's identity remains hidden. Only her capabilities are revealed.

3. **DAGs Enable Flexible Qualification**: The "senior_engineer" DAG allows multiple qualification paths - Alice could have taken PATH A or PATH B, but the verifier only learns she qualified, not which path.

4. **Composability is Simple**: Each contract only needs to call `verify_capability()` on the Identity contract - no complex cross-contract state sharing needed.

5. **Supply Chain to Executive Pipeline**: 
   - **Supply chain workers** prove capabilities via Identity (0x09-0x0d)
   - **Executives/Project owners** create tenders requiring capabilities (0x07-0x08 in Tender)
   - **Workers flow to Labor Market** for execution (0x0d in Labor Market)
   - **Insurance** provides risk coverage (0x09-0x0c in Insurance Market)

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

## Current Development Limitations

Several issues affect the ability to develop, test, and deploy smart contracts on DarkWow. These are documented here for awareness when designing or modifying contracts.

### Issue 1: async-trait Lifetime Bug (Rust 1.90+)

**Impact**: Cannot build `dwowd`, `dww`, or integration tests from source when using Rust 1.90+.

**Root cause**: The `dwow-serial/async` feature enables async-serialization which triggers a lifetime bug in `async-trait` 0.1.x.

**Feature chain**:
```
dwowd/validator
  └── dwowd/tx
        └── dwowd/async-serial
              └── dwow-serial/async
                    └── async-trait 0.1.x  (buggy on Rust 1.90+)
```

**Workaround**: Use Rust 1.89 with downgraded dependencies:
```bash
rustup install 1.89.0
rustup override set 1.89.0
cargo update typed-index-collections@3.4.0 --precise 3.3.0
```

**Status**: Pre-built DarkWowMain binaries (v0.5.0) work around this issue.

### Issue 2: Missing `dww wallet` Commands in v0.5.0 Binaries

**Impact**: Cannot create wallets, check balances, mine tokens, or list coins using the CLI.

**Available commands in v0.5.0**:
- `dww alias` - Token alias management
- `dww contract` - Contract deployment
- `dww token` - Token operations (mint, freeze, import)

**Workaround**: Use pre-built DarkWowMain binaries which include wallet functionality, or wait for v0.6.0 tooling.

---

## See Also

- [Identity Contract](../../src/contract/identity/) - O-Cap implementation
- [Attestation Contract](../contract/attestation.md) - Generalized attestation and claims
- [Oracle Contract](../contract/oracle.md) - Push-model oracle with attestation
- [DAO-Escrow Contract](../contract/dao_escrow.md) - DAO-governed endowment with voting
- [zkVM Primitive Layer](./zk/zkvm_primitives.md) — opcode-level reasoning for contract expressiveness
- [DarkWow Development Uncensored](https://technologytruth.substack.com/p/darkfi-development-uncensored-part-c9b) - Original analysis of structural bias
- [Zero-Knowledge Authorization (Authorization Inversion)](https://technologytruth.substack.com/p/the-zero-knowledge-authorization) - Mathematical foundation for O-Cap authorization
