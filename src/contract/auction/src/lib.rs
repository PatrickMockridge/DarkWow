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

//! DarkFi Auction Contract
//!
//! A privacy-preserving auction contract that uses the escrow contract
//! for bid deposits. Enables sealed-bid or English-style auctions.
//!
//! Trust model:
//! - Seller creates auction with item commitment and reserve price
//! - Bidders place bids (escrowed deposits, refundable if outbid)
//! - Winner claims item, seller receives payment
//! - Outbid bidders get refunds
//!
//! The auction contract COMPOSES with the escrow contract - bid deposits
//! are managed by separate escrow contracts, not built into this contract.

use darkfi_sdk::define_contract_function;

define_contract_function!(AuctionFunction {
    CreateAuctionV1 = 0x00,
    PlaceBidV1 = 0x01,
    CloseAuctionV1 = 0x02,
    ClaimWinningsV1 = 0x03,
    SettleAuctionV1 = 0x04,
    RefundBidV1 = 0x05,
});

/// Internal contract errors
pub mod error;

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// These are the different sled trees that will be created
pub const AUCTION_CONTRACT_AUCTIONS_TREE: &str = "auctions";
pub const AUCTION_CONTRACT_BIDS_TREE: &str = "bids";
pub const AUCTION_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const AUCTION_CONTRACT_INFO_TREE: &str = "info";

// These are keys inside the info tree
pub const AUCTION_CONTRACT_DB_VERSION: &[u8] = b"db_version";

// zkas circuit namespaces
pub const AUCTION_CONTRACT_ZKAS_CREATE_NS_V1: &str = "CreateAuction_V1";
pub const AUCTION_CONTRACT_ZKAS_PLACE_BID_NS_V1: &str = "PlaceBid_V1";
pub const AUCTION_CONTRACT_ZKAS_CLOSE_NS_V1: &str = "CloseAuction_V1";
pub const AUCTION_CONTRACT_ZKAS_CLAIM_WINNINGS_NS_V1: &str = "ClaimWinnings_V1";
pub const AUCTION_CONTRACT_ZKAS_SETTLE_NS_V1: &str = "SettleAuction_V1";
pub const AUCTION_CONTRACT_ZKAS_REFUND_BID_NS_V1: &str = "RefundBid_V1";