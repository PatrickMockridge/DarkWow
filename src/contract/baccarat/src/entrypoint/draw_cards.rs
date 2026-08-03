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

//! DrawCardsV1 Implementation
//!
//! Draws cards using PoW block hashes for entropy and applies Baccarat drawing rules.

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash},
    error::ContractError,
    msg,
    wasm,
};

use crate::error::BaccaratError;
use crate::model::{
    calculate_outcome, deal_cards_from_seed, Bet, BetState, DrawCardsParamsV1, DrawCardsUpdateV1,
};
use crate::BACCARAT_CONTRACT_BETS_TREE;

/// Process instruction for DrawCardsV1
pub fn baccarat_draw_cards_process_instruction_v1(
    cid: dwow_sdk::crypto::ContractId,
    call_idx: usize,
    calls: Vec<dwow_sdk::dark_tree::DarkLeaf<dwow_sdk::ContractCall>>,
) -> Result<Vec<u8>, ContractError> {
    let self_ = &calls[call_idx].data;
    let params = DrawCardsParamsV1::decode(&self_.data[1..])?;

    msg!("[baccarat::draw_cards] Drawing cards for bet_id: {:?}", params.bet_id);

    // Look up the bet
    let bets_db = wasm::db::db_lookup(cid, BACCARAT_CONTRACT_BETS_TREE)?;
    let bet_bytes = wasm::db::db_get(bets_db, &params.bet_id.to_repr())?
        .ok_or(BaccaratError::BetNotFound)?;

    let mut bet = Bet::decode(&bet_bytes)?;

    // Verify bet is in correct state
    if bet.state != BetState::Committed {
        return Err(BaccaratError::InvalidStateTransition.into())
    }

    // Verify ZK proof: prover knows secret_nonce matching stored commitment
    // The host-side ZK verification ensures H(secret_nonce) == secret_nonce_commit
    // We verify the commitment matches the bet's stored value
    let secret_nonce_commit = poseidon_hash([params.secret_nonce]);
    if secret_nonce_commit != bet.secret_nonce_commit {
        return Err(BaccaratError::CommitmentMismatch.into())
    }

    // Get current block height
    let current_block = wasm::util::get_verifying_block_height()?;

    // Verify settle block has been reached
    if current_block.get() < bet.settle_block {
        return Err(BaccaratError::BetTimeoutNotReached.into())
    }

    // Collect block hashes across confirmation_depth for entropy
    let confirmation_depth = u64::from(bet.confirmation_depth);
    let mut entropy_blocks = Vec::with_capacity(confirmation_depth as usize);

    for i in 0..confirmation_depth {
        let h = current_block.get().saturating_sub(i);
        let block_hash = wasm::util::get_block_hash(
            dwow_sdk::blockchain::BlockHeight::new(h),
        )?.0;
        entropy_blocks.push(dwow_entropy_contract::EntropyBlock { height: h, block_hash });
    }
    let seed = dwow_entropy_contract::derive_seed(&entropy_blocks);

    msg!("[baccarat::draw_cards] Entropy seed: {} from {} blocks", seed, entropy_blocks.len());

    // Deal cards using the entropy seed
    let (mut player_hand, mut banker_hand, player_third, banker_third) =
        deal_cards_from_seed(seed, bet.id);

    msg!("[baccarat::draw_cards] Player hand: {:?}, {:?}", player_hand.card1, player_hand.card2);
    msg!("[baccarat::draw_cards] Banker hand: {:?}, {:?}", banker_hand.card1, banker_hand.card2);

    // Calculate outcome using Baccarat drawing rules with entropy-derived third cards
    let game_outcome =
        calculate_outcome(&mut player_hand, &mut banker_hand, player_third, banker_third);

    msg!("[baccarat::draw_cards] Game outcome: {:?}", game_outcome);

    // Update bet with cards and outcome
    bet.player_hand = Some([player_hand.card1, player_hand.card2]);
    bet.banker_hand = Some([banker_hand.card1, banker_hand.card2]);
    bet.player_third_card = player_hand.third_card;
    bet.banker_third_card = banker_hand.third_card;
    bet.outcome = Some(game_outcome);
    bet.state = BetState::CardsDrawn;

    // Store updated bet
    wasm::db::db_set(bets_db, &bet.id.to_repr(), &bet.encode())?;

    // Create the update
    let update = DrawCardsUpdateV1 {
        bet_id: bet.id,
        player_card1: player_hand.card1,
        player_card2: player_hand.card2,
        banker_card1: banker_hand.card1,
        banker_card2: banker_hand.card2,
        player_third_card: player_hand.third_card,
        banker_third_card: banker_hand.third_card,
        outcome: game_outcome,
        state: BetState::CardsDrawn,
    };

    msg!("[baccarat::draw_cards] Cards drawn successfully");
    Ok(update.encode())
}

/// Process update for DrawCardsV1
pub fn baccarat_draw_cards_process_update_v1(
    _cid: dwow_sdk::crypto::ContractId,
    update: DrawCardsUpdateV1,
) -> Result<(), ContractError> {
    msg!("[baccarat::draw_cards::update] Cards drawn confirmed for bet_id: {:?}", update.bet_id);
    Ok(())
}