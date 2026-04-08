/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Slot contract integration tests

use darkfi_serial::{deserialize, serialize};
use darkfi_sdk::{crypto::pasta_prelude::Group, pasta::pallas};
use darkfi_slot_contract::{
    model::{
        CancelSpinParamsV1, CancelSpinUpdateV1, CommitSpinParamsV1, CommitSpinUpdateV1,
        GameConfig, Payline, Paytable, PaytableEntry, RevealSpinParamsV1, RevealSpinUpdateV1,
        SettleSpinParamsV1, SettleSpinUpdateV1, Spin, SpinResult, SpinState, Symbol,
    },
    SlotFunction,
    // Constants
    SLOT_CONTRACT_SPINS_TREE, SLOT_CONTRACT_NULLIFIERS_TREE,
    SLOT_CONTRACT_CONFIG_TREE, SLOT_CONTRACT_HOUSE_TREE,
    DEFAULT_HOUSE_EDGE, MIN_HOUSE_EDGE, MAX_HOUSE_EDGE,
    DEFAULT_SPIN_TIMEOUT, DEFAULT_CONFIRMATION_DEPTH,
    MAX_BET_VALUE, MIN_BET_VALUE,
    GAME_TYPE_CLASSIC, GAME_TYPE_VIDEO,
    BAR_PAYOUT_NUM, BAR_PAYOUT_DEN,
};

/// Helper to create a test PublicKey
fn make_pubkey(seed: u64) -> darkfi_sdk::crypto::PublicKey {
    use darkfi_sdk::crypto::{PublicKey, SecretKey};
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_slot_function_enum_valid() {
    assert!(SlotFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(SlotFunction::try_from(0x01).is_ok()); // CommitSpinV1
    assert!(SlotFunction::try_from(0x02).is_ok()); // RevealSpinV1
    assert!(SlotFunction::try_from(0x03).is_ok()); // SettleSpinV1
    assert!(SlotFunction::try_from(0x04).is_ok()); // CancelSpinV1
}

#[test]
fn test_slot_function_enum_invalid() {
    assert!(SlotFunction::try_from(0xFF).is_err());
    assert!(SlotFunction::try_from(0x05).is_err());
    assert!(SlotFunction::try_from(0x10).is_err());
}

#[test]
fn test_spin_state_values() {
    assert_eq!(SpinState::Committed as u8, 0);
    assert_eq!(SpinState::Revealed as u8, 1);
    assert_eq!(SpinState::Settled as u8, 2);
    assert_eq!(SpinState::Cancelled as u8, 3);
}

#[test]
fn test_symbol_values() {
    assert_eq!(Symbol::BLANK, Symbol(0));
    assert_eq!(Symbol::WILD, Symbol(10));
    assert_eq!(Symbol::SCATTER, Symbol(11));
}

#[test]
fn test_constants() {
    assert_eq!(SLOT_CONTRACT_SPINS_TREE, "spins");
    assert_eq!(SLOT_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(SLOT_CONTRACT_CONFIG_TREE, "config");
    assert_eq!(SLOT_CONTRACT_HOUSE_TREE, "house");
    assert_eq!(DEFAULT_HOUSE_EDGE, 500);
    assert_eq!(MIN_HOUSE_EDGE, 100);
    assert_eq!(MAX_HOUSE_EDGE, 1000);
    assert_eq!(DEFAULT_SPIN_TIMEOUT, 10);
    assert_eq!(DEFAULT_CONFIRMATION_DEPTH, 3);
    assert_eq!(MAX_BET_VALUE, 1_000_000_000);
    assert_eq!(MIN_BET_VALUE, 1);
    assert_eq!(GAME_TYPE_CLASSIC, 0);
    assert_eq!(GAME_TYPE_VIDEO, 1);
    assert_eq!(BAR_PAYOUT_NUM, 100);
    assert_eq!(BAR_PAYOUT_DEN, 1);
}

#[test]
fn test_commit_spin_params_encoding() {
    let params = CommitSpinParamsV1 {
        player_pub: make_pubkey(1),
        bet_value: 1000,
        paylines_played: 5,
        secret_nonce: pallas::Base::from(42),
        blind: pallas::Base::from(99),
        house_edge: 500,
        confirmation_depth: 3,
        token_id: pallas::Base::from(1),
        value_commit: pallas::Point::identity(),
    };

    let encoded = serialize(&params);
    let decoded: CommitSpinParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_value, params.bet_value);
    assert_eq!(decoded.paylines_played, params.paylines_played);
    assert_eq!(decoded.house_edge, params.house_edge);
}

#[test]
fn test_commit_spin_update_encoding() {
    let update = CommitSpinUpdateV1 {
        spin_id: pallas::Base::from(1),
        player_pub: make_pubkey(2),
        bet_value: 1000,
        paylines_played: 5,
        secret_nonce: pallas::Base::from(42),
        blind: pallas::Base::from(99),
        house_edge: 500,
        confirmation_depth: 3,
        token_id: pallas::Base::from(1),
        value_commit: pallas::Point::identity(),
        settle_block: 100,
        nullifier: pallas::Base::from(50),
        state: SpinState::Committed,
        created_at: 50,
    };

    let encoded = serialize(&update);
    let decoded: CommitSpinUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.spin_id, update.spin_id);
    assert_eq!(decoded.bet_value, update.bet_value);
    assert_eq!(decoded.state, update.state);
}

#[test]
fn test_reveal_spin_params_encoding() {
    let params = RevealSpinParamsV1 {
        spin_id: pallas::Base::from(1),
        secret_nonce: pallas::Base::from(42),
    };

    let encoded = serialize(&params);
    let decoded: RevealSpinParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.spin_id, params.spin_id);
    assert_eq!(decoded.secret_nonce, params.secret_nonce);
}

#[test]
fn test_reveal_spin_update_encoding() {
    let update = RevealSpinUpdateV1 {
        spin_id: pallas::Base::from(1),
        positions: vec![10, 20, 30],
        state: SpinState::Revealed,
    };

    let encoded = serialize(&update);
    let decoded: RevealSpinUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.spin_id, update.spin_id);
    assert_eq!(decoded.positions, vec![10, 20, 30]);
    assert_eq!(decoded.state, update.state);
}

#[test]
fn test_settle_spin_params_encoding() {
    let params = SettleSpinParamsV1 {
        spin_id: pallas::Base::from(1),
    };

    let encoded = serialize(&params);
    let decoded: SettleSpinParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.spin_id, params.spin_id);
}

#[test]
fn test_settle_spin_update_encoding() {
    let update = SettleSpinUpdateV1 {
        spin_id: pallas::Base::from(1),
        wins: vec![
            darkfi_slot_contract::model::Win {
                payline_id: 0,
                symbol: Symbol(1),
                count: 3,
                multiplier: 100,
            },
        ],
        payout: 1000,
        state: SpinState::Settled,
    };

    let encoded = serialize(&update);
    let decoded: SettleSpinUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.spin_id, update.spin_id);
    assert_eq!(decoded.payout, update.payout);
    assert_eq!(decoded.state, update.state);
}

#[test]
fn test_cancel_spin_params_encoding() {
    let params = CancelSpinParamsV1 {
        spin_id: pallas::Base::from(1),
    };

    let encoded = serialize(&params);
    let decoded: CancelSpinParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.spin_id, params.spin_id);
}

#[test]
fn test_cancel_spin_update_encoding() {
    let update = CancelSpinUpdateV1 {
        spin_id: pallas::Base::from(1),
        house_take: 50,
        state: SpinState::Cancelled,
    };

    let encoded = serialize(&update);
    let decoded: CancelSpinUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.spin_id, update.spin_id);
    assert_eq!(decoded.house_take, update.house_take);
    assert_eq!(decoded.state, update.state);
}

#[test]
fn test_spin_result_encoding() {
    let result = SpinResult {
        positions: vec![5, 10, 15, 20, 25],
    };

    let encoded = serialize(&result);
    let decoded: SpinResult = deserialize(&encoded).unwrap();

    assert_eq!(decoded.positions, vec![5, 10, 15, 20, 25]);
    assert_eq!(decoded.reel_count(), 5);
}

#[test]
fn test_payline_creation() {
    let payline = Payline::horizontal_middle(3);

    assert_eq!(payline.id, 0);
    assert_eq!(payline.rows, vec![1, 1, 1]);

    let top = Payline::horizontal_top(3);
    assert_eq!(top.rows, vec![0, 0, 0]);

    let bottom = Payline::horizontal_bottom(3);
    assert_eq!(bottom.rows, vec![2, 2, 2]);
}

#[test]
fn test_paytable_lookup() {
    let paytable = Paytable::new(vec![
        PaytableEntry { symbol: Symbol(1), count: 3, multiplier: 100 },
        PaytableEntry { symbol: Symbol(2), count: 3, multiplier: 50 },
        PaytableEntry { symbol: Symbol(2), count: 2, multiplier: 5 },
    ]);

    assert_eq!(paytable.lookup(Symbol(1), 3), Some(100));
    assert_eq!(paytable.lookup(Symbol(2), 3), Some(50));
    assert_eq!(paytable.lookup(Symbol(2), 2), Some(5));
    assert_eq!(paytable.lookup(Symbol(3), 3), None);
}
