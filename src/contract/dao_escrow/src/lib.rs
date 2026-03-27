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

//! DarkFi DAO-Escrow Contract
//!
//! Community insurance / collective escrow governed by DAO voting.
//!
//! ## Concept
//!
//! Combines DAO governance with escrow mechanics:
//! - **Members pay premiums** into a community endowment
//! - **Claims are proposals** — anyone can propose a payout
//! - **DAO votes** — token holders decide approve/reject
//! - **Conditional release** — approved claims release endowment (like escrow claim)
//!
//! ## Trust Model: DAO-Governed Escrow
//!
//! Unlike pure escrow with timeout-based refunds, this is governed:
//! - No automatic timeout refund
//! - DAO decides ALL outcomes via voting
//! - Dispute resolution is democratic, not algorithmic
//!
//! ## Use Cases
//!
//! - **Community Insurance**: Pool funds, vote on claims (healthcare, disaster relief)
//! - **Protocol-Owned Liquidity**: Pool funds, DAO allocates to strategic initiatives
//! - **Treasury Management**: DAOs manage endowment, vote on disbursements
//! - **Collective Investment**: Members contribute, DAO votes on investments

use darkfi_sdk::define_contract_function;

/// Functions available in the contract
define_contract_function!(DaoEscrowFunction {
    InitializeV1 = 0x00,
    UpdateV1 = 0x01,
    PayPremiumV1 = 0x02,
    ProposeClaimV1 = 0x03,
    VoteClaimV1 = 0x04,
    ExecuteClaimV1 = 0x05,
    CancelClaimV1 = 0x06,
    WithdrawV1 = 0x07,
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

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Info tree (version, config, governance params)
pub const DAO_ESCROW_CONTRACT_INFO_TREE: &str = "info";
/// Bullas tree (DAOEscrow instances)
pub const DAO_ESCROW_CONTRACT_BULLAS_TREE: &str = "bullas";
/// Premiums tree (premium payments tracking)
pub const DAO_ESCROW_CONTRACT_PREMIUMS_TREE: &str = "premiums";
/// Claims tree (pending/executed claims)
pub const DAO_ESCROW_CONTRACT_CLAIMS_TREE: &str = "claims";
/// Vote nullifiers tree (prevents double-voting)
pub const DAO_ESCROW_CONTRACT_VOTE_NULLIFIERS_TREE: &str = "vote_nullifiers";
/// Endowment pool tree (actual funds)
pub const DAO_ESCROW_CONTRACT_ENDOWMENT_TREE: &str = "endowment";

// ============================================================================
// KEYS
// ============================================================================

/// DB version key
pub const DAO_ESCROW_DB_VERSION: &[u8] = b"db_version";
/// Merkle tree key
pub const DAO_ESCROW_MERKLE_TREE: &[u8] = b"merkle_tree";
/// Latest root key
pub const DAO_ESCROW_LATEST_ROOT: &[u8] = b"last_root";

// ============================================================================
// ZKAS CIRCUIT NAMESPACES
// ============================================================================

/// ZKAS namespace for initialization
pub const DAO_ESCROW_ZKAS_INIT_NS: &str = "Init";
/// ZKAS namespace for premium payment
pub const DAO_ESCROW_ZKAS_PREMIUM_NS: &str = "PayPremium";
/// ZKAS namespace for claim proposal
pub const DAO_ESCROW_ZKAS_PROPOSE_NS: &str = "ProposeClaim";
/// ZKAS namespace for claim vote
pub const DAO_ESCROW_ZKAS_VOTE_NS: &str = "VoteClaim";
/// ZKAS namespace for claim execution
pub const DAO_ESCROW_ZKAS_EXEC_NS: &str = "ExecuteClaim";
