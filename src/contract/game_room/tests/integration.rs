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

//! Game Room contract integration tests

use darkfi_serial::{deserialize, serialize};
use darkfi_sdk::{
    crypto::pasta_prelude::Group,
    pasta::pallas,
};
use darkfi_game_room_contract::{
    model::{
        Bet, BetId, BetType, ClosePotParamsV1, ClosePotUpdateV1, ContributeEntropyParamsV1,
        ContributeEntropyUpdateV1, ClaimParamsV1, ClaimUpdateV1, CreateRoomParamsV1,
        CreateRoomUpdateV1, DepositParamsV1, DepositUpdateV1, EntropyContribution, EntropyMode,
        FoldParamsV1, FoldUpdateV1, GameRoom, PlayerAccount, PlaceBetParamsV1, PlaceBetUpdateV1,
        Pot, PotContribution, PotId, PotState, RaiseParamsV1, RaiseUpdateV1, RoomConfig,
        RoomId, RoomState, WithdrawParamsV1, WithdrawUpdateV1, CallParamsV1, CallUpdateV1,
        SettlePotParamsV1, SettlePotUpdateV1,
    },
    GameRoomFunction,
    // Constants
    GAME_ROOM_ROOMS_TREE, GAME_ROOM_ACCOUNTS_TREE,
    GAME_ROOM_POTS_TREE, GAME_ROOM_BETS_TREE,
    GAME_ROOM_NULLIFIERS_TREE, GAME_ROOM_ENTROPY_TREE,
};

/// Helper to create a test PublicKey
fn make_pubkey(seed: u64) -> darkfi_sdk::crypto::PublicKey {
    use darkfi_sdk::crypto::{PublicKey, SecretKey};
    let secret = SecretKey::from(pallas::Base::from(seed));
    PublicKey::from_secret(secret)
}

/// Helper to create a test ContractId
fn make_contract_id(pubkey: &darkfi_sdk::crypto::PublicKey) -> darkfi_sdk::crypto::ContractId {
    darkfi_sdk::crypto::ContractId::derive_public(*pubkey)
}

#[test]
fn test_game_room_function_enum_valid() {
    assert!(GameRoomFunction::try_from(0x00).is_ok()); // CreateRoomV1
    assert!(GameRoomFunction::try_from(0x01).is_ok()); // DepositV1
    assert!(GameRoomFunction::try_from(0x02).is_ok()); // WithdrawV1
    assert!(GameRoomFunction::try_from(0x03).is_ok()); // PlaceBetV1
    assert!(GameRoomFunction::try_from(0x04).is_ok()); // RaiseV1
    assert!(GameRoomFunction::try_from(0x05).is_ok()); // CallV1
    assert!(GameRoomFunction::try_from(0x06).is_ok()); // FoldV1
    assert!(GameRoomFunction::try_from(0x07).is_ok()); // ClosePotV1
    assert!(GameRoomFunction::try_from(0x08).is_ok()); // SettlePotV1
    assert!(GameRoomFunction::try_from(0x09).is_ok()); // ContributeEntropyV1
    assert!(GameRoomFunction::try_from(0x0A).is_ok()); // ClaimV1
}

#[test]
fn test_game_room_function_enum_invalid() {
    assert!(GameRoomFunction::try_from(0xFF).is_err());
    assert!(GameRoomFunction::try_from(0x0B).is_err());
    assert!(GameRoomFunction::try_from(0x10).is_err());
}

#[test]
fn test_room_state_values() {
    assert_eq!(RoomState::Open as u8, 0);
    assert_eq!(RoomState::Active as u8, 1);
    assert_eq!(RoomState::Concluded as u8, 2);
}

#[test]
fn test_room_state_try_from() {
    assert_eq!(RoomState::try_from(0).ok(), Some(RoomState::Open));
    assert_eq!(RoomState::try_from(1).ok(), Some(RoomState::Active));
    assert_eq!(RoomState::try_from(2).ok(), Some(RoomState::Concluded));
    assert!(RoomState::try_from(3).is_err());
    assert!(RoomState::try_from(255).is_err());
}

#[test]
fn test_pot_state_values() {
    assert_eq!(PotState::Open as u8, 0);
    assert_eq!(PotState::Closed as u8, 1);
    assert_eq!(PotState::Settled as u8, 2);
}

#[test]
fn test_pot_state_try_from() {
    assert_eq!(PotState::try_from(0).ok(), Some(PotState::Open));
    assert_eq!(PotState::try_from(1).ok(), Some(PotState::Closed));
    assert_eq!(PotState::try_from(2).ok(), Some(PotState::Settled));
    assert!(PotState::try_from(3).is_err());
    assert!(PotState::try_from(255).is_err());
}

#[test]
fn test_bet_type_values() {
    assert_eq!(BetType::Ante as u8, 0);
    assert_eq!(BetType::Blind as u8, 1);
    assert_eq!(BetType::Bet as u8, 2);
    assert_eq!(BetType::Raise as u8, 3);
    assert_eq!(BetType::Call as u8, 4);
    assert_eq!(BetType::AllIn as u8, 5);
    assert_eq!(BetType::Fold as u8, 6);
}

#[test]
fn test_bet_type_try_from() {
    assert_eq!(BetType::try_from(0).ok(), Some(BetType::Ante));
    assert_eq!(BetType::try_from(1).ok(), Some(BetType::Blind));
    assert_eq!(BetType::try_from(2).ok(), Some(BetType::Bet));
    assert_eq!(BetType::try_from(3).ok(), Some(BetType::Raise));
    assert_eq!(BetType::try_from(4).ok(), Some(BetType::Call));
    assert_eq!(BetType::try_from(5).ok(), Some(BetType::AllIn));
    assert_eq!(BetType::try_from(6).ok(), Some(BetType::Fold));
    assert!(BetType::try_from(7).is_err());
    assert!(BetType::try_from(255).is_err());
}

#[test]
fn test_entropy_mode_values() {
    assert_eq!(EntropyMode::BlockHash as u8, 0);
    assert_eq!(EntropyMode::TrustedSetup as u8, 1);
}

#[test]
fn test_entropy_mode_try_from() {
    assert_eq!(EntropyMode::try_from(0).ok(), Some(EntropyMode::BlockHash));
    assert_eq!(EntropyMode::try_from(1).ok(), Some(EntropyMode::TrustedSetup));
    assert!(EntropyMode::try_from(2).is_err());
    assert!(EntropyMode::try_from(255).is_err());
}

#[test]
fn test_constants() {
    assert_eq!(GAME_ROOM_ROOMS_TREE, "game_room_rooms");
    assert_eq!(GAME_ROOM_ACCOUNTS_TREE, "game_room_accounts");
    assert_eq!(GAME_ROOM_POTS_TREE, "game_room_pots");
    assert_eq!(GAME_ROOM_BETS_TREE, "game_room_bets");
    assert_eq!(GAME_ROOM_NULLIFIERS_TREE, "game_room_nullifiers");
    assert_eq!(GAME_ROOM_ENTROPY_TREE, "game_room_entropy");
}

#[test]
fn test_create_room_params_encoding() {
    let owner = make_pubkey(1);
    let params = CreateRoomParamsV1 {
        owner,
        token_id: pallas::Base::from(1),
        min_stake: 100,
        max_stake: 10000,
        entropy_mode: EntropyMode::BlockHash,
        confirmation_depth: 3,
        required_entropy_contributions: 2,
        entropy_contribution_deadline: 50,
        max_players: 6,
        nonce: pallas::Base::from(42),
    };

    let encoded = serialize(&params);
    let decoded: CreateRoomParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.owner, params.owner);
    assert_eq!(decoded.token_id, params.token_id);
    assert_eq!(decoded.min_stake, params.min_stake);
    assert_eq!(decoded.max_stake, params.max_stake);
    assert_eq!(decoded.entropy_mode, params.entropy_mode);
    assert_eq!(decoded.confirmation_depth, params.confirmation_depth);
    assert_eq!(decoded.required_entropy_contributions, params.required_entropy_contributions);
    assert_eq!(decoded.max_players, params.max_players);
}

#[test]
fn test_create_room_update_encoding() {
    let owner = make_pubkey(1);
    let owner_dao = make_contract_id(&owner);
    let config = RoomConfig {
        owner_dao,
        token_id: pallas::Base::from(1),
        min_stake: 100,
        max_stake: 10000,
        entropy_mode: EntropyMode::BlockHash,
        confirmation_depth: 3,
        required_entropy_contributions: 2,
        entropy_contribution_deadline: 50,
        max_players: 6,
    };

    let update = CreateRoomUpdateV1 {
        room_id: pallas::Base::from(1),
        owner_dao,
        config,
    };

    let encoded = serialize(&update);
    let decoded: CreateRoomUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.owner_dao, update.owner_dao);
    assert_eq!(decoded.config.min_stake, update.config.min_stake);
}

#[test]
fn test_deposit_params_encoding() {
    let params = DepositParamsV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        amount: 1000,
    };

    let encoded = serialize(&params);
    let decoded: DepositParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.player, params.player);
    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_deposit_update_encoding() {
    let update = DepositUpdateV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        new_balance: 2000,
    };

    let encoded = serialize(&update);
    let decoded: DepositUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.player, update.player);
    assert_eq!(decoded.new_balance, update.new_balance);
}

#[test]
fn test_withdraw_params_encoding() {
    let params = WithdrawParamsV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        amount: 500,
    };

    let encoded = serialize(&params);
    let decoded: WithdrawParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.player, params.player);
    assert_eq!(decoded.amount, params.amount);
}

#[test]
fn test_withdraw_update_encoding() {
    let update = WithdrawUpdateV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        new_balance: 1500,
    };

    let encoded = serialize(&update);
    let decoded: WithdrawUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.player, update.player);
    assert_eq!(decoded.new_balance, update.new_balance);
}

#[test]
fn test_place_bet_params_encoding() {
    let params = PlaceBetParamsV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        amount: 100,
        bet_type: BetType::Bet,
        nonce: pallas::Base::from(42),
    };

    let encoded = serialize(&params);
    let decoded: PlaceBetParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.player, params.player);
    assert_eq!(decoded.amount, params.amount);
    assert_eq!(decoded.bet_type, params.bet_type);
    assert_eq!(decoded.nonce, params.nonce);
}

#[test]
fn test_place_bet_update_encoding() {
    let update = PlaceBetUpdateV1 {
        room_id: pallas::Base::from(1),
        pot_id: pallas::Base::from(2),
        player: make_pubkey(3),
        bet_id: pallas::Base::from(4),
        new_balance: 900,
        new_locked: 100,
        new_pot_total: 200,
        new_current_bet: 100,
        new_current_better: make_pubkey(3),
    };

    let encoded = serialize(&update);
    let decoded: PlaceBetUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.pot_id, update.pot_id);
    assert_eq!(decoded.bet_id, update.bet_id);
    assert_eq!(decoded.new_balance, update.new_balance);
    assert_eq!(decoded.new_pot_total, update.new_pot_total);
}

#[test]
fn test_raise_params_encoding() {
    let params = RaiseParamsV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        amount: 200,
        nonce: pallas::Base::from(42),
    };

    let encoded = serialize(&params);
    let decoded: RaiseParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.player, params.player);
    assert_eq!(decoded.amount, params.amount);
    assert_eq!(decoded.nonce, params.nonce);
}

#[test]
fn test_raise_update_encoding() {
    let update = RaiseUpdateV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        new_balance: 800,
        new_locked: 300,
        new_pot_total: 400,
        new_current_bet: 300,
    };

    let encoded = serialize(&update);
    let decoded: RaiseUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.player, update.player);
    assert_eq!(decoded.new_balance, update.new_balance);
    assert_eq!(decoded.new_pot_total, update.new_pot_total);
}

#[test]
fn test_call_params_encoding() {
    let params = CallParamsV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        nonce: pallas::Base::from(42),
    };

    let encoded = serialize(&params);
    let decoded: CallParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.player, params.player);
    assert_eq!(decoded.nonce, params.nonce);
}

#[test]
fn test_call_update_encoding() {
    let update = CallUpdateV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        new_balance: 700,
        new_locked: 300,
        new_pot_total: 500,
    };

    let encoded = serialize(&update);
    let decoded: CallUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.player, update.player);
    assert_eq!(decoded.new_balance, update.new_balance);
    assert_eq!(decoded.new_pot_total, update.new_pot_total);
}

#[test]
fn test_fold_params_encoding() {
    let params = FoldParamsV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
    };

    let encoded = serialize(&params);
    let decoded: FoldParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.player, params.player);
}

#[test]
fn test_fold_update_encoding() {
    let update = FoldUpdateV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        has_folded: true,
    };

    let encoded = serialize(&update);
    let decoded: FoldUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.player, update.player);
    assert_eq!(decoded.has_folded, update.has_folded);
}

#[test]
fn test_close_pot_params_encoding() {
    let params = ClosePotParamsV1 {
        room_id: pallas::Base::from(1),
        pot_id: pallas::Base::from(2),
    };

    let encoded = serialize(&params);
    let decoded: ClosePotParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.pot_id, params.pot_id);
}

#[test]
fn test_close_pot_update_encoding() {
    let update = ClosePotUpdateV1 {
        room_id: pallas::Base::from(1),
        pot_id: pallas::Base::from(2),
        new_pot_state: PotState::Closed,
        new_betting_round: 1,
        new_current_bet: 0,
        new_current_better: None,
    };

    let encoded = serialize(&update);
    let decoded: ClosePotUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.pot_id, update.pot_id);
    assert_eq!(decoded.new_pot_state, update.new_pot_state);
    assert_eq!(decoded.new_betting_round, update.new_betting_round);
}

#[test]
fn test_settle_pot_params_encoding() {
    let params = SettlePotParamsV1 {
        caller: make_pubkey(1),
        room_id: pallas::Base::from(2),
        pot_id: pallas::Base::from(3),
        winners: vec![(make_pubkey(4), 1000), (make_pubkey(5), 500)],
        signature: vec![1, 2, 3, 4, 5],
    };

    let encoded = serialize(&params);
    let decoded: SettlePotParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.caller, params.caller);
    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.pot_id, params.pot_id);
    assert_eq!(decoded.winners.len(), 2);
}

#[test]
fn test_settle_pot_update_encoding() {
    let update = SettlePotUpdateV1 {
        room_id: pallas::Base::from(1),
        pot_id: pallas::Base::from(2),
        new_pot_state: PotState::Settled,
        winners: vec![make_pubkey(3), make_pubkey(4)],
        payouts: vec![1000, 500],
    };

    let encoded = serialize(&update);
    let decoded: SettlePotUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.pot_id, update.pot_id);
    assert_eq!(decoded.new_pot_state, update.new_pot_state);
    assert_eq!(decoded.winners.len(), 2);
    assert_eq!(decoded.payouts.len(), 2);
}

#[test]
fn test_contribute_entropy_params_encoding() {
    let params = ContributeEntropyParamsV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        commitment: pallas::Base::from(42),
        reveal: Some(pallas::Base::from(99)),
    };

    let encoded = serialize(&params);
    let decoded: ContributeEntropyParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.player, params.player);
    assert_eq!(decoded.commitment, params.commitment);
    assert_eq!(decoded.reveal, params.reveal);
}

#[test]
fn test_contribute_entropy_update_encoding() {
    let update = ContributeEntropyUpdateV1 {
        room_id: pallas::Base::from(1),
        player: make_pubkey(2),
        combined_entropy: Some(pallas::Base::from(999)),
        contributions_count: 2,
    };

    let encoded = serialize(&update);
    let decoded: ContributeEntropyUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.player, update.player);
    assert_eq!(decoded.contributions_count, update.contributions_count);
}

#[test]
fn test_claim_params_encoding() {
    let params = ClaimParamsV1 {
        room_id: pallas::Base::from(1),
        pot_id: pallas::Base::from(2),
        winner: make_pubkey(3),
        payout_amount: 1000,
        proof: vec![1, 2, 3, 4, 5],
    };

    let encoded = serialize(&params);
    let decoded: ClaimParamsV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, params.room_id);
    assert_eq!(decoded.pot_id, params.pot_id);
    assert_eq!(decoded.winner, params.winner);
    assert_eq!(decoded.payout_amount, params.payout_amount);
}

#[test]
fn test_claim_update_encoding() {
    let update = ClaimUpdateV1 {
        room_id: pallas::Base::from(1),
        pot_id: pallas::Base::from(2),
        winner: make_pubkey(3),
        amount: 1000,
        new_balance: 2000,
    };

    let encoded = serialize(&update);
    let decoded: ClaimUpdateV1 = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, update.room_id);
    assert_eq!(decoded.pot_id, update.pot_id);
    assert_eq!(decoded.winner, update.winner);
    assert_eq!(decoded.amount, update.amount);
    assert_eq!(decoded.new_balance, update.new_balance);
}

#[test]
fn test_room_config_encoding() {
    let owner = make_pubkey(1);
    let owner_dao = make_contract_id(&owner);

    let config = RoomConfig {
        owner_dao,
        token_id: pallas::Base::from(1),
        min_stake: 100,
        max_stake: 10000,
        entropy_mode: EntropyMode::BlockHash,
        confirmation_depth: 3,
        required_entropy_contributions: 2,
        entropy_contribution_deadline: 50,
        max_players: 6,
    };

    let encoded = serialize(&config);
    let decoded: RoomConfig = deserialize(&encoded).unwrap();

    assert_eq!(decoded.owner_dao, config.owner_dao);
    assert_eq!(decoded.token_id, config.token_id);
    assert_eq!(decoded.min_stake, config.min_stake);
    assert_eq!(decoded.max_stake, config.max_stake);
    assert_eq!(decoded.entropy_mode, config.entropy_mode);
}

#[test]
fn test_game_room_encoding() {
    let owner = make_pubkey(1);
    let owner_dao = make_contract_id(&owner);

    let config = RoomConfig {
        owner_dao,
        token_id: pallas::Base::from(1),
        min_stake: 100,
        max_stake: 10000,
        entropy_mode: EntropyMode::BlockHash,
        confirmation_depth: 3,
        required_entropy_contributions: 2,
        entropy_contribution_deadline: 50,
        max_players: 6,
    };

    let room = GameRoom {
        room_id: pallas::Base::from(1),
        config,
        state: RoomState::Open,
        current_pot_id: None,
        current_bet_amount: 0,
        current_better: None,
        total_entropy_contributions: 0,
        combined_entropy: None,
        created_at: 100,
        entropy_deadline: 150,
    };

    let encoded = serialize(&room);
    let decoded: GameRoom = deserialize(&encoded).unwrap();

    assert_eq!(decoded.room_id, room.room_id);
    assert_eq!(decoded.state, room.state);
    assert_eq!(decoded.created_at, room.created_at);
    assert_eq!(decoded.entropy_deadline, room.entropy_deadline);
}

#[test]
fn test_player_account_encoding() {
    let player = make_pubkey(2);

    let account = PlayerAccount {
        pubkey: player,
        balance: 1000,
        locked: 100,
        last_action_block: 50,
        has_folded: false,
        entropy_contribution: None,
    };

    let encoded = serialize(&account);
    let decoded: PlayerAccount = deserialize(&encoded).unwrap();

    assert_eq!(decoded.pubkey, account.pubkey);
    assert_eq!(decoded.balance, account.balance);
    assert_eq!(decoded.locked, account.locked);
    assert_eq!(decoded.has_folded, account.has_folded);
}

#[test]
fn test_player_account_available_balance() {
    let player = make_pubkey(2);

    let account = PlayerAccount {
        pubkey: player,
        balance: 1000,
        locked: 100,
        last_action_block: 50,
        has_folded: false,
        entropy_contribution: None,
    };

    assert_eq!(account.available_balance(), 900);
}

#[test]
fn test_entropy_contribution_encoding() {
    let contrib = EntropyContribution {
        commitment: pallas::Base::from(42),
        revealed_nonce: Some(pallas::Base::from(99)),
        contributed_at: 100,
    };

    let encoded = serialize(&contrib);
    let decoded: EntropyContribution = deserialize(&encoded).unwrap();

    assert_eq!(decoded.commitment, contrib.commitment);
    assert_eq!(decoded.revealed_nonce, contrib.revealed_nonce);
    assert_eq!(decoded.contributed_at, contrib.contributed_at);
}

#[test]
fn test_pot_encoding() {
    let pot = Pot {
        pot_id: pallas::Base::from(1),
        room_id: pallas::Base::from(2),
        total: 1000,
        contributions: vec![
            PotContribution {
                player: make_pubkey(3),
                amount: 500,
                bet_type: BetType::Bet,
                block: 50,
            },
            PotContribution {
                player: make_pubkey(4),
                amount: 500,
                bet_type: BetType::Call,
                block: 55,
            },
        ],
        state: PotState::Open,
        betting_round: 0,
        created_at: 50,
    };

    let encoded = serialize(&pot);
    let decoded: Pot = deserialize(&encoded).unwrap();

    assert_eq!(decoded.pot_id, pot.pot_id);
    assert_eq!(decoded.room_id, pot.room_id);
    assert_eq!(decoded.total, pot.total);
    assert_eq!(decoded.contributions.len(), 2);
    assert_eq!(decoded.state, pot.state);
}

#[test]
fn test_bet_encoding() {
    let bet = Bet {
        bet_id: pallas::Base::from(1),
        room_id: pallas::Base::from(2),
        pot_id: pallas::Base::from(3),
        player: make_pubkey(4),
        amount: 100,
        bet_type: BetType::Bet,
        round: 0,
        commitment: pallas::Base::from(42),
        block: 50,
    };

    let encoded = serialize(&bet);
    let decoded: Bet = deserialize(&encoded).unwrap();

    assert_eq!(decoded.bet_id, bet.bet_id);
    assert_eq!(decoded.room_id, bet.room_id);
    assert_eq!(decoded.pot_id, bet.pot_id);
    assert_eq!(decoded.player, bet.player);
    assert_eq!(decoded.amount, bet.amount);
    assert_eq!(decoded.bet_type, bet.bet_type);
}

#[test]
fn test_game_room_derive_room_id() {
    let owner = make_pubkey(1);
    let owner_dao = make_contract_id(&owner);
    let token_id = pallas::Base::from(1);
    let block_height = 100u64;
    let nonce = pallas::Base::from(42);

    let room_id: RoomId = GameRoom::derive_room_id(&owner_dao, token_id, block_height, nonce);

    // Room ID should be non-zero
    assert!(room_id != pallas::Base::zero());
}

#[test]
fn test_bet_new_commitment() {
    let bet_id = pallas::Base::from(1);
    let room_id = pallas::Base::from(2);
    let pot_id = pallas::Base::from(3);
    let player = make_pubkey(4);
    let amount = 100u64;
    let bet_type = BetType::Bet;
    let round = 0u8;
    let nonce = pallas::Base::from(42);
    let block = 50u64;

    let bet = Bet::new(bet_id, room_id, pot_id, player, amount, bet_type, round, nonce, block);

    // Commitment should be non-zero
    assert!(bet.commitment != pallas::Base::zero());
}