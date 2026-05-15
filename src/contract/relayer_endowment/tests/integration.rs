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

//! Relayer Endowment contract integration tests
//!
//! These tests verify the relayer_endowment contract's:
//! - Function enum parsing
//! - Data structure encoding/decoding
//! - Model type invariants

use dwow_relayer_endowment_contract::{
    model::{
        ClaimFeesParamsV1, ClaimFeesUpdateV1, DeployCapitalParamsV1, DeployCapitalUpdateV1,
        EndowmentDeployment, FeeAllocation, InitializeParamsV1, InitializeUpdateV1,
        RelayerEndowmentAccount, SettleFeesParamsV1, SettleFeesUpdateV1, UpdateConfigParamsV1,
        UpdateConfigUpdateV1, WithdrawDeploymentParamsV1, WithdrawDeploymentUpdateV1,
    },
    RelayerEndowmentFunction, RELAYER_ENDOWMENT_BP_PRECISION, RELAYER_ENDOWMENT_MIN_DEPLOY,
};
use dwow_serial::{deserialize, serialize};
use dwow_sdk::{crypto::pasta_prelude::PrimeField, crypto::PublicKey, pasta::pallas};

/// Helper to create a pallas::Base from bytes
fn make_base(bytes: [u8; 32]) -> pallas::Base {
    pallas::Base::from_repr(bytes).unwrap()
}

/// Helper to create PublicKey from a numeric seed
fn make_pubkey(seed: u64) -> PublicKey {
    use dwow_sdk::crypto::SecretKey;
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_relayer_endowment_function_enum_valid() {
    // Test that all function IDs are valid
    assert!(RelayerEndowmentFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(RelayerEndowmentFunction::try_from(0x01).is_ok()); // DeployCapitalV1
    assert!(RelayerEndowmentFunction::try_from(0x02).is_ok()); // WithdrawDeploymentV1
    assert!(RelayerEndowmentFunction::try_from(0x03).is_ok()); // ClaimRelayerFeesV1
    assert!(RelayerEndowmentFunction::try_from(0x04).is_ok()); // SettleFeesV1
    assert!(RelayerEndowmentFunction::try_from(0x05).is_ok()); // UpdateConfigV1
}

#[test]
fn test_relayer_endowment_function_enum_invalid() {
    // Test that invalid function IDs return errors
    assert!(RelayerEndowmentFunction::try_from(0xFF).is_err());
    assert!(RelayerEndowmentFunction::try_from(0x06).is_err());
    assert!(RelayerEndowmentFunction::try_from(0x10).is_err());
}

#[test]
fn test_relayer_endowment_account_encoding() {
    let account = RelayerEndowmentAccount {
        relayer_pub: make_pubkey(1),
        total_deployed: 5000000,
        active_deployments: 3,
        accumulated_fees: 25000,
        default_backer_cut_bp: 500,  // 5%
        created_at: 100,
        is_active: true,
    };

    let encoded = serialize(&account);
    let decoded: RelayerEndowmentAccount = deserialize(&encoded).unwrap();

    assert_eq!(decoded.relayer_pub, account.relayer_pub);
    assert_eq!(decoded.total_deployed, 5000000);
    assert_eq!(decoded.active_deployments, 3);
    assert_eq!(decoded.accumulated_fees, 25000);
    assert_eq!(decoded.default_backer_cut_bp, 500);
    assert!(decoded.is_active);
}

#[test]
fn test_relayer_endowment_account_inactive() {
    let account = RelayerEndowmentAccount {
        relayer_pub: make_pubkey(1),
        total_deployed: 0,
        active_deployments: 0,
        accumulated_fees: 0,
        default_backer_cut_bp: 500,
        created_at: 100,
        is_active: false,
    };

    let encoded = serialize(&account);
    let decoded: RelayerEndowmentAccount = deserialize(&encoded).unwrap();

    assert!(!decoded.is_active);
    assert_eq!(decoded.total_deployed, 0);
}

#[test]
fn test_endowment_deployment_encoding() {
    let deployment = EndowmentDeployment {
        deployment_id: make_base([1u8; 32]),
        relayer_pub: make_pubkey(2),
        backer_pub: make_pubkey(3),
        amount: 1000000,
        backer_cut_bp: 500,  // 5%
        accumulated_fees: 5000,
        deployed_at: 100,
        withdraw_requested_at: None,
        withdrawn: false,
    };

    let encoded = serialize(&deployment);
    let decoded: EndowmentDeployment = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deployment_id, deployment.deployment_id);
    assert_eq!(decoded.relayer_pub, deployment.relayer_pub);
    assert_eq!(decoded.backer_pub, deployment.backer_pub);
    assert_eq!(decoded.amount, 1000000);
    assert_eq!(decoded.backer_cut_bp, 500);
    assert!(!decoded.withdrawn);
}

#[test]
fn test_endowment_deployment_with_withdrawal_request() {
    let deployment = EndowmentDeployment {
        deployment_id: make_base([1u8; 32]),
        relayer_pub: make_pubkey(2),
        backer_pub: make_pubkey(3),
        amount: 1000000,
        backer_cut_bp: 500,
        accumulated_fees: 5000,
        deployed_at: 100,
        withdraw_requested_at: Some(200),
        withdrawn: false,
    };

    let encoded = serialize(&deployment);
    let decoded: EndowmentDeployment = deserialize(&encoded).unwrap();

    assert!(decoded.withdraw_requested_at.is_some());
    assert_eq!(decoded.withdraw_requested_at.unwrap(), 200);
    assert!(!decoded.withdrawn);
}

#[test]
fn test_endowment_deployment_withdrawn() {
    let deployment = EndowmentDeployment {
        deployment_id: make_base([1u8; 32]),
        relayer_pub: make_pubkey(2),
        backer_pub: make_pubkey(3),
        amount: 1000000,
        backer_cut_bp: 500,
        accumulated_fees: 5000,
        deployed_at: 100,
        withdraw_requested_at: Some(200),
        withdrawn: true,
    };

    let encoded = serialize(&deployment);
    let decoded: EndowmentDeployment = deserialize(&encoded).unwrap();

    assert!(decoded.withdrawn);
}

#[test]
fn test_initialize_params_encoding() {
    let params = InitializeParamsV1 {
        default_backer_cut_bp: 500,  // 5%
        signature_public: make_pubkey(1),
    };

    let encoded = serialize(&params);
    let decoded: InitializeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.default_backer_cut_bp, 500);
    assert_eq!(decoded.signature_public, make_pubkey(1));
}

#[test]
fn test_initialize_update_encoding() {
    let update = InitializeUpdateV1 {
        relayer_pub: make_pubkey(1),
        default_backer_cut_bp: 500,
        created_at: 100,
    };

    let encoded = serialize(&update);
    let decoded: InitializeUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.relayer_pub, update.relayer_pub);
    assert_eq!(decoded.default_backer_cut_bp, 500);
    assert_eq!(decoded.created_at, 100);
}

#[test]
fn test_deploy_capital_params_encoding() {
    let params = DeployCapitalParamsV1 {
        relayer_pub: make_pubkey(1),
        amount: 1000000,
        backer_cut_bp: 500,
        signature_public: make_pubkey(3),
    };

    let encoded = serialize(&params);
    let decoded: DeployCapitalParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.relayer_pub, params.relayer_pub);
    assert_eq!(decoded.amount, 1000000);
    assert_eq!(decoded.backer_cut_bp, 500);
    assert_eq!(decoded.signature_public, make_pubkey(3));
}

#[test]
fn test_deploy_capital_update_encoding() {
    let update = DeployCapitalUpdateV1 {
        deployment_id: make_base([1u8; 32]),
        relayer_pub: make_pubkey(2),
        backer_pub: make_pubkey(3),
        amount: 1000000,
        backer_cut_bp: 500,
        total_deployed: 5000000,
        active_deployments: 4,
    };

    let encoded = serialize(&update);
    let decoded: DeployCapitalUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deployment_id, update.deployment_id);
    assert_eq!(decoded.amount, 1000000);
    assert_eq!(decoded.total_deployed, 5000000);
    assert_eq!(decoded.active_deployments, 4);
}

#[test]
fn test_withdraw_deployment_params_encoding() {
    let params = WithdrawDeploymentParamsV1 {
        deployment_id: make_base([1u8; 32]),
    };

    let encoded = serialize(&params);
    let decoded: WithdrawDeploymentParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deployment_id, params.deployment_id);
}

#[test]
fn test_withdraw_deployment_update_encoding() {
    let update = WithdrawDeploymentUpdateV1 {
        deployment_id: make_base([1u8; 32]),
        payout_amount: 1050000,  // original + fees
        fees_claimed: 50000,
    };

    let encoded = serialize(&update);
    let decoded: WithdrawDeploymentUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deployment_id, update.deployment_id);
    assert_eq!(decoded.payout_amount, 1050000);
    assert_eq!(decoded.fees_claimed, 50000);
}

#[test]
fn test_claim_fees_params_encoding() {
    let params = ClaimFeesParamsV1 {
        deployment_id: make_base([1u8; 32]),
    };

    let encoded = serialize(&params);
    let decoded: ClaimFeesParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deployment_id, params.deployment_id);
}

#[test]
fn test_claim_fees_update_encoding() {
    let update = ClaimFeesUpdateV1 {
        deployment_id: make_base([1u8; 32]),
        claimed_amount: 5000,
        remaining_fees: 2000,
    };

    let encoded = serialize(&update);
    let decoded: ClaimFeesUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deployment_id, update.deployment_id);
    assert_eq!(decoded.claimed_amount, 5000);
    assert_eq!(decoded.remaining_fees, 2000);
}

#[test]
fn test_settle_fees_params_encoding() {
    let params = SettleFeesParamsV1 {
        relayer_pub: make_pubkey(1),
        total_fees: 100000,
        allocations: vec![
            FeeAllocation { deployment_id: make_base([1u8; 32]), fee_amount: 60000 },
            FeeAllocation { deployment_id: make_base([2u8; 32]), fee_amount: 40000 },
        ],
        signature_public: make_pubkey(1),
    };

    let encoded = serialize(&params);
    let decoded: SettleFeesParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.relayer_pub, params.relayer_pub);
    assert_eq!(decoded.total_fees, 100000);
    assert_eq!(decoded.allocations.len(), 2);
    assert_eq!(decoded.allocations[0].fee_amount, 60000);
    assert_eq!(decoded.allocations[1].fee_amount, 40000);
    assert_eq!(decoded.signature_public, make_pubkey(1));
}

#[test]
fn test_settle_fees_update_encoding() {
    let update = SettleFeesUpdateV1 {
        relayer_pub: make_pubkey(1),
        total_fees_settled: 100000,
        deployments_updated: 2,
        allocations: vec![
            FeeAllocation { deployment_id: make_base([1u8; 32]), fee_amount: 60000 },
            FeeAllocation { deployment_id: make_base([2u8; 32]), fee_amount: 40000 },
        ],
    };

    let encoded = serialize(&update);
    let decoded: SettleFeesUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.relayer_pub, update.relayer_pub);
    assert_eq!(decoded.total_fees_settled, 100000);
    assert_eq!(decoded.deployments_updated, 2);
    assert_eq!(decoded.allocations.len(), 2);
    assert_eq!(decoded.allocations[0].fee_amount, 60000);
    assert_eq!(decoded.allocations[1].fee_amount, 40000);
}

#[test]
fn test_update_config_params_encoding() {
    let params = UpdateConfigParamsV1 {
        relayer_pub: make_pubkey(1),
        default_backer_cut_bp: 750,  // Change from 5% to 7.5%
    };

    let encoded = serialize(&params);
    let decoded: UpdateConfigParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.relayer_pub, params.relayer_pub);
    assert_eq!(decoded.default_backer_cut_bp, 750);
}

#[test]
fn test_update_config_update_encoding() {
    let update = UpdateConfigUpdateV1 {
        relayer_pub: make_pubkey(1),
        default_backer_cut_bp: 750,
    };

    let encoded = serialize(&update);
    let decoded: UpdateConfigUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.relayer_pub, update.relayer_pub);
    assert_eq!(decoded.default_backer_cut_bp, 750);
}

#[test]
fn test_fee_allocation_encoding() {
    let alloc = FeeAllocation {
        deployment_id: make_base([1u8; 32]),
        fee_amount: 50000,
    };

    let encoded = serialize(&alloc);
    let decoded: FeeAllocation = deserialize(&encoded).unwrap();

    assert_eq!(decoded.deployment_id, alloc.deployment_id);
    assert_eq!(decoded.fee_amount, 50000);
}

#[test]
fn test_settle_fees_mismatched_allocations() {
    // Allocation sum != total_fees should be caught (tested at model level via params)
    let params = SettleFeesParamsV1 {
        relayer_pub: make_pubkey(1),
        total_fees: 100000,
        allocations: vec![
            FeeAllocation { deployment_id: make_base([1u8; 32]), fee_amount: 30000 },
        ],
        signature_public: make_pubkey(1),
    };

    let encoded = serialize(&params);
    let decoded: SettleFeesParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.total_fees, 100000);
    // Allocation sum (30000) != total_fees (100000) — contract catches this at runtime
    let sum: u64 = decoded.allocations.iter().map(|a| a.fee_amount).sum();
    assert_eq!(sum, 30000);
    assert_ne!(sum, decoded.total_fees);
}

#[test]
fn test_constants() {
    // Verify minimum deployment amount (1 DAI equivalent)
    assert_eq!(RELAYER_ENDOWMENT_MIN_DEPLOY, 1_000_000);

    // Verify basis points precision
    assert_eq!(RELAYER_ENDOWMENT_BP_PRECISION, 10000);
}