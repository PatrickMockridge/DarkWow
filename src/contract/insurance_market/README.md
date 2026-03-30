# Insurance Market Contract

A decentralized insurance marketplace that prices risk using prediction markets and connects underwriters (engineers) with risk buyers.

## Overview

The Insurance Market contract enables:

1. **Risk Type Registration**: Define categories of insurable risks (smart contract hacks, oracle manipulation, etc.)
2. **Insurance Markets**: Create markets for specific risks with premium pricing
3. **Underwriting**: Engineers post bonds to underwrite risks they can mitigate
4. **Coverage Purchase**: Risk buyers purchase insurance policies
5. **Claims Resolution**: Oracle-based resolution when covered events occur

## Key Features

- **Bond-based underwriting**: Engineers must post bonds that can be slashed for non-performance
- **Oracle-based resolution**: Trusted oracles determine claim validity
- **Premium pricing**: Markets price risk based on prediction market probabilities
- **Performance tracking**: Underwriters build reputation through slash history

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| InitializeV1 | 0x00 | Initialize contract |
| RegisterRiskTypeV1 | 0x01 | Register a new risk category |
| CreateMarketV1 | 0x02 | Create insurance market for risk type |
| UnderwriteV1 | 0x03 | Engineer posts bond to underwrite risk |
| PurchaseCoverageV1 | 0x04 | Buyer purchases coverage |
| FileClaimV1 | 0x05 | File claim when covered event occurs |
| ResolveClaimV1 | 0x06 | Oracle resolves claim validity |
| WithdrawPremiumV1 | 0x07 | Underwriter withdraws earned premiums |
| UpdatePremiumV1 | 0x08 | Adjust premium based on market conditions |

## Risk Categories

| ID | Category | Example |
|----|----------|---------|
| 0 | SmartContractHack | DeFi protocol exploit |
| 1 | OracleManipulation | Price oracle attack |
| 2 | KeyManagementFailure | Private key compromise |
| 3 | ProtocolInsolvency | Reserve shortfall |
| 4 | StablecoinDepeg | Stablecoin loses peg |
| 5 | LiquidityCrunch | Sudden liquidity withdrawal |
| 6 | GovernanceCapture | DAO governance attack |
| 7 | RegulatoryClampdown | Legal action |
| 8 | Custom | User-defined risk |

## Building

```bash
# Build WASM
cargo build --target wasm32-unknown-unknown --release -p darkfi_insurance_market_contract

# Run tests
cargo test -p darkfi_insurance_market_contract
```

## Usage Example

```rust
use darkfi_insurance_market_contract::client::{RegisterRiskTypeV1Builder, CreateMarketV1Builder};

// 1. Register a risk type
let (params, risk_type_id) = RegisterRiskTypeV1Builder::new(
    0, // SmartContractHack category
    "Smart contract exploit coverage".to_string(),
    oracle_pubkey,
)
.base_premium_rate(500) // 5%
.min_bond_rate(1000)    // 10%
.build();

// 2. Create an insurance market
let (params, market_id) = CreateMarketV1Builder::new(
    risk_type_id,
    1_000_000,      // $1M total coverage
    10000,          // Coverage period in blocks
)
.deductible(10000)  // $10k deductible
.build();

// 3. Engineer underwrites the risk
use darkfi_insurance_market_contract::client::UnderwriteV1Builder;

let (params, underwriter_id) = UnderwriteV1Builder::new(
    market_id,
    engineer_pubkey,
)
.bond_amount(100_000)    // $100k bond
.coverage_limit(500_000) // Up to $500k coverage
.build();

// 4. Buyer purchases coverage
use darkfi_insurance_market_contract::client::PurchaseCoverageV1Builder;

let (params, coverage_id) = PurchaseCoverageV1Builder::new(
    market_id,
    underwriter_id,
    buyer_pubkey,
)
.coverage_amount(100_000) // $100k coverage
.build();
```

## Integration with Other Contracts

| Contract | Integration |
|----------|-------------|
| Money | Premium payments and claim payouts |
| PredictionMarket | Risk probability pricing |
| Oracle | Claim resolution attestation |
| DaoEscrow | Dispute escalation for rejected claims |

## Risk Pricing

The insurance premium is calculated as:

```
premium = coverage_amount × premium_rate

where premium_rate is determined by:
- Base rate from risk type
- Market conditions (supply/demand of coverage)
- Prediction market probability of event
```

## Underwriter Bonding

Underwriters must post bonds equal to a percentage of their coverage:

```
min_bond = coverage × min_bond_rate
```

If a valid claim is filed and the underwriter fails to pay:
1. Bond is slashed up to the claim amount
2. Performance score decreases
3. Higher performance scores reduce slash amounts

## See Also

- [Prediction Market Contract](../prediction_market/) - Risk probability pricing
- [Tender Contract](../tender/) - Project allocation with insurance requirements
- [Money V2 Contract](../money_v2/) - Value transfer integration