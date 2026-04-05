# Insurance Market Contract

A decentralized insurance marketplace where underwriters post bonds to cover specific risk categories, and buyers purchase coverage with premiums earned by underwriters.

## O-Cap Authorization

This contract supports **O-Cap (Object Capability)** authorization, enabling **private qualification** where participants prove capabilities without revealing identity.

### Key Innovation

Instead of revealing identity documents, employment history, or other personal data:

- **Underwriters** prove: "I am a verified auditor" (capability)
- **Buyers** prove: "I meet low-risk criteria" (capability)

**Nothing else is revealed.**

### Security Properties

1. **Identity Hidden**: Neither underwriter nor buyer identity is revealed
2. **Capability Bounded**: Only the specific required capability is proven
3. **Revocable**: Issuers can revoke capabilities, preventing future use
4. **Non-Transferable**: Capabilities are bound to secrets only the holder knows

## Function Reference

### Standard Functions (0x00-0x08)

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize contract |
| RegisterRiskTypeV1 | 0x01 | Register risk category |
| CreateMarketV1 | 0x02 | Create market |
| UnderwriteV1 | 0x03 | Post bond to provide coverage |
| PurchaseCoverageV1 | 0x04 | Buyer purchases coverage |
| FileClaimV1 | 0x05 | Buyer files a claim |
| ResolveClaimV1 | 0x06 | Oracle resolves claim validity |
| WithdrawPremiumV1 | 0x07 | Underwriter withdraws premiums |
| UpdatePremiumV1 | 0x08 | Update premium rate |

### O-Cap Enabled Functions (0x09-0x0c)

| Function | Opcode | Description |
|----------|--------|-------------|
| UnderwriteWithCapabilityV1 | 0x09 | Underwrite with capability proof |
| PurchaseCoverageWithCapabilityV1 | 0x0a | Buy with capability proof |
| PurchaseCoverageWithDAGV1 | 0x0b | Buy with DAG competency proof |
| ResolveClaimWithCapabilityV1 | 0x0c | Resolve with capability proof |

## Usage Examples

### Traditional Flow (Without O-Cap)

```rust
// 1. Create a market (any underwriter can participate)
let (params, market_id) = CreateMarketV1Builder::new(
    risk_type_id,
    1_000_000,      // $1M total coverage
    10000,          // Coverage period
)
.deductible(10000)
.build();

// 2. Any engineer underwrites
let (params, underwriter_id) = UnderwriteV1Builder::new(
    market_id,
    engineer_pubkey,
)
.bond_amount(100_000)
.coverage_limit(500_000)
.build();

// 3. Any buyer purchases
let (params, coverage_id) = PurchaseCoverageV1Builder::new(
    market_id,
    underwriter_id,
    buyer_pubkey,
)
.coverage_amount(100_000)
.build();
```

### O-Cap Flow (With Capability Requirements)

```rust
// 1. Create market requiring capabilities
let (params, market_id) = CreateMarketV1Builder::new(
    risk_type_id,
    1_000_000,
    10000,
)
.deductible(10000)
// O-Cap: Require underwriters have "verified_auditor" capability
.required_underwriter_capability(Some(auditor_capability_id))
// O-Cap: Require buyers have "low_risk_profile" capability
.required_buyer_capability(Some(low_risk_capability_id))
// DAG: Require "senior_engineer" competency for premium coverage
.required_dag_id(Some(senior_engineer_dag_id))
.build();

// 2. Underwriter proves capability (via Identity contract)
let (params, underwriter_id) = UnderwriteWithCapabilityV1Builder::new(
    market_id,
    engineer_pubkey,
)
.bond_amount(100_000)
.coverage_limit(500_000)
// O-Cap proof from Identity contract
.capability_proof(auditor_capability_proof)
.capability_secret(auditor_capability_secret)
.build();

// 3. Buyer with capability purchases coverage
let (params, coverage_id) = PurchaseCoverageWithCapabilityV1Builder::new(
    market_id,
    underwriter_id,
    buyer_pubkey,
)
.coverage_amount(100_000)
.value_commit(buyer_commit)
.signature(buyer_sig)
// O-Cap proof from Identity contract
.capability_proof(low_risk_capability_proof)
.capability_secret(low_risk_capability_secret)
.build();

// 4. Premium buyer with DAG competency purchases enhanced coverage
let (params, premium_coverage_id) = PurchaseCoverageWithDAGV1Builder::new(
    market_id,
    underwriter_id,
    premium_buyer_pubkey,
)
.coverage_amount(500_000)  // Higher limit tier
.value_commit(premium_commit)
.signature(premium_sig)
// DAG proof from Identity contract
.dag_proof(senior_engineer_dag_proof)
.dag_path_index(0)  // Which path was satisfied (private)
.required_dag_id(senior_engineer_dag_id)
.build();
```

## Integration with Identity Contract

```
┌─────────────────────────────────────────────────────────────────┐
│                       IDENTITY INTEGRATION                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                   │
│  1. Capability Registration:                                      │
│     Issuer registers capability in Identity contract             │
│     e.g., "verified_auditor" with role >= 5 requirement          │
│                                                                   │
│  2. Capability Issuance:                                          │
│     Issuer issues capability to qualified holders                 │
│     Holder receives: capability_id + capability_secret           │
│                                                                   │
│  3. Insurance Market Authorization:                               │
│     - UnderwriteWithCapabilityV1: Verifies underwriter cap       │
│     - PurchaseCoverageWithCapabilityV1: Verifies buyer cap       │
│     - PurchaseCoverageWithDAGV1: Verifies DAG competency          │
│                                                                   │
│  4. Verification (off-chain + on-chain):                          │
│     ZK proof from Identity contract proves capability without    │
│     revealing holder identity                                     │
│                                                                   │
└─────────────────────────────────────────────────────────────────┘
```

### Identity Function Usage

| Identity Function | Insurance Market Usage |
|------------------|----------------------|
| `RegisterCapabilityV1` | Define market access requirements |
| `IssueCapabilityV1` | Issue capabilities to qualified participants |
| `VerifyCapabilityV1` | Verify underwriter/buyer capability |
| `CreateClaimDAGV1` | Create DAG competency proof for coverage tiers |

## O-Cap Error Handling

```rust
match result {
    Err(InsuranceMarketError::CapabilityRequired) => {
        // Operation requires capability - caller didn't provide one
    }
    Err(InsuranceMarketError::CapabilityNotMet) => {
        // Capability proof doesn't satisfy market requirement
    }
    Err(InsuranceMarketError::InvalidCapability) => {
        // Capability proof is malformed
    }
    Err(InsuranceMarketError::CapabilityRevoked) => {
        // Capability was revoked by issuer
    }
    Err(InsuranceMarketError::DAGRequirementNotMet) => {
        // DAG proof doesn't satisfy coverage tier
    }
    // ... other errors
}
```

## Risk Categories

| Category | Description |
|----------|-------------|
| SmartContractHack | Smart contract exploits |
| OracleManipulation | Oracle price manipulation |
| KeyManagementFailure | Key theft or loss |
| ProtocolInsolvency | Protocol becomes insolvent |
| StablecoinDepeg | Stablecoin loses peg |
| LiquidityCrunch | Liquidity freeze |
| GovernanceCapture | Governance attacked |
| RegulatoryClampdown | Regulatory action |
| Custom | Custom risk type |

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

## File Structure

```
src/contract/insurance_market/
├── Cargo.toml
├── src/
│   ├── lib.rs                    # Function enum, constants (0x09-0x0c added)
│   ├── error.rs                 # Error types (O-Cap errors added)
│   ├── entrypoint.rs            # exec/apply dispatch
│   ├── entrypoint/
│   │   ├── create_market_v1.rs   # Now includes O-Cap fields
│   │   ├── underwrite_v1.rs      # Standard underwriting
│   │   └── ...                  # Other entrypoints
│   ├── model/
│   │   └── mod.rs               # Data models + O-Cap params
│   └── client/
│       └── mod.rs               # Client builders
└── proof/
    ├── underwrite_with_capability_v1.zk    # NEW: Underwriter capability circuit
    └── purchase_coverage_with_capability_v1.zk  # NEW: Buyer capability circuit
```

## Building

```bash
# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_insurance_market_contract

# Run tests
cargo test -p darkfi_insurance_market_contract
```

## See Also

- [Identity Contract](../identity/README.md) - O-Cap authorization primitive
- [Insurance Market Architecture](../../doc/src/arch/insurance_market.md) - Detailed O-Cap coverage
- [O-Cap Architecture](../../doc/src/arch/ocap.md) - The O-Cap paradigm
- [Prediction Market](../../doc/src/arch/prediction_market.md) - Risk probability pricing
- [Money Contract](../../doc/src/arch/money.md) - Value transfer integration