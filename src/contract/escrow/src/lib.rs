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

//! DarkFi Escrow Contract
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

use darkfi_sdk::define_contract_function;

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

// These are the different sled trees that will be created
pub const ESCROW_CONTRACT_INFO_TREE: &str = "info";
pub const ESCROW_CONTRACT_ESCROWS_TREE: &str = "escrows";
pub const ESCROW_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
pub const ESCROW_CONTRACT_SPENT_FLAGS_TREE: &str = "spent_flags";

// These are keys inside the info tree
pub const ESCROW_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const ESCROW_CONTRACT_STATE: &[u8] = b"state";

// zkas circuit namespaces
pub const ESCROW_CONTRACT_ZKAS_CREATE_NS_V1: &str = "CreateEscrow_V1";
pub const ESCROW_CONTRACT_ZKAS_FUND_NS_V1: &str = "Fund_V1";
pub const ESCROW_CONTRACT_ZKAS_CLAIM_NS_V1: &str = "Claim_V1";
pub const ESCROW_CONTRACT_ZKAS_REFUND_NS_V1: &str = "Refund_V1";
