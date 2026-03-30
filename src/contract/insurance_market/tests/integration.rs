/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Insurance Market contract integration tests

use darkfi_insurance_market_contract::{
    model::{
        calculate_max_coverage, calculate_premium, calculate_slash, derive_claim_id,
        derive_coverage_id, derive_risk_type_id, derive_underwriter_id, Claim, Coverage,
        CoverageId, CoverageState, CreateMarketParamsV1, CreateMarketUpdateV1, EndowmentPool,
        FileClaimParamsV1, FileClaimUpdateV1, InsuranceMarket, RegisterRiskTypeParamsV1,
        RegisterRiskTypeUpdateV1, ResolveClaimParamsV1, ResolveClaimUpdateV1, RiskCategory,
        RiskType, RiskTypeId, UnderwriteParamsV1, UnderwriteUpdateV1, WithdrawPremiumParamsV1,
        WithdrawPremiumUpdateV1,
    },
    InsuranceMarketFunction,
    // Constants
    INSURANCE_CONTRACT_RISK_TYPES_TREE, INSURANCE_CONTRACT_MARKETS_TREE,
    INSURANCE_CONTRACT_UNDERWRITERS_TREE, INSURANCE_CONTRACT_COVERAGES_TREE,
    INSURANCE_CONTRACT_CLAIMS_TREE, INSURANCE_CONTRACT_ENDOWMENT_TREE,
    DEFAULT_COVERAGE_PERIOD, DEFAULT_PREMIUM_RATE, MIN_BOND_RATE,
    DEFAULT_COVERAGE_LEVERAGE, MAX_COVERAGE_LEVERAGE,
};

#[test]
fn test_insurance_market_function_enum_valid() {
    assert!(InsuranceMarketFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(InsuranceMarketFunction::try_from(0x01).is_ok()); // RegisterRiskTypeV1
    assert!(InsuranceMarketFunction::try_from(0x02).is_ok()); // CreateMarketV1
    assert!(InsuranceMarketFunction::try_from(0x03).is_ok()); // UnderwriteV1
    assert!(InsuranceMarketFunction::try_from(0x04).is_ok()); // PurchaseCoverageV1
    assert!(InsuranceMarketFunction::try_from(0x05).is_ok()); // FileClaimV1
    assert!(InsuranceMarketFunction::try_from(0x06).is_ok()); // ResolveClaimV1
    assert!(InsuranceMarketFunction::try_from(0x07).is_ok()); // WithdrawPremiumV1
    assert!(InsuranceMarketFunction::try_from(0x08).is_ok()); // UpdatePremiumV1
}

#[test]
fn test_insurance_market_function_enum_invalid() {
    assert!(InsuranceMarketFunction::try_from(0xFF).is_err());
    assert!(InsuranceMarketFunction::try_from(0x09).is_err());
    assert!(InsuranceMarketFunction::try_from(0x10).is_err());
}

#[test]
fn test_risk_category_from_u8() {
    assert_eq!(RiskCategory::try_from(0), Ok(RiskCategory::SmartContractHack));
    assert_eq!(RiskCategory::try_from(1), Ok(RiskCategory::OracleManipulation));
    assert_eq!(RiskCategory::try_from(2), Ok(RiskCategory::KeyManagementFailure));
    assert_eq!(RiskCategory::try_from(3), Ok(RiskCategory::ProtocolInsolvency));
    assert_eq!(RiskCategory::try_from(4), Ok(RiskCategory::StablecoinDepeg));
    assert_eq!(RiskCategory::try_from(5), Ok(RiskCategory::LiquidityCrunch));
    assert_eq!(RiskCategory::try_from(6), Ok(RiskCategory::GovernanceCapture));
    assert_eq!(RiskCategory::try_from(7), Ok(RiskCategory::RegulatoryClampdown));
    assert_eq!(RiskCategory::try_from(8), Ok(RiskCategory::Custom));
    assert!(RiskCategory::try_from(9).is_err());
    assert!(RiskCategory::try_from(255).is_err());
}

#[test]
fn test_coverage_state_from_u8() {
    assert_eq!(CoverageState::try_from(0), Ok(CoverageState::Active));
    assert_eq!(CoverageState::try_from(1), Ok(CoverageState::Expired));
    assert_eq!(CoverageState::try_from(2), Ok(CoverageState::Claimed));
    assert_eq!(CoverageState::try_from(3), Ok(CoverageState::Cancelled));
    assert!(CoverageState::try_from(4).is_err());
    assert!(CoverageState::try_from(255).is_err());
}

#[test]
fn test_claim_state_from_u8() {
    assert_eq!(crate::model::ClaimState::try_from(0), Ok(crate::model::ClaimState::Filed));
    assert_eq!(crate::model::ClaimState::try_from(1), Ok(crate::model::ClaimState::Resolved));
    assert_eq!(crate::model::ClaimState::try_from(2), Ok(crate::model::ClaimState::Rejected));
    assert_eq!(crate::model::ClaimState::try_from(3), Ok(crate::model::ClaimState::Paid));
    assert!(crate::model::ClaimState::try_from(4).is_err());
    assert!(crate::model::ClaimState::try_from(255).is_err());
}

#[test]
fn test_derive_risk_type_id() {
    let oracle_pubkey = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let description = b"Smart contract hack protection";

    let id = derive_risk_type_id(&RiskCategory::SmartContractHack, description, &oracle_pubkey);

    // Should be deterministic
    let id2 = derive_risk_type_id(&RiskCategory::SmartContractHack, description, &oracle_pubkey);
    assert_eq!(id, id2);
}

#[test]
fn test_derive_underwriter_id() {
    let market_id = darkfi_sdk::pasta::pallas::Base::from(1);
    let owner = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let bond_amount: u64 = 1000;

    let id = derive_underwriter_id(market_id, &owner, bond_amount);

    // Should be deterministic
    let id2 = derive_underwriter_id(market_id, &owner, bond_amount);
    assert_eq!(id, id2);
}

#[test]
fn test_derive_coverage_id() {
    let market_id = darkfi_sdk::pasta::pallas::Base::from(1);
    let buyer = darkfi_sdk::crypto::PublicKey::from_publickey(
        &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
    );
    let amount: u64 = 500;
    let timestamp: u64 = 1700000000;

    let id = derive_coverage_id(market_id, &buyer, amount, timestamp);

    // Should be deterministic
    let id2 = derive_coverage_id(market_id, &buyer, amount, timestamp);
    assert_eq!(id, id2);
}

#[test]
fn test_derive_claim_id() {
    let coverage_id = darkfi_sdk::pasta::pallas::Base::from(1);
    let evidence_hash = darkfi_sdk::pasta::pallas::Base::from(2);
    let timestamp: u64 = 1700000000;

    let id = derive_claim_id(coverage_id, evidence_hash, timestamp);

    // Should be deterministic
    let id2 = derive_claim_id(coverage_id, evidence_hash, timestamp);
    assert_eq!(id, id2);
}

#[test]
fn test_calculate_premium() {
    // 5% of 10000 = 500
    let premium = calculate_premium(10000, 500).unwrap();
    assert_eq!(premium, 500);

    // 10% of 10000 = 1000
    let premium = calculate_premium(10000, 1000).unwrap();
    assert_eq!(premium, 1000);
}

#[test]
fn test_calculate_max_coverage() {
    // 10x leverage on 1000 = 10000
    let coverage = calculate_max_coverage(1000, 10).unwrap();
    assert_eq!(coverage, 10000);

    // 50x leverage on 1000 = 50000
    let coverage = calculate_max_coverage(1000, 50).unwrap();
    assert_eq!(coverage, 50000);
}

#[test]
fn test_calculate_slash() {
    // Full slash (worst performance score 0)
    let slash = calculate_slash(100, 1000, 500, 0).unwrap();
    assert_eq!(slash, 100); // claim_amount, capped at bond_amount

    // No slash (perfect performance score 10000)
    let slash = calculate_slash(100, 1000, 500, 10000).unwrap();
    assert_eq!(slash, 0);

    // Partial slash
    let slash = calculate_slash(100, 1000, 500, 5000).unwrap();
    assert_eq!(slash, 50);
}

#[test]
fn test_constants() {
    assert_eq!(INSURANCE_CONTRACT_RISK_TYPES_TREE, "risk_types");
    assert_eq!(INSURANCE_CONTRACT_MARKETS_TREE, "markets");
    assert_eq!(INSURANCE_CONTRACT_UNDERWRITERS_TREE, "underwriters");
    assert_eq!(INSURANCE_CONTRACT_COVERAGES_TREE, "coverages");
    assert_eq!(INSURANCE_CONTRACT_CLAIMS_TREE, "claims");
    assert_eq!(INSURANCE_CONTRACT_ENDOWMENT_TREE, "endowment");

    assert_eq!(DEFAULT_COVERAGE_PERIOD, 10000);
    assert_eq!(DEFAULT_PREMIUM_RATE, 500);
    assert_eq!(MIN_BOND_RATE, 1000);
    assert_eq!(DEFAULT_COVERAGE_LEVERAGE, 10);
    assert_eq!(MAX_COVERAGE_LEVERAGE, 50);
}