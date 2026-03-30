# Insurance Market Contract

A decentralized insurance marketplace where underwriters post bonds to cover specific risk categories, and buyers purchase coverage with premiums earned by underwriters.

## Overview

Insurance markets connect:

- **Buyers**: Want to transfer risk for a premium
- **Underwriters**: Post bonds and earn premiums for taking on risk
- **Protocol**: Holds collateral, processes claims, enforces slashing

## The Core Pricing Formula

```
insurance_premium = prediction_market_price × (1 - engineering_confidence)

where:
- prediction_market_price = P(bad_event) × monetary_impact
- engineering_confidence = 1 - (mitigation_cost / premium_charged)
```

This means:
- Engineers who can mitigate risks cheaply earn more premium
- High confidence engineers get lower premiums but win more deals
- Market prices reflect true risk assessment

## Contract Functions

| Function | Opcode | Description |
|----------|--------|-------------|
| RegisterRiskTypeV1 | 0x00 | Register a new risk category |
| CreateMarketV1 | 0x01 | Create an insurance market for a risk |
| UnderwriteV1 | 0x02 | Post bond to provide coverage |
| PurchaseCoverageV1 | 0x03 | Buyer purchases coverage |
| FileClaimV1 | 0x04 | Buyer files a claim |
| ResolveClaimV1 | 0x05 | Oracle resolves claim validity |
| WithdrawPremiumV1 | 0x06 | Underwriter withdraws earned premiums |
| UpdatePremiumV1 | 0x07 | Update premium rate (admin) |

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

## Key Data Structures

### Underwriter

```rust
struct Underwriter {
    id: UnderwriterId,
    owner: PublicKey,                // Underwriter's public key
    market_id: MarketId,             // Which market
    bond_amount: u64,                 // Bond posted (slashable)
    coverage_provided: u64,          // Max coverage they can sell
    coverage_sold: u64,               // Coverage already sold
    earned_premiums: u64,            // Available for withdrawal
    claims_paid: u64,               // Total claims paid
    slash_count: u32,                // Number of times slashed
    performance_score: u32,           // 0-10000, higher = better
}
```

### Coverage

```rust
struct Coverage {
    id: CoverageId,
    market_id: MarketId,
    buyer: PublicKey,                  // Coverage owner
    underwriter_id: UnderwriterId,   // Which underwriter
    amount: u64,                      // Coverage amount
    premium_paid: u64,                // Premium paid
    state: CoverageState,             // Active/Expired/Claimed
    claim_id: Option<ClaimId>,        // If claim filed
}
```

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

### Money Contract

- **Underwrite**: User burns bond tokens via Money::BurnV2
- **PurchaseCoverage**: User pays premium via Money::BurnV2
- **ResolveClaim**: Valid claims trigger Money::MintV2 to payout
- **WithdrawPremium**: Underwriter receives via Money::MintV2

### Prediction Market

- Insurance premium = prediction market price × (1 - confidence)
- Market prices from prediction market inform premium rates

### Oracle Contract

- Claims are resolved by oracle attestation
- Oracle verifies claim evidence and approves/rejects

### Attestation Contract

- Underwriter competency could be attested
- Risk type descriptions attested

## Known Limitations

### Access Control (CRITICAL - FIXED)

The original implementation had critical access control bugs:

**Fixed Issues:**
1. `WithdrawPremiumV1`: Now verifies `underwriter.owner == params.owner`
2. `FileClaimV1`: Now verifies `coverage.buyer == params.buyer`
3. `PurchaseCoverageV1`: Now tracks `coverage_sold` separately from `coverage_provided`

### Oracle Signature Verification (HIGH)

Claim resolution accepts oracle signature but does not verify it cryptographically.

**TODO**: Implement proper oracle signature verification similar to prediction market.

### ZK Proof Verification (HIGH)

The file_claim and resolve_claim functions do not verify ZK proofs that should prove claim validity.

**TODO**: Integrate with zkas verifier for claim evidence proofs.

## Slash Mechanics

When a valid claim is paid out, the underwriter's bond is slashed:

```rust
slash_amount = claim_amount × (10000 - performance_score) / 10000
```

Example:
- Underwriter with 8000 performance score (80%)
- $1000 claim paid
- Slash = $1000 × 2000 / 10000 = $200

After slashing:
- `bond_amount -= slash_amount`
- `claims_paid += payout`
- `slash_count += 1`
- `performance_score = max(0, performance_score - 1000)`

## File Structure

```
src/contract/insurance_market/
├── Cargo.toml
├── src/
│   ├── lib.rs                     # Function enum, constants
│   ├── error.rs                   # Error types
│   ├── entrypoint.rs              # init, metadata, exec, update
│   ├── entrypoint/
│   │   ├── register_risk_type_v1.rs
│   │   ├── create_market_v1.rs
│   │   ├── underwrite_v1.rs
│   │   ├── purchase_coverage_v1.rs
│   │   ├── file_claim_v1.rs
│   │   ├── resolve_claim_v1.rs
│   │   ├── withdraw_premium_v1.rs
│   │   └── update_premium_v1.rs
│   ├── model/
│   │   └── mod.rs                # InsuranceMarket, Underwriter, Coverage, etc.
│   └── client/
│       └── mod.rs                # Client-side builders
└── proof/                         # ZK circuits (TODO)
```

## See Also

- [Risk Market Ecosystem](../contract/risk_market_ecosystem.md) — How insurance combines with prediction markets
- [Prediction Market](prediction_market.md) — Risk probability pricing
- [Oracle Contract](oracle.md) — Claim resolution
- [Attestation Contract](attestation.md) — Claims verification
