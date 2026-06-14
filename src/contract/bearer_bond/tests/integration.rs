/* This file is part of DarkWow
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * DarkWow is a tool for people and nations to establish sovereignty
 * according to human rights law. See the UN Declaration on the Rights
 * of Indigenous Peoples and associated documents:
 * https://documents.un.org/doc/undoc/gen/g26/031/70/pdf/g2603170.pdf
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

//! Bearer Bond contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::crypto::ContractId;
use dwow_sdk::pasta::pallas;
use dwow_bearer_bond_contract::{
    model::{BondSeriesInfo, SeriesStatus},
    BearerBondFunction,
    BEARER_BOND_CONTRACT_INFO_TREE, BEARER_BOND_CONTRACT_COINS_TREE,
    BEARER_BOND_CONTRACT_NULLIFIERS_TREE, BEARER_BOND_CONTRACT_BONDS_INFO_TREE,
    BEARER_BOND_CONTRACT_COIN_ROOTS_TREE, BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE,
};

// ============================================================================
// Function enum tests
// ============================================================================

#[test]
fn test_function_enum_all_opcodes() {
    assert!(matches!(BearerBondFunction::try_from(0u8).unwrap(), BearerBondFunction::IssueStakeV1));
    assert!(matches!(BearerBondFunction::try_from(1u8).unwrap(), BearerBondFunction::TransferStakeV1));
    assert!(matches!(BearerBondFunction::try_from(2u8).unwrap(), BearerBondFunction::RequestInterestV1));
    assert!(matches!(BearerBondFunction::try_from(3u8).unwrap(), BearerBondFunction::EmergencyUnstakeV1));
    assert!(matches!(BearerBondFunction::try_from(4u8).unwrap(), BearerBondFunction::UnstakeV1));
    assert!(matches!(BearerBondFunction::try_from(5u8).unwrap(), BearerBondFunction::BurnStakeV1));
    assert!(matches!(BearerBondFunction::try_from(6u8).unwrap(), BearerBondFunction::ProveCoverageV1));
    assert!(matches!(BearerBondFunction::try_from(7u8).unwrap(), BearerBondFunction::VerifyCoverageV1));
    assert!(matches!(BearerBondFunction::try_from(8u8).unwrap(), BearerBondFunction::PayInterestV1));
}

#[test]
fn test_function_enum_invalid() {
    assert!(BearerBondFunction::try_from(0xFF).is_err());
    assert!(BearerBondFunction::try_from(9u8).is_err());
}

// ============================================================================
// Model tests
// ============================================================================

#[test]
fn test_series_status_roundtrip() {
    let status = SeriesStatus::Active;
    let encoded = serialize(&status);
    let decoded: SeriesStatus = deserialize(&encoded).unwrap();
    assert!(matches!(decoded, SeriesStatus::Active));
}

#[test]
fn test_series_status_all_variants() {
    for status in [SeriesStatus::Active, SeriesStatus::Matured] {
        let encoded = serialize(&status);
        let _decoded: SeriesStatus = deserialize(&encoded).unwrap();
    }
}

#[test]
fn test_bond_series_info_serializable() {
    let info = BondSeriesInfo {
        series_token_id: pallas::Base::from(1),
        interest_rate_bps: 500,
        maturity_block: 1000,
        status: SeriesStatus::Active,
        issuer_contract: ContractId::from(pallas::Base::from(99)),
        total_staked: 10000,
    };
    let encoded = serialize(&info);
    let decoded: BondSeriesInfo = deserialize(&encoded).unwrap();
    assert_eq!(decoded.interest_rate_bps, info.interest_rate_bps);
    assert_eq!(decoded.maturity_block, info.maturity_block);
    assert_eq!(decoded.total_staked, info.total_staked);
}

// ============================================================================
// Constants tests
// ============================================================================

#[test]
fn test_tree_constants_non_empty() {
    assert!(!BEARER_BOND_CONTRACT_INFO_TREE.is_empty());
    assert!(!BEARER_BOND_CONTRACT_COINS_TREE.is_empty());
    assert!(!BEARER_BOND_CONTRACT_NULLIFIERS_TREE.is_empty());
    assert!(!BEARER_BOND_CONTRACT_BONDS_INFO_TREE.is_empty());
}

#[test]
fn test_root_tree_keys_exist() {
    assert!(!BEARER_BOND_CONTRACT_COIN_ROOTS_TREE.is_empty());
    assert!(!BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE.is_empty());
}

// ============================================================================
// Manifest test
// ============================================================================

#[test]
fn test_manifest_exists() {
    let manifest_path = concat!(env!("CARGO_MANIFEST_DIR"), "/manifest.toml");
    assert!(
        std::path::Path::new(manifest_path).exists(),
        "bearer_bond manifest.toml must exist"
    );
}

#[test]
fn test_manifest_parseable() {
    let manifest_path = concat!(env!("CARGO_MANIFEST_DIR"), "/manifest.toml");
    let toml_str =
        std::fs::read_to_string(manifest_path).expect("manifest.toml must be readable");
    let manifest = dwow_sdk::manifest::ContractManifest::from_toml(&toml_str)
        .expect("manifest.toml must parse");
    assert_eq!(manifest.name, "bearer_bond");
    assert!(!manifest.functions.is_empty(), "must declare functions");
    assert!(!manifest.trees.is_empty(), "must declare trees");
    assert!(!manifest.circuits.is_empty(), "must declare circuits");
}
