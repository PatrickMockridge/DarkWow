# Identity & Competency (DRAFT)

*This document describes the design for a privacy-preserving identity and
competency verification system on DarkFi, based on ZK-verified competency
DAGs. It enables selective disclosure of capabilities without revealing
unnecessary personal information.*

## Reusable Private Authorization Layer

**This is not just about "identity" or "credentials".**

The structure described here (commitment, nullifier, revocation, proof-carrying call parameters) is a **reusable private authorization layer** that applies uniformly across DarkFi contracts:

| Contract | Commitment | Nullifier | Revocation |
|----------|------------|------------| ------------|
| **Bridge** | `DepositParams.commitment` | `WithdrawParams.nullifier` | None |
| **DEX** | `CreateSwapParams.lock_commitment` | `AcceptSwapParams.lock_commitment` | `CancelSwapParams.secret` |
| **Identity** | `IssueCredentialParams.commitment` | `Credential.nullifier` | `RevokeCredentialParams.nullifier` |
| **Stablecoin** | `OpenPositionParams.commitment` | `LiquidateParams.nullifier` | None |

**The shared pattern** is:
1. Create a private capability (commitment)
2. Consume it atomically once (nullifier)
3. Optionally revoke it before use (issuer revocation)

This structure appears in all privacy-heavy DarkFi contracts. The identity contract demonstrates it in the "credential" domain, but the same pattern applies to bridge deposits, DEX swaps, and stablecoin positions.

## The Problem: Identity Verification Destroys Privacy

Traditional identity systems require revealing everything:
- **KYC**: Name, DOB, address, SSN — handed to every verifier
- **OAuth/OIDC**: Your identity revealed to third parties
- **Proof of Personhood**: Reveals who you are to verify humanness

**The fundamental problem**: Most transactions only need to know IF you meet some criteria, not WHO you are.

```
Example: DAO Governance

Traditional:     Reveal name, wallet, token balance, voting history
Actually needed:  "This wallet has voting rights" — that's it!

Example: Age-Gated Content

Traditional:     Reveal full name, DOB, address, ID number
Actually needed:  "This person is over 18" — that's it!
```

## Our Solution: ZK-Verified Competency DAGs

Based on [ZK-Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags),
we implement a system where:

1. **Competencies form a DAG** (Directed Acyclic Graph)
2. **ZK proofs verify possession** without revealing evidence
3. **Privacy gradient** allows graduated disclosure
4. **Trust-based revelation** based on interaction history

```
┌─────────────────────────────────────────────────────────────────┐
│                 Competency DAG Structure                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│                    ┌──────────────┐                                │
│                    │   HUMAN      │                                │
│                    │  (nullifier) │                                │
│                    └──────┬───────┘                                │
│                           │                                        │
│              ┌────────────┼────────────┐                          │
│              ▼            ▼            ▼                          │
│       ┌───────────┐ ┌──────────┐ ┌──────────┐                   │
│       │ CREDITIAL  │ │ CREDENTIAL│ │CREDENTIAL│                   │
│       │ (age>18)   │ │(dao.member)│ │(accred.) │                   │
│       └─────┬─────┘ └────┬─────┘ └────┬─────┘                   │
│             │            │            │                           │
│             ▼            ▼            ▼                           │
│       ┌───────────┐ ┌──────────┐ ┌──────────┐                    │
│       │ PROOF:    │ │ PROOF:   │ │ PROOF:   │                    │
│       │ age>=18   │ │ is_member│ │ accredited│                   │
│       │ (true)    │ │ (true)   │ │ (true)   │                    │
│       └───────────┘ └──────────┘ └──────────┘                    │
│                                                                   │
│  KEY INSIGHT: Proof reveals only "meets criteria"                  │
│               The actual credential data stays hidden              │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Core Concepts

### The Privacy Gradient

Rather than binary public/private, we implement graduated privacy levels:

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

### Competency DAG

Competencies form a directed acyclic graph where:
- **Leaf nodes**: Base competencies (issued directly)
- **Internal nodes**: Derived competencies (require prerequisite proofs)
- **Edges**: Prerequisite relationships

```
                    ┌─────────────────┐
                    │   K-ASSET       │
                    │ (K-DAG Token)   │
                    └────────┬────────┘
                             │
          ┌──────────────────┼──────────────────┐
          ▼                  ▼                  ▼
   ┌────────────┐      ┌────────────┐      ┌────────────┐
   │ COMPETENCY │      │ COMPETENCY │      │ COMPETENCY │
   │   Layer    │      │   Layer    │      │   Layer    │
   │  (L2+)     │      │   (L1)     │      │   (L0)     │
   └─────┬──────┘      └─────┬──────┘      └─────┬──────┘
         │                  │                   │
         └──────────────────┼───────────────────┘
                            ▼
                    ┌───────────────┐
                    │   CREDENTIAL  │
                    │   (Base)      │
                    └───────────────┘
```

### Trust-Based Disclosure

Disclosure correlates with calculated trust scores:

```
┌─────────────────────────────────────────────────────────────────┐
│                 Trust-Based Disclosure                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Trust Score = f(interaction_history, privacy_preferences)      │
│                                                                   │
│  Low Trust (stranger)           High Trust (repeated)             │
│  ┌─────────────────┐          ┌─────────────────┐              │
│  │ • zk_only only   │          │ • selective OK  │              │
│  │ • Predicate only │          │ • More details  │              │
│  │ • No issuer info │          │ • Issuer known  │              │
│  └─────────────────┘          └─────────────────┘              │
│                                                                   │
│  This models real-world relationship dynamics:                    │
│  - Strangers get minimal info                                     │
│  - Repeated interactions build trust                            │
│  - Graduated disclosure mirrors human relationships              │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation Levels

### Level 0: zk_only (Maximum Privacy)

Level 0 uses the safemath assertion pattern. The verifier learns **only** whether the proof is valid or invalid — no predicate result is revealed:

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

**Use case**: When the verifier only needs to know "this person is authorized" without revealing why.

### Level 1: Selective Disclosure (Bounded Equation)

Level 1 uses the **bounded equation construction** to return a public `predicate_result` bit (0 or 1). Note: `LessThanOrEqual` is now verified sound and could also be used:

```
threshold + delta = attribute_value + (1 - predicate_result) * 2^64
```

**How it works**:
- `predicate_result = 1`: Equation becomes `threshold + delta = attribute_value`
  - Solvable iff `threshold <= attribute_value` (delta absorbs the gap)
- `predicate_result = 0`: Equation becomes `threshold + delta = attribute_value + 2^64`
  - Solvable iff `threshold > attribute_value` (needs 2^64 slack)

**Implementation** (from `create_claim_v1_l1.zk`):

```zk
# Bounded equation for Level 1 (selective disclosure)
range_check(64, attribute_value);
range_check(64, threshold);
range_check(64, delta);
bool_check(predicate_result);

# Compute 2^64 as 2^32 * 2^32 (avoids large constants)
TWO_POW_32 = witness_base(4294967296);
TWO_POW_64 = base_mul(TWO_POW_32, TWO_POW_32);

# RHS = attribute_value + (1 - predicate_result) * 2^64
ONE = witness_base(1);
one_minus_result = base_sub(ONE, predicate_result);
result_term = base_mul(one_minus_result, TWO_POW_64);
rhs = base_add(attribute_value, result_term);

# LHS = threshold + delta
lhs = base_add(threshold, delta);

# Constrain the equation
constrain_equal_base(lhs, rhs);
```

**Why this matters**: The bounded equation uses only proven opcodes (`range_check`, `bool_check`, `base_mul`, `base_add`, `constrain_equal_base`). `LessThanOrEqual` is now verified sound but the bounded equation remains useful. `IsEqualBase` has a known bug (delta-invert issue).

**Use case**: When the verifier needs to know "is_over_18 = true" but nothing about the actual birthdate.

## Level 0 MVP: Basic Credential Proofs

The MVP implements the Issuer-Holder-Verifier model:

```
┌─────────────────────────────────────────────────────────────────┐
│              Level 0 MVP: Issuer-Holder-Verifier                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  ISSUER (e.g., Government, DAO, Employer)                        │
│     │                                                             │
│     │ 1. Issue credential                                         │
│     │    Credential = H(issuer_key, attributes, expiration)       │
│     │─────────────────────────────→ HOLDER                       │
│     │                                                             │
│  HOLDER (you)                                                     │
│     │                                                             │
│     │ 2. Create claim (ZK proof)                                  │
│     │    "I prove: credential.valid AND predicate(attributes)"   │
│     │    e.g., "age >= 18" without revealing DOB                │
│     │←──────────────────────────────────────                    │
│     │                                                             │
│     │ 3. Send claim to verifier                                   │
│     │──────────────────────────────→ VERIFIER                     │
│     │                                                             │
│  VERIFIER (e.g., website, contract)                              │
│     │                                                             │
│     │ 4. Verify ZK proof                                          │
│     │    ✓ Proof valid                                            │
│     │    ✓ Not expired/revoked                                    │
│     │    ✓ Issuer trusted                                         │
│     │←──────────────────────────────────────                    │
│     │                                                             │
│  RESULT: ✓ or ✗ — NO additional information                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### What the MVP Stores On-Chain

The contract stores **commitments, not data**:

| Stored | NOT Stored |
|--------|-----------|
| Credential commitment: `H(issuer, holder, schema, attrs_encrypted)` | Actual attributes |
| Nullifier: `H(holder_secret, credential_secret)` | Holder identity |
| Issuer public key | Attribute values |
| Schema hash | When credential expires |
| Revocation status | Who holds the credential |

### Privacy Properties by Level

**Level 0 (zk_only)**:

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| "Proof valid/invalid" | Predicate result |
| "Credential valid" | Whether predicate satisfied |
| "Not revoked" | The actual attribute values |
| Issuer is trusted | Who the holder is |

**Level 1 (selective disclosure)**:

| What You Reveal | What Stays Hidden |
|-----------------|-------------------|
| "Predicate satisfied: yes/no" | The actual attribute values |
| "Proof valid" | Exact threshold or attribute |
| "Credential valid" | Who the holder is |
| Issuer is trusted | Full credential contents |

## Expansion: Competency DAG (Future)

Level 1 adds competency hierarchies:

```
┌─────────────────────────────────────────────────────────────────┐
│              Level 1: Competency DAG                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Base Credential                                                  │
│  ┌─────────────────┐                                            │
│  │ CS Degree (MIT)  │                                            │
│  │ • Proves degree  │                                            │
│  │ • From MIT pubkey│                                            │
│  └────────┬────────┘                                             │
│           │                                                       │
│           │ prerequisite                                          │
│           ▼                                                       │
│  ┌─────────────────┐                                            │
│  │ Software Eng    │ ← Derived competency                         │
│  │ • CS Degree +   │   (proves BOTH degree AND experience)        │
│  │ • 5yrs exp +    │                                            │
│  │ • ZK proofs     │                                            │
│  └────────┬────────┘                                             │
│           │                                                       │
│           │ prerequisite                                          │
│           ▼                                                       │
│  ┌─────────────────┐                                            │
│  │ Senior Eng      │ ← Further derived                            │
│  │ • Software Eng +│                                            │
│  │ • Tech lead +   │                                            │
│  │ • ZK proofs     │                                            │
│  └─────────────────┘                                            │
│                                                                   │
│  KEY INSIGHT: Each step only reveals "meets criteria"            │
│               Full history stays private                         │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### DAG Operations

- **Prove competency**: ZK proof that path exists in DAG
- **Merge paths**: Combine multiple credentials into one proof
- **Selective reveal**: Show only specific nodes on path
- **Time-bounded proofs**: Credentials can expire at nodes

## Expansion: Trust Networks (Future)

Level 2 adds trust-based revelation:

```
┌─────────────────────────────────────────────────────────────────┐
│              Level 2: Trust Networks                               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Traditional Web of Trust:          DarkFi Trust Network:        │
│  ┌─────────────────────┐           ┌─────────────────────┐       │
│  │ Alice trusts Bob    │           │ ZK proof of:        │       │
│  │ Bob trusts Charlie  │     →     │ "Alice and Bob have │       │
│  │ Therefore Alice     │           │  interacted N times" │       │
│  │ trusts Charlie      │           │                     │       │
│  └─────────────────────┘           │ Result: Trust score │       │
│                                     │ No identities       │       │
│                                     └─────────────────────┘       │
│                                                                   │
│  Trust scores enable graduated disclosure:                        │
│  - Trust = 0.0 → zk_only                                         │
│  - Trust = 0.5 → selective                                       │
│  - Trust = 0.9+ → attested/public                                │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Expansion: K-Assets (Future)

Level 3 adds economic activation:

```
┌─────────────────────────────────────────────────────────────────┐
│              Level 3: K-Assets (Knowledge Assets)                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Competencies become tradeable:                                  │
│                                                                   │
│  ┌─────────────────┐     ┌─────────────────┐                    │
│  │ Competency      │────→│ K-Asset Token    │                    │
│  │ (proof)         │     │ (ERC-20 style)  │                    │
│  └─────────────────┘     └────────┬────────┘                    │
│                                   │                               │
│                    ┌──────────────┼──────────────┐               │
│                    ▼              ▼              ▼               │
│             ┌───────────┐  ┌───────────┐  ┌───────────┐         │
│             │   Hire     │  │  Fraction │  │  Stake    │         │
│             │  (pay for  │  │  (split   │  │ (bond for │         │
│             │  skills)   │  │  value)   │  │  quality) │         │
│             └───────────┘  └───────────┘  └───────────┘         │
│                                                                   │
│  Economic properties:                                              │
│  - K-Assets have market price                                    │
│  - Competency holders can monetize                                │
│  - Consumers can verify quality                                  │
│  - No single party controls the market                          │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Roadmap

```
Level 0 (MVP - NOW)
├── Issuer-Holder-Verifier model
├── Single issuer credentials
├── Basic predicates (>=, ==, etc.)
├── On-chain verification
└── ZK proofs for attribute hiding

Level 1 (Future)
├── Competency DAG structure
├── Derived competencies (prerequisite chains)
├── Multiple issuer support
├── Off-chain proof generation
└── Credential chaining

Level 2 (Future)
├── Trust networks (Web of Trust + ZK)
├── Graduated disclosure based on trust
├── Anonymous reputation
├── Interaction history preservation
└── Privacy-preserving referrals

Level 3 (Future)
├── K-Assets (knowledge assets)
├── Competency markets
├── Fractional ownership
├── Economic activation
└── Self-sovereign identity
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

| Feature | Traditional KYC | ZK Email | DarkFi Identity |
|---------|-----------------|----------|-----------------|
| Identity revealed | Everything | Email exists | Nothing (MVI) |
| Data minimization | None | Email only | Full |
| Revocability | Full | Limited | Full |
| Offline verification | No | No | Yes |
| Competency DAG | No | No | Yes |
| Trust networks | No | No | Yes |
| K-Assets | No | No | Yes |

## Open Questions

1. **How do we prevent fake credentials?**
   - Trust issuers, not holders
   - Issuer slashing for misissued credentials
   - Reputation systems for issuers

2. **How do we handle credential expiration?**
   - Time-bounded nullifiers
   - Refresh credentials periodically
   - Long-lived vs short-lived credentials

3. **How do we enable cross-credential proofs?**
   - Credential chaining via ZK
   - AND/OR compositions
   - Threshold predicates

4. **How do we prevent Sybil with minimal info?**
   - Proof of personhood (unique human)
   - Credential + personhood combo
   - Social graph analysis (ZK)

## References

- [DarkFi Identity Contract](../../src/contract/identity/)
- [ZK Verified Competency DAGs](https://technologytruth.substack.com/p/zk-verified-competency-dags)
- [Anonymous Credentials](https://en.wikipedia.org/wiki/Anonymous_credentials)
- [Differential Privacy](https://en.wikipedia.org/wiki/Differential_privacy)
- [Web of Trust](https://en.wikipedia.org/wiki/Web_of_trust)