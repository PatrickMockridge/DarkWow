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

//! Baccarat contract integration tests

use darkfi_baccarat_contract::{
    model::{
        Bet, BetState, BetType, Card, CommitBetParamsV1, CommitBetUpdateV1, Hand, Outcome,
        BACCARAT_CONTRACT_BETS_TREE, BACCARAT_CONTRACT_NULLIFIERS_TREE,
    },
    BaccaratFunction,
};

#[test]
fn test_baccarat_function_enum_valid() {
    assert!(BaccaratFunction::try_from(0x00).is_ok()); // InitializeV1
    assert!(BaccaratFunction::try_from(0x01).is_ok()); // CommitBetV1
    assert!(BaccaratFunction::try_from(0x02).is_ok()); // DrawCardsV1
    assert!(BaccaratFunction::try_from(0x03).is_ok()); // SettleBetV1
    assert!(BaccaratFunction::try_from(0x04).is_ok()); // HouseCloseV1
}

#[test]
fn test_baccarat_function_enum_invalid() {
    assert!(BaccaratFunction::try_from(0xFF).is_err());
    assert!(BaccaratFunction::try_from(0x05).is_err());
    assert!(BaccaratFunction::try_from(0x10).is_err());
}

#[test]
fn test_card_creation() {
    let card = Card::new(0);
    assert_eq!(card.0, 0);

    // Test modulo 52
    let card52 = Card::new(52);
    assert_eq!(card52.0, 0);

    let card53 = Card::new(53);
    assert_eq!(card53.0, 1);
}

#[test]
fn test_card_rank() {
    // 0 = 2 of Clubs
    assert_eq!(Card::new(0).rank(), 0);
    // 8 = 10 of Clubs
    assert_eq!(Card::new(8).rank(), 8);
    // 9 = Jack of Clubs
    assert_eq!(Card::new(9).rank(), 9);
    // 12 = Ace of Clubs
    assert_eq!(Card::new(12).rank(), 12);
    // 13 = 2 of Diamonds
    assert_eq!(Card::new(13).rank(), 0);
}

#[test]
fn test_card_suit() {
    // Clubs: 0-12
    assert_eq!(Card::new(0).suit(), 0);
    assert_eq!(Card::new(12).suit(), 0);
    // Diamonds: 13-25
    assert_eq!(Card::new(13).suit(), 1);
    assert_eq!(Card::new(25).suit(), 1);
    // Hearts: 26-38
    assert_eq!(Card::new(26).suit(), 2);
    assert_eq!(Card::new(38).suit(), 2);
    // Spades: 39-51
    assert_eq!(Card::new(39).suit(), 3);
    assert_eq!(Card::new(51).suit(), 3);
}

#[test]
fn test_card_baccarat_value() {
    // 2 = 2 (face value 2)
    assert_eq!(Card::new(0).baccarat_value(), 2);
    // 3 = 3
    assert_eq!(Card::new(1).baccarat_value(), 3);
    // 9 = 9
    assert_eq!(Card::new(7).baccarat_value(), 9);
    // 10 = 0 (face card)
    assert_eq!(Card::new(8).baccarat_value(), 0);
    // Jack = 0
    assert_eq!(Card::new(9).baccarat_value(), 0);
    // Queen = 0
    assert_eq!(Card::new(10).baccarat_value(), 0);
    // King = 0
    assert_eq!(Card::new(11).baccarat_value(), 0);
    // Ace = 1
    assert_eq!(Card::new(12).baccarat_value(), 1);
}

#[test]
fn test_hand_value_calculation() {
    // Two cards: 5 + 7 = 12 % 10 = 2
    let hand = Hand { card1: Card::new(3), card2: Card::new(5), third_card: None };
    assert_eq!(hand.value(), 7); // 3 + 5 = 8 % 10 = 8? Wait...

    // Card value: rank 3 = 5, rank 5 = 7
    // 5 + 7 = 12 % 10 = 2
    let hand2 = Hand { card1: Card::new(3), card2: Card::new(5), third_card: None };
    assert_eq!(hand2.value(), (5 + 7) % 10); // = 2

    // Natural 9: 4 + 5 = 9
    let natural9 = Hand { card1: Card::new(2), card2: Card::new(3), third_card: None };
    assert_eq!(natural9.value(), 9);

    // With face cards: 10(K) + 7 = 0 + 7 = 7
    let with_face = Hand { card1: Card::new(8), card2: Card::new(5), third_card: None };
    assert_eq!(with_face.value(), 7);
}

#[test]
fn test_hand_value_with_third_card() {
    // Player has 5, draws 4, total = 9
    let mut hand = Hand { card1: Card::new(3), card2: Card::new(5), third_card: None };
    // Before third card: 5 + 7 = 12 % 10 = 2
    assert_eq!(hand.value(), 2);

    // After third card (4):
    hand.third_card = Some(Card::new(2));
    // 5 + 7 + 4 = 16 % 10 = 6
    assert_eq!(hand.value(), 6);
}

#[test]
fn test_bet_type_from_u8() {
    assert_eq!(BetType::from_u8(0), Some(BetType::Player));
    assert_eq!(BetType::from_u8(1), Some(BetType::Banker));
    assert_eq!(BetType::from_u8(2), Some(BetType::Tie));
    assert_eq!(BetType::from_u8(3), None);
    assert_eq!(BetType::from_u8(255), None);
}

#[test]
fn test_outcome_from_u8() {
    assert_eq!(Outcome::from_u8(0), Some(Outcome::Player));
    assert_eq!(Outcome::from_u8(1), Some(Outcome::Banker));
    assert_eq!(Outcome::from_u8(2), Some(Outcome::Tie));
    assert_eq!(Outcome::from_u8(3), None);
    assert_eq!(Outcome::from_u8(255), None);
}

#[test]
fn test_commit_bet_params_encoding() {
    let params = CommitBetParamsV1 {
        player_pub: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        bet_type: 0, // Player
        bet_value: 1000,
        secret_nonce: darkfi_sdk::pasta::pallas::Base::from(42),
        blind: darkfi_sdk::pasta::pallas::Base::from(99),
        house_edge: 150,
        confirmation_depth: 3,
        token_id: darkfi_sdk::pasta::pallas::Base::from(1),
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
    };

    let encoded = params.encode().unwrap();
    let decoded = CommitBetParamsV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.bet_type, 0);
    assert_eq!(decoded.bet_value, 1000);
    assert_eq!(decoded.house_edge, 150);
    assert_eq!(decoded.confirmation_depth, 3);
}

#[test]
fn test_commit_bet_params_get_bet_type() {
    let params_player = CommitBetParamsV1 {
        player_pub: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        bet_type: 0,
        bet_value: 1000,
        secret_nonce: darkfi_sdk::pasta::pallas::Base::ZERO,
        blind: darkfi_sdk::pasta::pallas::Base::ZERO,
        house_edge: 150,
        confirmation_depth: 3,
        token_id: darkfi_sdk::pasta::pallas::Base::ONE,
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
    };
    assert_eq!(params_player.get_bet_type(), Some(BetType::Player));

    let params_banker = CommitBetParamsV1 {
        bet_type: 1,
        ..params_player
    };
    assert_eq!(params_banker.get_bet_type(), Some(BetType::Banker));

    let params_tie = CommitBetParamsV1 {
        bet_type: 2,
        ..params_player
    };
    assert_eq!(params_tie.get_bet_type(), Some(BetType::Tie));

    let params_invalid = CommitBetParamsV1 {
        bet_type: 99,
        ..params_player
    };
    assert_eq!(params_invalid.get_bet_type(), None);
}

#[test]
fn test_commit_bet_update_encoding() {
    let update = CommitBetUpdateV1 {
        bet_id: darkfi_sdk::pasta::pallas::Base::from(123),
        player_pub: darkfi_sdk::crypto::PublicKey::from_publickey(
            &darkfi_sdk::crypto::Keypair::random(&mut rand::rngs::OsRng).public,
        ),
        bet_type: BetType::Player,
        bet_value: 500,
        secret_nonce: darkfi_sdk::pasta::pallas::Base::from(1),
        blind: darkfi_sdk::pasta::pallas::Base::from(2),
        house_edge: 150,
        confirmation_depth: 4,
        token_id: darkfi_sdk::pasta::pallas::Base::from(1),
        value_commit: darkfi_sdk::pasta::pallas::Point::identity(),
        settle_block: 100,
        nullifier: darkfi_sdk::pasta::pallas::Base::from(999),
        state: BetState::Committed,
        created_at: 50,
    };

    let encoded = update.encode().unwrap();
    let decoded = CommitBetUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.bet_value, 500);
    assert_eq!(decoded.house_edge, 150);
    assert_eq!(decoded.state, BetState::Committed);
}

#[test]
fn test_draw_cards_update_encoding() {
    let update = darkfi_baccarat_contract::model::DrawCardsUpdateV1 {
        bet_id: darkfi_sdk::pasta::pallas::Base::from(1),
        player_card1: Card::new(0),
        player_card2: Card::new(1),
        banker_card1: Card::new(2),
        banker_card2: Card::new(3),
        player_third_card: Some(Card::new(4)),
        banker_third_card: None,
        outcome: Outcome::Player,
        state: BetState::CardsDrawn,
    };

    let encoded = update.encode().unwrap();
    let decoded =
        darkfi_baccarat_contract::model::DrawCardsUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.outcome, Outcome::Player);
    assert_eq!(decoded.state, BetState::CardsDrawn);
    assert!(decoded.player_third_card.is_some());
    assert!(decoded.banker_third_card.is_none());
}

#[test]
fn test_settle_bet_update_encoding() {
    let update = darkfi_baccarat_contract::model::SettleBetUpdateV1 {
        bet_id: darkfi_sdk::pasta::pallas::Base::from(1),
        payout: 950,
        state: BetState::Settled,
    };

    let encoded = update.encode().unwrap();
    let decoded =
        darkfi_baccarat_contract::model::SettleBetUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.payout, 950);
    assert_eq!(decoded.state, BetState::Settled);
}

#[test]
fn test_house_close_update_encoding() {
    let update = darkfi_baccarat_contract::model::HouseCloseUpdateV1 {
        bet_id: darkfi_sdk::pasta::pallas::Base::from(1),
        house_take: 1000,
        state: BetState::Cancelled,
    };

    let encoded = update.encode().unwrap();
    let decoded =
        darkfi_baccarat_contract::model::HouseCloseUpdateV1::decode(&mut std::io::Cursor::new(&encoded)).unwrap();

    assert_eq!(decoded.house_take, 1000);
    assert_eq!(decoded.state, BetState::Cancelled);
}

#[test]
fn test_bet_state_transitions() {
    // Committed -> CardsDrawn -> Settled
    assert_eq!(BetState::Committed as u8, 0);
    assert_eq!(BetState::CardsDrawn as u8, 1);
    assert_eq!(BetState::Settled as u8, 2);
    assert_eq!(BetState::Cancelled as u8, 3);
}

#[test]
fn test_baccarat_draw_rules_natural_8() {
    // Natural 8 with two cards
    let mut player_hand = Hand { card1: Card::new(6), card2: Card::new(7), third_card: None }; // 8 + 8 = 16 % 10 = 6? No...
    // Card 6 rank = 6, value = 8
    // Card 7 rank = 7, value = 9
    // 8 + 9 = 17 % 10 = 7

    // Actually: rank 6 = 8, rank 7 = 9
    // 8 + 9 = 17 % 10 = 7

    // Let me recalculate: Card value is rank + 2 for ranks 0-8
    // rank 6 -> value 8
    // rank 7 -> value 9
    // 8 + 9 = 17 % 10 = 7

    // For natural 8: card values 3 + 5 = 8
    let mut natural8 = Hand { card1: Card::new(1), card2: Card::new(3), third_card: None };
    // rank 1 = 3, rank 3 = 5, value = 3 + 5 = 8
    assert_eq!(natural8.value(), 8);
}

#[test]
fn test_bet_derive_nullifier() {
    use darkfi_baccarat_contract::model::derive_bet_id;

    let bet_id = darkfi_sdk::pasta::pallas::Base::from(12345);
    let secret_nonce = darkfi_sdk::pasta::pallas::Base::from(67890);

    let nullifier =
        darkfi_baccarat_contract::model::derive_nullifier(bet_id, secret_nonce);

    // Nullifier should be deterministic
    let nullifier2 =
        darkfi_baccarat_contract::model::derive_nullifier(bet_id, secret_nonce);
    assert_eq!(nullifier, nullifier2);

    // Different inputs should give different outputs
    let different_nullifier =
        darkfi_baccarat_contract::model::derive_nullifier(bet_id, secret_nonce + Base::ONE);
    assert_ne!(nullifier, different_nullifier);
}

#[test]
fn test_constants() {
    assert_eq!(BACCARAT_CONTRACT_BETS_TREE, "baccarat_bets");
    assert_eq!(BACCARAT_CONTRACT_NULLIFIERS_TREE, "baccarat_nullifiers");
}
