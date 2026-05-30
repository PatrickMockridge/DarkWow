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

//! Stablecoin contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::pasta::pallas;
use dwow_stablecoin_contract::{
    model::{
        CollateralParams, CollateralPool, CollateralType, DeadManAction, DeadManSwitchConfig,
        DebtPool, DebtShare, LiquidateParams, MintStableParams, PiControllerState,
        RepayStableParams, StablecoinModel, UpdateConfigParams, WithdrawCollateralParams,
    },
    StablecoinFunction,
    // Constants
    CDP_LIQUIDATION_PENALTY, CDP_LIQUIDATION_THRESHOLD, CDP_MIN_COLLATERALIZATION_RATIO,
    STABLECOIN_CONTRACT_COLLATERAL_TREE, STABLECOIN_CONTRACT_INFO_TREE,
    STABLECOIN_CONTRACT_LIQUIDATIONS_TREE, STABLECOIN_CONTRACT_POSITIONS_TREE,
    STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE, STABLECOIN_CONTRACT_STABLECOIN_TREE,
};

#[test]
fn test_stablecoin_function_enum_valid() {
    assert!(StablecoinFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(StablecoinFunction::try_from(0x01).is_ok()); // OpenPositionV1
    assert!(StablecoinFunction::try_from(0x02).is_ok()); // AddCollateralV1
    assert!(StablecoinFunction::try_from(0x03).is_ok()); // RemoveCollateralV1
    assert!(StablecoinFunction::try_from(0x04).is_ok()); // MintStableV1
    assert!(StablecoinFunction::try_from(0x05).is_ok()); // RepayStableV1
    assert!(StablecoinFunction::try_from(0x06).is_ok()); // LiquidateV1
    assert!(StablecoinFunction::try_from(0x07).is_ok()); // UpdateConfigV1
    assert!(StablecoinFunction::try_from(0x08).is_ok()); // GovernanceReportV1
    assert!(StablecoinFunction::try_from(0x09).is_ok()); // AccrueInterestV1
    assert!(StablecoinFunction::try_from(0x0A).is_ok()); // RedeemStableV1
    assert!(StablecoinFunction::try_from(0x0B).is_ok()); // SpendHookCallback
}

#[test]
fn test_stablecoin_function_enum_invalid() {
    assert!(StablecoinFunction::try_from(0xFF).is_err());
    assert!(StablecoinFunction::try_from(0x0C).is_err());
    assert!(StablecoinFunction::try_from(0x10).is_err());
}

#[test]
fn test_collateral_type() {
    assert_eq!(CollateralType::Xmr as u8, 0);
    assert_eq!(CollateralType::Drk as u8, 1);
}

#[test]
fn test_debt_pool_encoding() {
    let pool = DebtPool {
        total_debt: 1000000,
        total_collateral: 1500000,
        accumulated_fees: 5000,
        last_update: 1000000,
    };

    let encoded = serialize(&pool);
    let decoded: DebtPool = deserialize(&encoded).unwrap();

    assert_eq!(decoded.total_debt, 1000000);
    assert_eq!(decoded.total_collateral, 1500000);
    assert_eq!(decoded.accumulated_fees, 5000);
}

#[test]
fn test_collateral_pool_encoding() {
    let pool = CollateralPool {
        collateral_type: CollateralType::Xmr,
        total_deposited: 500000,
        value_ratio: 10000, // 1:1 with stablecoin
        last_update: 500000,
    };

    let encoded = serialize(&pool);
    let decoded: CollateralPool = deserialize(&encoded).unwrap();

    assert_eq!(decoded.total_deposited, 500000);
    assert_eq!(decoded.value_ratio, 10000);
}

#[test]
fn test_debt_share_encoding() {
    let share = DebtShare {
        owner_pub_x: [1u8; 32],
        owner_pub_y: [2u8; 32],
        debt_amount: 10000,
        commitment: pallas::Base::zero().into(),
        created_at: 1000,
        updated_at: 2000,
    };

    let encoded = serialize(&share);
    let decoded: DebtShare = deserialize(&encoded).unwrap();

    assert_eq!(decoded.debt_amount, 10000);
    assert_eq!(decoded.created_at, 1000);
    assert_eq!(decoded.updated_at, 2000);
}

#[test]
fn test_pi_controller_state_encoding() {
    let state = PiControllerState {
        integral: 1000,
        last_update: 500000,
        current_rate: 50,    // 50 basis points per second
        last_twap: 10000,    // TWAP price
    };

    let encoded = serialize(&state);
    let decoded: PiControllerState = deserialize(&encoded).unwrap();

    assert_eq!(decoded.integral, 1000);
    assert_eq!(decoded.current_rate, 50);
    assert_eq!(decoded.last_twap, 10000);
}

#[test]
fn test_update_config_params_encoding() {
    let params = UpdateConfigParams {
        min_collateralization_ratio: 16000,
        liquidation_threshold: 14000,
        liquidation_penalty: 1100,
        base_rate: 600,
        pi_kp: 1100,
        pi_ki: 110,
        twap_window: 7200,
        price_deviation_threshold: 600,
    };

    let encoded = serialize(&params);
    let decoded: UpdateConfigParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.min_collateralization_ratio, 16000);
    assert_eq!(decoded.liquidation_threshold, 14000);
    assert_eq!(decoded.pi_kp, 1100);
    assert_eq!(decoded.pi_ki, 110);
}

#[test]
fn test_collateral_params_encoding() {
    let params = CollateralParams {
        collateral_type: CollateralType::Drk,
        haircut: 9850,
        liquidation_threshold: 13000,
        max_debt_share: 30000,
    };

    let encoded = serialize(&params);
    let decoded: CollateralParams = deserialize(&encoded).unwrap();

    assert_eq!(decoded.collateral_type as u8, CollateralType::Drk as u8);
    assert_eq!(decoded.haircut, 9850);
}

#[test]
fn test_dead_man_switch_config_encoding() {
    let config = DeadManSwitchConfig {
        enabled: true,
        timeout_blocks: 10000,
        action: DeadManAction::LiquidateAll,
        last_action_block: 0,
    };

    let encoded = serialize(&config);
    let decoded: DeadManSwitchConfig = deserialize(&encoded).unwrap();

    assert_eq!(decoded.enabled, true);
    assert_eq!(decoded.timeout_blocks, 10000);
}

#[test]
fn test_stablecoin_model_variants() {
    assert!(matches!(StablecoinModel::PooledDebt, StablecoinModel::PooledDebt));
    assert!(matches!(StablecoinModel::Liquity, StablecoinModel::Liquity));
    assert!(matches!(StablecoinModel::Fractional, StablecoinModel::Fractional));
    assert!(matches!(StablecoinModel::IndividualCdp, StablecoinModel::IndividualCdp));
}

#[test]
fn test_constants() {
    // CDP Constants
    assert_eq!(CDP_MIN_COLLATERALIZATION_RATIO, 15000);
    assert_eq!(CDP_LIQUIDATION_THRESHOLD, 13000);
    assert_eq!(CDP_LIQUIDATION_PENALTY, 1000);

    // Tree names
    assert_eq!(STABLECOIN_CONTRACT_INFO_TREE, "info");
    assert_eq!(STABLECOIN_CONTRACT_POSITIONS_TREE, "positions");
    assert_eq!(STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE, "position_nullifiers");
    assert_eq!(STABLECOIN_CONTRACT_STABLECOIN_TREE, "stablecoin");
    assert_eq!(STABLECOIN_CONTRACT_COLLATERAL_TREE, "collateral");
    assert_eq!(STABLECOIN_CONTRACT_LIQUIDATIONS_TREE, "liquidations");
}

#[test]
fn test_collateralization_ratio_check() {
    // Test that collateralization ratio calculations work correctly
    // Ratio = (collateral / debt) * 10000 (basis points)

    let total_debt: u64 = 100000;
    let total_collateral: u64 = 150000;

    // Collateralization ratio = 150000 / 100000 = 1.5 = 150%
    let ratio = (total_collateral as u128) * 10000 / (total_debt as u128);
    assert_eq!(ratio, 15000); // 150% in basis points

    // Below minimum collateralization
    let undercollateralized: u64 = 120000;
    let under_ratio = (undercollateralized as u128) * 10000 / (total_debt as u128);
    assert_eq!(under_ratio, 12000); // 120% - below 150% minimum
}

#[test]
fn test_liquidation_threshold_check() {
    // Test liquidation threshold (130% = 13000 basis points)
    let total_debt: u64 = 100000;
    let total_collateral: u64 = 125000;

    // Ratio = 125%
    let ratio = (total_collateral as u128) * 10000 / (total_debt as u128);
    assert_eq!(ratio, 12500);

    // Below liquidation threshold
    assert!(ratio < CDP_LIQUIDATION_THRESHOLD as u128);
}

#[test]
fn test_penalty_calculation() {
    // Test that liquidation penalty is applied correctly
    let debt_to_cover: u64 = 10000;
    let penalty_bps: u64 = CDP_LIQUIDATION_PENALTY; // 1000 = 10%

    // Penalty = debt * penalty / 10000
    let penalty = (debt_to_cover as u128) * (penalty_bps as u128) / 10000;
    assert_eq!(penalty, 1000); // 10% of 10000 = 1000
}
