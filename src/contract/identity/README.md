# DarkFi Identity Contract

Minimal credential proofs for selective disclosure of attributes.

## The Problem: Identity Verification = Surveillance

Traditional identity verification requires revealing everything:
- **KYC**: Name, DOB, address, SSN — all given to the verifier
- **OAuth/OIDC**: Your identity handed to third parties
- **Proof of Personhood**: Reveals who you are to verify humanness

**But sometimes you just need to prove you're over 18, or hold a token, or are a DAO member — without revealing WHO you are.**

## Our Solution: Minimal Viable Information (MVI)

Release only the **minimum information necessary**:

```
Traditional KYC:              DarkFi Identity (MVI):
┌─────────────────────┐      ┌─────────────────────────┐
│ Name: Alice          │      │ Age: ✓ (over 18)         │
│ DOB: 1990-01-01      │ →    │ Residency: ✓            │
│ Address: 123 Main St │      │ Not OFAC: ✓              │
│ SSN: ***-**-1234     │      │ Credential: DAO Member ✓ │
└─────────────────────┘      └─────────────────────────┘
    ALL THE DATA                    JUST A PROOF
```

## Level 0 MVP: Issuer-Holder-Verifier

```
┌─────────────────────────────────────────────────────────────────────┐
│                  Identity Contract Flow                                  │
├─────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ISSUER                                  HOLDER                       │
│     │                                        │                        │
│     │  1. Issues credential                  │                        │
│     │     Credential = signed(               │                        │
│     │       attributes,                      │                        │
│     │       expiration                      │                        │
│     │─────────────────────────────→         │                        │
│     │                                        │                        │
│     │  2. Holder creates claim               │                        │
│     │     Claim = ZKProof{                   │                        │
│     │       "I hold valid credential"        │                        │
│     │       "age > 18"                       │                        │
│     │       "nullifier"                      │                        │
│     │     }                                  │                        │
│     │←──────────────────────────────────────│                        │
│     │                                        │                        │
│     │  3. Verifier checks claim             │                        │
│     │     ZK proof verifies:                 │                        │
│     │       - Credential valid               │                        │
│     │       - Conditions met               │                        │
│     │       - Not revoked                  │                        │
│     │───────────────────→───────────────────────→ VERIFIER            │
│                                                                       │
│  RESULT: ✓ or ✗ — NO additional information revealed                 │
│                                                                       │
└─────────────────────────────────────────────────────────────────────┘
```

## Privacy Properties

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| "I meet criteria" | Who you are |
| "Credential valid" | Your actual data |
| "Not revoked" | When credential expires |
| Issuer is trusted | Full credential contents |
| Predicate result (e.g., true/false) | The actual attribute values |

## Use Cases

### Age Verification
```rust
// Prove you're over 18 without revealing DOB
let claim = CreateClaimBuilder::new()
    .claim_type(b"age_over_18")
    .predicate(b">= 18")
    .build()?;
// Reveals: Only "age >= 18" — birthdate hidden
```

### DAO Membership
```rust
// Prove you're a DAO member without revealing wallet
let claim = CreateClaimBuilder::new()
    .claim_type(b"dao_member")
    .predicate(b">= 1")
    .build()?;
// Reveals: Only "holds >= 1 token" — balance and address hidden
```

### Accredited Investor
```rust
// Prove accredited status without revealing income
let claim = CreateClaimBuilder::new()
    .claim_type(b"accredited_investor")
    .predicate(b"== true")
    .build()?;
// Reveals: Only "is accredited" — income/net worth hidden
```

### Sybil Resistance
```rust
// Prove you're a unique human without deanonymizing
let claim = CreateClaimBuilder::new()
    .claim_type(b"unique_human")
    .predicate(b"== true")
    .build()?;
// Reveals: Only "is unique human" — identity hidden
```

## Contract Functions

### InitializeV1 (0x00)

Initializes the identity contract:
- Creates `credentials` tree for issued credentials
- Creates `nullifiers` tree for revocation tracking
- Creates `issuers` tree for trusted issuers
- Creates `config` tree for settings

### IssueCredentialV1 (0x01)

Issuer creates a credential for a holder:
- Verifies issuer is trusted
- Verifies nullifier uniqueness
- Stores credential record (commitment, not attributes)
- Emits `CredentialIssued` event (only nullifier + issuer revealed)

### RevokeCredentialV1 (0x02)

Issuer revokes a credential:
- Verifies issuer signature
- Marks credential as revoked
- Updates revocation list

### CreateClaimV1 (0x03)

Holder creates a claim from their credential:
- Verifies credential exists and is valid
- Verifies not expired or revoked
- Generates ZK proof of predicate satisfaction
- Emits `ClaimCreated` event

### VerifyClaimV1 (0x04)

Verifier checks a claim:
- Verifies credential exists and is valid
- Verifies ZK proof
- Verifies not expired or revoked
- Emits `ClaimVerified` event with result

## ZK Circuits

### issue_credential_v1.zk

Proves the issuer legitimately issued this credential:
- **Public inputs**: commitment, issuer_pub, holder_pub, schema_hash
- **Private inputs**: attributes, issuer_signature
- **Verification**: Signature valid, commitment matches

### create_claim_v1.zk

Proves the claim without revealing attributes:
- **Public inputs**: nullifier, claim_type, predicate_result
- **Private inputs**: credential_attributes, holder_secret
- **Verification**: Credential valid, predicate satisfied, not revoked

### verify_claim_v1.zk (STUB)

Proves the claim can be verified:
- **Public inputs**: claim, verifier_pub
- **Private inputs**: issuer_pub, holder_secret
- **Verification**: Claim valid, issuer trusted, not double-spent

## Reasoned Opcodes

The identity circuits use existing zkVM opcodes for the basic proof structure. The predicate verification requires:

### `LessThanOrEqual(a, b)` (Reasoned)
**Purpose**: Returns 1 if `a <= b`, 0 otherwise
**Reasoning**: Required for predicates like "age >= 18" (threshold <= attribute_value).

**Current workaround**: The circuit uses a placeholder that always passes. The verifier must trust the predicate_result public input.

**Implementation**:
```
LessThanOrEqual(a, b) = IsEqualBase(a, b) OR LessThanLoose(a, b)
```

**See also**: [zkVM Primitive Layer](../../doc/src/arch/zkvm_primitives.md) for full reasoning on comparison opcodes.

## The Privacy Gradient

Based on [ZK-Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags),
we implement graduated privacy levels rather than binary public/private:

| Level | Name | What Verifier Sees | Use Case |
|-------|------|-------------------|----------|
| **0** | `zk_only` | Nothing | Maximum privacy |
| **1** | `selective` | Predicate result only | Basic verification |
| **2** | `attested` | Issuer confirms | Trusted issuers |
| **3** | `public` | Full disclosure | Regulatory compliance |

```
Example: Age Verification

zk_only:    "I prove age >= 18" → Verifier sees: ✓
selective:   "age >= 18, issued by Gov" → Verifier sees: ✓ + issuer
attested:    "DOB: 1990-01-01, issued by Gov" → Verifier sees: full DOB
public:      Full KYC disclosure → Verifier sees: everything
```

## Roadmap: ZK-Verified Competency DAGs

```
Level 0 (MVP - NOW)     Level 1 (Future)        Level 2 (Future)        Level 3 (Future)
─────────────────────────────────────────────────────────────────────────────────────────
Issuer-Holder-          Competency DAG           Trust Networks           K-Assets
  Verifier              Prerequisite chains      Graduated disclosure     Knowledge markets
Single issuer           Multiple issuers        Web of Trust + ZK       Competency tokens
Basic predicates        Derived competencies     Interaction history      Economic activation
On-chain verify         Off-chain proofs        Anonymous reputation     Self-sovereign
```

### Level 0 (MVP - NOW): Issuer-Holder-Verifier

- Minimal blast radius for bugs
- Clear trust model (issuer is trusted)
- Foundation for everything else
- **What it proves**: Single credential → single claim

### Level 1 (Future): Competency DAG

Competencies form a DAG where derived competencies require prerequisite proofs:

```
                    ┌─────────────────┐
                    │   K-ASSET       │
                    │ (derived comp.) │
                    └────────┬────────┘
                             │
          ┌──────────────────┼──────────────────┐
          ▼                  ▼                  ▼
   ┌────────────┐      ┌────────────┐      ┌────────────┐
   │ COMPETENCY │      │ COMPETENCY │      │ COMPETENCY │
   │   (L2+)    │      │   (L1)     │      │   (L0)     │
   └─────┬──────┘      └─────┬──────┘      └─────┬──────┘
         │                    │                    │
         └────────────────────┼────────────────────┘
                              ▼
                    ┌─────────────────┐
                    │    CREDENTIAL    │
                    │   (Base Issued)  │
                    └─────────────────┘
```

- **What it adds**: Credential chaining, prerequisite proofs
- **What it proves**: Path exists in DAG without revealing full path

### Level 2 (Future): Trust Networks

Web of Trust meets ZK proofs:

```
Traditional Web of Trust:          DarkFi Trust Network:
┌─────────────────────┐           ┌─────────────────────┐
│ Alice trusts Bob    │           │ ZK proof of:        │
│ Bob trusts Charlie  │     →     │ "Alice and Bob have  │
│ Therefore Alice     │           │  interacted N times" │
│ trusts Charlie      │           │                     │
└─────────────────────┘           │ Result: Trust score │
                                  │ No identities       │
                                  └─────────────────────┘

Trust Score = f(interaction_history, privacy_preferences)

- Trust = 0.0 → zk_only
- Trust = 0.5 → selective
- Trust = 0.9+ → attested/public
```

- **What it adds**: Graduated disclosure based on trust
- **What it proves**: Relationship exists without revealing identity

### Level 3 (Future): K-Assets (Knowledge Assets)

Competencies become tradeable economic assets:

```
Competency (proof) ──────→ K-Asset Token (ERC-20 style)
                                  │
               ┌──────────────────┼──────────────────┐
               ▼                  ▼                  ▼
        ┌───────────┐      ┌───────────┐      ┌───────────┐
        │   Hire    │      │  Fraction │      │   Stake   │
        │ (pay for  │      │  (split   │      │ (bond for │
        │  skills)  │      │  value)   │      │  quality) │
        └───────────┘      └───────────┘      └───────────┘
```

- **What it adds**: Market price for competencies
- **What it enables**: Monetizing knowledge, quality bonding

### Why This Roadmap?

Each level builds on the previous:

```
Level 0: The Foundation
├── Prove you have a credential
├── Prove a predicate is met
└── Minimal blast radius

Level 1: The Structure
├── Credentials form DAGs
├── Prerequisites verifiable
└── Building blocks for reputation

Level 2: The Network
├── Trust relationships verifiable
├── Graduated disclosure
└── Modeling human relationships

Level 3: The Economy
├── K-Assets have market value
├── Economic activation
└── Competency as capital
```

## Competency DAG Example

```
Level 0: Base Credentials
├── "University Degree (MIT)" — Issued by MIT
├── "5 Years Software Experience" — Issued by Employer
└── "Open Source Contributor" — Verified by GitHub

Level 1: Derived Competencies
├── "Software Engineer" — Requires: Degree + Experience
├── "ML Engineer" — Requires: Degree + ML Courses + Published Paper
└── "Tech Lead" — Requires: Engineer + Management Course + Team Size

Level 2: Expert Competencies
├── "Principal Engineer" — Requires: Tech Lead + Patents + Speaking
└── "Fellow" — Requires: Principal + Major Contributions + Recognition

Level 3: K-Assets
├── "Principal Engineer K-Token" — Tradeable, fractional
└── "Fellow Recognition K-Token" — Reputation market
```

Each step reveals only "meets criteria" — full history stays private.

## Comparison

| Feature | Traditional KYC | ZK-Based Verifier | DarkFi Identity |
|---------|-----------------|-------------------|-----------------|
| Identity revealed | Everything | Depends on verifier | Nothing (MVI) |
| Data minimization | None | Partial | Full |
| Revocability | Full | Limited | Full |
| Issuer tracking | Full | Partial | None |
| Offline verification | No | Possible | Possible |

## MVP Status

**Blocked on Opcodes** — basic structure exists, but predicate verification is stubbed.

| Circuit | Status | Notes |
|---------|--------|-------|
| `issue_credential_v1.zk` | Unread | Likely needs review |
| `create_claim_v1.zk` | Has placeholder | Predicate verification uses placeholder that always passes |

### Blockers

1. **`LessThanOrEqual` not implemented** — Required for predicates like `age >= 18`. Currently `create_claim_v1.zk` uses a placeholder that always passes. The verifier must trust the `predicate_result` public input.
2. **`IsEqualBase` not implemented** — Needed for schema validation and credential type comparison.

### What It Needs

Implement `LessThanOrEqual` in the zkVM. Once available, update `create_claim_v1.zk` to use it for in-circuit predicate verification.

**See**: [Contract MVP Status](../../doc/src/arch/mvp_status.md) for the full cross-contract analysis.

## References

- [DarkFi Identity Contract](../../src/contract/identity/)
- [DarkFi DEX Contract](../dex/)
- [DarkFi Money Contract](../money/)
- [DarkFi Bridge Contract](../bridge/)
- [Contract MVP Status](../../doc/src/arch/mvp_status.md)
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)
- [Differential Privacy](https://en.wikipedia.org/wiki/Differential_privacy)
- [Anonymous Credentials](https://en.wikipedia.org/wiki/Anonymous_credentials)