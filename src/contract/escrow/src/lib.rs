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

//! DarkWow Escrow Contract
//!
//! Privacy-preserving conditional payment contract. Funds are locked in a
//! commitment and released to the seller upon proof of knowledge of a secret,
//! or returned to the buyer after a timeout.
//!
//! Trust model: Hashed Timelock (Variant 3)
//! - Seller claims by proving knowledge of seller_secret
//! - Buyer refunds after timeout by proving knowledge of buyer_secret
//! - A spent flag prevents both claim and refund from succeeding
//!
//! Privacy properties:
//! - Amount hidden in Pedersen commitment
//! - Parties hidden (public keys derived from secrets)
//! - Claim/refund linkable only via nullifiers

use dwow_sdk::define_contract_function;

define_contract_function!(EscrowFunction {
    InitializeV1 = 0x00,
    CreateEscrowV1 = 0x01,
    FundV1 = 0x02,
    ClaimV1 = 0x03,
    RefundV1 = 0x04,
    CancelV1 = 0x05,
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

/// Capability descriptor for wallet state machine
pub mod capability;

// These are the different sled trees that will be created
pub const ESCROW_CONTRACT_INFO_TREE: &str = "info";
pub const ESCROW_CONTRACT_ESCROWS_TREE: &str = "escrows";
pub const ESCROW_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const ESCROW_CONTRACT_SPENT_FLAGS_TREE: &str = "spent_flags";

// These are keys inside the info tree
pub const ESCROW_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Promissory Note contract ID for cross-contract routing validation
pub const PROMISSORY_NOTE_CONTRACT_ID_KEY: &[u8] = b"promissory_note_cid";
/// Purse contract ID (genesis counter 8) for fund escrow child calls
pub const PURSE_CONTRACT_ID_KEY: &[u8] = b"purse_cid";
/// Box contract ID (genesis counter 9) for claim/refund child calls
pub const BOX_CONTRACT_ID_KEY: &[u8] = b"box_cid";
pub const ESCROW_CONTRACT_STATE: &[u8] = b"state";

// zkas circuit namespaces
pub const ESCROW_CONTRACT_ZKAS_CREATE_NS_V1: &str = "CreateEscrow";
pub const ESCROW_CONTRACT_ZKAS_FUND_NS_V1: &str = "FundEscrow";
pub const ESCROW_CONTRACT_ZKAS_CLAIM_NS_V1: &str = "ClaimEscrow";
pub const ESCROW_CONTRACT_ZKAS_REFUND_NS_V1: &str = "RefundEscrow";
