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

//! DarkWow Tender Contract
//!
//! A privacy-preserving sealed-bid tendering system that integrates with:
//! - Identity/Competency framework for skill verification
//! - Labor Market for job execution
//! - Tau for task tracking
//!
//! ## Trust Model
//!
//! - **Requester creates tender** with specifications and requirements
//! - **Workers submit sealed bids** with competency proofs
//! - **Bids revealed** after deadline
//! - **Winner selected** based on competency + price
//! - **Job created** via Labor Market for execution
//! - **Task tracked** via Tau

use dwow_sdk::define_contract_function;

define_contract_function!(TenderFunction {
    CreateTenderV1 = 0x00,
    SubmitBidV1 = 0x01,
    RevealBidV1 = 0x02,
    CloseTenderV1 = 0x03,
    SelectWinnerV1 = 0x04,
    CancelTenderV1 = 0x05,
    RejectBidV1 = 0x06,
    // O-Cap enabled functions
    CreateTenderWithCapabilityV1 = 0x07,
    SubmitBidWithCapabilityV1 = 0x08,
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
pub const TENDER_CONTRACT_TENDERS_TREE: &str = "tenders";
pub const TENDER_CONTRACT_BIDS_TREE: &str = "bids";
pub const TENDER_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const TENDER_CONTRACT_INFO_TREE: &str = "info";

// These are keys inside the info tree
pub const TENDER_CONTRACT_DB_VERSION: &[u8] = b"db_version";

// zkas circuit namespaces
pub const TENDER_CONTRACT_ZKAS_CREATE_NS_V1: &str = "CreateTender";
pub const TENDER_CONTRACT_ZKAS_SUBMIT_BID_NS_V1: &str = "SubmitBid";
pub const TENDER_CONTRACT_ZKAS_REVEAL_BID_NS_V1: &str = "RevealBid";
pub const TENDER_CONTRACT_ZKAS_SELECT_WINNER_NS_V1: &str = "SelectWinner";
// O-Cap circuit namespaces
pub const TENDER_CONTRACT_ZKAS_SUBMIT_BID_WITH_CAP_NS_V1: &str = "SubmitBidWithCapability";