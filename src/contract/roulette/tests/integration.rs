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

//! Roulette contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::{crypto::schnorr::Signature, pasta::pallas};
use dwow_roulette_contract::{
    model::{
        BetType, HouseCloseParamsV1, HouseCloseUpdateV1, InitializeParamsV1,
        InitializeUpdateV1, PlaceBetParamsV1, PlaceBetUpdateV1, RouletteTableState,
        SettleBetsParamsV1, SettleBetsUpdateV1, SpinWheelParamsV1, SpinWheelUpdateV1,
    },
    RouletteFunction,
    // Constants
    ROULETTE_CONTRACT_BETS_HISTORY_TREE, ROULETTE_CONTRACT_BETS_TREE,
    ROULETTE_CONTRACT_NULLIFIERS_TREE, ROULETTE_CONTRACT_TABLES_TREE,
    AMERICAN_HOUSE_EDGE_BP, AMERICAN_WHEEL_SIZE, EUROPEAN_HOUSE_EDGE_BP, EUROPEAN_WHEEL_SIZE,
};

/// Helper to create a test PublicKey
fn make_pubkey(seed: u64) -> dwow_sdk::crypto::PublicKey {
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// Helper to create a test Signature
fn make_signature() -> Signature {
    Signature::dummy()
}

#[test]
fn test_roulette_function_enum_valid() {
    assert!(RouletteFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(RouletteFunction::try_from(0x01).is_ok()); // PlaceBetV1
    assert!(RouletteFunction::try_from(0x02).is_ok()); // SpinWheelV1
    assert!(RouletteFunction::try_from(0x03).is_ok()); // SettleBetsV1
    assert!(RouletteFunction::try_from(0x04).is_ok()); // HouseCloseV1
}

#[test]
fn test_roulette_function_enum_invalid() {
    assert!(RouletteFunction::try_from(0xFF).is_err());
    assert!(RouletteFunction::try_from(0x05).is_err());
    assert!(RouletteFunction::try_from(0x10).is_err());
}

#[test]
fn test_bet_type_values() {
    assert_eq!(BetType::Straight as u8, 0);
    assert_eq!(BetType::Split as u8, 1);
    assert_eq!(BetType::Street as u8, 2);
    assert_eq!(BetType::Corner as u8, 3);
    assert_eq!(BetType::SixLine as u8, 4);
    assert_eq!(BetType::Dozen as u8, 5);
    assert_eq!(BetType::Column as u8, 6);
    assert_eq!(BetType::EvenMoney as u8, 7);
}

#[test]
fn test_bet_type_payout_ratio() {
    assert_eq!(BetType::Straight.payout_ratio(), 35);
    assert_eq!(BetType::Split.payout_ratio(), 17);
    assert_eq!(BetType::Street.payout_ratio(), 11);
    assert_eq!(BetType::Corner.payout_ratio(), 8);
    assert_eq!(BetType::SixLine.payout_ratio(), 5);
    assert_eq!(BetType::Dozen.payout_ratio(), 2);
    assert_eq!(BetType::Column.payout_ratio(), 2);
    assert_eq!(BetType::EvenMoney.payout_ratio(), 1);
}

#[test]
fn test_roulette_table_state_values() {
    assert_eq!(RouletteTableState::Active as u8, 0);
    assert_eq!(RouletteTableState::WaitingForSpin as u8, 1);
    assert_eq!(RouletteTableState::Spun as u8, 2);
    assert_eq!(RouletteTableState::Settled as u8, 3);
    assert_eq!(RouletteTableState::Closed as u8, 4);
}

#[test]
fn test_constants() {
    assert_eq!(EUROPEAN_WHEEL_SIZE, 37);
    assert_eq!(AMERICAN_WHEEL_SIZE, 38);
    assert_eq!(EUROPEAN_HOUSE_EDGE_BP, 270);
    assert_eq!(AMERICAN_HOUSE_EDGE_BP, 526);
    assert_eq!(ROULETTE_CONTRACT_TABLES_TREE, "roulette_tables");
    assert_eq!(ROULETTE_CONTRACT_BETS_TREE, "roulette_bets");
    assert_eq!(ROULETTE_CONTRACT_NULLIFIERS_TREE, "roulette_nullifiers");
    assert_eq!(ROULETTE_CONTRACT_BETS_HISTORY_TREE, "roulette_history");
}

#[test]
fn test_initialize_params_encoding() {
    let house_pub = make_pubkey(1);
    let params = InitializeParamsV1 {
        house_pub,
        american_wheel: false,
        house_capital: 1000000,
        max_straight_bet: 10000,
        duration_blocks: 10,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: InitializeParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.house_pub, params.house_pub);
    assert_eq!(decoded.american_wheel, params.american_wheel);
    assert_eq!(decoded.house_capital, params.house_capital);
    assert_eq!(decoded.max_straight_bet, params.max_straight_bet);
    assert_eq!(decoded.duration_blocks, params.duration_blocks);
}

#[test]
fn test_initialize_update_encoding() {
    let update = InitializeUpdateV1 {
        table_id: pallas::Base::from(1),
        house_pub: make_pubkey(2),
        wheel_size: 37,
        house_edge_bp: 270,
        house_capital: 1000000,
        max_straight_bet: 10000,
        bets_close_block: 100,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&update);
    let decoded: InitializeUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, update.table_id);
    assert_eq!(decoded.wheel_size, update.wheel_size);
    assert_eq!(decoded.house_edge_bp, update.house_edge_bp);
    assert_eq!(decoded.house_capital, update.house_capital);
}

#[test]
fn test_place_bet_params_encoding() {
    let params = PlaceBetParamsV1 {
        table_id: pallas::Base::from(1),
        player_pub: make_pubkey(2),
        bet_type: BetType::Straight,
        numbers: vec![7, 14, 21],
        amount: 1000,
        signature: pallas::Base::from(42),
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: PlaceBetParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, params.table_id);
    assert_eq!(decoded.bet_type, params.bet_type);
    assert_eq!(decoded.numbers, params.numbers);
    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_place_bet_update_encoding() {
    let update = PlaceBetUpdateV1 {
        bet_id: pallas::Base::from(1),
        table_id: pallas::Base::from(2),
        player_pub: make_pubkey(3),
        bet_type: BetType::Dozen,
        numbers: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        amount: 500,
        payout: 1000,
        spin_number: 5,
        nullifier: pallas::Base::from(99),
        table_house_capital: 500000,
        total_bets: 10000,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&update);
    let decoded: PlaceBetUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, update.bet_id);
    assert_eq!(decoded.table_id, update.table_id);
    assert_eq!(decoded.bet_type, update.bet_type);
    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.payout, update.payout);
}

#[test]
fn test_spin_wheel_params_encoding() {
    let params = SpinWheelParamsV1 {
        table_id: pallas::Base::from(1),
        nonce: pallas::Base::from(42),
        house_pub: make_pubkey(2),
        signature: make_signature(),
    };

    let encoded = serialize(&params);
    let decoded: SpinWheelParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, params.table_id);
    assert_eq!(decoded.nonce, params.nonce);
}

#[test]
fn test_spin_wheel_update_encoding() {
    let update = SpinWheelUpdateV1 {
        table_id: pallas::Base::from(1),
        winning_number: 17,
        spin_number: 5,
        spun_at_block: 100,
    };

    let encoded = serialize(&update);
    let decoded: SpinWheelUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, update.table_id);
    assert_eq!(decoded.winning_number, update.winning_number);
    assert_eq!(decoded.spin_number, update.spin_number);
}

#[test]
fn test_settle_bets_params_encoding() {
    let params = SettleBetsParamsV1 {
        table_id: pallas::Base::from(1),
        bet_ids: vec![pallas::Base::from(10), pallas::Base::from(20), pallas::Base::from(30)],
        payout: 0,
    };

    let encoded = serialize(&params);
    let decoded: SettleBetsParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, params.table_id);
    assert_eq!(decoded.bet_ids.len(), 3);
}

#[test]
fn test_settle_bets_update_encoding() {
    let update = SettleBetsUpdateV1 {
        table_id: pallas::Base::from(1),
        winning_number: 7,
        settled_count: 10,
        house_payout: 5000,
        house_new_capital: 995000,
        state: RouletteTableState::Settled,
    };

    let encoded = serialize(&update);
    let decoded: SettleBetsUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, update.table_id);
    assert_eq!(decoded.winning_number, update.winning_number);
    assert_eq!(decoded.settled_count, update.settled_count);
    assert_eq!(decoded.state, update.state);
}

#[test]
fn test_house_close_params_encoding() {
    let params = HouseCloseParamsV1 {
        table_id: pallas::Base::from(1),
        house_pub: make_pubkey(2),
        signature: make_signature(),
    };

    let encoded = serialize(&params);
    let decoded: HouseCloseParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, params.table_id);
}

#[test]
fn test_house_close_update_encoding() {
    let update = HouseCloseUpdateV1 {
        table_id: pallas::Base::from(1),
        remaining_capital: 950000,
    };

    let encoded = serialize(&update);
    let decoded: HouseCloseUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.table_id, update.table_id);
    assert_eq!(decoded.remaining_capital, update.remaining_capital);
}

#[test]
fn test_bet_type_house_edge() {
    // European wheel (37 numbers)
    assert_eq!(BetType::Straight.house_edge_bp(37), EUROPEAN_HOUSE_EDGE_BP);
    assert_eq!(BetType::EvenMoney.house_edge_bp(37), EUROPEAN_HOUSE_EDGE_BP);

    // American wheel (38 numbers)
    assert_eq!(BetType::Straight.house_edge_bp(38), AMERICAN_HOUSE_EDGE_BP);
    assert_eq!(BetType::EvenMoney.house_edge_bp(38), AMERICAN_HOUSE_EDGE_BP);
}
