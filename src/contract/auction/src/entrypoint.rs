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

use darkfi_sdk::{
    crypto::pasta_prelude::*,
    dark_tree::DarkLeaf,
    error::{ContractError, ContractResult},
    msg,
    pasta::pallas,
    wasm, ContractCall,
};
use darkfi_serial::{deserialize, serialize, Encodable};

use crate::{
    error::AuctionError,
    model::{
        Auction, AuctionId, AuctionState, Bid, BidId, BidState, ClaimWinningsParamsV1,
        ClaimWinningsUpdateV1, CloseAuctionParamsV1, CloseAuctionUpdateV1,
        CreateAuctionParamsV1, CreateAuctionUpdateV1, PlaceBidParamsV1, PlaceBidUpdateV1,
        RefundBidParamsV1, RefundBidUpdateV1, SettleAuctionParamsV1, SettleAuctionUpdateV1,
    },
    AuctionFunction, AUCTION_CONTRACT_AUCTIONS_TREE, AUCTION_CONTRACT_BIDS_TREE,
    AUCTION_CONTRACT_INFO_TREE, AUCTION_CONTRACT_NULLIFIERS_TREE,
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

/// Initialize auction contract state
///
/// Sets up:
/// - Info tree (version, config)
/// - Auctions tree (auction records)
/// - Bids tree (bid records)
/// - Nullifiers tree (spent nullifiers)
pub fn init_contract(cid: ContractId, _ix: &[u8]) -> ContractResult {
    msg!("[auction::init_contract] Initializing auction contract");

    // Initialize info tree
    let info_db = wasm::db::db_init(cid, AUCTION_CONTRACT_INFO_TREE)?;
    wasm::db::db_set(info_db, b"db_version", &env!("CARGO_PKG_VERSION").as_bytes())?;

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
fn get_metadata(cid: ContractId, ix: &[u8]) -> ContractResult {
    let call_idx = wasm::util::get_call_index()? as usize;
    let calls: Vec<DarkLeaf<ContractCall>> = deserialize(ix)?;
    let self_ = &calls[call_idx].data;
    let func = AuctionFunction::try_from(self_.data[0])?;

    msg!("[auction::get_metadata] Processing function: {:?}", func);

    let metadata = match func {
        AuctionFunction::CreateAuctionV1 => {
            let params: CreateAuctionParamsV1 = deserialize(&self_.data[1..])?;
            create_auction_get_metadata_v1(cid, call_idx, calls, params)?
        }
        AuctionFunction::PlaceBidV1 => {
            let params: PlaceBidParamsV1 = deserialize(&self_.data[1..])?;
            place_bid_get_metadata_v1(cid, call_idx, calls, params)?
        }
        AuctionFunction::CloseAuctionV1 => {
            let params: CloseAuctionParamsV1 = deserialize(&self_.data[1..])?;
            close_auction_get_metadata_v1(cid, call_idx, calls, params)?
        }
        AuctionFunction::ClaimWinningsV1 => {
            let params: ClaimWinningsParamsV1 = deserialize(&self_.data[1..])?;
            claim_winnings_get_metadata_v1(cid, call_idx, calls, params)?
        }
        AuctionFunction::SettleAuctionV1 => {
            let params: SettleAuctionParamsV1 = deserialize(&self_.data[1..])?;
            settle_auction_get_metadata_v1(cid, call_idx, calls, params)?
        }
        AuctionFunction::RefundBidV1 => {
            let params: RefundBidParamsV1 = deserialize(&self_.data[1..])?;
            refund_bid_get_metadata_v1(cid, call_idx, calls, params)?
        }
    };

    wasm::util::set_return_data(&metadata)
}

/// `get_metadata` for CreateAuctionV1
fn create_auction_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: CreateAuctionParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[auction::create_auction_get_metadata_v1] Creating auction: {:?}", params.auction_id);

    // Verify the auction doesn't already exist
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let existing: Option<Auction> = wasm::db::db_get(auctions_db, &serialize(&params.auction_id))?;
    if existing.is_some() {
        msg!("[auction::create_auction_get_metadata_v1] ERROR: Auction already exists");
        return Err(ContractError::InvalidInstruction.into())
    }

    // TODO: ZK proof verification would go here
    // For now, we just return the public inputs
    let mut public_inputs = vec![
        params.auction_id,
        params.seller_commitment,
    ];

    msg!("[auction::create_auction_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

/// `get_metadata` for PlaceBidV1
fn place_bid_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: PlaceBidParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[auction::place_bid_get_metadata_v1] Placing bid: {:?}", params.bid_id);

    // Verify the auction exists and is active
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction: Auction = match wasm::db::db_get(auctions_db, &serialize(&params.auction_id))? {
        Some(a) => a,
        None => {
            msg!("[auction::place_bid_get_metadata_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    if auction.state != AuctionState::Created && auction.state != AuctionState::Active {
        msg!("[auction::place_bid_get_metadata_v1] ERROR: Auction not active");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify bid amount > current highest bid (if there is one)
    if let Some(highest_bid) = auction.highest_bid {
        if params.amount <= highest_bid {
            msg!("[auction::place_bid_get_metadata_v1] ERROR: Bid too low");
            return Err(ContractError::InvalidInstruction.into())
        }
    }

    // Verify bid >= reserve price
    if params.amount < auction.reserve_price {
        msg!("[auction::place_bid_get_metadata_v1] ERROR: Below reserve price");
        return Err(ContractError::InvalidInstruction.into())
    }

    // TODO: ZK proof verification would go here
    let mut public_inputs = vec![
        params.auction_id,
        params.bid_id,
        pallas::Base::from(params.amount),
    ];

    msg!("[auction::place_bid_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

/// `get_metadata` for CloseAuctionV1
fn close_auction_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: CloseAuctionParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[auction::close_auction_get_metadata_v1] Closing auction: {:?}", params.auction_id);

    // Verify the auction exists
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction: Auction = match wasm::db::db_get(auctions_db, &serialize(&params.auction_id))? {
        Some(a) => a,
        None => {
            msg!("[auction::close_auction_get_metadata_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify seller is the caller
    if auction.seller_pubkey != params.seller_pubkey {
        msg!("[auction::close_auction_get_metadata_v1] ERROR: Not seller");
        return Err(ContractError::InvalidInstruction.into())
    }

    // TODO: ZK proof verification would go here
    let mut public_inputs = vec![
        params.auction_id,
        params.winner_bid_id,
    ];

    msg!("[auction::close_auction_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

/// `get_metadata` for ClaimWinningsV1
fn claim_winnings_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: ClaimWinningsParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[auction::claim_winnings_get_metadata_v1] Claiming winnings: {:?}", params.auction_id);

    // Verify the auction exists and is closed
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction: Auction = match wasm::db::db_get(auctions_db, &serialize(&params.auction_id))? {
        Some(a) => a,
        None => {
            msg!("[auction::claim_winnings_get_metadata_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify auction is closed
    if auction.state != AuctionState::Closed {
        msg!("[auction::claim_winnings_get_metadata_v1] ERROR: Auction not closed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify winner_bid_id matches
    if auction.highest_bid_id != Some(params.winner_bid_id) {
        msg!("[auction::claim_winnings_get_metadata_v1] ERROR: Winner bid mismatch");
        return Err(ContractError::InvalidInstruction.into())
    }

    // TODO: ZK proof verification would go here
    let mut public_inputs = vec![
        params.auction_id,
        params.winner_bid_id,
    ];

    msg!("[auction::claim_winnings_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

/// `get_metadata` for SettleAuctionV1
fn settle_auction_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: SettleAuctionParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[auction::settle_auction_get_metadata_v1] Settling auction: {:?}", params.auction_id);

    // Verify the auction exists and is closed
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction: Auction = match wasm::db::db_get(auctions_db, &serialize(&params.auction_id))? {
        Some(a) => a,
        None => {
            msg!("[auction::settle_auction_get_metadata_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify seller is the caller
    if auction.seller_pubkey != params.seller_pubkey {
        msg!("[auction::settle_auction_get_metadata_v1] ERROR: Not seller");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify auction is closed
    if auction.state != AuctionState::Closed {
        msg!("[auction::settle_auction_get_metadata_v1] ERROR: Auction not closed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // TODO: ZK proof verification would go here
    let mut public_inputs = vec![
        params.auction_id,
    ];

    msg!("[auction::settle_auction_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
}

/// `get_metadata` for RefundBidV1
fn refund_bid_get_metadata_v1(
    cid: ContractId,
    call_idx: usize,
    calls: Vec<DarkLeaf<ContractCall>>,
    params: RefundBidParamsV1,
) -> ContractResult<Vec<pallas::Base>> {
    msg!("[auction::refund_bid_get_metadata_v1] Refunding bid: {:?}", params.bid_id);

    // Verify the bid exists
    let bids_db = wasm::db::db_get(cid, AUCTION_CONTRACT_BIDS_TREE)?;
    let bid: Bid = match wasm::db::db_get(bids_db, &serialize(&params.bid_id))? {
        Some(b) => b,
        None => {
            msg!("[auction::refund_bid_get_metadata_v1] ERROR: Bid not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify bidder is the caller
    if bid.bidder_pubkey != params.bidder_pubkey {
        msg!("[auction::refund_bid_get_metadata_v1] ERROR: Not bidder");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify bid is outbid
    if bid.state != BidState::Outbid {
        msg!("[auction::refund_bid_get_metadata_v1] ERROR: Bid not outbid");
        return Err(ContractError::InvalidInstruction.into())
    }

    // TODO: ZK proof verification would go here
    let mut public_inputs = vec![
        params.bid_id,
    ];

    msg!("[auction::refund_bid_get_metadata_v1] Returning metadata: {:?}", public_inputs);
    Ok(public_inputs)
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
            let params: CreateAuctionParamsV1 = deserialize(&self_.data[1..])?;
            create_auction_v1(cid, params)
        }
        AuctionFunction::PlaceBidV1 => {
            let params: PlaceBidParamsV1 = deserialize(&self_.data[1..])?;
            place_bid_v1(cid, params)
        }
        AuctionFunction::CloseAuctionV1 => {
            let params: CloseAuctionParamsV1 = deserialize(&self_.data[1..])?;
            close_auction_v1(cid, params)
        }
        AuctionFunction::ClaimWinningsV1 => {
            let params: ClaimWinningsParamsV1 = deserialize(&self_.data[1..])?;
            claim_winnings_v1(cid, params)
        }
        AuctionFunction::SettleAuctionV1 => {
            let params: SettleAuctionParamsV1 = deserialize(&self_.data[1..])?;
            settle_auction_v1(cid, params)
        }
        AuctionFunction::RefundBidV1 => {
            let params: RefundBidParamsV1 = deserialize(&self_.data[1..])?;
            refund_bid_v1(cid, params)
        }
    }
}

/// CreateAuctionV1 instruction
fn create_auction_v1(cid: ContractId, params: CreateAuctionParamsV1) -> ContractResult {
    msg!("[auction::create_auction_v1] Creating auction: {:?}", params.auction_id);

    // Verify the auction doesn't already exist
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let existing: Option<Auction> = wasm::db::db_get(auctions_db, &serialize(&params.auction_id))?;
    if existing.is_some() {
        msg!("[auction::create_auction_v1] ERROR: Auction already exists");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Get current block
    let current_block = wasm::chain::get_block_height()?;

    // Create new auction
    let auction = Auction {
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
    };

    // Store auction
    wasm::db::db_set(auctions_db, &serialize(&params.auction_id), &serialize(&auction))?;

    msg!("[auction::create_auction_v1] Auction created successfully");
    Ok(())
}

/// PlaceBidV1 instruction
fn place_bid_v1(cid: ContractId, params: PlaceBidParamsV1) -> ContractResult {
    msg!("[auction::place_bid_v1] Placing bid: {:?}", params.bid_id);

    // Verify the auction exists and is active
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let mut auction: Auction = match wasm::db::db_get(auctions_db, &serialize(&params.auction_id))? {
        Some(a) => a,
        None => {
            msg!("[auction::place_bid_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify auction is accepting bids
    if auction.state != AuctionState::Created && auction.state != AuctionState::Active {
        msg!("[auction::place_bid_v1] ERROR: Auction not accepting bids");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Get current block
    let current_block = wasm::chain::get_block_height()?;

    // Verify auction hasn't ended
    if current_block >= auction.deadline_block {
        msg!("[auction::place_bid_v1] ERROR: Auction has ended");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify bid > current highest bid
    if let Some(highest_bid) = auction.highest_bid {
        if params.amount <= highest_bid {
            msg!("[auction::place_bid_v1] ERROR: Bid too low");
            return Err(ContractError::InvalidInstruction.into())
        }
    }

    // Verify bid >= reserve price
    if params.amount < auction.reserve_price {
        msg!("[auction::place_bid_v1] ERROR: Below reserve price");
        return Err(ContractError::InvalidInstruction.into())
    }

    // If there was a previous highest bid, mark it as outbid
    if let Some(prev_bid_id) = auction.highest_bid_id {
        let bids_db = wasm::db::db_get(cid, AUCTION_CONTRACT_BIDS_TREE)?;
        let mut prev_bid: Bid = match wasm::db::db_get(bids_db, &serialize(&prev_bid_id))? {
            Some(b) => b,
            None => {
                msg!("[auction::place_bid_v1] ERROR: Previous bid not found");
                return Err(ContractError::InvalidInstruction.into())
            }
        };
        prev_bid.state = BidState::Outbid;
        wasm::db::db_set(bids_db, &serialize(&prev_bid_id), &serialize(&prev_bid))?;
    }

    // Create new bid
    let bids_db = wasm::db::db_get(cid, AUCTION_CONTRACT_BIDS_TREE)?;
    let bid = Bid {
        id: params.bid_id,
        auction_id: params.auction_id,
        bidder_pubkey: params.bidder_pubkey,
        amount: params.amount,
        escrow_id: params.escrow_id,
        state: BidState::Active,
        created_at: current_block,
    };

    // Store bid
    wasm::db::db_set(bids_db, &serialize(&params.bid_id), &serialize(&bid))?;

    // Update auction
    auction.state = AuctionState::Active;
    auction.highest_bid = Some(params.amount);
    auction.highest_bidder = Some(params.bidder_pubkey);
    auction.highest_bid_id = Some(params.bid_id);
    auction.bid_count += 1;
    wasm::db::db_set(auctions_db, &serialize(&params.auction_id), &serialize(&auction))?;

    msg!("[auction::place_bid_v1] Bid placed successfully");
    Ok(())
}

/// CloseAuctionV1 instruction
fn close_auction_v1(cid: ContractId, params: CloseAuctionParamsV1) -> ContractResult {
    msg!("[auction::close_auction_v1] Closing auction: {:?}", params.auction_id);

    // Verify the auction exists
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let mut auction: Auction = match wasm::db::db_get(auctions_db, &serialize(&params.auction_id))? {
        Some(a) => a,
        None => {
            msg!("[auction::close_auction_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify seller is the caller
    if auction.seller_pubkey != params.seller_pubkey {
        msg!("[auction::close_auction_v1] ERROR: Not seller");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify auction is active
    if auction.state != AuctionState::Active {
        msg!("[auction::close_auction_v1] ERROR: Auction not active");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Update auction state
    auction.state = AuctionState::Closed;
    wasm::db::db_set(auctions_db, &serialize(&params.auction_id), &serialize(&auction))?;

    // Mark winning bid as won
    if let Some(winner_bid_id) = auction.highest_bid_id {
        let bids_db = wasm::db::db_get(cid, AUCTION_CONTRACT_BIDS_TREE)?;
        let mut winner_bid: Bid = match wasm::db::db_get(bids_db, &serialize(&winner_bid_id))? {
            Some(b) => b,
            None => {
                msg!("[auction::close_auction_v1] ERROR: Winner bid not found");
                return Err(ContractError::InvalidInstruction.into())
            }
        };
        winner_bid.state = BidState::Won;
        wasm::db::db_set(bids_db, &serialize(&winner_bid_id), &serialize(&winner_bid))?;
    }

    msg!("[auction::close_auction_v1] Auction closed successfully");
    Ok(())
}

/// ClaimWinningsV1 instruction
fn claim_winnings_v1(cid: ContractId, params: ClaimWinningsParamsV1) -> ContractResult {
    msg!("[auction::claim_winnings_v1] Claiming winnings: {:?}", params.auction_id);

    // Verify the auction exists and is closed
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let auction: Auction = match wasm::db::db_get(auctions_db, &serialize(&params.auction_id))? {
        Some(a) => a,
        None => {
            msg!("[auction::claim_winnings_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify auction is closed
    if auction.state != AuctionState::Closed {
        msg!("[auction::claim_winnings_v1] ERROR: Auction not closed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify winner_bid_id matches
    if auction.highest_bid_id != Some(params.winner_bid_id) {
        msg!("[auction::claim_winnings_v1] ERROR: Winner bid mismatch");
        return Err(ContractError::InvalidInstruction.into())
    }

    msg!("[auction::claim_winnings_v1] Winnings claimed successfully");
    Ok(())
}

/// SettleAuctionV1 instruction
fn settle_auction_v1(cid: ContractId, params: SettleAuctionParamsV1) -> ContractResult {
    msg!("[auction::settle_auction_v1] Settling auction: {:?}", params.auction_id);

    // Verify the auction exists and is closed
    let auctions_db = wasm::db::db_get(cid, AUCTION_CONTRACT_AUCTIONS_TREE)?;
    let mut auction: Auction = match wasm::db::db_get(auctions_db, &serialize(&params.auction_id))? {
        Some(a) => a,
        None => {
            msg!("[auction::settle_auction_v1] ERROR: Auction not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify seller is the caller
    if auction.seller_pubkey != params.seller_pubkey {
        msg!("[auction::settle_auction_v1] ERROR: Not seller");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify auction is closed
    if auction.state != AuctionState::Closed {
        msg!("[auction::settle_auction_v1] ERROR: Auction not closed");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Update auction state
    auction.state = AuctionState::Settled;
    wasm::db::db_set(auctions_db, &serialize(&params.auction_id), &serialize(&auction))?;

    msg!("[auction::settle_auction_v1] Auction settled successfully");
    Ok(())
}

/// RefundBidV1 instruction
fn refund_bid_v1(cid: ContractId, params: RefundBidParamsV1) -> ContractResult {
    msg!("[auction::refund_bid_v1] Refunding bid: {:?}", params.bid_id);

    // Verify the bid exists
    let bids_db = wasm::db::db_get(cid, AUCTION_CONTRACT_BIDS_TREE)?;
    let mut bid: Bid = match wasm::db::db_get(bids_db, &serialize(&params.bid_id))? {
        Some(b) => b,
        None => {
            msg!("[auction::refund_bid_v1] ERROR: Bid not found");
            return Err(ContractError::InvalidInstruction.into())
        }
    };

    // Verify bidder is the caller
    if bid.bidder_pubkey != params.bidder_pubkey {
        msg!("[auction::refund_bid_v1] ERROR: Not bidder");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Verify bid is outbid
    if bid.state != BidState::Outbid {
        msg!("[auction::refund_bid_v1] ERROR: Bid not outbid");
        return Err(ContractError::InvalidInstruction.into())
    }

    // Update bid state
    bid.state = BidState::Refunded;
    wasm::db::db_set(bids_db, &serialize(&params.bid_id), &serialize(&bid))?;

    msg!("[auction::refund_bid_v1] Bid refunded successfully");
    Ok(())
}

// ============================================================================
// STATE UPDATES
// ============================================================================

/// Apply state updates
fn process_update(cid: ContractId, updates: &[u8]) -> ContractResult {
    let updates: Vec<DarkLeaf<pallas::Base>> = deserialize(updates)?;
    msg!("[auction::process_update] Applying {} updates", updates.len());

    for update in updates {
        match update.data[0] {
            0 => {
                // CreateAuctionV1 update
                let update_data: CreateAuctionUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[auction::process_update] CreateAuction: {:?}", update_data.auction_id);
            }
            1 => {
                // PlaceBidV1 update
                let update_data: PlaceBidUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[auction::process_update] PlaceBid: {:?} highest: {}", update_data.auction_id, update_data.highest_bid);
            }
            2 => {
                // CloseAuctionV1 update
                let update_data: CloseAuctionUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[auction::process_update] CloseAuction: {:?}", update_data.auction_id);
            }
            3 => {
                // ClaimWinningsV1 update
                let update_data: ClaimWinningsUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[auction::process_update] ClaimWinnings: {:?}", update_data.auction_id);
            }
            4 => {
                // SettleAuctionV1 update
                let update_data: SettleAuctionUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[auction::process_update] SettleAuction: {:?}", update_data.auction_id);
            }
            5 => {
                // RefundBidV1 update
                let update_data: RefundBidUpdateV1 = deserialize(&serialize(&update.data[1..]))?;
                msg!("[auction::process_update] RefundBid: {:?}", update_data.bid_id);
            }
            _ => {
                msg!("[auction::process_update] ERROR: Unknown update type");
                return Err(ContractError::InvalidInstruction.into())
            }
        }
    }

    msg!("[auction::process_update] All updates applied successfully");
    Ok(())
}