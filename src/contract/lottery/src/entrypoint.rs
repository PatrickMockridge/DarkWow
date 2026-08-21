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
    // Embed zkas circuits (V2 only — V1 binaries removed, rc3 Batch 4)

    let commit_ticket_v2_bincode = include_bytes!("../proof/commit_ticket.zk.bin");
    wasm::db::zkas_db_set(&commit_ticket_v2_bincode[..])?;
    let reveal_ticket_v2_bincode = include_bytes!("../proof/reveal_ticket.zk.bin");
    wasm::db::zkas_db_set(&reveal_ticket_v2_bincode[..])?;
    let claim_prize_v2_bincode = include_bytes!("../proof/claim_prize.zk.bin");
    wasm::db::zkas_db_set(&claim_prize_v2_bincode[..])?;
    let draw_winners_v2_bincode = include_bytes!("../proof/draw_winners.zk.bin");
    wasm::db::zkas_db_set(&draw_winners_v2_bincode[..])?;
    let expire_lottery_v2_bincode = include_bytes!("../proof/expire_lottery.zk.bin");
    wasm::db::zkas_db_set(&expire_lottery_v2_bincode[..])?;
    let initialize_v2_bincode = include_bytes!("../proof/initialize.zk.bin");
    wasm::db::zkas_db_set(&initialize_v2_bincode[..])?;

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
    wasm::db::db_set(info_db, crate::LOTTERY_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID, &dwow_sdk::crypto::PROMISSORY_NOTE_CONTRACT_ID.to_bytes())?;

    Ok(())
}

/// Get metadata for ZK proof verification
#[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = LotteryFunction::try_from(self_.data[0])?;

    // tx fields are zero in heavyweight; the V2 clients commit to poseidon_hash([3, 0, 0]).
    let tx_binding = poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]);

    let mut zk_public_inputs: Vec<(String, Vec<pallas::Base>)> = vec![];
    match func {
        LotteryFunction::InitializeV1 => {
            // Non-ZK (setup-step, like roulette/slot InitializeV1): return a *valid
            // encoding of an empty* list, not raw `vec![]`.
        }
        LotteryFunction::BuyTicketV1 => {
            let params = crate::model::BuyTicketParamsV1::decode(&self_.data[1..])?;
            let ticket_id = crate::model::derive_ticket_id(
                params.lottery_id,
                &params.player_pub,
                params.value,
                params.nonce,
            );
            zk_public_inputs.push((
                crate::LOTTERY_CONTRACT_ZKAS_COMMIT_NS_V2.to_string(),
                vec![ticket_id, tx_binding, pallas::Base::zero()],
            ));
        }
        LotteryFunction::DrawWinnersV1 => {
            let params = crate::model::DrawWinnersParamsV1::decode(&self_.data[1..])?;
            zk_public_inputs.push((
                crate::LOTTERY_CONTRACT_ZKAS_DRAW_NS_V2.to_string(),
                vec![
                    params.house_pub.x().expect("pk not identity"),
                    params.house_pub.y().expect("pk not identity"),
                    params.house_nullifier,
                    tx_binding,
                    pallas::Base::zero(),
                ],
            ));
        }
        LotteryFunction::RevealTicketV1 => {
            zk_public_inputs.push((
                crate::LOTTERY_CONTRACT_ZKAS_REVEAL_NS_V2.to_string(),
                vec![tx_binding, pallas::Base::zero()],
            ));
        }
        LotteryFunction::ClaimPrizeV1 => {
            let params = crate::model::ClaimPrizeParamsV1::decode(&self_.data[1..])?;
            zk_public_inputs.push((
                crate::LOTTERY_CONTRACT_ZKAS_CLAIM_NS_V2.to_string(),
                vec![params.computed_commit, tx_binding, pallas::Base::zero()],
            ));
        }
        LotteryFunction::ExpireLotteryV1 => {
            let params = crate::model::ExpireLotteryParamsV1::decode(&self_.data[1..])?;
            zk_public_inputs.push((
                crate::LOTTERY_CONTRACT_ZKAS_EXPIRE_NS_V2.to_string(),
                vec![
                    params.house_pub.x().expect("pk not identity"),
                    params.house_pub.y().expect("pk not identity"),
                    params.house_nullifier,
                    tx_binding,
                    pallas::Base::zero(),
                ],
            ));
        }
    }

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    wasm::util::set_return_data(&metadata)
}

/// Process instruction
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func_byte = self_.data[0];
    let func = LotteryFunction::try_from(func_byte)?;

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

    wasm::util::set_return_data(&[&[func_byte], &update_data[..]].concat())
}

/// Process update
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match LotteryFunction::try_from(update_data[0])? {
        LotteryFunction::InitializeV1 => {
            let update = InitializeUpdateV1::decode(&update_data[1..])?;
            lottery_initialize_process_update_v1(cid, update)
        }
        LotteryFunction::BuyTicketV1 => {
            let update = BuyTicketUpdateV1::decode(&update_data[1..])?;
            lottery_buy_ticket_process_update_v1(cid, update)
        }
        LotteryFunction::DrawWinnersV1 => {
            let update = DrawWinnersUpdateV1::decode(&update_data[1..])?;
            lottery_draw_winners_process_update_v1(cid, update)
        }
        LotteryFunction::RevealTicketV1 => {
            let update = RevealTicketUpdateV1::decode(&update_data[1..])?;
            lottery_reveal_ticket_process_update_v1(cid, update)
        }
        LotteryFunction::ClaimPrizeV1 => {
            let update = ClaimPrizeUpdateV1::decode(&update_data[1..])?;
            lottery_claim_prize_process_update_v1(cid, update)
        }
        LotteryFunction::ExpireLotteryV1 => {
            let update = ExpireLotteryUpdateV1::decode(&update_data[1..])?;
            lottery_expire_lottery_process_update_v1(cid, update)
        }
    }
}

// Modules for function implementations
mod initialize;
mod buy_ticket;
mod draw_winners;
mod reveal_ticket;
mod claim_prize;
mod expire_lottery;

use initialize::*;
use buy_ticket::*;
use draw_winners::*;
use reveal_ticket::*;
use claim_prize::*;
use expire_lottery::*;
