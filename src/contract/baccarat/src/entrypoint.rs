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

//! Baccarat Contract Entrypoint

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::ContractResult,
    pasta::pallas, wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};
use pasta_curves::group::Curve;
use pasta_curves::arithmetic::CurveAffine;

use crate::error::BaccaratError;
use crate::model::{
    CommitBetUpdateV1, DrawCardsUpdateV1, HouseCloseUpdateV1, SettleBetUpdateV1,
};
use crate::BaccaratFunction;

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Embed zkas circuits
    let commit_bet_bincode = include_bytes!("../proof/commit_bet_v1.zk.bin");
    let draw_cards_bincode = include_bytes!("../proof/draw_cards_v1.zk.bin");
    let house_close_bincode = include_bytes!("../proof/house_close_v1.zk.bin");
    let settle_bet_bincode = include_bytes!("../proof/settle_bet_v1.zk.bin");

    wasm::db::zkas_db_set(&commit_bet_bincode[..])?;
    wasm::db::zkas_db_set(&draw_cards_bincode[..])?;
    wasm::db::zkas_db_set(&house_close_bincode[..])?;
    wasm::db::zkas_db_set(&settle_bet_bincode[..])?;

    // Initialize database trees
    wasm::db::db_init(cid, crate::BACCARAT_CONTRACT_BETS_TREE)?;
    wasm::db::db_init(cid, crate::BACCARAT_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_init(cid, crate::BACCARAT_CONTRACT_INFO_TREE)?;
    wasm::db::db_init(cid, crate::BACCARAT_CONTRACT_HOUSE_TREE)?;

    // Initialize default house settings
    let info_db = wasm::db::db_lookup(cid, crate::BACCARAT_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, crate::BACCARAT_CONTRACT_HOUSE_EDGE, &serialize(&crate::DEFAULT_HOUSE_EDGE))?;
    wasm::db::db_set(info_db, crate::BACCARAT_CONTRACT_BET_TIMEOUT, &serialize(&crate::DEFAULT_BET_TIMEOUT))?;

    // Store promissory_note contract ID for cross-contract validation
    wasm::db::db_set(info_db, crate::BACCARAT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;

    Ok(())
}

/// Get metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BaccaratFunction::try_from(self_.data[0])?;

    let metadata = match func {
        BaccaratFunction::CommitBetV1 => {
            let params: crate::model::CommitBetParamsV1 = deserialize(&self_.data[1..])?;
            let player_x = params.player_pub.x();
            let player_y = params.player_pub.y();
            let bet_id = poseidon_hash([
                player_x,
                player_y,
                pallas::Base::from(params.bet_type as u64),
                pallas::Base::from(params.bet_value),
                params.secret_nonce,
                params.blind,
                params.token_id,
            ]);
            let vc_affine = params.value_commit.to_affine();
            let coords = vc_affine.coordinates();
            if coords.is_none().into() {
                vec![]
            } else {
            let vc_coords = coords.unwrap();
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::BACCARAT_CONTRACT_ZKAS_COMMIT_NS.to_string(),
                vec![bet_id, *vc_coords.x(), *vc_coords.y()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
            }
        }
        BaccaratFunction::DrawCardsV1 => {
            let params: crate::model::DrawCardsParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            let secret_nonce_commit = poseidon_hash([params.secret_nonce]);
            zk_public_inputs.push((
                crate::BACCARAT_CONTRACT_ZKAS_DRAW_NS.to_string(),
                vec![params.bet_id, secret_nonce_commit],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        BaccaratFunction::HouseCloseV1 => {
            let params: crate::model::HouseCloseParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::BACCARAT_CONTRACT_ZKAS_HOUSE_CLOSE_NS.to_string(),
                vec![params.bet_id, params.house_pub_x, params.house_pub_y, params.close_nullifier],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        BaccaratFunction::SettleBetV1 => {
            let params: crate::model::SettleBetParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::BACCARAT_CONTRACT_ZKAS_SETTLE_NS.to_string(),
                vec![params.bet_id],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        _ => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = BaccaratFunction::try_from(self_.data[0])?;

    let update_data = match func {
        BaccaratFunction::CommitBetV1 => baccarat_commit_bet_process_instruction_v1(cid, call_idx, calls)?,
        BaccaratFunction::DrawCardsV1 => baccarat_draw_cards_process_instruction_v1(cid, call_idx, calls)?,
        BaccaratFunction::SettleBetV1 => baccarat_settle_bet_process_instruction_v1(cid, call_idx, calls)?,
        BaccaratFunction::HouseCloseV1 => baccarat_house_close_process_instruction_v1(cid, call_idx, calls)?,
        BaccaratFunction::InitializeV1 => return Err(BaccaratError::InvalidFunction.into()),
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match BaccaratFunction::try_from(update_data[0])? {
        BaccaratFunction::CommitBetV1 => {
            let update: CommitBetUpdateV1 = deserialize(&update_data[1..])?;
            baccarat_commit_bet_process_update_v1(cid, update)
        }
        BaccaratFunction::DrawCardsV1 => {
            let update: DrawCardsUpdateV1 = deserialize(&update_data[1..])?;
            baccarat_draw_cards_process_update_v1(cid, update)
        }
        BaccaratFunction::SettleBetV1 => {
            let update: SettleBetUpdateV1 = deserialize(&update_data[1..])?;
            baccarat_settle_bet_process_update_v1(cid, update)
        }
        BaccaratFunction::HouseCloseV1 => {
            let update: HouseCloseUpdateV1 = deserialize(&update_data[1..])?;
            baccarat_house_close_process_update_v1(cid, update)
        }
        BaccaratFunction::InitializeV1 => Err(BaccaratError::InvalidFunction.into()),
    }
}

// Modules for function implementations
mod commit_bet_v1;
mod draw_cards_v1;
mod settle_bet_v1;
mod house_close_v1;

use commit_bet_v1::*;
use draw_cards_v1::*;
use house_close_v1::*;
use settle_bet_v1::*;
