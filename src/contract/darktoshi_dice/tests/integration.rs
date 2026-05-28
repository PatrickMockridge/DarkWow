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

//! DarkToshi Dice contract integration tests

use dwow_serial::{deserialize, serialize};
use dwow_sdk::{
    crypto::pasta_prelude::Group,
    pasta::pallas,
};
use dwow_darktoshi_dice_contract::{
    model::{
        Bet, BetId, BetState, CommitBetParamsV1, CommitBetUpdateV1, HouseCloseParamsV1,
        HouseCloseUpdateV1, RevealRollParamsV1, RevealRollUpdateV1, SettleBetParamsV1,
        SettleBetUpdateV1,
    },
    DiceFunction,
    // Constants
    DICE_CONTRACT_BETS_TREE, DICE_CONTRACT_NULLIFIERS_TREE,
    DICE_CONTRACT_INFO_TREE, DICE_CONTRACT_HOUSE_TREE,
    DEFAULT_HOUSE_EDGE, MIN_HOUSE_EDGE, MAX_HOUSE_EDGE,
    DEFAULT_ROLL_TIMEOUT, MAX_TARGET, ROLL_RANGE,
};

/// Helper to create a test PublicKey
fn make_pubkey(seed: u64) -> dwow_sdk::crypto::PublicKey {
    use dwow_sdk::crypto::{PublicKey, SecretKey};
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

#[test]
fn test_dice_function_enum_valid() {
    assert!(DiceFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(DiceFunction::try_from(0x01).is_ok()); // CommitBetV1
    assert!(DiceFunction::try_from(0x02).is_ok()); // RevealRollV1
    assert!(DiceFunction::try_from(0x03).is_ok()); // SettleBetV1
    assert!(DiceFunction::try_from(0x04).is_ok()); // HouseCloseV1
}

#[test]
fn test_dice_function_enum_invalid() {
    assert!(DiceFunction::try_from(0xFF).is_err());
    assert!(DiceFunction::try_from(0x05).is_err());
    assert!(DiceFunction::try_from(0x10).is_err());
}

#[test]
fn test_bet_state_values() {
    assert_eq!(BetState::Committed as u8, 0);
    assert_eq!(BetState::Revealed as u8, 1);
    assert_eq!(BetState::SettledPlayer as u8, 2);
    assert_eq!(BetState::SettledHouse as u8, 3);
    assert_eq!(BetState::Cancelled as u8, 4);
}

#[test]
fn test_bet_state_try_from() {
    assert_eq!(BetState::try_from(0).ok(), Some(BetState::Committed));
    assert_eq!(BetState::try_from(1).ok(), Some(BetState::Revealed));
    assert_eq!(BetState::try_from(2).ok(), Some(BetState::SettledPlayer));
    assert_eq!(BetState::try_from(3).ok(), Some(BetState::SettledHouse));
    assert_eq!(BetState::try_from(4).ok(), Some(BetState::Cancelled));
    assert!(BetState::try_from(5).is_err());
    assert!(BetState::try_from(255).is_err());
}

#[test]
fn test_constants() {
    assert_eq!(DICE_CONTRACT_BETS_TREE, "bets");
    assert_eq!(DICE_CONTRACT_NULLIFIERS_TREE, "nullifiers");
    assert_eq!(DICE_CONTRACT_INFO_TREE, "info");
    assert_eq!(DICE_CONTRACT_HOUSE_TREE, "house");
    assert_eq!(DEFAULT_HOUSE_EDGE, 200);
    assert_eq!(MIN_HOUSE_EDGE, 100);
    assert_eq!(MAX_HOUSE_EDGE, 500);
    assert_eq!(DEFAULT_ROLL_TIMEOUT, 10);
    assert_eq!(MAX_TARGET, 99);
    assert_eq!(ROLL_RANGE, 100);
}

#[test]
fn test_commit_bet_params_encoding() {
    let params = CommitBetParamsV1 {
        player_pub: make_pubkey(1),
        bet_value: 1000,
        target: 50,
        secret_nonce: pallas::Base::from(42),
        blind: pallas::Base::from(99),
        token_id: pallas::Base::from(1),
        value_commit: pallas::Point::identity(),
        signature: pallas::Base::from(12345),
        house_edge: 200,
        confirmation_depth: 3,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&params);
    let decoded: CommitBetParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.player_pub, params.player_pub);
    assert_eq!(decoded.bet_value, params.bet_value);
    assert_eq!(decoded.target, params.target);
    assert_eq!(decoded.house_edge, params.house_edge);
    assert_eq!(decoded.confirmation_depth, params.confirmation_depth);
}

#[test]
fn test_commit_bet_update_encoding() {
    let update = CommitBetUpdateV1 {
        bet_id: pallas::Base::from(1),
        player_pub: make_pubkey(2),
        bet_value: 1000,
        target: 50,
        secret_nonce: pallas::Base::from(42),
        blind: pallas::Base::from(99),
        value_commit: pallas::Point::identity(),
        token_id: pallas::Base::from(1),
        house_edge: 200,
        confirmation_depth: 3,
        settle_block: 100,
        nullifier: pallas::Base::from(50),
        created_at: 50,
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&update);
    let decoded: CommitBetUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, update.bet_id);
    assert_eq!(decoded.bet_value, update.bet_value);
    assert_eq!(decoded.target, update.target);
    assert_eq!(decoded.settle_block, update.settle_block);
}

#[test]
fn test_reveal_roll_params_encoding() {
    let params = RevealRollParamsV1 {
        bet_id: pallas::Base::from(1),
        secret_nonce: pallas::Base::from(42),
    };

    let encoded = serialize(&params);
    let decoded: RevealRollParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, params.bet_id);
    assert_eq!(decoded.secret_nonce, params.secret_nonce);
}

#[test]
fn test_reveal_roll_update_encoding() {
    let update = RevealRollUpdateV1 {
        bet_id: pallas::Base::from(1),
        roll: 42,
        state: BetState::Revealed,
        revealed_at: 100,
    };

    let encoded = serialize(&update);
    let decoded: RevealRollUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, update.bet_id);
    assert_eq!(decoded.roll, update.roll);
    assert_eq!(decoded.state, update.state);
    assert_eq!(decoded.revealed_at, update.revealed_at);
}

#[test]
fn test_settle_bet_params_encoding() {
    let params = SettleBetParamsV1 {
        bet_id: pallas::Base::from(1),
        proof: vec![1, 2, 3, 4, 5],
        roll_hash: pallas::Base::from(42),
    };

    let encoded = serialize(&params);
    let decoded: SettleBetParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, params.bet_id);
    assert_eq!(decoded.proof, params.proof);
}

#[test]
fn test_settle_bet_update_encoding() {
    let update = SettleBetUpdateV1 {
        bet_id: pallas::Base::from(1),
        state: BetState::SettledPlayer,
        payout: 1960,
    };

    let encoded = serialize(&update);
    let decoded: SettleBetUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, update.bet_id);
    assert_eq!(decoded.state, update.state);
    assert_eq!(decoded.payout, update.payout);
}

#[test]
fn test_house_close_params_encoding() {
    let params = HouseCloseParamsV1 {
        bet_id: pallas::Base::from(1),
        house_pub: make_pubkey(2),
        signature: dwow_sdk::crypto::schnorr::Signature::dummy(),
    };

    let encoded = serialize(&params);
    let decoded: HouseCloseParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, params.bet_id);
}

#[test]
fn test_house_close_update_encoding() {
    let update = HouseCloseUpdateV1 {
        bet_id: pallas::Base::from(1),
        state: BetState::Cancelled,
    };

    let encoded = serialize(&update);
    let decoded: HouseCloseUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, update.bet_id);
    assert_eq!(decoded.state, update.state);
}

#[test]
fn test_bet_struct_encoding() {
    let bet = Bet {
        id: pallas::Base::from(1),
        player_pub: make_pubkey(2),
        bet_value: 1000,
        target: 50,
        secret_nonce: pallas::Base::from(42),
        blind: pallas::Base::from(99),
        roll: Some(42),
        state: BetState::Revealed,
        house_edge: 200,
        confirmation_depth: 3,
        created_at: 50,
        revealed_at: 100,
        settle_block: 110,
        value_commit: pallas::Point::identity(),
        token_id: pallas::Base::from(1),
        nullifier: pallas::Base::from(50),
        instance_seed: [0u8; 32],
    };

    let encoded = serialize(&bet);
    let decoded: Bet = deserialize(&encoded).unwrap();

    assert_eq!(decoded.id, bet.id);
    assert_eq!(decoded.bet_value, bet.bet_value);
    assert_eq!(decoded.target, bet.target);
    assert_eq!(decoded.roll, bet.roll);
    assert_eq!(decoded.state, bet.state);
    assert_eq!(decoded.calculate_payout(), bet.calculate_payout());
}

#[test]
fn test_bet_calculate_payout() {
    let bet = Bet {
        id: pallas::Base::from(1),
        player_pub: make_pubkey(2),
        bet_value: 1000,
        target: 50,
        secret_nonce: pallas::Base::from(42),
        blind: pallas::Base::from(99),
        roll: None,
        state: BetState::Committed,
        house_edge: 200, // 2%
        confirmation_depth: 3,
        created_at: 50,
        revealed_at: 0,
        settle_block: 110,
        value_commit: pallas::Point::identity(),
        token_id: pallas::Base::from(1),
        nullifier: pallas::Base::from(50),
        instance_seed: [0u8; 32],
    };

    // payout = bet_value * (10000 - house_edge) / (target * 100)
    // = 1000 * 9800 / 5000 = 1960
    assert_eq!(bet.calculate_payout(), Some(1960));
}

#[test]
fn test_bet_calculate_house_take() {
    let bet = Bet {
        id: pallas::Base::from(1),
        player_pub: make_pubkey(2),
        bet_value: 1000,
        target: 50,
        secret_nonce: pallas::Base::from(42),
        blind: pallas::Base::from(99),
        roll: None,
        state: BetState::Committed,
        house_edge: 200, // 2%
        confirmation_depth: 3,
        created_at: 50,
        revealed_at: 0,
        settle_block: 110,
        value_commit: pallas::Point::identity(),
        token_id: pallas::Base::from(1),
        nullifier: pallas::Base::from(50),
        instance_seed: [0u8; 32],
    };

    // house_take = (bet_value - base_win) + (base_win * house_edge / 10000)
    // where base_win = bet_value * 100 / target = 1000 * 100 / 50 = 2000
    // profit = saturating_sub(bet_value, base_win) = saturating_sub(1000, 2000) = 0
    // house_cut = base_win * house_edge / 10000 = 2000 * 200 / 10000 = 40
    // house_take = profit + house_cut = 0 + 40 = 40
    assert_eq!(bet.calculate_house_take(), Some(40));
}

#[test]
fn test_derive_bet_id() {
    use dwow_darktoshi_dice_contract::model::derive_bet_id;

    let player_pub = make_pubkey(1);
    let bet_value = 1000u64;
    let target = 50u8;
    let secret_nonce = pallas::Base::from(42);
    let blind = pallas::Base::from(99);
    let token_id = pallas::Base::from(1);

    let bet_id: BetId = derive_bet_id(&player_pub, bet_value, target, secret_nonce, blind, token_id);

    // BetId should be a valid pallas::Base (non-zero check)
    assert!(bet_id != pallas::Base::zero());
}

#[test]
fn test_derive_nullifier() {
    use dwow_darktoshi_dice_contract::model::derive_nullifier;

    let bet_id = pallas::Base::from(1);
    let secret_nonce = pallas::Base::from(42);

    let nullifier: BetId = derive_nullifier(bet_id, secret_nonce);

    // Nullifier should be a valid pallas::Base (non-zero check)
    assert!(nullifier != pallas::Base::zero());
}

#[test]
fn test_roll_range_is_100() {
    // Roll range is 0-99, so 100 possible outcomes
    assert_eq!(ROLL_RANGE, 100);
}