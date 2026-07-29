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

//! Betting Stake contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::{crypto::{pasta_prelude::Group, schnorr::Signature}, pasta::pallas};
use dwow_betting_stake_contract::{
    model::{
        ClaimEarningsParamsV1, ClaimEarningsUpdateV1, InitializeParamsV1, InitializeUpdateV1,
        Stake, StakeParamsV1, StakeUpdateV1, TableStakeRegistry, UnstakeParamsV1,
        UnstakeUpdateV1, UpdateRiskParamsV1, UpdateRiskUpdateV1,
    },
    BettingStakeFunction, RiskProfile,
    // Constants
    BETTING_STAKE_REGISTRY_TREE, BETTING_STAKE_STAKES_TREE,
    BETTING_STAKE_EARNINGS_TREE, MIN_STAKE_AMOUNT, MAX_STAKE_RATIO,
    EARNINGS_BP,
};

/// Helper to create a test PublicKey
fn make_pubkey(seed: u64) -> dwow_sdk::crypto::PublicKey {
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    let secret = SecretKey::from_base(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_betting_stake_function_enum_valid() {
    assert!(BettingStakeFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(BettingStakeFunction::try_from(0x01).is_ok()); // StakeV1
    assert!(BettingStakeFunction::try_from(0x02).is_ok()); // UnstakeV1
    assert!(BettingStakeFunction::try_from(0x03).is_ok()); // ClaimEarningsV1
    assert!(BettingStakeFunction::try_from(0x04).is_ok()); // UpdateRiskV1
}

#[test]
fn test_betting_stake_function_enum_invalid() {
    assert!(BettingStakeFunction::try_from(0xFF).is_err());
    assert!(BettingStakeFunction::try_from(0x05).is_err());
    assert!(BettingStakeFunction::try_from(0x10).is_err());
}

#[test]
fn test_risk_profile_values() {
    assert_eq!(RiskProfile::Low as u8, 0);
    assert_eq!(RiskProfile::Medium as u8, 1);
    assert_eq!(RiskProfile::High as u8, 2);
}

#[test]
fn test_risk_profile_premium() {
    assert_eq!(RiskProfile::Low.risk_premium_bp(), 100);
    assert_eq!(RiskProfile::Medium.risk_premium_bp(), 250);
    assert_eq!(RiskProfile::High.risk_premium_bp(), 500);
}

#[test]
fn test_constants() {
    assert_eq!(BETTING_STAKE_REGISTRY_TREE, "staking_registry");
    assert_eq!(BETTING_STAKE_STAKES_TREE, "staking_stakes");
    assert_eq!(BETTING_STAKE_EARNINGS_TREE, "staking_earnings");
    assert_eq!(MIN_STAKE_AMOUNT, 100);
    assert_eq!(MAX_STAKE_RATIO, 100);
    assert_eq!(EARNINGS_BP, 10000);
}

#[test]
fn test_initialize_params_encoding() {
    let params = InitializeParamsV1 {
        instance_seed: [0u8; 32],
        betting_contract_id: pallas::Base::from(1),
        house_edge_bp: 200,
        risk_profile: 0,
        nonce: pallas::Base::from(0),
        signature: Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: InitializeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.betting_contract_id, params.betting_contract_id);
    assert_eq!(decoded.house_edge_bp, params.house_edge_bp);
    assert_eq!(decoded.risk_profile, params.risk_profile);
}

#[test]
fn test_initialize_update_encoding() {
    let update = InitializeUpdateV1 {
        instance_seed: [0u8; 32],
        table_id: pallas::Base::from(1),
        betting_contract_id: pallas::Base::from(2),
        house_edge_bp: 200,
        risk_profile: 0,
    };

    let encoded = serialize(&update);
    let decoded: InitializeUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, update.table_id);
    assert_eq!(decoded.betting_contract_id, update.betting_contract_id);
    assert_eq!(decoded.house_edge_bp, update.house_edge_bp);
    assert_eq!(decoded.risk_profile, update.risk_profile);
}

#[test]
fn test_stake_params_encoding() {
    let params = StakeParamsV1 {
        instance_seed: [0u8; 32],
        table_id: pallas::Base::from(1),
        staker_pub: make_pubkey(2),
        amount: 1000,
        nonce: pallas::Base::from(0),
        value_commit: pallas::Point::identity(),
        staker_nullifier: pallas::Base::zero(),
        spend_hook: pallas::Base::from(0),
        user_data: pallas::Base::from(0),
    };

    let encoded = serialize(&params);
    let decoded: StakeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, params.table_id);
    assert_eq!(decoded.staker_pub, params.staker_pub);
    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_stake_update_encoding() {
    let update = StakeUpdateV1 {
        instance_seed: [0u8; 32],
        stake_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        staker_pub: make_pubkey(3),
        amount: 1000,
        total_stake: 5000,
        staker_count: 10,
        staker_nullifier: pallas::Base::zero(),
    };

    let encoded = serialize(&update);
    let decoded: StakeUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake_id, update.stake_id);
    assert_eq!(decoded.table_id, update.table_id);
    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.total_stake, update.total_stake);
    assert_eq!(decoded.staker_count, update.staker_count);
}

#[test]
fn test_unstake_params_encoding() {
    let params = UnstakeParamsV1 {
        stake_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        staker_pub: make_pubkey(3),
        original_amount: 1000,
        nonce: pallas::Base::from(0),
        value_commit: pallas::Point::identity(),
        staker_nullifier: pallas::Base::zero(),
        spend_hook: pallas::Base::from(0),
        user_data: pallas::Base::from(0),
    };

    let encoded = serialize(&params);
    let decoded: UnstakeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake_id, params.stake_id);
}

#[test]
fn test_unstake_update_encoding() {
    let update = UnstakeUpdateV1 {
        stake_id: pallas::Base::from(1),
        payout_amount: 1100,
        unstake_penalty: 0,
        staker_nullifier: pallas::Base::zero(),
    };

    let encoded = serialize(&update);
    let decoded: UnstakeUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake_id, update.stake_id);
    assert_eq!(decoded.payout_amount, update.payout_amount);
    assert_eq!(decoded.unstake_penalty, update.unstake_penalty);
}

#[test]
fn test_claim_earnings_params_encoding() {
    let params = ClaimEarningsParamsV1 {
        stake_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        staker_pub: make_pubkey(3),
        current_amount: 1000,
        nonce: pallas::Base::from(0),
        value_commit: pallas::Point::identity(),
        staker_nullifier: pallas::Base::zero(),
    };

    let encoded = serialize(&params);
    let decoded: ClaimEarningsParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake_id, params.stake_id);
}

#[test]
fn test_claim_earnings_update_encoding() {
    let update = ClaimEarningsUpdateV1 {
        stake_id: pallas::Base::from(1),
        claimed_amount: 50,
        remaining_earnings: 150,
        staker_nullifier: pallas::Base::zero(),
    };

    let encoded = serialize(&update);
    let decoded: ClaimEarningsUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.stake_id, update.stake_id);
    assert_eq!(decoded.claimed_amount, update.claimed_amount);
    assert_eq!(decoded.remaining_earnings, update.remaining_earnings);
}

#[test]
fn test_update_risk_params_encoding() {
    let params = UpdateRiskParamsV1 {
        table_id: pallas::Base::from(1),
        payout_amount: 1000,
        house_share: 100,
        betting_contract_id: pallas::Base::from(2),
        nonce: pallas::Base::from(0),
    };

    let encoded = serialize(&params);
    let decoded: UpdateRiskParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, params.table_id);
    assert_eq!(decoded.payout_amount, params.payout_amount);
    assert_eq!(decoded.house_share, params.house_share);
}

#[test]
fn test_update_risk_update_encoding() {
    let update = UpdateRiskUpdateV1 {
        table_id: pallas::Base::from(1),
        total_payout: 1000,
        staker_loss: 900,
        staker_count: 10,
        new_total_stake: 4100,
    };

    let encoded = serialize(&update);
    let decoded: UpdateRiskUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, update.table_id);
    assert_eq!(decoded.total_payout, update.total_payout);
    assert_eq!(decoded.staker_loss, update.staker_loss);
    assert_eq!(decoded.staker_count, update.staker_count);
    assert_eq!(decoded.new_total_stake, update.new_total_stake);
}

#[test]
fn test_table_stake_registry_encoding() {
    let registry = TableStakeRegistry {

        version: 0,        betting_contract_id: pallas::Base::from(1),
        total_stake: 10000,
        accumulated_earnings: 500,
        accumulated_losses: 200,
        staker_count: 10,
        house_edge_bp: 200,
        risk_profile: 0,
    };

    let encoded = registry.encode();
    let decoded: TableStakeRegistry = TableStakeRegistry::decode(&encoded).unwrap();

    assert_eq!(decoded.betting_contract_id, registry.betting_contract_id);
    assert_eq!(decoded.total_stake, registry.total_stake);
    assert_eq!(decoded.accumulated_earnings, registry.accumulated_earnings);
    assert_eq!(decoded.accumulated_losses, registry.accumulated_losses);
    assert_eq!(decoded.staker_count, registry.staker_count);
    assert_eq!(decoded.house_edge_bp, registry.house_edge_bp);
    assert_eq!(decoded.risk_profile, registry.risk_profile);
}

#[test]
fn test_table_stake_registry_earnings_rate() {
    let registry = TableStakeRegistry {

        version: 0,        betting_contract_id: pallas::Base::from(1),
        total_stake: 10000,
        accumulated_earnings: 500,
        accumulated_losses: 200,
        staker_count: 10,
        house_edge_bp: 200,
        risk_profile: 0,
    };

    // earnings_rate_bp = house_edge_bp - (accumulated_losses * EARNINGS_BP / total_stake)
    // = 200 - (200 * 10000 / 10000) = 200 - 200 = 0
    assert_eq!(registry.earnings_rate_bp(), 0);
}

#[test]
fn test_table_stake_registry_loss_absorption() {
    let registry = TableStakeRegistry {

        version: 0,        betting_contract_id: pallas::Base::from(1),
        total_stake: 10000,
        accumulated_earnings: 500,
        accumulated_losses: 2000,
        staker_count: 10,
        house_edge_bp: 200,
        risk_profile: 0,
    };

    // loss_absorption_capacity = total_stake - accumulated_losses
    assert_eq!(registry.loss_absorption_capacity(), 8000);
}

#[test]
fn test_stake_encoding() {
    let stake = Stake {

        version: 0,        instance_seed: [0u8; 32],
        stake_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        staker_pub: make_pubkey(3),
        original_amount: 1000,
        current_amount: 900,
        accumulated_earnings: 50,
        created_at: 100,
        unstake_requested_at: None,
        is_active: true,
    };

    let encoded = stake.encode();
    let decoded: Stake = Stake::decode(&encoded).unwrap();

    assert_eq!(decoded.stake_id, stake.stake_id);
    assert_eq!(decoded.table_id, stake.table_id);
    assert_eq!(decoded.staker_pub, stake.staker_pub);
    assert_eq!(decoded.original_amount, stake.original_amount);
    assert_eq!(decoded.current_amount, stake.current_amount);
    assert_eq!(decoded.accumulated_earnings, stake.accumulated_earnings);
    assert_eq!(decoded.created_at, stake.created_at);
    assert_eq!(decoded.unstake_requested_at, stake.unstake_requested_at);
    assert_eq!(decoded.is_active, stake.is_active);
}

#[test]
fn test_stake_earnings_share() {
    let stake = Stake {

        version: 0,        stake_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        staker_pub: make_pubkey(3),
        original_amount: 1000,
        current_amount: 1000, // Full stake
        accumulated_earnings: 0,
        created_at: 100,
        unstake_requested_at: None,
        is_active: true,
        instance_seed: [0u8; 32],
    };

    let table = TableStakeRegistry {

        version: 0,        betting_contract_id: pallas::Base::from(1),
        total_stake: 10000,
        accumulated_earnings: 1000,
        accumulated_losses: 0,
        staker_count: 10,
        house_edge_bp: 200,
        risk_profile: 0,
    };

    // earnings_share = accumulated_earnings * current_amount / total_stake
    // = 1000 * 1000 / 10000 = 100
    assert_eq!(stake.earnings_share(&table), 100);
}

#[test]
fn test_stake_can_unstake() {
    let stake_active = Stake {

        version: 0,        instance_seed: [0u8; 32],
        stake_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        staker_pub: make_pubkey(3),
        original_amount: 1000,
        current_amount: 1000,
        accumulated_earnings: 0,
        created_at: 100,
        unstake_requested_at: None,
        is_active: true,
    };

    // No pending unstake request - can unstake immediately
    assert!(stake_active.can_unstake(10, 50));

    // Inactive stake - cannot unstake
    let stake_inactive = Stake {

        version: 0,        instance_seed: [0u8; 32],
        stake_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        staker_pub: make_pubkey(3),
        original_amount: 1000,
        current_amount: 1000,
        accumulated_earnings: 0,
        created_at: 100,
        unstake_requested_at: None,
        is_active: false,
    };
    assert!(!stake_inactive.can_unstake(10, 50));
}

#[test]
fn test_stake_can_unstake_with_lock() {
    let stake = Stake {

        version: 0,        stake_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        staker_pub: make_pubkey(3),
        original_amount: 1000,
        current_amount: 1000,
        accumulated_earnings: 0,
        created_at: 100,
        unstake_requested_at: Some(50), // Requested at block 50
        is_active: true,
        instance_seed: [0u8; 32],
    };

    // Lock period not passed (current_block=50, req_at=50, lock=10 => need 60)
    assert!(!stake.can_unstake(10, 55));

    // Lock period passed
    assert!(stake.can_unstake(10, 65));
}

#[test]
fn test_derive_table_id() {
    use dwow_betting_stake_contract::model::derive_table_id;

    let betting_contract_id = pallas::Base::from(1);
    let nonce = 42u64;

    let table_id = derive_table_id(betting_contract_id, nonce);

    // Table ID should be non-zero
    assert!(table_id != pallas::Base::zero());
}

#[test]
fn test_derive_stake_id() {
    use dwow_betting_stake_contract::model::derive_stake_id;

    let table_id = pallas::Base::from(1);
    let staker_pub = make_pubkey(2);
    let nonce = 42u64;

    let stake_id = derive_stake_id(table_id, &staker_pub, 1000, nonce);

    // Stake ID should be non-zero
    assert!(stake_id != pallas::Base::zero());
}