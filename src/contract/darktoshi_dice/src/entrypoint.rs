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

//! DarkToshi Dice Contract Entrypoint

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::ContractResult,
    pasta::pallas, wasm, ContractCall,
};
use dwow_serial::{deserialize, serialize, Encodable};
use pasta_curves::group::Curve;
use pasta_curves::arithmetic::CurveAffine;

use crate::error::DiceError;
use crate::model::{
    CommitBetUpdateV1, HouseCloseUpdateV1, RevealRollUpdateV1, SettleBetUpdateV1,
};
use crate::DiceFunction;

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
    let settle_bet_bincode = include_bytes!("../proof/settle_bet_v1.zk.bin");

    wasm::db::zkas_db_set(&commit_bet_bincode[..])?;
    wasm::db::zkas_db_set(&settle_bet_bincode[..])?;

    // Initialize database trees
    wasm::db::db_init(cid, crate::DICE_CONTRACT_BETS_TREE)?;
    wasm::db::db_init(cid, crate::DICE_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_init(cid, crate::DICE_CONTRACT_INFO_TREE)?;
    wasm::db::db_init(cid, crate::DICE_CONTRACT_HOUSE_TREE)?;

    // Initialize default house settings
    let info_db = wasm::db::db_lookup(cid, crate::DICE_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, crate::DICE_CONTRACT_HOUSE_EDGE, &serialize(&crate::DEFAULT_HOUSE_EDGE))?;
    wasm::db::db_set(info_db, crate::DICE_CONTRACT_ROLL_TIMEOUT, &serialize(&crate::DEFAULT_ROLL_TIMEOUT))?;

    Ok(())
}

/// Get metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = DiceFunction::try_from(self_.data[0])?;

    let metadata = match func {
        DiceFunction::CommitBetV1 => {
            let params: crate::model::CommitBetParamsV1 = deserialize(&self_.data[1..])?;
            let player_x = params.player_pub.x();
            let player_y = params.player_pub.y();
            let bet_id = poseidon_hash([
                player_x,
                player_y,
                pallas::Base::from(params.bet_value),
                pallas::Base::from(params.target as u64),
                params.secret_nonce,
                params.blind,
                params.token_id,
            ]);
            let vc_affine = params.value_commit.to_affine();
            let vc_coords = vc_affine.coordinates().unwrap();
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DICE_CONTRACT_ZKAS_COMMIT_NS.to_string(),
                vec![bet_id, *vc_coords.x(), *vc_coords.y()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
        }
        DiceFunction::SettleBetV1 => {
            let params: crate::model::SettleBetParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::DICE_CONTRACT_ZKAS_SETTLE_NS.to_string(),
                vec![params.bet_id, params.roll_hash],
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
    let func = DiceFunction::try_from(self_.data[0])?;

    let update_data = match func {
        DiceFunction::CommitBetV1 => dice_commit_bet_process_instruction_v1(cid, call_idx, calls)?,
        DiceFunction::RevealRollV1 => dice_reveal_roll_process_instruction_v1(cid, call_idx, calls)?,
        DiceFunction::SettleBetV1 => dice_settle_bet_process_instruction_v1(cid, call_idx, calls)?,
        DiceFunction::HouseCloseV1 => dice_house_close_process_instruction_v1(cid, call_idx, calls)?,
        DiceFunction::InitializeV1 => return Err(DiceError::InvalidFunction.into()),
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match DiceFunction::try_from(update_data[0])? {
        DiceFunction::CommitBetV1 => {
            let update: CommitBetUpdateV1 = deserialize(&update_data[1..])?;
            dice_commit_bet_process_update_v1(cid, update)
        }
        DiceFunction::RevealRollV1 => {
            let update: RevealRollUpdateV1 = deserialize(&update_data[1..])?;
            dice_reveal_roll_process_update_v1(cid, update)
        }
        DiceFunction::SettleBetV1 => {
            let update: SettleBetUpdateV1 = deserialize(&update_data[1..])?;
            dice_settle_bet_process_update_v1(cid, update)
        }
        DiceFunction::HouseCloseV1 => {
            let update: HouseCloseUpdateV1 = deserialize(&update_data[1..])?;
            dice_house_close_process_update_v1(cid, update)
        }
        DiceFunction::InitializeV1 => Err(DiceError::InvalidFunction.into()),
    }
}

// Modules for function implementations
mod commit_bet_v1;
mod reveal_roll_v1;
mod settle_bet_v1;
mod house_close_v1;

use commit_bet_v1::*;
use house_close_v1::*;
use reveal_roll_v1::*;
use settle_bet_v1::*;
