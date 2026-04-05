# Parallel Societies: Privacy for Social Reproduction

*This document maps industries vital to social reproduction to DarkFi's composable privacy stack, explaining how the dual-layer architecture enables privacy-preserving contracts for ordinary people doing essential work.*

## The Problem: Privacy as a Luxury

Current privacy-preserving systems create **parallel societies**:

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         PARALLEL SOCIETIES                                  │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│   SURVEILLED SOCIETY (Default)              PRIVATE SOCIETY (Luxury)     │
│   ─────────────────────────────             ─────────────────────────   │
│                                                                          │
│   • Employment contracts visible       • Gambling contracts private    │
│   • Healthcare decisions on-chain        • DeFi speculation private      │
│   • Union membership public             • Prediction markets private     │
│   • Insurance claims recorded            • Simple swaps private          │
│   • Freelance work tracked               • Only the wealthy afford ZK     │
│                                                                          │
│   Result: Privacy becomes a marker of class privilege, not a human right │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

This is not hypothetical - it's happening now. ZK privacy systems currently serve:
- **Speculative finance**: Gambling, prediction markets, yield farming
- **Wealth preservation**: Asset protection for those with significant capital

But they systematically exclude:
- **Labor**: Employment contracts, freelance agreements, gig work
- **Mutual aid**: Community insurance, cooperative economics
- **Social reproduction**: Healthcare, education, care work

## Why This Matters for Privacy as a Human Right

The [UDHR Article 12](https://www.un.org.org/universal-declaration-human-rights/) states:

> "No one shall be subjected to arbitrary interference with his privacy, family, home or correspondence, nor to attacks upon his honour and reputation. Everyone has the right to the protection of the law against such interference or attacks."

For privacy to be meaningful:
1. **It must be accessible** to ordinary people doing ordinary work
2. **It must enable economic activity** - employment, contracts, insurance
3. **It must not create surveillance states** where all transactions are visible

### The ZK-First Structural Bias

ZK-first privacy systems bias toward mathematically simple operations:

| Contract Type | ZK Feasibility | Social Reproduction Role |
|---------------|---------------|------------------------|
| Gambling | Easy (fixed odds) | None |
| Prediction markets | Easy (binary outcomes) | Minimal |
| Simple swaps | Easy (token transfers) | Limited |
| **Labor contracts** | **Hard (complex conditions)** | **Essential** |
| **Insurance** | **Hard (actuarial math)** | **Essential** |
| **Professional credentials** | **Hard (hierarchical verification)** | **Essential** |

The bias is not ideological - it's mathematical. ZK circuits cannot express division, complex conditionals, or variable exponentiation efficiently. Real-economy applications require these operations.

## Industries Vital to Social Reproduction

### Healthcare

**Role in society**: Medical decisions should be private. Healthcare workers need to coordinate care without exposing patient data. Insurance claims should not be public record.

**Current DarkFi ZK support**: None. Cannot build privacy-preserving health records, insurance, or coordination systems.

**Plain contract path**: `attestation_plain` for hierarchical credentials, `insurance_plain` for community health pools.

### Domestic and Care Labor

**Role in society**: Cleaning, cooking, childcare, elder care - the work that maintains human life. Often unpaid or underpaid, and systematically invisible in economic statistics.

**Current DarkFi ZK support**: None. Cannot build private housekeeping contracts, childcare coordination, or care worker collectives.

**Plain contract path**: `labor_market_plain` for time-based contracts, milestone tracking for domestic work.

### Education and Skill Training

**Role in society**: Transmitting knowledge from one generation to the next. Professional certifications enable employment. Private tutoring allows knowledge transfer without institutional surveillance.

**Current DarkFi ZK support**: Basic attestation for competency verification, but cannot express:
- Hierarchical credential chains (degree < certification < expertise level)
- Time-weighted learning milestones
- Private tutoring contracts

**Plain contract path**: `attestation_plain` for hierarchical credentials, `subscription_plain` for tutoring access.

### Freelance and Gig Work

**Role in society**: Independent workers providing services (programming, writing, design, consulting) without formal employment. A growing sector in post-industrial economies.

**Current DarkFi ZK support**: `subscription` contract for access control, but cannot express:
- Milestone-based payment release
- Time-weighted compensation
- Complex deliverable verification

**Plain contract path**: `labor_market_plain` for milestone-based escrow, `subscription_plain` for access control.

### Mutual Insurance and Cooperative Economics

**Role in society**: Communities pooling resources to manage risk collectively. Traditional mutual aid societies, cooperative insurance, community savings pools.

**Current DarkFi ZK support**: Basic `insurance_market` with known limitations on actuarial calculations.

**Plain contract path**: `insurance_plain` for actuarial premium calculations, claims verification, pool capital tracking.

### Union Organization and Collective Bargaining

**Role in society**: Workers organizing collectively to negotiate wages and conditions. Historically surveilled and suppressed. Privacy is essential for effective organizing.

**Current DarkFi ZK support**: None. Cannot build private union membership systems, collective bargaining contracts, or strike funds.

**Plain contract path**: `dao_escrow` for treasury management (partial), `attestation_plain` for credential chains proving membership.

## DarkFi's Composability Stack for Social Reproduction

### O-Cap Enabled ZK Contracts (Now Fully Capable)

**RESOLUTION**: `base_div` (0x58) and `LessThanOrEqual` (0x55) are now implemented and verified sound. Plain contracts are deprecated. ZK contracts now have full functionality for real-economy applications.

| Contract | Social Reproduction Use | O-Cap Integration |
|----------|------------------------|-------------------|
| `identity` | Credential + capability issuance | O-Cap (0x09-0x0c) enables cross-contract authorization |
| `money` | Token transfers | Amount hidden via ZK commitment |
| `subscription` | Access control | O-Cap `can_access` capability |
| `dao_escrow` | Treasury management | O-Cap governance capabilities |
| `attestation` | Credentials | Hierarchical credential chains via `base_div` |
| `oracle` | Data aggregation | O-Cap data attestation |
| `labor_market` | Freelance escrow, milestones | O-Cap `can_work_on_freelance_jobs` |
| `insurance_market` | Mutual insurance pools | O-Cap `verified_underwriter` |
| `tender` | Sealed bid procurement | O-Cap `qualified_contractor` |

## The O-Cap Advantage for Social Reproduction

O-Cap authorization enables privacy for social reproduction without the dual-layer tradeoff:

```
┌─────────────────────────────────────────────────────────────────────────┐
│            O-CAP ENABLED SOCIAL REPRODUCTION                                │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  The key insight: Authorization (what you CAN do)                       │
│  doesn't require Identity (who you ARE)                                 │
│                                                                          │
│  ZK Layer (Identity, Money):                                            │
│     │                                                                     │
│     │  ZK: Verify signatures, constrain value transfers                   │
│     │  O-Cap: Authorization without identity exposure                    │
│     ▼                                                                     │
│  Labor Market (O-Cap enabled):                                          │
│     │                                                                     │
│     │  CreateJob(required_capability="can_work_on_freelance_jobs")     │
│     │  SubmitBid(prove: can_work_on_freelance_jobs)                    │
│     │                                                                     │
│     │  Result: Worker gets job WITHOUT revealing identity                │
│     │          Employer learns capability, not who                      │
│     ▼                                                                     │
│  Insurance Market (O-Cap enabled):                                       │
│     │                                                                     │
│     │  RegisterCapability("verified_underwriter")                        │
│     │  PurchaseCoverage(prove: verified_underwriter)                   │
│     │                                                                     │
│     │  Result: Customer gets coverage WITHOUT revealing risk profile      │
│     │          Insurer learns risk category, not details                │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

### Example: Private Freelance Contract with O-Cap

```
┌─────────────────────────────────────────────────────────────────────────┐
│            FREELANCE CONTRACT WITH O-CAP PRIVACY                             │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  IDENTITY CONTRACT (O-Cap 0x09-0x0c):                                   │
│     │                                                                     │
│     │  RegisterCapability("can_work_on_freelance_jobs"):                 │
│     │    - requires: credential.senior_engineer                          │
│     │    - requires: predicate(experience >= 5)                         │
│     │    - issuer: Industry Authority                                    │
│     │                                                                     │
│     │  IssueCapability(worker, "can_work_on_freelance_jobs"):            │
│     │    - worker proves: role >= senior, experience >= 5                │
│     │    - Hides: actual role, employer, salary, identity               │
│     ▼                                                                     │
│  Client                                                                   │
│     │                                                                     │
│     │  1. Create labor_market job with required_capability             │
│     │     - capability: can_work_on_freelance_jobs                      │
│     │     - Milestone 1: Design mockup (50%)                            │
│     │     - Milestone 2: Implementation (50%)                            │
│     │                                                                     │
│     │  2. Fund escrow in Money contract (ZK)                           │
│     │     - Funds locked until milestone completion                      │
│     │     - Amount hidden via ZK commitment                              │
│     ▼                                                                     │
│  Worker                                                                   │
│     │                                                                     │
│     │  3. SubmitBid(prove: can_work_on_freelance_jobs)                  │
│     │     - Prover identity HIDDEN                                       │
│     │     - Client learns: capability is valid                           │
│     │     - Client DOES NOT learn: who, employer, salary                │
│     │                                                                     │
│     │  4. Submit deliverable for Milestone 1 │
│     │     - Hash submitted on-chain (visible)                           │
│     │     - Actual work content off-chain (private)                     │
│     │                                                                     │
│     │  4. Client approves → Payment released via Money (ZK)              │
│     │     - Atomic transfer, no intermediary                             │
│     │     - Amount hidden in ZK proof                                   │
│     ▼                                                                     │
│  Privacy Properties:                                                      │
│  - Work content: PRIVATE (off-chain)                                     │
│  - Payment amounts: HIDDEN (ZK commitment)                                │
│  - Milestone completion: HIDDEN (O-Cap authorization)                   │
│  - Client/Worker identity: HIDDEN (O-Cap)                               │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Resolution: Plain Contracts Deprecated

**COMPLETED**: `base_div` (0x58) and `LessThanOrEqual` (0x55) are now implemented and verified sound.

Plain contracts in `src/contract_plain/` are **deprecated**. ZK contracts now have full functionality via:
- O-Cap authorization (0x09-0x0c) for cross-contract capability verification
- `base_div` (0x58) for actuarial calculations
- `LessThanOrEqual` (0x55) for predicate evaluation

## The O-Cap Privacy Rule

The fundamental rule for O-Cap privacy:

> **You reveal ONLY what you prove. Nothing more. Always.**

```
┌─────────────────────────────────────────────────────────────────────────┐
│            THE INTUITIVE PRIVACY RULE                                         │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  If you prove "can_work_on_freelance_jobs":                            │
│  - Verifier learns: can_work_on_freelance_jobs = VALID                 │
│  - Verifier DOES NOT learn: Who, employer, salary, age, gender        │
│                                                                          │
│  If you prove "verified_underwriter":                                  │
│  - Verifier learns: verified_underwriter = VALID                       │
│  - Verifier DOES NOT learn: Real name, company, claims history        │
│                                                                          │
│  If you prove "can_vote":                                              │
│  - Verifier learns: can_vote = VALID                                   │
│  - Verifier DOES NOT learn: Who, what DAO, voting weight               │
│                                                                          │
│  This is PROVABLE privacy, not just policy.                            │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

## Conclusion

For privacy to be a human right rather than a luxury:
1. **Accessibility**: Must work for ordinary people doing ordinary jobs
2. **Economic viability**: Must enable contracts, insurance, employment
3. **No surveillance**: Must not create parallel surveilled society

DarkFi's O-Cap architecture enables this vision:
- **O-Cap (0x09-0x0c)** provides authorization without identity exposure
- **ZK circuits** provide privacy for transactions and calculations
- **Composable capabilities** enable real-economy applications

The goal is to ensure that privacy is not just for gamblers and speculators, but for everyone who does essential work maintaining social reproduction.

## See Also

- [Composability](./composability.md) - O-Cap cross-contract patterns
- [Private Authorization Layer](./privauth.md) - O-Cap paradigm documentation
- [DarkFi Development Uncensored](https://technologytruth.substack.com/p/darkfi-development-uncensored-part-c9b) - Original analysis of structural bias