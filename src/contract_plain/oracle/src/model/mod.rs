/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Plain Oracle Contract Model
//!
//! # Privacy Notice
//!
//! This contract uses **partial transparency** - state is public on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.
//!
//! # ZK vs Native Operations
//!
//! | Operation | Method | Reason |
//! |-----------|--------|--------|
//! | Signature verification | ZK (Schnorr) | Sound, constrainable |
//! | Data commitment | ZK (Pedersen) | Privacy-preserving |
//! | Weighted average | Native Rust | Needs `base_div` (not in ZK) |
//! | Aggregation logic | Native Rust | Arbitrary complexity |

use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use darkfi_sdk::crypto::schnorr::Signature;
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// DATA POINT (Submitted by stakers - visible on-chain)
// ============================================================================

/// A data point submitted by a staker
/// PRIVACY NOTICE: All data is PUBLIC in plain version
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DataPoint {
    /// Unique data point identifier
    pub id: pallas::Base,
    /// Feed this data point belongs to
    pub feed_id: pallas::Base,
    /// Staker's public key
    pub staker: PublicKey,
    /// The data value
    pub value: u64,
    /// Weight/stake multiplier for this staker
    pub weight: u64,
    /// Block when submitted
    pub submitted_at_block: u64,
}

/// A staker in the oracle network
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Staker {
    /// Staker's public key
    pub public_key: PublicKey,
    /// Total stake deposited
    pub stake_amount: u64,
    /// Accumulated weight (sum of stakes)
    pub total_weight: u64,
    /// Number of data points submitted
    pub data_point_count: u64,
    /// Number of times slashed
    pub slash_count: u64,
    /// Whether staker is active
    pub is_active: bool,
}

// ============================================================================
// PARAMETERS (Input types for contract calls)
// ============================================================================

/// Parameters for creating a new feed
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateFeedParamsV1 {
    /// Feed name/description hash
    pub name_hash: pallas::Base,
    /// Minimum stake required to participate
    pub min_stake: u64,
    /// Token for stake deposits
    pub stake_token: pallas::Base,
    /// Aggregation function type (0=weighted_avg, 1=median, etc.)
    pub aggregation_type: u8,
    /// Creator's signature over feed params
    pub signature: Signature,
}

/// Parameters for registering as a staker
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterStakerParamsV1 {
    /// Feed ID
    pub feed_id: pallas::Base,
    /// Staker's public key
    pub staker: PublicKey,
    /// Stake amount
    pub stake_amount: u64,
    /// Staker's signature over registration
    pub signature: Signature,
}

/// Parameters for submitting a data point
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubmitDataPointParamsV1 {
    /// Feed ID
    pub feed_id: pallas::Base,
    /// The data value
    pub value: u64,
    /// Staker's signature over data point
    pub signature: Signature,
}

/// Parameters for slashing a staker
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlashStakerParamsV1 {
    /// Feed ID
    pub feed_id: pallas::Base,
    /// Staker to slash
    pub staker: PublicKey,
    /// Reason for slashing (hashed)
    pub reason_hash: pallas::Base,
    /// Data point ID being disputed
    pub data_point_id: pallas::Base,
    /// Slasher's signature
    pub signature: Signature,
}

/// Parameters for unregistering a staker
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnregisterStakerParamsV1 {
    /// Feed ID
    pub feed_id: pallas::Base,
    /// Staker's signature over unregistration
    pub signature: Signature,
}

// ============================================================================
// UPDATE TYPES (Output from process_instruction, input to process_update)
// ============================================================================

/// Update produced by feed creation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateFeedUpdateV1 {
    pub feed_id: pallas::Base,
    pub name_hash: pallas::Base,
    pub min_stake: u64,
    pub aggregation_type: u8,
}

/// Update produced by staker registration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RegisterStakerUpdateV1 {
    pub feed_id: pallas::Base,
    pub staker: PublicKey,
    pub stake_amount: u64,
}

/// Update produced by data point submission
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubmitDataPointUpdateV1 {
    pub data_point_id: pallas::Base,
    pub feed_id: pallas::Base,
    pub staker: PublicKey,
    pub value: u64,
    pub weight: u64,
    pub submitted_at_block: u64,
}

/// Update produced by slashing
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SlashStakerUpdateV1 {
    pub feed_id: pallas::Base,
    pub staker: PublicKey,
    pub slash_amount: u64,
    pub reason_hash: pallas::Base,
}

/// Update produced by unregistration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnregisterStakerUpdateV1 {
    pub feed_id: pallas::Base,
    pub staker: PublicKey,
    pub refund_amount: u64,
}