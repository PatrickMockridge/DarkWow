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

//! WASM entrypoint for the tender contract
//!
//! ## Tender Contract Overview
//!
//! A privacy-preserving sealed-bid tendering system that integrates with:
//! - Identity/Competency framework for skill verification
//! - Labor Market for job execution
//! - Tau for task tracking
//!
//! ## Flow
//!
//! 1. Requester creates tender with specs + requirements
//! 2. Workers prove competency via Identity contract
//! 3. Workers submit sealed bids with competency proofs
//! 4. After bid deadline, bids are revealed
//! 5. Requester selects winner based on competency + price
//! 6. Job created in Labor Market for execution
//! 7. Task tracked via Tau

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta,
    wasm, ContractCall,
};
use dwow_serial::{deserialize, Encodable};

use crate::{
    TENDER_CONTRACT_ZKAS_CREATE_NS_V1, TENDER_CONTRACT_ZKAS_REVEAL_BID_NS_V1,
    TENDER_CONTRACT_ZKAS_SELECT_WINNER_NS_V1, TENDER_CONTRACT_ZKAS_SUBMIT_BID_NS_V1,
    TENDER_CONTRACT_ZKAS_SUBMIT_BID_WITH_CAP_NS_V1,
    TENDER_CONTRACT_ZKAS_CREATE_TENDER_NS_V2, TENDER_CONTRACT_ZKAS_CREATE_NS_V2, TENDER_CONTRACT_ZKAS_REVEAL_BID_NS_V2,
    TENDER_CONTRACT_ZKAS_SUBMIT_BID_NS_V2, TENDER_CONTRACT_ZKAS_SUBMIT_BID_WITH_CAP_NS_V2,
    TENDER_CONTRACT_ZKAS_SELECT_WINNER_NS_V2,
};

use crate::{
    model::{
        Bid, BidState, CancelTenderParamsV1, CancelTenderUpdateV1, CloseTenderParamsV1,
        CloseTenderUpdateV1, CreateTenderParamsV1, CreateTenderUpdateV1,
        CreateTenderWithCapabilityParamsV1, CreateTenderWithCapabilityUpdateV1,
        RejectBidParamsV1, RejectBidUpdateV1, RevealBidParamsV1, RevealBidUpdateV1,
        SelectWinnerParamsV1, SelectWinnerUpdateV1, SubmitBidParamsV1, SubmitBidUpdateV1,
        SubmitBidWithCapabilityParamsV1, SubmitBidWithCapabilityUpdateV1, Tender,
        TenderState,
    },
    TenderFunction, TENDER_CONTRACT_BIDS_TREE, TENDER_CONTRACT_INFO_TREE,
    TENDER_CONTRACT_NULLIFIERS_TREE, TENDER_CONTRACT_TENDERS_TREE,
};

dwow_sdk::define_contract!(
    init: init_contract,
    exec: process_instruction,
    apply: process_update,
    metadata: get_metadata
);

// ============================================================================
// INITIALIZATION
// ============================================================================

/// Initialize tender contract state
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[tender::init_contract] Initializing tender contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, TENDER_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, b"db_version", &env!("CARGO_PKG_VERSION").as_bytes())?;

    // Initialize tenders tree
    wasm::db::db_init(cid, TENDER_CONTRACT_TENDERS_TREE)?;

    // Initialize bids tree
    wasm::db::db_init(cid, TENDER_CONTRACT_BIDS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, TENDER_CONTRACT_NULLIFIERS_TREE)?;

    msg!("[tender::init_contract] Tender contract initialized successfully");

    let create_tender_v1_bincode = include_bytes!("../proof/create_tender_v1.zk.bin");
    wasm::db::zkas_db_set(&create_tender_v1_bincode[..])?;
    let reveal_bid_v1_bincode = include_bytes!("../proof/reveal_bid_v1.zk.bin");
    wasm::db::zkas_db_set(&reveal_bid_v1_bincode[..])?;
    let select_winner_v1_bincode = include_bytes!("../proof/select_winner_v1.zk.bin");
    wasm::db::zkas_db_set(&select_winner_v1_bincode[..])?;
    let submit_bid_v1_bincode = include_bytes!("../proof/submit_bid_v1.zk.bin");
    wasm::db::zkas_db_set(&submit_bid_v1_bincode[..])?;
    let submit_bid_with_capability_v1_bincode =
        include_bytes!("../proof/submit_bid_with_capability_v1.zk.bin");
    wasm::db::zkas_db_set(&submit_bid_with_capability_v1_bincode[..])?;

    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = TenderFunction::try_from(self_.data[0])?;

    msg!("[tender::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        TenderFunction::CreateTenderV1 => {
            let params = CreateTenderParamsV1::decode(&self_.data[1..])?;
            create_tender_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::SubmitBidV1 => {
            let params = SubmitBidParamsV1::decode(&self_.data[1..])?;
            submit_bid_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::RevealBidV1 => {
            let params = RevealBidParamsV1::decode(&self_.data[1..])?;
            reveal_bid_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::CloseTenderV1 => {
            let params = CloseTenderParamsV1::decode(&self_.data[1..])?;
            close_tender_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::SelectWinnerV1 => {
            let params = SelectWinnerParamsV1::decode(&self_.data[1..])?;
            select_winner_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::CancelTenderV1 => vec![],
        TenderFunction::RejectBidV1 => vec![],
        TenderFunction::CreateTenderWithCapabilityV1 => vec![],
        TenderFunction::SubmitBidWithCapabilityV1 => {
            let params = SubmitBidWithCapabilityParamsV1::decode(&self_.data[1..])?;
            submit_bid_with_capability_get_metadata_v1(cid, call_idx, calls, params)?
        }
    };

    wasm::util::set_return_data(&metadata)
}

fn create_tender_get_metadata_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateTenderParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::create_tender_get_metadata_v1] Creating tender: {:?}", params.tender_id);

    // Verify tender doesn't already exist
    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    if wasm::db::db_contains_key(tenders_db, &params.tender_id.to_repr())? {
        msg!("[tender::create_tender_get_metadata_v1] ERROR: Tender already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        TENDER_CONTRACT_ZKAS_CREATE_NS_V2.to_string(),
        vec![params.requester_pub_x, params.requester_pub_y],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn submit_bid_get_metadata_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: SubmitBidParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::submit_bid_get_metadata_v1] Submitting bid: {:?}", params.bid_id);

    // Verify tender exists and is accepting bids
    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::submit_bid_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    if tender.state != TenderState::Created && tender.state != TenderState::Bidding {
        msg!("[tender::submit_bid_get_metadata_v1] ERROR: Tender not accepting bids");
        return Err(ContractError::InvalidFunction.into())
    }

    if params.amount < tender.min_bid || params.amount > tender.max_bid {
        msg!("[tender::submit_bid_get_metadata_v1] ERROR: Bid out of range");
        return Err(ContractError::InvalidFunction.into())
    }

    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block >= tender.bid_deadline {
        msg!("[tender::submit_bid_get_metadata_v1] ERROR: Bidding period ended");
        return Err(ContractError::InvalidFunction.into())
    }

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        TENDER_CONTRACT_ZKAS_SUBMIT_BID_NS_V2.to_string(),
        vec![
            params.bidder_pub_x,
            params.bidder_pub_y,
            params.tender_id,
            params.bid_id,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn submit_bid_with_capability_get_metadata_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: SubmitBidWithCapabilityParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::submit_bid_with_capability_get_metadata_v1] Submitting bid with capability: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::submit_bid_with_capability_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    if tender.state != TenderState::Created && tender.state != TenderState::Bidding {
        msg!("[tender::submit_bid_with_capability_get_metadata_v1] ERROR: Tender not accepting bids");
        return Err(ContractError::InvalidFunction.into())
    }

    if params.amount < tender.min_bid || params.amount > tender.max_bid {
        msg!("[tender::submit_bid_with_capability_get_metadata_v1] ERROR: Bid out of range");
        return Err(ContractError::InvalidFunction.into())
    }

    let _current_block = wasm::util::get_verifying_block_height()?.get();
    if _current_block >= tender.bid_deadline {
        msg!("[tender::submit_bid_with_capability_get_metadata_v1] ERROR: Bidding period ended");
        return Err(ContractError::InvalidFunction.into())
    }

    let cap_id = pasta::pallas::Base::from_raw([
        u64::from_le_bytes(params.required_capability_id[0..8].try_into().unwrap()),
        u64::from_le_bytes(params.required_capability_id[8..16].try_into().unwrap()),
        u64::from_le_bytes(params.required_capability_id[16..24].try_into().unwrap()),
        u64::from_le_bytes(params.required_capability_id[24..32].try_into().unwrap()),
    ]);

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        TENDER_CONTRACT_ZKAS_SUBMIT_BID_WITH_CAP_NS_V2.to_string(),
        vec![
            params.bidder_pub_x,
            params.bidder_pub_y,
            params.tender_id,
            params.bid_id,
            cap_id,
            params.capability_predicate_result,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn reveal_bid_get_metadata_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: RevealBidParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::reveal_bid_get_metadata_v1] Revealing bid: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::reveal_bid_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    if tender.state != TenderState::Revealed {
        msg!("[tender::reveal_bid_get_metadata_v1] ERROR: Tender not in reveal state");
        return Err(ContractError::InvalidFunction.into())
    }

    // Look up bid to get bidder_pub coordinates for ZK proof public inputs
    let bids_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_BIDS_TREE)?;
    let bid_data = wasm::db::db_get(bids_db, &params.bid_id.to_repr())?;
    let bid: Bid = match bid_data {
        Some(data) => Bid::decode(&data)?,
        None => {
            msg!("[tender::reveal_bid_get_metadata_v1] ERROR: Bid not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        TENDER_CONTRACT_ZKAS_REVEAL_BID_NS_V2.to_string(),
        vec![
            params.tender_id,
            params.bid_id,
            pasta::pallas::Base::from(params.revealed_amount),
            bid.bidder_pub_x,
            bid.bidder_pub_y,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

fn close_tender_get_metadata_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: CloseTenderParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::close_tender_get_metadata_v1] Closing tender: {:?}", params.tender_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::close_tender_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    if tender.requester_pub_x != params.requester_pub_x || tender.requester_pub_y != params.requester_pub_y {
        msg!("[tender::close_tender_get_metadata_v1] ERROR: Not requester");
        return Err(ContractError::InvalidFunction.into())
    }

    if tender.state != TenderState::Bidding {
        msg!("[tender::close_tender_get_metadata_v1] ERROR: Tender not in bidding state");
        return Err(ContractError::InvalidFunction.into())
    }

    // No ZK proof for close_tender — empty metadata
    Ok(vec![])
}

fn select_winner_get_metadata_v1(
    cid: ContractId,
    _call_idx: usize,
    _calls: Vec<DarkLeaf<ContractCall>>,
    params: SelectWinnerParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::select_winner_get_metadata_v1] Selecting winner: {:?}", params.winner_bid_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::select_winner_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    if tender.state != TenderState::Revealed {
        msg!("[tender::select_winner_get_metadata_v1] ERROR: Tender not in revealed state");
        return Err(ContractError::InvalidFunction.into())
    }

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        TENDER_CONTRACT_ZKAS_SELECT_WINNER_NS_V2.to_string(),
        vec![
            params.tender_id,
            params.winner_bid_id,
            tender.requester_pub_x,
            tender.requester_pub_y,
        ],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func_byte = self_.data[0];
    let func = TenderFunction::try_from(func_byte)?;

    msg!("[tender::process_instruction] Processing function: {:?}", func);

    let update_bytes = match func {
        TenderFunction::CreateTenderV1 => {
            let params = CreateTenderParamsV1::decode(&self_.data[1..])?;
            create_tender_v1(cid, params)?
        }
        TenderFunction::SubmitBidV1 => {
            let params = SubmitBidParamsV1::decode(&self_.data[1..])?;
            submit_bid_v1(cid, params)?
        }
        TenderFunction::RevealBidV1 => {
            let params = RevealBidParamsV1::decode(&self_.data[1..])?;
            reveal_bid_v1(cid, params)?
        }
        TenderFunction::CloseTenderV1 => {
            let params = CloseTenderParamsV1::decode(&self_.data[1..])?;
            close_tender_v1(cid, params)?
        }
        TenderFunction::SelectWinnerV1 => {
            let params = SelectWinnerParamsV1::decode(&self_.data[1..])?;
            select_winner_v1(cid, params)?
        }
        TenderFunction::CancelTenderV1 => {
            let params = CancelTenderParamsV1::decode(&self_.data[1..])?;
            cancel_tender_v1(cid, params)?
        }
        TenderFunction::RejectBidV1 => {
            let params = RejectBidParamsV1::decode(&self_.data[1..])?;
            reject_bid_v1(cid, params)?
        }
        // O-Cap enabled functions
        TenderFunction::CreateTenderWithCapabilityV1 => {
            let params = CreateTenderWithCapabilityParamsV1::decode(&self_.data[1..])?;
            create_tender_with_capability_v1(cid, params)?
        }
        TenderFunction::SubmitBidWithCapabilityV1 => {
            let params = SubmitBidWithCapabilityParamsV1::decode(&self_.data[1..])?;
            submit_bid_with_capability_v1(cid, params)?
        }
    };

    wasm::util::set_return_data(&[&[func_byte], &update_bytes[..]].concat())
}

fn create_tender_v1(cid: ContractId, params: CreateTenderParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::create_tender_v1] Creating tender: {:?}", params.tender_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;

    // Check if tender already exists
    let existing_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    if existing_data.is_some() {
        msg!("[tender::create_tender_v1] ERROR: Tender already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create tender
    let tender = Tender {
        version: 1,
        id: params.tender_id,
        requester_pub_x: params.requester_pub_x,
        requester_pub_y: params.requester_pub_y,
        title: params.title.clone(),
        specification: params.specification,
        attestation_id: params.attestation_id,
        min_bid: params.min_bid,
        max_bid: params.max_bid,
        bid_deadline: params.bid_deadline,
        reveal_deadline: params.reveal_deadline,
        delivery_deadline: params.delivery_deadline,
        state: TenderState::Created,
        selected_bid_id: None,
        bid_count: 0,
        created_at: current_block,
        required_capability: None,
        required_dag_id: None,
    };

    wasm::db::db_set(tenders_db, &params.tender_id.to_repr(), &tender.encode())?;

    msg!("[tender::create_tender_v1] Tender created successfully");
    Ok(CreateTenderUpdateV1 { tender_id: params.tender_id }.encode())
}

fn submit_bid_v1(cid: ContractId, params: SubmitBidParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::submit_bid_v1] Submitting bid: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_BIDS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_NULLIFIERS_TREE)?;

    // Get and verify tender
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let mut tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::submit_bid_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify tender is accepting bids
    if tender.state != TenderState::Created && tender.state != TenderState::Bidding {
        msg!("[tender::submit_bid_v1] ERROR: Tender not accepting bids");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get current block and verify deadline
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block >= tender.bid_deadline {
        msg!("[tender::submit_bid_v1] ERROR: Bidding period ended");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify bid amount is in range
    if params.amount < tender.min_bid || params.amount > tender.max_bid {
        msg!("[tender::submit_bid_v1] ERROR: Bid amount out of range");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check for double submission using nullifier
    if wasm::db::db_contains_key(nullifiers_db, &params.bid_id.to_repr())? {
        msg!("[tender::submit_bid_v1] ERROR: Bid already submitted");
        return Err(ContractError::InvalidFunction.into())
    }

    // Create bid
    let bid = Bid {
        version: 1,
        id: params.bid_id,
        tender_id: params.tender_id,
        bidder_pub_x: params.bidder_pub_x,
        bidder_pub_y: params.bidder_pub_y,
        amount: params.amount,
        claim_id: params.claim_id,
        encrypted_payload: params.encrypted_payload,
        state: BidState::Sealed,
        revealed_amount: None,
        created_at: current_block,
    };

    // Store bid
    wasm::db::db_set(bids_db, &params.bid_id.to_repr(), &bid.encode())?;

    // Store nullifier to prevent double submission
    wasm::db::db_set(nullifiers_db, &params.bid_id.to_repr(), &[])?;

    // Update tender state and count
    if tender.state == TenderState::Created {
        tender.state = TenderState::Bidding;
    }
    tender.bid_count += 1;
    wasm::db::db_set(tenders_db, &params.tender_id.to_repr(), &tender.encode())?;

    msg!("[tender::submit_bid_v1] Bid submitted successfully");
    Ok(SubmitBidUpdateV1 { tender_id: params.tender_id, bid_id: params.bid_id }.encode())
}

fn reveal_bid_v1(cid: ContractId, params: RevealBidParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::reveal_bid_v1] Revealing bid: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_BIDS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_NULLIFIERS_TREE)?;

    // Get and verify tender is in Revealed state
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::reveal_bid_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    if tender.state != TenderState::Revealed {
        msg!("[tender::reveal_bid_v1] ERROR: Tender not in reveal state");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get and verify bid
    let bid_data = wasm::db::db_get(bids_db, &params.bid_id.to_repr())?;
    let mut bid: Bid = match bid_data {
        Some(data) => Bid::decode(&data)?,
        None => {
            msg!("[tender::reveal_bid_v1] ERROR: Bid not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify bid is in Sealed state
    if bid.state != BidState::Sealed {
        msg!("[tender::reveal_bid_v1] ERROR: Bid already revealed");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check for double reveal using nullifier (separate from submit nullifier)
    let reveal_nullifier = poseidon_hash([params.bid_id, pasta::pallas::Base::one()]);
    if wasm::db::db_contains_key(nullifiers_db, &reveal_nullifier.to_repr())? {
        msg!("[tender::reveal_bid_v1] ERROR: Bid already revealed");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify revealed amount matches the sealed bid amount
    if params.revealed_amount != bid.amount {
        msg!("[tender::reveal_bid_v1] ERROR: Revealed amount does not match sealed bid");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update bid with revealed amount
    bid.state = BidState::Revealed;
    bid.revealed_amount = Some(params.revealed_amount);

    wasm::db::db_set(bids_db, &params.bid_id.to_repr(), &bid.encode())?;

    // Store nullifier to prevent double reveal
    wasm::db::db_set(nullifiers_db, &reveal_nullifier.to_repr(), &[])?;

    msg!("[tender::reveal_bid_v1] Bid revealed successfully");
    Ok(RevealBidUpdateV1 { tender_id: params.tender_id, bid_id: params.bid_id }.encode())
}

fn close_tender_v1(cid: ContractId, params: CloseTenderParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::close_tender_v1] Closing tender: {:?}", params.tender_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;

    // Get and verify tender
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let mut tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::close_tender_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify caller is requester
    if tender.requester_pub_x != params.requester_pub_x || tender.requester_pub_y != params.requester_pub_y {
        msg!("[tender::close_tender_v1] ERROR: Not requester");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify tender is in Bidding state
    if tender.state != TenderState::Bidding {
        msg!("[tender::close_tender_v1] ERROR: Tender not in bidding state");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify bid deadline has passed
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block < tender.bid_deadline {
        msg!("[tender::close_tender_v1] ERROR: Bid deadline not yet passed");
        return Err(ContractError::InvalidFunction.into())
    }

    // Transition to Revealed state
    tender.state = TenderState::Revealed;
    wasm::db::db_set(tenders_db, &params.tender_id.to_repr(), &tender.encode())?;

    msg!("[tender::close_tender_v1] Tender closed successfully");
    Ok(CloseTenderUpdateV1 { tender_id: params.tender_id }.encode())
}

fn select_winner_v1(cid: ContractId, params: SelectWinnerParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::select_winner_v1] Selecting winner: {:?}", params.winner_bid_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_BIDS_TREE)?;

    // Get and verify tender
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let mut tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::select_winner_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify caller is the tender requester
    if tender.requester_pub_x != params.requester_pub_x || tender.requester_pub_y != params.requester_pub_y {
        msg!("[tender::select_winner_v1] ERROR: Caller is not the tender requester");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify tender is in Revealed state
    if tender.state != TenderState::Revealed {
        msg!("[tender::select_winner_v1] ERROR: Tender not in revealed state");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify winner hasn't been selected yet
    if tender.selected_bid_id.is_some() {
        msg!("[tender::select_winner_v1] ERROR: Winner already selected");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get and verify winning bid
    let bid_data = wasm::db::db_get(bids_db, &params.winner_bid_id.to_repr())?;
    let mut winner_bid: Bid = match bid_data {
        Some(data) => Bid::decode(&data)?,
        None => {
            msg!("[tender::select_winner_v1] ERROR: Winner bid not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify bid is for this tender
    if winner_bid.tender_id != params.tender_id {
        msg!("[tender::select_winner_v1] ERROR: Bid not for this tender");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify bid is in Revealed state
    if winner_bid.state != BidState::Revealed {
        msg!("[tender::select_winner_v1] ERROR: Winner bid not revealed");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify winning amount matches the revealed bid amount
    if params.winning_amount != winner_bid.revealed_amount.expect("revealed_amount must be Some for a revealed bid") {
        msg!("[tender::select_winner_v1] ERROR: Winning amount does not match revealed bid");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify winner public key matches the bid's bidder public key
    if params.winner_pub_x != winner_bid.bidder_pub_x || params.winner_pub_y != winner_bid.bidder_pub_y {
        msg!("[tender::select_winner_v1] ERROR: Winner public key does not match bid");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update tender
    tender.state = TenderState::Awarded;
    tender.selected_bid_id = Some(params.winner_bid_id);
    wasm::db::db_set(tenders_db, &params.tender_id.to_repr(), &tender.encode())?;

    // Update winner bid
    winner_bid.state = BidState::Accepted;
    wasm::db::db_set(bids_db, &params.winner_bid_id.to_repr(), &winner_bid.encode())?;

    msg!("[tender::select_winner_v1] Winner selected successfully");
    Ok(SelectWinnerUpdateV1 { tender_id: params.tender_id, winner_bid_id: params.winner_bid_id, labor_job_id: None }.encode())
}

fn cancel_tender_v1(cid: ContractId, params: CancelTenderParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::cancel_tender_v1] Cancelling tender: {:?}", params.tender_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;

    // Get and verify tender
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let mut tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::cancel_tender_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify caller is requester
    if tender.requester_pub_x != params.requester_pub_x || tender.requester_pub_y != params.requester_pub_y {
        msg!("[tender::cancel_tender_v1] ERROR: Not requester");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify tender can be cancelled (not already Awarded)
    if tender.state == TenderState::Awarded {
        msg!("[tender::cancel_tender_v1] ERROR: Cannot cancel awarded tender");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update tender
    tender.state = TenderState::Cancelled;
    wasm::db::db_set(tenders_db, &params.tender_id.to_repr(), &tender.encode())?;

    msg!("[tender::cancel_tender_v1] Tender cancelled successfully");
    Ok(CancelTenderUpdateV1 { tender_id: params.tender_id }.encode())
}

fn reject_bid_v1(cid: ContractId, params: RejectBidParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::reject_bid_v1] Rejecting bid: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_BIDS_TREE)?;

    // Get and verify tender
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::reject_bid_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify caller is requester
    if tender.requester_pub_x != params.requester_pub_x || tender.requester_pub_y != params.requester_pub_y {
        msg!("[tender::reject_bid_v1] ERROR: Not requester");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify tender is in Revealed state (not Awarded)
    if tender.state != TenderState::Revealed {
        msg!("[tender::reject_bid_v1] ERROR: Tender not in reveal state");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get and verify bid
    let bid_data = wasm::db::db_get(bids_db, &params.bid_id.to_repr())?;
    let mut bid: Bid = match bid_data {
        Some(data) => Bid::decode(&data)?,
        None => {
            msg!("[tender::reject_bid_v1] ERROR: Bid not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify bid is in Revealed state
    if bid.state != BidState::Revealed {
        msg!("[tender::reject_bid_v1] ERROR: Bid not revealed");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update bid
    bid.state = BidState::Rejected;
    wasm::db::db_set(bids_db, &params.bid_id.to_repr(), &bid.encode())?;

    msg!("[tender::reject_bid_v1] Bid rejected successfully");
    Ok(RejectBidUpdateV1 { tender_id: params.tender_id, bid_id: params.bid_id }.encode())
}

// ============================================================================
// O-CAP ENABLED FUNCTIONS (0x07-0x08)
// ============================================================================

fn create_tender_with_capability_v1(
    cid: ContractId,
    params: CreateTenderWithCapabilityParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::create_tender_with_capability_v1] Creating tender with capability: {:?}", params.tender_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;

    // Check if tender already exists
    let existing_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    if existing_data.is_some() {
        msg!("[tender::create_tender_with_capability_v1] ERROR: Tender already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create tender with O-Cap fields
    let tender = Tender {
        version: 1,
        id: params.tender_id,
        requester_pub_x: params.requester_pub_x,
        requester_pub_y: params.requester_pub_y,
        title: params.title.clone(),
        specification: params.specification,
        attestation_id: params.attestation_id,
        min_bid: params.min_bid,
        max_bid: params.max_bid,
        bid_deadline: params.bid_deadline,
        reveal_deadline: params.reveal_deadline,
        delivery_deadline: params.delivery_deadline,
        state: TenderState::Created,
        selected_bid_id: None,
        bid_count: 0,
        created_at: current_block,
        required_capability: params.required_capability,
        required_dag_id: params.required_dag_id,
    };

    wasm::db::db_set(tenders_db, &params.tender_id.to_repr(), &tender.encode())?;

    msg!("[tender::create_tender_with_capability_v1] Tender created successfully");
    Ok(CreateTenderWithCapabilityUpdateV1 { tender_id: params.tender_id }.encode())
}

fn submit_bid_with_capability_v1(
    cid: ContractId,
    params: SubmitBidWithCapabilityParamsV1,
) -> Result<Vec<u8>, ContractError> {
    msg!("[tender::submit_bid_with_capability_v1] Submitting bid with capability: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_BIDS_TREE)?;
    let nullifiers_db = wasm::db::db_lookup(cid, TENDER_CONTRACT_NULLIFIERS_TREE)?;

    // Get and verify tender
    let tender_data = wasm::db::db_get(tenders_db, &params.tender_id.to_repr())?;
    let mut tender: Tender = match tender_data {
        Some(data) => Tender::decode(&data)?,
        None => {
            msg!("[tender::submit_bid_with_capability_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify tender is accepting bids
    if tender.state != TenderState::Created && tender.state != TenderState::Bidding {
        msg!("[tender::submit_bid_with_capability_v1] ERROR: Tender not accepting bids");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get current block and verify deadline
    let current_block = wasm::util::get_verifying_block_height()?.get();
    if current_block >= tender.bid_deadline {
        msg!("[tender::submit_bid_with_capability_v1] ERROR: Bidding period ended");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify bid amount is in range
    if params.amount < tender.min_bid || params.amount > tender.max_bid {
        msg!("[tender::submit_bid_with_capability_v1] ERROR: Bid amount out of range");
        return Err(ContractError::InvalidFunction.into())
    }

    // Check for double submission using nullifier
    if wasm::db::db_contains_key(nullifiers_db, &params.bid_id.to_repr())? {
        msg!("[tender::submit_bid_with_capability_v1] ERROR: Bid already submitted");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify capability matches tender's requirement
    if let Some(required_cap) = tender.required_capability {
        if params.required_capability_id != required_cap {
            msg!("[tender::submit_bid_with_capability_v1] ERROR: Capability mismatch");
            return Err(ContractError::InvalidFunction.into())
        }
    }

    // Verify capability predicate result is 1 (satisfied)
    if params.capability_predicate_result != pasta::pallas::Base::one() {
        msg!("[tender::submit_bid_with_capability_v1] ERROR: Capability requirement not met");
        return Err(ContractError::InvalidFunction.into())
    }

    // Create bid
    let bid = Bid {
        version: 1,
        id: params.bid_id,
        tender_id: params.tender_id,
        bidder_pub_x: params.bidder_pub_x,
        bidder_pub_y: params.bidder_pub_y,
        amount: params.amount,
        claim_id: params.claim_id,
        encrypted_payload: params.encrypted_payload,
        state: BidState::Sealed,
        revealed_amount: None,
        created_at: current_block,
    };

    // Store bid
    wasm::db::db_set(bids_db, &params.bid_id.to_repr(), &bid.encode())?;

    // Store nullifier to prevent double submission
    wasm::db::db_set(nullifiers_db, &params.bid_id.to_repr(), &[])?;

    // Update tender state and count
    if tender.state == TenderState::Created {
        tender.state = TenderState::Bidding;
    }
    tender.bid_count += 1;
    wasm::db::db_set(tenders_db, &params.tender_id.to_repr(), &tender.encode())?;

    msg!("[tender::submit_bid_with_capability_v1] Bid submitted successfully");
    Ok(SubmitBidWithCapabilityUpdateV1 { tender_id: params.tender_id, bid_id: params.bid_id }.encode())
}

// ============================================================================
// PROCESS UPDATE
// ============================================================================

fn process_update(_cid: ContractId, update_data: &[u8]) -> ContractResult {
    let func = TenderFunction::try_from(update_data[0])?;
    match func {
        TenderFunction::CreateTenderV1 => {
            let update = CreateTenderUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] CreateTender: {:?}", update.tender_id);
            Ok(())
        }
        TenderFunction::SubmitBidV1 => {
            let update = SubmitBidUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] SubmitBid: {:?} {:?}", update.tender_id, update.bid_id);
            Ok(())
        }
        TenderFunction::RevealBidV1 => {
            let update = RevealBidUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] RevealBid: {:?} {:?}", update.tender_id, update.bid_id);
            Ok(())
        }
        TenderFunction::CloseTenderV1 => {
            let update = CloseTenderUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] CloseTender: {:?}", update.tender_id);
            Ok(())
        }
        TenderFunction::SelectWinnerV1 => {
            let update = SelectWinnerUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] SelectWinner: {:?} {:?}", update.tender_id, update.winner_bid_id);
            Ok(())
        }
        TenderFunction::CancelTenderV1 => {
            let update = CancelTenderUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] CancelTender: {:?}", update.tender_id);
            Ok(())
        }
        TenderFunction::RejectBidV1 => {
            let update = RejectBidUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] RejectBid: {:?} {:?}", update.tender_id, update.bid_id);
            Ok(())
        }
        TenderFunction::CreateTenderWithCapabilityV1 => {
            let update = CreateTenderWithCapabilityUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] CreateTenderWithCapability: {:?}", update.tender_id);
            Ok(())
        }
        TenderFunction::SubmitBidWithCapabilityV1 => {
            let update = SubmitBidWithCapabilityUpdateV1::decode(&update_data[1..])?;
            msg!("[tender::process_update] SubmitBidWithCapability: {:?} {:?}", update.tender_id, update.bid_id);
            Ok(())
        }
    }
}
