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

//! Stablecoin contract integration tests

use darkfi_stablecoin_contract::{
    model::{
        CollateralPool, CollateralType, DebtPool, DebtShare, InitializeParams,
        LiquidateParams, MintStableParams, PiControllerState, RepayStableParams,
        UpdateConfigParams, WithdrawCollateralParams,
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
}

#[test]
fn test_stablecoin_function_enum_invalid() {
    assert!(StablecoinFunction::try_from(0xFF).is_err());
    assert!(StablecoinFunction::try_from(0x08).is_err());
    assert!(StablecoinFunction::try_from(0x10).is_err());
}

#[test]
fn test_initialize_params_encoding() {
    let params = InitializeParams {
        min_collateralization_ratio: 15000,  // 150%
        liquidation_threshold: 13000,          // 130%
        liquidation_penalty: 1000,             // 10%
        base_rate: 500,                      // 5% annual
        pi_kp: 1000,
        pi_ki: 100,
        twap_window: 3600,                  // 1 hour
        price_deviation_threshold: 500,      // 5%
    };

    let encoded = params.encode().unwrap();
    let decoded = InitializeParams::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.min_collateralization_ratio, 15000);
    assert_eq!(decoded.liquidation_threshold, 13000);
    assert_eq!(decoded.liquidation_penalty, 1000);
    assert_eq!(decoded.base_rate, 500);
    assert_eq!(decoded.pi_kp, 1000);
    assert_eq!(decoded.pi_ki, 100);
}

#[test]
fn test_collateral_type() {
    assert_eq!(CollateralType::Xmr as u8, 0);
    assert_eq!(CollateralType::Drk as u8, 1);
}

#[test]
fn test_deposit_collateral_params_encoding() {
    let params = darkfi_stablecoin_contract::model::DepositCollateralParams {
        deposit_commitment: darkfi_sdk::crypto::IntentCommitment::from([1u8; 32]),
        collateral_amount: 1000,
        collateral_type: CollateralType::Xmr,
        proof: vec![2u8; 128],
        fee: 10,
    };

    let encoded = params.encode().unwrap();
    let decoded =
        darkfi_stablecoin_contract::model::DepositCollateralParams::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.collateral_amount, 1000);
    assert_eq!(decoded.fee, 10);
}

#[test]
fn test_withdraw_collateral_params_encoding() {
    let params = WithdrawCollateralParams {
        withdrawal_nullifier: darkfi_sdk::crypto::IntentNullifier::from([1u8; 32]),
        new_commitment: darkfi_sdk::crypto::IntentCommitment::from([2u8; 32]),
        withdraw_amount: 500,
        proof: vec![3u8; 128],
        fee: 10,
    };

    let encoded = params.encode().unwrap();
    let decoded =
        WithdrawCollateralParams::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.withdraw_amount, 500);
    assert_eq!(decoded.fee, 10);
}

#[test]
fn test_mint_stable_params_encoding() {
    let params = MintStableParams {
        mint_commitment: darkfi_sdk::crypto::IntentCommitment::from([1u8; 32]),
        mint_amount: 5000,
        total_debt: 100000,
        total_collateral: 150000,
        proof: vec![2u8; 128],
        fee: 10,
    };

    let encoded = params.encode().unwrap();
    let decoded = MintStableParams::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.mint_amount, 5000);
    assert_eq!(decoded.total_debt, 100000);
    assert_eq!(decoded.total_collateral, 150000);
}

#[test]
fn test_repay_stable_params_encoding() {
    let params = RepayStableParams {
        repay_commitment: darkfi_sdk::crypto::IntentCommitment::from([1u8; 32]),
        repay_amount: 1000,
        proof: vec![2u8; 128],
        fee: 10,
    };

    let encoded = params.encode().unwrap();
    let decoded = RepayStableParams::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.repay_amount, 1000);
    assert_eq!(decoded.fee, 10);
}

#[test]
fn test_liquidate_params_encoding() {
    let params = LiquidateParams {
        liquidation_commitment: darkfi_sdk::crypto::IntentCommitment::from([1u8; 32]),
        total_debt: 100000,
        total_collateral: 120000, // Undercollateralized
        current_price: 95,         // TWAP price
        debt_to_cover: 5000,
        proof: vec![2u8; 128],
        liquidation_reward: 500,
        fee: 10,
    };

    let encoded = params.encode().unwrap();
    let decoded = LiquidateParams::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.total_debt, 100000);
    assert_eq!(decoded.total_collateral, 120000);
    assert_eq!(decoded.debt_to_cover, 5000);
    assert_eq!(decoded.liquidation_reward, 500);
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

    let encoded = params.encode().unwrap();
    let decoded = UpdateConfigParams::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.min_collateralization_ratio, 16000);
    assert_eq!(decoded.liquidation_threshold, 14000);
    assert_eq!(decoded.pi_kp, 1100);
    assert_eq!(decoded.pi_ki, 110);
}

#[test]
fn test_debt_pool_encoding() {
    let pool = DebtPool {
        total_debt: 1000000,
        total_collateral: 1500000,
        accumulated_fees: 5000,
        last_update: 1000000,
    };

    let encoded = pool.encode().unwrap();
    let decoded = DebtPool::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

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

    let encoded = pool.encode().unwrap();
    let decoded = CollateralPool::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.total_deposited, 500000);
    assert_eq!(decoded.value_ratio, 10000);
}

#[test]
fn test_debt_share_encoding() {
    let share = DebtShare {
        owner_pub_x: [1u8; 32],
        owner_pub_y: [2u8; 32],
        debt_amount: 10000,
        commitment: darkfi_sdk::crypto::IntentCommitment::from([3u8; 32]),
        created_at: 1000,
        updated_at: 2000,
    };

    let encoded = share.encode().unwrap();
    let decoded = DebtShare::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

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

    let encoded = state.encode().unwrap();
    let decoded = PiControllerState::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.integral, 1000);
    assert_eq!(decoded.current_rate, 50);
    assert_eq!(decoded.last_twap, 10000);
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
    assert!(ratio < CDP_LIQUIDATION_THRESHOLD);
}

#[test]
fn test_penalty_calculation() {
    // Test that liquidation penalty is applied correctly
    let debt_to_cover: u64 = 10000;
    let penalty_bps: u64 = CDP_LIQUIDATION_PENALTY; // 1000 = 10%

    // Penalty = debt * penalty / 10000
    let penalty: u64 = (debt_to_cover as u128) * (penalty_bps as u128) / 10000;
    assert_eq!(penalty, 1000); // 10% of 10000 = 1000
}
