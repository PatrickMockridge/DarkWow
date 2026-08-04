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

//! WASM entrypoint for the auction contract
//!
//! ## Auction Contract Overview
//!
//! A privacy-preserving auction contract that uses the escrow contract
//! for bid deposits. Enables sealed-bid or English-style auctions.
//!
//! ## Trust Model
//!
//! - **Seller creates auction** with item commitment and reserve price
//! - **Bidders place bids** (escrowed deposits, refundable if outbid)
//! - **Auction ends** at deadline
//! - **Winner claims** the item, **seller receives** payment
//! - **Outbid bidders** get refunds via escrow
//!
//! ## Composition with Escrow
//!
//! The auction contract COMPOSES with the escrow contract:
//! - Each bid creates an escrow to hold the deposit
//! - When outbid, the bidder gets a refund via escrow.refund()
//! - Winner claims via escrow.claim()
//! - Seller settles to receive the winning bid amount

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId},
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg, pasta,
    pasta::pallas, ContractCall,
    wasm,
};
use dwow_promissory_note_contract::validation::{validate_child_contract_id, validate_child_value_commit};
use dwow_serial::{deserialize, Encodable};

use crate::{
    error::AuctionError,
    model::{
        Auction, AuctionState, Bid, BidState, ClaimWinningsParamsV1,
        ClaimWinningsUpdateV1, CloseAuctionParamsV1, CloseAuctionUpdateV1,
        CreateAuctionParamsV1, CreateAuctionUpdateV1, PlaceBidParamsV1, PlaceBidUpdateV1,
        RefundBidParamsV1, RefundBidUpdateV1, SettleAuctionParamsV1, SettleAuctionUpdateV1,
    },
    AuctionFunction, AUCTION_CONTRACT_AUCTIONS_TREE, AUCTION_CONTRACT_BIDS_TREE,
    AUCTION_CONTRACT_INFO_TREE, AUCTION_CONTRACT_NULLIFIERS_TREE,
    AUCTION_CONTRACT_ZKAS_CREATE_NS_V2, AUCTION_CONTRACT_ZKAS_PLACE_BID_NS_V2,
    AUCTION_CONTRACT_ZKAS_CLOSE_NS_V2, AUCTION_CONTRACT_ZKAS_CLAIM_WINNINGS_NS_V2,
    AUCTION_CONTRACT_ZKAS_SETTLE_NS_V2, AUCTION_CONTRACT_ZKAS_REFUND_BID_NS_V2,
    PROMISSORY_NOTE_CONTRACT_ID_KEY,
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

/// Initialize auction contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Auctions tree (auction records)
/// - Bids tree (bid records)
/// - Nullifiers tree (spent nullifiers)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[auction::init_contract] Initializing auction contract");

    let _create_auction_v1_bincode = include_bytes!("../proof/create_auction.zk.bin");
    let _place_bid_v1_bincode = include_bytes!("../proof/place_bid.zk.bin");
    let _close_auction_v1_bincode = include_bytes!("../proof/close_auction.zk.bin");
    let _refund_bid_v1_bincode = include_bytes!("../proof/refund_bid.zk.bin");
    let _claim_winnings_v1_bincode = include_bytes!("../proof/claim_winnings.zk.bin");
    let _settle_auction_v1_bincode = include_bytes!("../proof/settle_auction.zk.bin");


    // Initialize info tree
    let info_db = wasm::db::db_init(cid, AUCTION_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, b"db_version", &env!("CARGO_PKG_VERSION").as_bytes())?;
    wasm::db::db_set(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY, &[0u8; 32])?;

    // Initialize auctions tree
    wasm::db::db_init(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;

    // Initialize bids tree
    wasm::db::db_init(cid, AUCTION_CONTRACT_BIDS_TREE)?;

    // Initialize nullifiers tree
    wasm::db::db_init(cid, AUCTION_CONTRACT_NULLIFIERS_TREE)?;

    msg!("[auction::init_contract] Auction contract initialized successfully");
    Ok(())
}

// ============================================================================
// METADATA (ZK proof verification)
// ============================================================================

/// Fetch metadata for ZK proof verification
fn get_metadata(_cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = AuctionFunction::try_from(self_.data[0])?;

    msg!("[auction::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        AuctionFunction::CreateAuctionV1 => {
            let params= CreateAuctionParamsV1::decode(&self_.data[1..])?;
            create_auction_get_metadata_v1(params)?
        }
        AuctionFunction::PlaceBidV1 => {
            let params= PlaceBidParamsV1::decode(&self_.data[1..])?;
            place_bid_get_metadata_v1(params)?
        }
        AuctionFunction::CloseAuctionV1 => {
            let params= CloseAuctionParamsV1::decode(&self_.data[1..])?;
            close_auction_get_metadata_v1(params)?
        }
        AuctionFunction::ClaimWinningsV1 => {
            let params= ClaimWinningsParamsV1::decode(&self_.data[1..])?;
            claim_winnings_get_metadata_v1(params)?
        }
        AuctionFunction::SettleAuctionV1 => {
            let params= SettleAuctionParamsV1::decode(&self_.data[1..])?;
            settle_auction_get_metadata_v1(params)?
        }
        AuctionFunction::RefundBidV1 => {
            let params= RefundBidParamsV1::decode(&self_.data[1..])?;
            refund_bid_get_metadata_v1(params)?
        }
    };

    wasm::util::set_return_data(&metadata)
}

/// `get_metadata` for CreateAuctionV1
fn create_auction_get_metadata_v1(params: CreateAuctionParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[auction::create_auction_get_metadata_v1] Creating auction: {:?}", params.auction_id);

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        AUCTION_CONTRACT_ZKAS_CREATE_NS_V2.to_string(),
        vec![params.auction_id, pallas::Base::zero(), pallas::Base::zero()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for PlaceBidV1
fn place_bid_get_metadata_v1(params: PlaceBidParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[auction::place_bid_get_metadata_v1] Placing bid: {:?}", params.bid_id);

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        AUCTION_CONTRACT_ZKAS_PLACE_BID_NS_V2.to_string(),
        vec![params.bid_id, pallas::Base::zero(), pallas::Base::zero()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for CloseAuctionV1
fn close_auction_get_metadata_v1(params: CloseAuctionParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[auction::close_auction_get_metadata_v1] Closing auction: {:?}", params.auction_id);

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        AUCTION_CONTRACT_ZKAS_CLOSE_NS_V2.to_string(),
        vec![pallas::Base::zero(), pallas::Base::zero()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for ClaimWinningsV1
fn claim_winnings_get_metadata_v1(params: ClaimWinningsParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[auction::claim_winnings_get_metadata_v1] Claiming winnings: {:?}", params.auction_id);

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        AUCTION_CONTRACT_ZKAS_CLAIM_WINNINGS_NS_V2.to_string(),
        vec![pallas::Base::zero(), pallas::Base::zero()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for SettleAuctionV1
fn settle_auction_get_metadata_v1(params: SettleAuctionParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[auction::settle_auction_get_metadata_v1] Settling auction: {:?}", params.auction_id);

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        AUCTION_CONTRACT_ZKAS_SETTLE_NS_V2.to_string(),
        vec![params.settlement_nullifier, pallas::Base::zero(), pallas::Base::zero()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

/// `get_metadata` for RefundBidV1
fn refund_bid_get_metadata_v1(params: RefundBidParamsV1) -> Result<Vec<u8>, ContractError> {
    msg!("[auction::refund_bid_get_metadata_v1] Refunding bid: {:?}", params.bid_id);

    let mut zk_public_inputs: Vec<(String, Vec<pasta::pallas::Base>)> = vec![];
    zk_public_inputs.push((
        AUCTION_CONTRACT_ZKAS_REFUND_BID_NS_V2.to_string(),
        vec![params.refund_nullifier, pallas::Base::zero(), pallas::Base::zero()],
    ));

    let mut metadata = vec![];
    zk_public_inputs.encode(&mut metadata)?;
    Ok(metadata)
}

// ============================================================================
// INSTRUCTION PROCESSING
// ============================================================================

/// Process contract instructions
fn process_instruction(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = AuctionFunction::try_from(self_.data[0])?;

    msg!("[auction::process_instruction] Processing function: {:?}", func);

    match func {
        AuctionFunction::CreateAuctionV1 => {
            let params= CreateAuctionParamsV1::decode(&self_.data[1..])?;
            create_auction_v1(cid, params)
        }
        AuctionFunction::PlaceBidV1 => {
            let params= PlaceBidParamsV1::decode(&self_.data[1..])?;
            place_bid_v1(cid, params)
        }
        AuctionFunction::CloseAuctionV1 => {
            let params= CloseAuctionParamsV1::decode(&self_.data[1..])?;
            close_auction_v1(cid, params)
        }
        AuctionFunction::ClaimWinningsV1 => {
            // Validate promissory_note::transfer_v1 child call for payout
            let this_call = &calls[call_idx];
            if this_call.children_indexes.len() != 1 {
                msg!("[ClaimWinningsV1] Expected 1 child call (promissory_note::transfer_v1)");
                return Err(AuctionError::InvalidChildrenIndexes.into())
            }
            let child_idx = this_call.children_indexes[0];
            if calls[child_idx].data.data[0] != 0x04 {
                msg!("[ClaimWinningsV1] Child call is not promissory_note::transfer_v1 (0x04)");
                return Err(AuctionError::InvalidChildCall.into())
            }
            // Validate child call targets promissory_note (prevent cross-contract routing)
            let info_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_INFO_TREE)?;
            let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
                .ok_or(AuctionError::InvalidChildCall)?;
            let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
            // HAZOP H-11: fail-closed — reject if promissory_note not configured
            if promissory_note_cid == ContractId::ZERO {
                return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
            }
            validate_child_contract_id(&calls[child_idx].data.contract_id, &promissory_note_cid)?;
            let params= ClaimWinningsParamsV1::decode(&self_.data[1..])?;
            claim_winnings_v1(cid, params, &calls[child_idx].data.data)
        }
        AuctionFunction::SettleAuctionV1 => {
            // Validate promissory_note::transfer_v1 child call for seller payout
            let this_call = &calls[call_idx];
            if this_call.children_indexes.len() != 1 {
                msg!("[SettleAuctionV1] Expected 1 child call (promissory_note::transfer_v1)");
                return Err(AuctionError::InvalidChildrenIndexes.into())
            }
            let child_idx = this_call.children_indexes[0];
            if calls[child_idx].data.data[0] != 0x04 {
                msg!("[SettleAuctionV1] Child call is not promissory_note::transfer_v1 (0x04)");
                return Err(AuctionError::InvalidChildCall.into())
            }
            // Validate child call targets promissory_note (prevent cross-contract routing)
            let info_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_INFO_TREE)?;
            let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
                .ok_or(AuctionError::InvalidChildCall)?;
            let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
            // HAZOP H-11: fail-closed — reject if promissory_note not configured
            if promissory_note_cid == ContractId::ZERO {
                return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
            }
            validate_child_contract_id(&calls[child_idx].data.contract_id, &promissory_note_cid)?;
            let params= SettleAuctionParamsV1::decode(&self_.data[1..])?;
            settle_auction_v1(cid, params, &calls[child_idx].data.data)
        }
        AuctionFunction::RefundBidV1 => {
            // Validate promissory_note::transfer_v1 child call for bid refund
            let this_call = &calls[call_idx];
            if this_call.children_indexes.len() != 1 {
                msg!("[RefundBidV1] Expected 1 child call (promissory_note::transfer_v1)");
                return Err(AuctionError::InvalidChildrenIndexes.into())
            }
            let child_idx = this_call.children_indexes[0];
            if calls[child_idx].data.data[0] != 0x04 {
                msg!("[RefundBidV1] Child call is not promissory_note::transfer_v1 (0x04)");
                return Err(AuctionError::InvalidChildCall.into())
            }
            // Validate child call targets promissory_note (prevent cross-contract routing)
            let info_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_INFO_TREE)?;
            let promissory_note_bytes = wasm::db::db_get(info_db, PROMISSORY_NOTE_CONTRACT_ID_KEY)?
                .ok_or(AuctionError::InvalidChildCall)?;
            let promissory_note_cid: ContractId = deserialize(&promissory_note_bytes)?;
            // HAZOP H-11: fail-closed — reject if promissory_note not configured
            if promissory_note_cid == ContractId::ZERO {
                return Err(ContractError::IoError("promissory_note contract ID not configured".into()));
            }
            validate_child_contract_id(&calls[child_idx].data.contract_id, &promissory_note_cid)?;
            let params= RefundBidParamsV1::decode(&self_.data[1..])?;
            refund_bid_v1(cid, params, &calls[child_idx].data.data)
        }
    }
}

/// CreateAuctionV1 instruction
fn create_auction_v1(cid: ContractId, params: CreateAuctionParamsV1) -> ContractResult {
    msg!("[auction::create_auction_v1] Creating auction: {:?}", params.auction_id);

    // Verify the auction doesn't already exist
    let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let existing_data = wasm::db::db_get(auctions_db, &params.auction_id.to_repr())?;
    let existing: Option<Auction> = match existing_data {
        Some(data) => Some(Auction::decode(&data)?),
        None => None,
    };
    if existing.is_some() {
        msg!("[auction::create_auction_v1] ERROR: Auction already exists");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Create new auction
    let auction = Auction {
        version: 1,
        id: params.auction_id,
        seller_pubkey: params.seller_pubkey,
        item_commitment: params.item_commitment,
        reserve_price: params.reserve_price,
        token_id: params.token_id,
        deadline_block: params.deadline_block,
        state: AuctionState::Created,
        highest_bid: None,
        highest_bidder: None,
        highest_bid_id: None,
        bid_count: 0,
        created_at: current_block,
        instance_seed: params.instance_seed,
    };

    // Build update for apply phase — the actual DB write happens in process_update.
    let update = CreateAuctionUpdateV1 {
        auction_id: params.auction_id,
        auction,
    };

    msg!("[auction::create_auction_v1] Auction creation computed");
    wasm::util::set_return_data(&encode_create_auction_update_v1(&update))
}

/// PlaceBidV1 instruction
fn place_bid_v1(cid: ContractId, params: PlaceBidParamsV1) -> ContractResult {
    msg!("[auction::place_bid_v1] Placing bid: {:?}", params.bid_id);

    // Verify the auction exists and is active
    let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction_data = wasm::db::db_get(auctions_db, &params.auction_id.to_repr())?;
    let mut auction: Auction = match auction_data {
        Some(a) => Auction::decode(&a)?,
        None => {
            msg!("[auction::place_bid_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify auction is accepting bids
    if auction.state != AuctionState::Created && auction.state != AuctionState::Active {
        msg!("[auction::place_bid_v1] ERROR: Auction not accepting bids");
        return Err(ContractError::InvalidFunction.into())
    }

    // Get current block
    let current_block = wasm::util::get_verifying_block_height()?.get();

    // Verify auction hasn't ended
    if current_block >= auction.deadline_block {
        msg!("[auction::place_bid_v1] ERROR: Auction has ended");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify bid > current highest bid
    if let Some(highest_bid) = auction.highest_bid {
        if params.amount <= highest_bid {
            msg!("[auction::place_bid_v1] ERROR: Bid too low");
            return Err(ContractError::InvalidFunction.into())
        }
    }

    // Verify bid >= reserve price
    if params.amount < auction.reserve_price {
        msg!("[auction::place_bid_v1] ERROR: Below reserve price");
        return Err(ContractError::InvalidFunction.into())
    }

    // If there was a previous highest bid, it will be marked as outbid in apply.
    let (prev_bid_id_opt, prev_bid_opt) = if let Some(prev_bid_id) = auction.highest_bid_id {
        let bids_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_BIDS_TREE)?;
        let prev_bid_data = wasm::db::db_get(bids_db, &prev_bid_id.to_repr())?;
        let mut prev_bid: Bid = match prev_bid_data {
            Some(b) => Bid::decode(&b)?,
            None => {
                msg!("[auction::place_bid_v1] ERROR: Previous bid not found");
                return Err(ContractError::InvalidFunction.into())
            }
        };
        prev_bid.state = BidState::Outbid;
        (Some(prev_bid_id), Some(prev_bid))
    } else {
        (None, None)
    };

    // Create new bid (write deferred to apply phase)
    let bid = Bid {
        version: 1,
        id: params.bid_id,
        auction_id: params.auction_id,
        bidder_pubkey: params.bidder_pubkey,
        amount: params.amount,
        escrow_id: params.escrow_id,
        state: BidState::Active,
        created_at: current_block,
        instance_seed: params.instance_seed,
    };

    // Update auction state in memory (write deferred to apply phase)
    auction.state = AuctionState::Active;
    auction.highest_bid = Some(params.amount);
    auction.highest_bidder = Some(params.bidder_pubkey);
    auction.highest_bid_id = Some(params.bid_id);
    auction.bid_count += 1;

    // Build update for apply phase — all DB writes happen there.
    let update = PlaceBidUpdateV1 {
        auction_id: params.auction_id,
        highest_bid: params.amount,
        highest_bidder: params.bidder_pubkey,
        highest_bid_id: params.bid_id,
        auction,
        bid,
        prev_bid_id: prev_bid_id_opt,
        prev_bid: prev_bid_opt,
    };

    msg!("[auction::place_bid_v1] Bid computed");
    wasm::util::set_return_data(&encode_place_bid_update_v1(&update))
}

/// CloseAuctionV1 instruction
fn close_auction_v1(cid: ContractId, params: CloseAuctionParamsV1) -> ContractResult {
    msg!("[auction::close_auction_v1] Closing auction: {:?}", params.auction_id);

    // Verify the auction exists
    let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction_data = wasm::db::db_get(auctions_db, &params.auction_id.to_repr())?;
    let mut auction: Auction = match auction_data {
        Some(a) => Auction::decode(&a)?,
        None => {
            msg!("[auction::close_auction_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify seller is the caller
    if auction.seller_pubkey != params.seller_pubkey {
        msg!("[auction::close_auction_v1] ERROR: Not seller");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify auction is active
    if auction.state != AuctionState::Active {
        msg!("[auction::close_auction_v1] ERROR: Auction not active");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update auction state in memory (write deferred to apply phase)
    auction.state = AuctionState::Closed;

    // Mark winning bid as won in memory (write deferred to apply phase)
    let (winner_bid_id, winner_bid_opt) =
        if let Some(wbid_id) = auction.highest_bid_id {
            let bids_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_BIDS_TREE)?;
            let winner_bid_data = wasm::db::db_get(bids_db, &wbid_id.to_repr())?;
            let mut winner_bid: Bid = match winner_bid_data {
                Some(b) => Bid::decode(&b)?,
                None => {
                    msg!("[auction::close_auction_v1] ERROR: Winner bid not found");
                    return Err(ContractError::InvalidFunction.into())
                }
            };
            winner_bid.state = BidState::Won;
            (wbid_id, Some(winner_bid))
        } else {
            (pasta::pallas::Base::from(0u64), None)
        };

    let update = CloseAuctionUpdateV1 {
        auction_id: params.auction_id,
        winner_bid_id,
        auction,
        winner_bid: winner_bid_opt,
    };

    msg!("[auction::close_auction_v1] Auction close computed");
    wasm::util::set_return_data(&encode_close_auction_update_v1(&update))
}

/// ClaimWinningsV1 instruction
fn claim_winnings_v1(cid: ContractId, params: ClaimWinningsParamsV1, child_call_data: &[u8]) -> ContractResult {
    msg!("[auction::claim_winnings_v1] Claiming winnings: {:?}", params.auction_id);

    // Verify the auction exists and is closed
    let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction_data = wasm::db::db_get(auctions_db, &params.auction_id.to_repr())?;
    let auction: Auction = match auction_data {
        Some(a) => Auction::decode(&a)?,
        None => {
            msg!("[auction::claim_winnings_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify auction is closed
    if auction.state != AuctionState::Closed {
        msg!("[auction::claim_winnings_v1] ERROR: Auction not closed");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify winner_bid_id matches
    if auction.highest_bid_id != Some(params.winner_bid_id) {
        msg!("[auction::claim_winnings_v1] ERROR: Winner bid mismatch");
        return Err(ContractError::InvalidFunction.into())
    }

    // Validate child transfer amount using value_commit comparison
    if let Some(highest_bid) = auction.highest_bid {
        let value_blind = poseidon_hash([
            pallas::Base::from(highest_bid),
            params.auction_id,
        ]);
        validate_child_value_commit(child_call_data, highest_bid, value_blind)?;
    }

    msg!("[auction::claim_winnings_v1] Winnings claimed successfully");
    Ok(())
}

/// SettleAuctionV1 instruction
fn settle_auction_v1(cid: ContractId, params: SettleAuctionParamsV1, child_call_data: &[u8]) -> ContractResult {
    msg!("[auction::settle_auction_v1] Settling auction: {:?}", params.auction_id);

    // Verify the auction exists and is closed
    let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction_data = wasm::db::db_get(auctions_db, &params.auction_id.to_repr())?;
    let mut auction: Auction = match auction_data {
        Some(a) => Auction::decode(&a)?,
        None => {
            msg!("[auction::settle_auction_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify seller is the caller
    if auction.seller_pubkey != params.seller_pubkey {
        msg!("[auction::settle_auction_v1] ERROR: Not seller");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify auction is closed
    if auction.state != AuctionState::Closed {
        msg!("[auction::settle_auction_v1] ERROR: Auction not closed");
        return Err(ContractError::InvalidFunction.into())
    }

    // SECURITY FIX: Verify settlement nullifier hasn't been used
    let nullifiers_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_NULLIFIERS_TREE)?;
    if wasm::db::db_contains_key(nullifiers_db, &params.settlement_nullifier.to_repr())? {
        msg!("[auction::settle_auction_v1] ERROR: Already settled");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update auction state in memory (write deferred to apply phase)
    auction.state = AuctionState::Settled;

    // Validate child transfer amount using value_commit comparison
    if let Some(highest_bid) = auction.highest_bid {
        let value_blind = poseidon_hash([
            pallas::Base::from(highest_bid),
            params.auction_id,
        ]);
        validate_child_value_commit(child_call_data, highest_bid, value_blind)?;
    }

    let update = SettleAuctionUpdateV1 {
        auction_id: params.auction_id,
        settlement_nullifier: params.settlement_nullifier,
        auction,
    };

    msg!("[auction::settle_auction_v1] Settlement computed");
    wasm::util::set_return_data(&encode_settle_auction_update_v1(&update))
}

/// RefundBidV1 instruction
fn refund_bid_v1(cid: ContractId, params: RefundBidParamsV1, child_call_data: &[u8]) -> ContractResult {
    msg!("[auction::refund_bid_v1] Refunding bid: {:?}", params.bid_id);

    // Verify the bid exists
    let bids_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_BIDS_TREE)?;
    let bid_data = wasm::db::db_get(bids_db, &params.bid_id.to_repr())?;
    let mut bid: Bid = match bid_data {
        Some(b) => Bid::decode(&b)?,
        None => {
            msg!("[auction::refund_bid_v1] ERROR: Bid not found");
            return Err(ContractError::InvalidFunction.into())
        }
    };

    // Verify bidder is the caller
    if bid.bidder_pubkey != params.bidder_pubkey {
        msg!("[auction::refund_bid_v1] ERROR: Not bidder");
        return Err(ContractError::InvalidFunction.into())
    }

    // Verify bid is outbid
    if bid.state != BidState::Outbid {
        msg!("[auction::refund_bid_v1] ERROR: Bid not outbid");
        return Err(ContractError::InvalidFunction.into())
    }

    // Update bid state in memory (write deferred to apply phase)
    bid.state = BidState::Refunded;

    // Validate child transfer amount using value_commit comparison
    let value_blind = poseidon_hash([
        pallas::Base::from(bid.amount),
        params.bid_id,
    ]);
    validate_child_value_commit(child_call_data, bid.amount, value_blind)?;

    let update = RefundBidUpdateV1 {
        bid_id: params.bid_id,
        refund_nullifier: params.refund_nullifier,
        bid,
    };

    msg!("[auction::refund_bid_v1] Refund computed");
    wasm::util::set_return_data(&encode_refund_bid_update_v1(&update))
}

// ============================================================================
// RHO-CALCULUS EXPLICIT BRIDGE ENCODE/DECODE
// ============================================================================

fn encode_create_auction_update_v1(update: &CreateAuctionUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(AuctionFunction::CreateAuctionV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_create_auction_update_v1(data: &[u8]) -> Result<CreateAuctionUpdateV1, ContractError> {
    CreateAuctionUpdateV1::decode(data)
}

fn encode_place_bid_update_v1(update: &PlaceBidUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(AuctionFunction::PlaceBidV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_place_bid_update_v1(data: &[u8]) -> Result<PlaceBidUpdateV1, ContractError> {
    PlaceBidUpdateV1::decode(data)
}

fn encode_close_auction_update_v1(update: &CloseAuctionUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(AuctionFunction::CloseAuctionV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_close_auction_update_v1(data: &[u8]) -> Result<CloseAuctionUpdateV1, ContractError> {
    CloseAuctionUpdateV1::decode(data)
}

#[allow(dead_code)]
fn encode_claim_winnings_update_v1(update: &ClaimWinningsUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(AuctionFunction::ClaimWinningsV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_claim_winnings_update_v1(data: &[u8]) -> Result<ClaimWinningsUpdateV1, ContractError> {
    ClaimWinningsUpdateV1::decode(data)
}

fn encode_settle_auction_update_v1(update: &SettleAuctionUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(AuctionFunction::SettleAuctionV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_settle_auction_update_v1(data: &[u8]) -> Result<SettleAuctionUpdateV1, ContractError> {
    SettleAuctionUpdateV1::decode(data)
}

fn encode_refund_bid_update_v1(update: &RefundBidUpdateV1) -> Vec<u8> {
    let inner = update.encode();
    let mut buf = Vec::with_capacity(1 + inner.len());
    buf.push(AuctionFunction::RefundBidV1 as u8);
    buf.extend_from_slice(&inner);
    buf
}

fn decode_refund_bid_update_v1(data: &[u8]) -> Result<RefundBidUpdateV1, ContractError> {
    RefundBidUpdateV1::decode(data)
}

// ============================================================================
// STATE UPDATES
// ============================================================================

/// Apply state updates
fn process_update(cid: ContractId, update_data: &[u8]) -> ContractResult {
    match AuctionFunction::try_from(update_data[0])? {
        AuctionFunction::CreateAuctionV1 => {
            let update = decode_create_auction_update_v1(&update_data[1..])?;
            let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
            wasm::db::db_set(auctions_db, &update.auction_id.to_repr(), &update.auction.encode())?;
            msg!("[auction::process_update] CreateAuction written: {:?}", update.auction_id);
            Ok(())
        }
        AuctionFunction::PlaceBidV1 => {
            let update = decode_place_bid_update_v1(&update_data[1..])?;
            let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
            let bids_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_BIDS_TREE)?;
            // Mark previous bid as outbid if one existed
            if let (Some(prev_id), Some(prev_bid)) = (update.prev_bid_id, &update.prev_bid) {
                wasm::db::db_set(bids_db, &prev_id.to_repr(), &prev_bid.encode())?;
            }
            // Store new bid
            wasm::db::db_set(bids_db, &update.highest_bid_id.to_repr(), &update.bid.encode())?;
            // Update auction
            wasm::db::db_set(auctions_db, &update.auction_id.to_repr(), &update.auction.encode())?;
            msg!("[auction::process_update] PlaceBid written: {:?}", update.auction_id);
            Ok(())
        }
        AuctionFunction::CloseAuctionV1 => {
            let update = decode_close_auction_update_v1(&update_data[1..])?;
            let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
            wasm::db::db_set(auctions_db, &update.auction_id.to_repr(), &update.auction.encode())?;
            if let Some(ref winner_bid) = update.winner_bid {
                let bids_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_BIDS_TREE)?;
                wasm::db::db_set(bids_db, &update.winner_bid_id.to_repr(), &winner_bid.encode())?;
            }
            msg!("[auction::process_update] CloseAuction written: {:?}", update.auction_id);
            Ok(())
        }
        AuctionFunction::ClaimWinningsV1 => {
            let update = decode_claim_winnings_update_v1(&update_data[1..])?;
            msg!("[auction::process_update] ClaimWinnings: {:?}", update.auction_id);
            Ok(())
        }
        AuctionFunction::SettleAuctionV1 => {
            let update = decode_settle_auction_update_v1(&update_data[1..])?;
            let auctions_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
            let nullifiers_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_NULLIFIERS_TREE)?;
            wasm::db::db_set(auctions_db, &update.auction_id.to_repr(), &update.auction.encode())?;
            wasm::db::db_set(nullifiers_db, &update.settlement_nullifier.to_repr(), &[])?;
            msg!("[auction::process_update] SettleAuction written: {:?}", update.auction_id);
            Ok(())
        }
        AuctionFunction::RefundBidV1 => {
            let update = decode_refund_bid_update_v1(&update_data[1..])?;
            let bids_db = wasm::db::db_lookup(cid, AUCTION_CONTRACT_BIDS_TREE)?;
            wasm::db::db_set(bids_db, &update.bid_id.to_repr(), &update.bid.encode())?;
            msg!("[auction::process_update] RefundBid written: {:?}", update.bid_id);
            Ok(())
        }
    }
}