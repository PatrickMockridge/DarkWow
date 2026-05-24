# DarkWow Identity Contract

**O-Cap (Object Capability) Authorization**: Prove you have access without revealing who you are.

See the [O-Cap & Composable Privacy](../../doc/src/arch/ocap.md) chapter for the full paradigm and the [Identity Contract](../../doc/src/arch/identity.md) for ZK circuit design and deployment roadmap.

## The Paradigm Shift: ACLs vs O-Cap

### Traditional Authorization (ACLs)
**"Who has access?"**

```
┌──────────────────┐
│ alice@co → repo  │
│ bob@co → repo    │
│ charlie@co → admin│
└──────────────────┘

PROBLEM: We know WHO has access, their identity, and can track every action.
```

### O-Cap Authorization
**"Can you prove you have access?"**

```
┌──────────────────┐
│ prove(role>=Y)  │
│ → ACCESS GRANTED │
│ (identity hidden)│
└──────────────────┘

SOLUTION: Only learn WHAT they can do. Never learn WHO they are.
```

**Key insight:** O-Cap changes the question from "who are you?" to "what can you prove?".

## The Ambient Authority Problem

Traditional authorization systems suffer from **ambient authority** — every operation runs in an environment filled with the user's full identity and all permissions.

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
│  PROBLEM: Every operation runs in an environment                  │
│  filled with ambient authority that can be exploited.            │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

**Why ACLs fail:**
- Every request carries full identity context
- Permissions persist as ambient state
- Attackers can exploit any ambient authority
- Privilege escalation is always possible

## O-Caps Eliminate Ambient Authority

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
│  → Be tricked because there's nothing to impersonate             │
│                                                                   │
│  KEY INSIGHT: O-Caps make authority EXPLICIT and BOUNDED.        │
│               The proof IS the authority - nothing more.          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## The Intuitive Privacy Rule

```
┌─────────────────────────────────────────────────────────────────┐
│  O-Cap Privacy: You reveal ONLY what you prove.                 │
│                                                                   │
│  If you prove "can_vote", verifier learns "can_vote"             │
│  Nothing else. Nothing more. Always.                            │
└─────────────────────────────────────────────────────────────────┘
```

**Why this is intuitive:**
- Privacy is PROVABLE, not just policy
- The ZK proof GUARANTEES what is/isn't revealed
- No trust in system admins or database security
- Reasoning is LOCAL to the capability being proven

## O-Cap in Action: Concrete Use Cases

### Labor Markets: Prove Qualifications Without Revealing Identity

**Traditional approach:**
```
Alice applies for job
HR sees: Alice Smith, SSN, DOB, full resume, current salary...
```

**O-Cap approach:**
```
Alice proves:
- "I have a CS degree from an accredited university"
- "I have 5+ years of software engineering experience"
- "I have completed security training"

HR learns only: ✓ Alice meets requirements
                ✗ Alice's identity is NOT revealed
```

**Why this matters:**
- Alice can't be discriminated by age, gender, or name
- Alice's current employer doesn't know she's looking
- Competitors can't poach by offering more than she revealed

### Insurance: Prove Risk Profile Without Personal Data

**Traditional approach:**
```
Insurer sees: John Doe, SSN, medical records, driving history, credit score...
```

**O-Cap approach:**
```
John proves:
- "I am over 25 years old"
- "I have no major violations in the last 3 years"
- "My annual mileage is under 15,000"

Insurer learns only: ✓ John meets risk criteria
                    ✗ John's identity is NOT revealed
```

**Why this matters:**
- Insurers can't discriminate based on protected characteristics
- No data breach exposes full profiles
- Customers prove characteristics, not surrender privacy

### Tendering: Prove Credentials Without Revealing Bidders

**Traditional approach:**
```
Requester sees: All bidder identities, their company names, past performance...
```

**O-Cap approach:**
```
Bidders prove (sealed):
- "I have completed similar projects"
- "I have required certifications"
- "My team has relevant expertise"

Requester learns only: ✓ These bidders are qualified
                       ✗ Which bidder is which (until selection)
```

**Why this matters:**
- Prevents corruption (bidders don't know who's competing)
- Eliminates bias based on company size or reputation
- Enables fair competition on purely qualifications

## O-Cap Authorization Flow

```
┌─────────────────────────────────────────────────────────────────┐
│              O-CAP AUTHORIZATION FLOW                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ISSUER (e.g., ACME Corp)         HOLDER (e.g., Alice)            │
│       │                                    │                       │
│       │  1. Issues credential              │                       │
│       │     Credential = signed(          │                       │
│       │       attributes: role=senior     │                       │
│       │       secret                     │                       │
│       │───────────────────────────────→ │                       │
│       │                                    │                       │
│       │  2. Registers capability           │                       │
│       │     Capability: "can_merge_pr"    │                       │
│       │     Requirement: role >= senior   │                       │
│       │───────────────────────────────→ │                       │
│       │                                    │                       │
│       │                          CreateProof(                      │
│       │                            prove: role >= senior          │
│       │                          )──────────────────→              │
│       │                                           │                │
│       │  OTHER CONTRACT (DAO)                     │                │
│       │       │                                   │                │
│       │       │  verify_capability(                │                │
│       │       │    capability="can_merge_pr",       │                │
│       │       │    proof=Alice's_proof             │                │
│       │       │  )────────────────────────────────→                │
│       │       │                                                             │
│       │       │  Result: VALID                                            │
│       │       │  Alice's identity: NOT REVEALED                            │
│       │       │  Only proven: "role >= senior" → can_merge_pr             │
│       │       │                                                             │
└─────────────────────────────────────────────────────────────────┘
```

**Key insight:** The verifier learns ONLY that Alice meets the requirement, NOT:
- Who Alice is
- What her actual role is
- What other attributes her credential contains

## ACL vs O-Cap Comparison

| Aspect | ACL (Traditional) | O-Cap (DarkWow) |
|--------|------------------|----------------|
| **Core Question** | "Who has access?" | "Can you prove access?" |
| **Identity** | Always revealed | Never revealed |
| **Authorization** | Identity-based | Capability-based |
| **Delegation** | Identity chain | Capability chain |
| **Revocation** | Remove from list | Revoke credential |
| **Scalability** | O(n) per user | O(1) per capability |
| **Privacy** | Full exposure | Minimal revelation |
| **Audit Trail** | Who accessed what | (capability used) |

**When to use ACL:**
- Internal systems with trusted parties
- When identity is required for business logic
- Regulatory compliance requires identity

**When to use O-Cap:**
- Privacy is important
- Identity not needed for the action
- Minimal disclosure principle

## Core O-Cap Functions (0x09-0x0c)

| ID | Function | Description |
|----|----------|-------------|
| `RegisterCapabilityV1` | 0x09 | Register a new capability type |
| `IssueCapabilityV1` | 0x0a | Issue capability to holder |
| `VerifyCapabilityV1` | 0x0b | Verify capability proof |
| `RevokeCapabilityV1` | 0x0c | Revoke capability |

## Supporting Credential Functions (0x00-0x08)

| ID | Function | Description |
|----|----------|-------------|
| `InitializeV1` | 0x00 | Initialize identity registry |
| `IssueCredentialV1` | 0x01 | Issuer issues credential to holder |
| `RevokeCredentialV1` | 0x02 | Issuer revokes a credential |
| `CreateClaimV1` | 0x03 | Holder creates claim (Level 0 zk_only) |
| `CreateClaimV1L1` | 0x05 | Holder creates claim (Level 1 selective) |
| `VerifyClaimV1` | 0x04 | Verifier checks claim on-chain |
| `CreateClaimV1L1V2` | 0x06 | Level 1 with LessThanOrEqual |
| `CreateClaimV1Multi` | 0x07 | Multi-credential AND claim |
| `CreateClaimV1Ratio` | 0x08 | Ratio-based predicate claim |
| `CreateClaimDAGV1` | 0x0d | DAG-based claim (multiple paths) |

## ZK Circuits

| Circuit | Namespace | Purpose |
|---------|-----------|---------|
| `issue_credential_v1.zk` | `IssueCredential_V1` | Prove credential valid |
| `create_claim_v1.zk` | `CreateClaim_V1` | Level 0 zk_only claim |
| `create_claim_v1_l1.zk` | `CreateClaim_V1_L1` | Level 1 bounded equation |
| `create_claim_v1_l1_v2.zk` | `CreateClaim_V1L1V2` | Level 1 LessThanOrEqual |
| `create_claim_v1_multi.zk` | `CreateClaim_V1Multi` | Multi-credential AND |
| `create_claim_v1_ratio.zk` | `CreateClaim_V1Ratio` | Ratio-based predicate |
| `create_claim_v1_dag.zk` | `CreateClaim_V1DAG` | Multi-path DAG claim |
| `verify_capability_v1.zk` | `VerifyCapability_V1` | Capability verification |

## Database Trees

| Tree | Purpose |
|------|---------|
| `credentials` | Issued credentials |
| `nullifiers` | Revocation tracking |
| `issuers` | Trusted issuers |
| `config` | Configuration |
| `capabilities` | Capability definitions |
| `capability_issuances` | Holder → capability mapping |

## O-Cap Composability

O-Cap authorization is a **cross-contract primitive** that integrates with other DarkWow contracts:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    O-Cap Composability                                 │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  Identity Contract (O-Cap)                                          │
│       │                                                              │
│       ├── Registers capabilities (can_merge_pr, can_approve_audit)   │
│       ├── Issues capabilities to holders                             │
│       └── Verifies capability proofs                                 │
│              │                                                       │
│              ▼                                                       │
│  DAO Contract                                                          │
│       │                                                              │
│       └── verify_capability("can_propose")                          │
│              │                                                       │
│              ▼                                                       │
│  Result: ✓ Alice can propose (without revealing her identity)        │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

**Example integrations:**

| Contract | O-Cap Usage | Capability |
|----------|-------------|------------|
| DAO | Submit proposal | `can_propose` |
| DAO | Vote on proposal | `can_vote` |
| Labor Market | Submit bid | `verified_contractor` |
| Tender | Submit sealed bid | `qualified_provider` |
| Insurance | Purchase coverage | `low_risk_profile` |
| Bridge | Cross-chain transfer | `authorized_signer` |

## Competency DAGs (0x0d)

Competency DAGs enable **multiple credential paths** where any path can be satisfied to achieve a competency:

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

**Function:** `CreateClaimDAGV1` (0x0d)

**Use cases:**
- "Qualified Developer" via degree OR certifications
- "Senior Engineer" via 10 years exp OR 5 years + certs
- Medical licenses in multiple jurisdictions
- Multi-path skill equivalency

## What Engineers Can Prove

| Credential | Proves | Enables |
|-----------|--------|---------|
| `software_engineer_v1` | Passed technical assessment | Commit to main branch |
| `security_auditor` | Completed security training | Approve security-sensitive PRs |
| `code_reviewer` | Minimum code review count | Approve PR merges |
| `team_member` | HR verified employment | Access internal repos |
| `senior_engineer` | 5+ years exp + reviews | Senior merge approvals |
| `principal_engineer` | Distinguished + patents | Architecture decisions |

## Key Properties of O-Cap System

1. **No Identity Revealed**: Verifier only learns capability, not holder identity
2. **Issuer-Based Revocation**: Issuer revokes credential → all capabilities fail
3. **Composable**: Capabilities can require multiple credentials (AND logic)
4. **Hierarchical**: Derived capabilities build on base credentials
5. **Predicate-Based**: Can express "role >= X" or "balance >= Y" without revealing values
6. **Bounded Authority**: Proof only contains the specific capability being proven
7. **Transient Proofs**: Proofs don't persist as ambient authority

## O-Cap Reduces Attack Surface

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-Cap Reduces Attack Surface                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ACL Attack Surface:                                              │
│  1. Stolen credentials → full identity compromise                │
│  2. SQL injection → ACL modification                              │
│  3. Privilege escalation → admin access                          │
│  4. Insider threat → ACL bypass                                   │
│  5. Cross-site scripting → session hijacking                    │
│  → ATTACK SURFACE: Every point where identity exists             │
│                                                                   │
│  O-Cap Attack Surface:                                           │
│  1. Stolen credential_secret → specific cap only                 │
│  2. No ACL to modify (capabilities are derived)                  │
│  3. No privilege escalation (authority is explicit)              │
│  4. No insider threat (identity never in system)                  │
│  5. No session to hijack (proof is transient)                   │
│  → ATTACK SURFACE: Only the specific capability being proven      │
│                                                                   │
│  WHY O-Caps REDUCE attack surface:                                │
│  - Authority is BOUNDED to what's being proven                   │
│  - Identity is NEVER in the system                               │
│  - Proofs are TRANSIENT (don't persist as ambient authority)     │
│  - Revocation is CASCADING (credential revoke → all caps fail)   │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## The Privacy Gradient

O-Cap is NOT a privacy level - it's a paradigm that works at all privacy levels:

| Privacy Level | O-Cap Behavior |
|---------------|----------------|
| `zk_only` | Prove capability without revealing result |
| `selective` | Prove capability, reveal only pass/fail |
| `attested` | Prove capability with issuer confirmation |

## Comparison

| Feature | Traditional KYC | ZK-Based Verifier | DarkWow O-Cap |
|---------|-----------------|-------------------|--------------|
| Identity revealed | Everything | Depends on verifier | Nothing (MVI) |
| Data minimization | None | Partial | Full |
| Revocability | Full | Limited | Full |
| O-Cap authorization | No | No | Yes |
| Offline verification | No | Possible | Possible |

## Base Field Arithmetic

ZK circuits operate in a finite field — the Pallas field defined by prime `p = 2^254 - 2^32 - 2^7 - 2^4 - 2 - 1`. All arithmetic wraps at `p`, which breaks normal integer intuitions.

**Why this matters for identity**: Predicate checks like "age >= 18" or "score >= threshold" require comparing field elements as integers. Comparison opcodes handle this with careful gadget design.

## Safemath Integration

The identity contract uses [darkfi-safemath](https://codeberg.org/rusticml/darkfi-safemath) assertion gadgets for predicate verification:

**Pattern** (from `create_claim_v1.zk`):
```zk
# Proves: threshold <= attribute_value
# Using safemath assert_lte pattern: threshold < attribute + 1
range_check(64, attribute_value);
range_check(64, threshold);
attribute_plus_one = base_add(attribute_value, witness_base(1));
less_than_strict(threshold, attribute_plus_one);
```

## Future Expansion

- **Delegation**: Capability delegation without identity leakage
- **Trust Networks**: Graduated disclosure based on trust scores
- **K-Assets**: Knowledge assets with market value

## References

- [DarkWow Identity Contract](./)
- [DarkWow DEX Contract](../dex/)
- [DarkWow Money Contract](../money/)
- [DarkWow Bridge Contract](../bridge/)
- [Contract Architecture](../../../doc/src/arch/identity.md)
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)
- [Anonymous Credentials](https://en.wikipedia.org/wiki/Anonymous_credentials)
