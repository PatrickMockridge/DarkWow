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

//! Lottery Contract Entrypoint

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::ContractResult,
    pasta::pallas, wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable};
use dwow_promissory_note_contract::validation::validate_child_contract_id;
use pasta_curves::group::Curve;
use pasta_curves::arithmetic::CurveAffine;

use crate::model::{
    BuyTicketUpdateV1, ClaimPrizeUpdateV1, DrawWinnersUpdateV1, ExpireLotteryUpdateV1,
    InitializeUpdateV1, RevealTicketUpdateV1,
};
use crate::LotteryFunction;

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

/// Initialize the contract
fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    // Embed zkas circuits
    let commit_ticket_bincode = include_bytes!("../proof/commit_ticket_v1.zk.bin");
    let reveal_ticket_bincode = include_bytes!("../proof/reveal_ticket_v1.zk.bin");

    wasm::db::zkas_db_set(&commit_ticket_bincode[..])?;
    wasm::db::zkas_db_set(&reveal_ticket_bincode[..])?;

    // Initialize database trees
    wasm::db::db_init(cid, crate::LOTTERY_CONTRACT_LOTTERIES_TREE)?;
    wasm::db::db_init(cid, crate::LOTTERY_CONTRACT_TICKETS_TREE)?;
    wasm::db::db_init(cid, crate::LOTTERY_CONTRACT_NULLIFIERS_TREE)?;
    wasm::db::db_init(cid, crate::LOTTERY_CONTRACT_CLAIMS_TREE)?;
    // Initialize SMT databases for ticket Merkle tree
    wasm::db::db_init(cid, crate::LOTTERY_CONTRACT_TICKETS_SMT_TREE)?;
    wasm::db::db_init(cid, crate::LOTTERY_CONTRACT_TICKETS_ROOTS_TREE)?;
    wasm::db::db_init(cid, crate::LOTTERY_CONTRACT_INFO_TREE)?;

    // Store promissory_note contract ID for cross-contract validation
    let info_db = wasm::db::db_lookup(cid, crate::LOTTERY_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, crate::LOTTERY_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &[0u8; 32])?;

    Ok(())
}

/// Get metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = LotteryFunction::try_from(self_.data[0])?;

    let metadata = match func {
        LotteryFunction::BuyTicketV1 => {
            let params: crate::model::BuyTicketParamsV1 = deserialize(&self_.data[1..])?;
            let player_x = params.player_pub.x();
            let player_y = params.player_pub.y();
            let ticket_id = poseidon_hash([
                player_x,
                player_y,
                params.commitment,
                params.token_id,
                pallas::Base::from(params.value),
            ]);
            let vc_affine = params.value_commit.to_affine();
            let coords = vc_affine.coordinates();
            if coords.is_none().into() {
                vec![]
            } else {
            let vc_coords = coords.unwrap();
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::LOTTERY_CONTRACT_ZKAS_COMMIT_NS.to_string(),
                vec![ticket_id, *vc_coords.x(), *vc_coords.y()],
            ));
            let mut metadata = vec![];
            zk_public_inputs.encode(&mut metadata)?;
            metadata
            }
        }
        LotteryFunction::RevealTicketV1 => {
            let params: crate::model::RevealTicketParamsV1 = deserialize(&self_.data[1..])?;
            let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
            zk_public_inputs.push((
                crate::LOTTERY_CONTRACT_ZKAS_REVEAL_NS.to_string(),
                vec![params.revealed_commitment, pallas::Base::from(params.matches as u64)],
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
    let func = LotteryFunction::try_from(self_.data[0])?;

    let update_data = match func {
        LotteryFunction::InitializeV1 => {
            lottery_initialize_process_instruction_v1(cid, call_idx, calls)?
        }
        LotteryFunction::BuyTicketV1 => lottery_buy_ticket_process_instruction_v1(cid, call_idx, calls)?,
        LotteryFunction::DrawWinnersV1 => {
            lottery_draw_winners_process_instruction_v1(cid, call_idx, calls)?
        }
        LotteryFunction::RevealTicketV1 => {
            lottery_reveal_ticket_process_instruction_v1(cid, call_idx, calls)?
        }
        LotteryFunction::ClaimPrizeV1 => lottery_claim_prize_process_instruction_v1(cid, call_idx, calls)?,
        LotteryFunction::ExpireLotteryV1 => {
            lottery_expire_lottery_process_instruction_v1(cid, call_idx, calls)?
        }
    };

    wasm::util::set_return_data(&update_data)
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match LotteryFunction::try_from(update_data[0])? {
        LotteryFunction::InitializeV1 => {
            let update: InitializeUpdateV1 = deserialize(&update_data[1..])?;
            lottery_initialize_process_update_v1(cid, update)
        }
        LotteryFunction::BuyTicketV1 => {
            let update: BuyTicketUpdateV1 = deserialize(&update_data[1..])?;
            lottery_buy_ticket_process_update_v1(cid, update)
        }
        LotteryFunction::DrawWinnersV1 => {
            let update: DrawWinnersUpdateV1 = deserialize(&update_data[1..])?;
            lottery_draw_winners_process_update_v1(cid, update)
        }
        LotteryFunction::RevealTicketV1 => {
            let update: RevealTicketUpdateV1 = deserialize(&update_data[1..])?;
            lottery_reveal_ticket_process_update_v1(cid, update)
        }
        LotteryFunction::ClaimPrizeV1 => {
            let update: ClaimPrizeUpdateV1 = deserialize(&update_data[1..])?;
            lottery_claim_prize_process_update_v1(cid, update)
        }
        LotteryFunction::ExpireLotteryV1 => {
            let update: ExpireLotteryUpdateV1 = deserialize(&update_data[1..])?;
            lottery_expire_lottery_process_update_v1(cid, update)
        }
    }
}

// Modules for function implementations
mod initialize_v1;
mod buy_ticket_v1;
mod draw_winners_v1;
mod reveal_ticket_v1;
mod claim_prize_v1;
mod expire_lottery_v1;

use initialize_v1::*;
use buy_ticket_v1::*;
use draw_winners_v1::*;
use reveal_ticket_v1::*;
use claim_prize_v1::*;
use expire_lottery_v1::*;
