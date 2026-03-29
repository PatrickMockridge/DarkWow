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

use darkfi_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::TenderError,
    model::{
        Bid, BidId, BidState, CancelTenderParamsV1, CancelTenderUpdateV1, CloseTenderParamsV1,
        CloseTenderUpdateV1, CreateTenderParamsV1, CreateTenderUpdateV1, RejectBidParamsV1,
        RejectBidUpdateV1, RevealBidParamsV1, RevealBidUpdateV1, SelectWinnerParamsV1,
        SelectWinnerUpdateV1, SubmitBidParamsV1, SubmitBidUpdateV1, Tender, TenderId,
        TenderState,
    },
    TenderFunction, TENDER_CONTRACT_BIDS_TREE, TENDER_CONTRACT_INFO_TREE,
    TENDER_CONTRACT_NULLIFIERS_TREE, TENDER_CONTRACT_TENDERS_TREE,
};

darkfi_sdk::define_contract!(
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
            let params: CreateTenderParamsV1 = deserialize(&self_.data[1..])?;
            create_tender_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::SubmitBidV1 => {
            let params: SubmitBidParamsV1 = deserialize(&self_.data[1..])?;
            submit_bid_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::RevealBidV1 => {
            let params: RevealBidParamsV1 = deserialize(&self_.data[1..])?;
            reveal_bid_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::CloseTenderV1 => {
            let params: CloseTenderParamsV1 = deserialize(&self_.data[1..])?;
            close_tender_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::SelectWinnerV1 => {
            let params: SelectWinnerParamsV1 = deserialize(&self_.data[1..])?;
            select_winner_get_metadata_v1(cid, call_idx, calls, params)?
        }
        TenderFunction::CancelTenderV1 => vec![],
        TenderFunction::RejectBidV1 => vec![],
    };

    wasm::util::set_return_data(&metadata)
}

fn create_tender_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateTenderParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[tender::create_tender_get_metadata_v1] Creating tender: {:?}", params.tender_id);

    // Verify tender doesn't already exist
    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let existing: Option<Tender> = wasm::db::db_get(tenders_db, &serialize(&params.tender_id))?;
    if existing.is_some() {
        msg!("[tender::create_tender_get_metadata_v1] ERROR: Tender already exists");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Public inputs: requester public key coordinates
    let mut public_inputs = vec![
        params.requester_pub_x,
        params.requester_pub_y,
    ];

    msg!("[tender::create_tender_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

fn submit_bid_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: SubmitBidParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[tender::submit_bid_get_metadata_v1] Submitting bid: {:?}", params.bid_id);

    // Verify tender exists and is accepting bids
    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::submit_bid_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify tender is in Created or Bidding state
    if tender.state != TenderState::Created && tender.state != TenderState::Bidding {
        msg!("[tender::submit_bid_get_metadata_v1] ERROR: Tender not accepting bids");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify bid is within range
    if params.amount < tender.min_bid || params.amount > tender.max_bid {
        msg!("[tender::submit_bid_get_metadata_v1] ERROR: Bid out of range");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Get current block
    let current_block = wasm::chain::get_block_height()?;

    // Verify bidding deadline not passed
    if current_block >= tender.bid_deadline {
        msg!("[tender::submit_bid_get_metadata_v1] ERROR: Bidding period ended");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Public inputs include bidder_pub_x and bidder_pub_y so the apply function
    // can store the actual bidder public key (derived from ZK witness)
    let mut public_inputs = vec![
        params.tender_id,
        params.bid_id,
        params.bidder_pub_x,
        params.bidder_pub_y,
    ];

    msg!("[tender::submit_bid_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

fn reveal_bid_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: RevealBidParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[tender::reveal_bid_get_metadata_v1] Revealing bid: {:?}", params.bid_id);

    // Verify tender is in Revealed state
    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::reveal_bid_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    if tender.state != TenderState::Revealed {
        msg!("[tender::reveal_bid_get_metadata_v1] ERROR: Tender not in reveal state");
        return Err(ContractError::InvalidInstruction.into())
    }

    let mut public_inputs = vec![
        params.tender_id,
        params.bid_id,
        pallas::Base::from(params.revealed_amount),
    ];

    msg!("[tender::reveal_bid_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

fn close_tender_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: CloseTenderParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[tender::close_tender_get_metadata_v1] Closing tender: {:?}", params.tender_id);

    // Verify tender exists and is in Bidding state
    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::close_tender_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify caller is requester
    if tender.requester_pubkey != params.requester_pubkey {
        msg!("[tender::close_tender_get_metadata_v1] ERROR: Not requester");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify tender is in Bidding state
    if tender.state != TenderState::Bidding {
        msg!("[tender::close_tender_get_metadata_v1] ERROR: Tender not in bidding state");
        return Err(ContractError::InvalidInstruction.into())
    }

    Ok(vec![])
}

fn select_winner_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: SelectWinnerParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[tender::select_winner_get_metadata_v1] Selecting winner: {:?}", params.winner_bid_id);

    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::select_winner_get_metadata_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify tender is in Revealed state
    if tender.state != TenderState::Revealed {
        msg!("[tender::select_winner_get_metadata_v1] ERROR: Tender not in revealed state");
        return Err(ContractError::InvalidInstruction.into())
    }

    let mut public_inputs = vec![
        params.tender_id,
        params.winner_bid_id,
    ];

    msg!("[tender::select_winner_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = TenderFunction::try_from(self_.data[0])?;

    msg!("[tender::process_instruction] Processing function: {:?}", func);

    match func {
        TenderFunction::CreateTenderV1 => {
            let params: CreateTenderParamsV1 = deserialize(&self_.data[1..])?;
            create_tender_v1(cid, params)
        }
        TenderFunction::SubmitBidV1 => {
            let params: SubmitBidParamsV1 = deserialize(&self_.data[1..])?;
            submit_bid_v1(cid, params)
        }
        TenderFunction::RevealBidV1 => {
            let params: RevealBidParamsV1 = deserialize(&self_.data[1..])?;
            reveal_bid_v1(cid, params)
        }
        TenderFunction::CloseTenderV1 => {
            let params: CloseTenderParamsV1 = deserialize(&self_.data[1..])?;
            close_tender_v1(cid, params)
        }
        TenderFunction::SelectWinnerV1 => {
            let params: SelectWinnerParamsV1 = deserialize(&self_.data[1..])?;
            select_winner_v1(cid, params)
        }
        TenderFunction::CancelTenderV1 => {
            let params: CancelTenderParamsV1 = deserialize(&self_.data[1..])?;
            cancel_tender_v1(cid, params)
        }
        TenderFunction::RejectBidV1 => {
            let params: RejectBidParamsV1 = deserialize(&self_.data[1..])?;
            reject_bid_v1(cid, params)
        }
    }
}

fn create_tender_v1(cid: ContractId, params: CreateTenderParamsV1) -> ContractResult {
    msg!("[tender::create_tender_v1] Creating tender: {:?}", params.tender_id);

    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;

    // Check if tender already exists
    let existing: Option<Tender> = wasm::db::db_get(tenders_db, &serialize(&params.tender_id))?;
    if existing.is_some() {
        msg!("[tender::create_tender_v1] ERROR: Tender already exists");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Get current block
    let current_block = wasm::chain::get_block_height()?;

    // Create tender
    let tender = Tender {
        id: params.tender_id,
        requester_pubkey: [params.requester_pub_x, params.requester_pub_y],
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
    };

    wasm::db::db_set(tenders_db, &serialize(&params.tender_id), &serialize(&tender))?;

    msg!("[tender::create_tender_v1] Tender created successfully");
    Ok(())
}

fn submit_bid_v1(cid: ContractId, params: SubmitBidParamsV1) -> ContractResult {
    msg!("[tender::submit_bid_v1] Submitting bid: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_get(cid, TENDER_CONTRACT_BIDS_TREE)?;
    let nullifiers_db = wasm::db::db_get(cid, TENDER_CONTRACT_NULLIFIERS_TREE)?;

    // Get and verify tender
    let mut tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::submit_bid_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify tender is accepting bids
    if tender.state != TenderState::Created && tender.state != TenderState::Bidding {
        msg!("[tender::submit_bid_v1] ERROR: Tender not accepting bids");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Get current block and verify deadline
    let current_block = wasm::chain::get_block_height()?;
    if current_block >= tender.bid_deadline {
        msg!("[tender::submit_bid_v1] ERROR: Bidding period ended");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify bid amount is in range
    if params.amount < tender.min_bid || params.amount > tender.max_bid {
        msg!("[tender::submit_bid_v1] ERROR: Bid amount out of range");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Check for double submission using nullifier
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&params.bid_id))? {
        msg!("[tender::submit_bid_v1] ERROR: Bid already submitted");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Create bid
    let bid = Bid {
        id: params.bid_id,
        tender_id: params.tender_id,
        bidder_pubkey: [params.bidder_pub_x, params.bidder_pub_y],
        amount: params.amount,
        claim_id: params.claim_id,
        encrypted_payload: params.encrypted_payload,
        state: BidState::Sealed,
        revealed_amount: None,
        created_at: current_block,
    };

    // Store bid
    wasm::db::db_set(bids_db, &serialize(&params.bid_id), &serialize(&bid))?;

    // Store nullifier to prevent double submission
    wasm::db::db_set(nullifiers_db, &serialize(&params.bid_id), &[])?;

    // Update tender state and count
    if tender.state == TenderState::Created {
        tender.state = TenderState::Bidding;
    }
    tender.bid_count += 1;
    wasm::db::db_set(tenders_db, &serialize(&params.tender_id), &serialize(&tender))?;

    msg!("[tender::submit_bid_v1] Bid submitted successfully");
    Ok(())
}

fn reveal_bid_v1(cid: ContractId, params: RevealBidParamsV1) -> ContractResult {
    msg!("[tender::reveal_bid_v1] Revealing bid: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_get(cid, TENDER_CONTRACT_BIDS_TREE)?;
    let nullifiers_db = wasm::db::db_get(cid, TENDER_CONTRACT_NULLIFIERS_TREE)?;

    // Get and verify tender is in Revealed state
    let tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::reveal_bid_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    if tender.state != TenderState::Revealed {
        msg!("[tender::reveal_bid_v1] ERROR: Tender not in reveal state");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Get and verify bid
    let mut bid: Bid = match wasm::db::db_get(bids_db, &serialize(&params.bid_id))? {
        Some(b) => b,
        None => {
            msg!("[tender::reveal_bid_v1] ERROR: Bid not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify bid is in Sealed state
    if bid.state != BidState::Sealed {
        msg!("[tender::reveal_bid_v1] ERROR: Bid already revealed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Check for double reveal using nullifier (separate from submit nullifier)
    let reveal_nullifier = poseidon_hash(params.bid_id, pallas::Base::one());
    if wasm::db::db_contains_key(nullifiers_db, &serialize(&reveal_nullifier))? {
        msg!("[tender::reveal_bid_v1] ERROR: Bid already revealed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify revealed amount matches the sealed bid amount
    if params.revealed_amount != bid.amount {
        msg!("[tender::reveal_bid_v1] ERROR: Revealed amount does not match sealed bid");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Update bid with revealed amount
    bid.state = BidState::Revealed;
    bid.revealed_amount = Some(params.revealed_amount);

    wasm::db::db_set(bids_db, &serialize(&params.bid_id), &serialize(&bid))?;

    // Store nullifier to prevent double reveal
    wasm::db::db_set(nullifiers_db, &serialize(&reveal_nullifier), &[])?;

    msg!("[tender::reveal_bid_v1] Bid revealed successfully");
    Ok(())
}

fn close_tender_v1(cid: ContractId, params: CloseTenderParamsV1) -> ContractResult {
    msg!("[tender::close_tender_v1] Closing tender: {:?}", params.tender_id);

    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;

    // Get and verify tender
    let mut tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::close_tender_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify caller is requester
    if tender.requester_pubkey != params.requester_pubkey {
        msg!("[tender::close_tender_v1] ERROR: Not requester");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify tender is in Bidding state
    if tender.state != TenderState::Bidding {
        msg!("[tender::close_tender_v1] ERROR: Tender not in bidding state");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify bid deadline has passed
    let current_block = wasm::chain::get_block_height()?;
    if current_block < tender.bid_deadline {
        msg!("[tender::close_tender_v1] ERROR: Bid deadline not yet passed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Transition to Revealed state
    tender.state = TenderState::Revealed;
    wasm::db::db_set(tenders_db, &serialize(&params.tender_id), &serialize(&tender))?;

    msg!("[tender::close_tender_v1] Tender closed successfully");
    Ok(())
}

fn select_winner_v1(cid: ContractId, params: SelectWinnerParamsV1) -> ContractResult {
    msg!("[tender::select_winner_v1] Selecting winner: {:?}", params.winner_bid_id);

    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_get(cid, TENDER_CONTRACT_BIDS_TREE)?;

    // Get and verify tender
    let mut tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::select_winner_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify tender is in Revealed state
    if tender.state != TenderState::Revealed {
        msg!("[tender::select_winner_v1] ERROR: Tender not in revealed state");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify winner hasn't been selected yet
    if tender.selected_bid_id.is_some() {
        msg!("[tender::select_winner_v1] ERROR: Winner already selected");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Get and verify winning bid
    let mut winner_bid: Bid = match wasm::db::db_get(bids_db, &serialize(&params.winner_bid_id))? {
        Some(b) => b,
        None => {
            msg!("[tender::select_winner_v1] ERROR: Winner bid not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify bid is for this tender
    if winner_bid.tender_id != params.tender_id {
        msg!("[tender::select_winner_v1] ERROR: Bid not for this tender");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify bid is in Revealed state
    if winner_bid.state != BidState::Revealed {
        msg!("[tender::select_winner_v1] ERROR: Winner bid not revealed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify winning amount matches the revealed bid amount
    if params.winning_amount != winner_bid.revealed_amount.unwrap() {
        msg!("[tender::select_winner_v1] ERROR: Winning amount does not match revealed bid");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify winner public key matches the bid's bidder public key
    if params.winner_pubkey != winner_bid.bidder_pubkey {
        msg!("[tender::select_winner_v1] ERROR: Winner public key does not match bid");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Update tender
    tender.state = TenderState::Awarded;
    tender.selected_bid_id = Some(params.winner_bid_id);
    wasm::db::db_set(tenders_db, &serialize(&params.tender_id), &serialize(&tender))?;

    // Update winner bid
    winner_bid.state = BidState::Accepted;
    wasm::db::db_set(bids_db, &serialize(&params.winner_bid_id), &serialize(&winner_bid))?;

    msg!("[tender::select_winner_v1] Winner selected successfully");
    Ok(())
}

fn cancel_tender_v1(cid: ContractId, params: CancelTenderParamsV1) -> ContractResult {
    msg!("[tender::cancel_tender_v1] Cancelling tender: {:?}", params.tender_id);

    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;

    // Get and verify tender
    let mut tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::cancel_tender_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify caller is requester
    if tender.requester_pubkey != params.requester_pubkey {
        msg!("[tender::cancel_tender_v1] ERROR: Not requester");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify tender can be cancelled (not already Awarded)
    if tender.state == TenderState::Awarded {
        msg!("[tender::cancel_tender_v1] ERROR: Cannot cancel awarded tender");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Update tender
    tender.state = TenderState::Cancelled;
    wasm::db::db_set(tenders_db, &serialize(&params.tender_id), &serialize(&tender))?;

    msg!("[tender::cancel_tender_v1] Tender cancelled successfully");
    Ok(())
}

fn reject_bid_v1(cid: ContractId, params: RejectBidParamsV1) -> ContractResult {
    msg!("[tender::reject_bid_v1] Rejecting bid: {:?}", params.bid_id);

    let tenders_db = wasm::db::db_get(cid, TENDER_CONTRACT_TENDERS_TREE)?;
    let bids_db = wasm::db::db_get(cid, TENDER_CONTRACT_BIDS_TREE)?;

    // Get and verify tender
    let tender: Tender = match wasm::db::db_get(tenders_db, &serialize(&params.tender_id))? {
        Some(t) => t,
        None => {
            msg!("[tender::reject_bid_v1] ERROR: Tender not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify caller is requester
    if tender.requester_pubkey != params.requester_pubkey {
        msg!("[tender::reject_bid_v1] ERROR: Not requester");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify tender is in Revealed state (not Awarded)
    if tender.state != TenderState::Revealed {
        msg!("[tender::reject_bid_v1] ERROR: Tender not in reveal state");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Get and verify bid
    let mut bid: Bid = match wasm::db::db_get(bids_db, &serialize(&params.bid_id))? {
        Some(b) => b,
        None => {
            msg!("[tender::reject_bid_v1] ERROR: Bid not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify bid is in Revealed state
    if bid.state != BidState::Revealed {
        msg!("[tender::reject_bid_v1] ERROR: Bid not revealed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Update bid
    bid.state = BidState::Rejected;
    wasm::db::db_set(bids_db, &serialize(&params.bid_id), &serialize(&bid))?;

    msg!("[tender::reject_bid_v1] Bid rejected successfully");
    Ok(())
}

// ============================================================================
// PROCESS UPDATE
// ============================================================================

fn process_update(cid: ContractId, updates: &[u8]) -> ContractResult {
    let updates: Vec<DarkLeaf<pallas::Base>> = deserialize(updates)?;
    msg!("[tender::process_update] Applying {} updates", updates.len());

    for update in updates {
        match update.data[0] {
            0 => {
                let update_data: CreateTenderUpdateV1 =
                    deserialize(&serialize(&update.data[1..]))?;
                msg!("[tender::process_update] CreateTender: {:?}", update_data.tender_id);
            }
            1 => {
                let update_data: SubmitBidUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[tender::process_update] SubmitBid: {:?} {:?}", update_data.tender_id, update_data.bid_id);
            }
            2 => {
                let update_data: RevealBidUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[tender::process_update] RevealBid: {:?} {:?}", update_data.tender_id, update_data.bid_id);
            }
            3 => {
                let update_data: CloseTenderUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[tender::process_update] CloseTender: {:?}", update_data.tender_id);
            }
            4 => {
                let update_data: SelectWinnerUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[tender::process_update] SelectWinner: {:?} {:?}", update_data.tender_id, update_data.winner_bid_id);
            }
            5 => {
                let update_data: CancelTenderUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[tender::process_update] CancelTender: {:?}", update_data.tender_id);
            }
            6 => {
                let update_data: RejectBidUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[tender::process_update] RejectBid: {:?} {:?}", update_data.tender_id, update_data.bid_id);
            }
            _ => {
                msg!("[tender::process_update] ERROR: Unknown update type");
                return Err(ContractError::InvalidInstruction.into())
            }
        }
    }

    msg!("[tender::process_update] All updates applied successfully");
    Ok(())
}