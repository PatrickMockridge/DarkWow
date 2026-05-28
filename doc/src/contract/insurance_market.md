# Insurance Market Contract - Architecture

A decentralized insurance marketplace where underwriters post bonds to cover specific risk categories, and buyers purchase coverage with premiums earned by underwriters.

## O-Cap Authorization Paradigm

The Insurance Market leverages **O-Cap (Object Capability)** authorization, enabling **private qualification** where participants prove capabilities without revealing identity.

### The Core Problem: Why O-Cap for Insurance?

Traditional insurance requires revealing:
- Full identity (name, address, SSN)
- Financial history, credit score
- Employment history, salary
- Medical records (for health insurance)

**O-Cap Insurance**: Prove you meet requirements WITHOUT revealing who you are.

```
┌─────────────────────────────────────────────────────────────────┐
│                    O-CAP INSURANCE: PROVE WITHOUT REVEALING         │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  UNDERWRITER proves:                                              │
│  - "I am a verified smart contract auditor" (capability)        │
│  - "I have no major slashing incidents" (credential predicate)  │
│  NOT REVEALED: Identity, exact experience, employer              │
│                                                                   │
│  BUYER proves:                                                    │
│  - "I meet low-risk profile criteria" (capability)              │
│  - "No major violations in 3 years" (credential)                 │
│  NOT REVEALED: Age, exact record, identity                      │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Key Security Properties

1. **Identity Hidden**: Neither underwriter nor buyer identity is revealed to the counterparty
2. **Capability Bounded**: Only the specific required capability is proven, nothing more
3. **Revocable**: Issuers can revoke capabilities, preventing future use
4. **Non-Transferable**: Capabilities are bound to secrets only the holder knows
5. **DAG Privacy**: Which path in a DAG was satisfied remains private

## Capability-Based Qualification

Markets can require participants to have specific capabilities registered with the Identity contract.

### How It Works

```
┌─────────────────────────────────────────────────────────────────┐
│                 O-CAP INSURANCE AUTHORIZATION FLOW                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. ISSUER (e.g., ACME Security Cert Org)                       │
│     │                                                             │
│     │ Issues credential: "software_engineer_v1"                   │
│     │   { role_level: 7, certifications: [...], ... }           │
│     │───────────────────────────────► Alice                      │
│                                                                   │
│  2. Alice registers capability:                                  │
│     Capability: "verified_smart_contract_auditor"               │
│     Requirement: credential.role_level >= 5                      │
│     │───────────────────────────────► Identity Contract          │
│                                                                   │
│  3. Alice UNDERWRITES in Insurance Market:                       │
│     │                                                             │
│     │ Call UnderwriteWithCapabilityV1 (0x09)                    │
│     │   - market_id: specific market requiring capability       │
│     │   - capability_proof: ZK proof from Identity contract     │
│     │   - capability_secret: proves Alice owns this cap        │
│     │───────────────────────────────► Insurance Market            │
│                                                                   │
│  4. Insurance Market VERIFIES (via Identity contract):        │
│     │                                                             │
│     │ verify_capability(                                          │
│     │   capability_id: "verified_smart_contract_auditor",       │
│     │   proof: Alice's_proof,                                    │
│     │   predicate_result: 1  // satisfied                       │
│     │ )─────────────────────────────────────────────────────►    │
│     │                                                             │
│     │ Result: ✓ Alice can underwrite                             │
│     │         ✗ Alice's identity NOT revealed                     │
│     │         ✗ Alice's role level NOT revealed (only >= 5)     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Market Capability Requirements

When creating a market, the creator can define capability requirements:

```rust
struct CreateMarketParamsV1 {
    risk_type_id: RiskTypeId,
    // ... existing fields ...

    // O-Cap: Require underwriter has specific capability
    required_underwriter_capability: Option<[u8; 32]>,  // e.g., "verified_smart_contract_auditor"
    // O-Cap: Require buyer has specific capability
    required_buyer_capability: Option<[u8; 32]>,        // e.g., "low_risk_profile"
    // DAG-based: Require specific competency DAG
    required_dag_id: Option<[u8; 32]>,                  // e.g., "senior_engineer_competency"
}
```

## DAG-Based Coverage Tiers

Coverage tiers can be gated by **Competency DAGs** (Directed Acyclic Graphs) from the Identity contract. This allows complex qualification paths where any one of multiple paths can be satisfied.

```
┌─────────────────────────────────────────────────────────────────┐
│              DAG-BASED COVERAGE TIER QUALIFICATION                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  "Senior Engineer" Coverage Tier (Higher limits, lower rates)  │
│                                                                   │
│  ┌─────────────────────────────────────────────────────────┐     │
│  │                    DAG: Senior Engineer                     │     │
│  │                                                           │     │
│  │   PATH A (OR):                                            │     │
│  │   ┌──────────┐    ┌──────────────┐    ┌──────────────┐   │     │
│  │   │BSC Degree│───►│5+ Years Exp  │───►│ Senior Lead  │   │     │
│  │   └──────────┘    └──────────────┘    └──────────────┘   │     │
│  │                                                           │     │
│  │                         OR                                │     │
│  │                                                           │     │
│  │   PATH B (OR):                                            │     │
│  │   ┌──────────────────┐    ┌──────────────────┐             │     │
│  │   │Industry Cert     │───►│10+ Years Exp     │             │     │
│  │   │(CFA, CISSP, etc)│    │                  │             │     │
│  │   └──────────────────┘    └──────────────────┘             │     │
│  │                                                           │     │
│  └─────────────────────────────────────────────────────────┘     │
│                                                                   │
│  Any PATH satisfied = "Senior Engineer" competency achieved    │
│  Buyer proves they satisfied a path (without revealing which) │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Coverage Tier Examples

```
STANDARD_COVERAGE (up to $100k):
  PATH A: Verified identity + 1 prior coverage + no disputes
  PATH B: DAO member + Treasury backing + no prior claims
  PATH C: Institutional investor + regulatory compliance + audit cert

PREMIUM_COVERAGE (up to $1M):
  PATH A: Standard tier + multi-sig + escrow + 2yr history
  PATH B: Standard tier + legal credentials + insurance cert
```

## Function Reference

### Original Functions (0x00-0x08)

| Opcode | Function | Description |
|--------|----------|-------------|
| 0x00 | InitializeV1 | Initialize contract |
| 0x01 | RegisterRiskTypeV1 | Register risk category |
| 0x02 | CreateMarketV1 | Create market (now with O-Cap fields) |
| 0x03 | UnderwriteV1 | Standard underwriting |
| 0x04 | PurchaseCoverageV1 | Standard coverage purchase |
| 0x05 | FileClaimV1 | File claim when covered event occurs |
| 0x06 | ResolveClaimV1 | Oracle-based claim resolution |
| 0x07 | WithdrawPremiumV1 | Underwriter withdraws premiums |
| 0x08 | UpdatePremiumV1 | Adjust premium rate |

### O-Cap Enabled Functions (0x09-0x0c)

| Opcode | Function | Description | Use Case |
|--------|----------|-------------|----------|
| 0x09 | UnderwriteWithCapabilityV1 | Underwrite with capability proof | Restricted markets requiring verified underwriters |
| 0x0a | PurchaseCoverageWithCapabilityV1 | Buy coverage with capability proof | Buyers must prove qualification (e.g., low risk) |
| 0x0b | PurchaseCoverageWithDAGV1 | Buy coverage with DAG proof | Coverage tiers gated by competency DAGs |
| 0x0c | ResolveClaimWithCapabilityV1 | Resolve with capability proof | Authorized resolvers only |

## Data Structures

### InsuranceMarket (Extended with O-Cap)

```rust
pub struct InsuranceMarket {
    // ... existing fields ...
    /// Required capability ID for underwriters (None = any underwriter)
    pub required_underwriter_capability: Option<[u8; 32]>,
    /// Required capability ID for buyers (None = any buyer)
    pub required_buyer_capability: Option<[u8; 32]>,
    /// Required DAG ID for coverage tier qualification (None = no DAG requirement)
    pub required_dag_id: Option<[u8; 32]>,
}
```

## Integration with Identity Contract

```
┌─────────────────────────────────────────────────────────────────┐
│                 INSURANCE + IDENTITY O-CAP INTEGRATION               │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  Insurance Market Contract          Identity Contract            │
│           │                                  │                    │
│           │  CreateMarket(                   │                    │
│           │    required_capability: X        │                    │
│           │  )                               │                    │
│           │──────────────────────────────────│                    │
│           │                                  │                    │
│           │                    RegisterCapability(X)             │
│           │                    IssueCapability(holder, X)        │
│           │                                  │                    │
│           │  UnderwriteWithCapability(       │                    │
│           │    capability_proof: P           │                    │
│           │  )───────────────────────────────►│                    │
│           │                                  │                    │
│           │                    VerifyCapability(                 │
│           │                      capability_id: X,               │
│           │                      proof: P                        │
│           │                    )                                │
│           │                                  │                    │
│           │◄─────────────────────────────────│                    │
│           │              Result: VALID (predicate = 1)           │
│           │              Holder identity: NOT REVEALED           │
│           │─────────────────────────────────────────────────────│
│           │                                                             │
│           │  ✓ UNDERWRITER APPROVED                                 │
│           │    - Has capability X                                   │
│           │    - Capability is valid and not revoked                │
│           │    - Identity hidden                                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

## O-Cap + DAG Insurance Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        O-CAP + DAG INSURANCE FLOW                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  MARKET CREATOR              UNDERWRITER                    BUYER              │
│         │                         │                          │                │
│         │ CreateMarket(           │                          │                │
│         │   required_cap:         │                          │                │
│         │     "auditor_v1"        │                          │                │
│         │   required_dag:        │                          │                │
│         │     "senior_eng"        │                          │                │
│         │ )                       │                          │                │
│         │                         │                          │                │
│         │─────────────────────────►│                          │                │
│         │                         │                          │                │
│         │               UnderwriteWithCapability(             │                │
│         │                 cap_proof: ZK(                      │                │
│         │                   "auditor_v1",                     │                │
│         │                   predicate=1                       │                │
│         │                 )                                  │                │
│         │                         │                          │                │
│         │                    (verifies via Identity)          │                │
│         │                         │                          │                │
│         │               UNDERWRITER APPROVED                │                │
│         │               (identity hidden)                     │                │
│         │                         │                          │                │
│         │                         │        PurchaseCoverageWithDAG(  │
│         │                         │          dag_proof: ZK(            │
│         │                         │            "senior_eng",          │
│         │                         │            path_index: 0,         │
│         │                         │            predicate=1             │
│         │                         │          )                       │
│         │                         │────────────────────────►│         │
│         │                         │                          │         │
│         │                         │              (verifies DAG via Identity)  │
│         │                         │                          │         │
│         │                         │         COVERAGE APPROVED         │
│         │                         │         (identity hidden)         │
│         │                         │                          │         │
│         │                         │         Coverage Issued!          │
│         │                         │                          │         │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

## O-Cap Error Types

The contract defines specific errors for capability verification failures:

| Error | Code | Description |
|-------|------|-------------|
| `CapabilityRequired` | 28 | Operation requires a capability that was not provided |
| `CapabilityNotMet` | 29 | Capability proof does not satisfy the market's requirement |
| `InvalidCapability` | 30 | The capability proof is malformed or invalid |
| `CapabilityRevoked` | 31 | The capability has been revoked by the issuer |
| `DAGRequirementNotMet` | 32 | DAG proof does not satisfy the coverage tier requirement |

## ZK Circuits

Two ZK circuits enable O-Cap authorization in the Insurance Market:

### underwrite_with_capability_v1.zk

Verifies:
1. Underwriter knows their secret key
2. Underwriter's capability matches required capability
3. Capability predicate is satisfied

### purchase_coverage_with_capability_v1.zk

Verifies:
1. Buyer knows their secret key
2. Buyer's capability matches required capability
3. Capability predicate is satisfied

## Risk Categories

| ID | Category | Description |
|----|----------|-------------|
| 0 | SmartContractHack | Smart contract exploits |
| 1 | OracleManipulation | Oracle price manipulation |
| 2 | KeyManagementFailure | Key theft or loss |
| 3 | ProtocolInsolvency | Protocol becomes insolvent |
| 4 | StablecoinDepeg | Stablecoin loses peg |
| 5 | LiquidityCrunch | Liquidity freeze |
| 6 | GovernanceCapture | Governance attacked |
| 7 | RegulatoryClampdown | Regulatory action |
| 8 | Custom | Custom risk type |

## Underwriter Bonding

Underwriters must post bonds before selling coverage:

```
min_bond = coverage_limit × min_bond_rate (basis points)
```

Bond mechanics:
- Bond is slashed if claims are paid out
- Slash amount = `claim_amount × (1 - performance_score/10000)`
- Better performance → smaller slash
- Underwriter can withdraw bond when coverage expires

## Integration with Other Contracts

| Contract | Integration |
|----------|-------------|
| Identity | O-Cap authorization via capabilities and DAGs |
| PromissoryNote | Premium payments and claim payouts via BurnV1/MintV1 |
| Oracle | Claim resolution attestation |

## See Also

- [Identity Contract](./identity.md) - O-Cap authorization primitive
- [O-Cap Architecture](../arch/ocap.md) - The O-Cap paradigm
- [DarkBet Exchange](darkbet_exchange.md) - Risk probability pricing (AMM mode)
- [PromissoryNote Contract](promissory_note.md) - Value transfer integration