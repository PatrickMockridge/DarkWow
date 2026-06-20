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

//! DarkToshi Dice Contract Data Models

use dwow_sdk::{
    crypto::{
        combine_block_hashes, mix_entropy, pasta_prelude::PrimeField, poseidon_hash,
        tx_hash_to_base, PublicKey,
    },
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

use crate::error::DiceError;
use crate::{MAX_HOUSE_EDGE, MAX_TARGET, MIN_HOUSE_EDGE, ROLL_RANGE};

// ============================================================================
// STATE TYPES
// ============================================================================

/// Unique bet identifier (Poseidon hash of bet parameters)
pub type BetId = pallas::Base;

/// Represents the current state of a bet in the state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum BetState {
    Committed = 0,
    Revealed = 1,
    SettledPlayer = 2,
    SettledHouse = 3,
    Cancelled = 4,
}

impl TryFrom<u8> for BetState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Committed),
            1 => Ok(Self::Revealed),
            2 => Ok(Self::SettledPlayer),
            3 => Ok(Self::SettledHouse),
            4 => Ok(Self::Cancelled),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Core bet data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Bet {
    pub version: u8,
    pub id: BetId,
    pub player_pub: PublicKey,
    pub bet_value: u64,
    pub target: u8,
    pub secret_nonce_commit: pallas::Base,
    pub blind: pallas::Base,
    pub roll: Option<u8>,
    pub state: BetState,
    pub house_edge: u32,
    pub confirmation_depth: u8,  // Number of blocks to wait for randomness
    pub created_at: u64,
    pub revealed_at: u64,
    pub settle_block: u64,       // Block at which settlement becomes allowed
    pub value_commit: pallas::Point,
    pub token_id: pallas::Base,
    pub nullifier: BetId,
    pub instance_seed: [u8; 32],
}

impl Bet {
    /// Calculate payout for player winning.
    /// Formula: bet_value * (10000 - house_edge) / (target * 100)
    /// Example: bet=100, target=50, house_edge=200bp (2%)
    ///   payout = 100 * 9800 / 5000 = 196
    pub fn calculate_payout(&self) -> Option<u64> {
        let multiplier = 10000u64.checked_sub(self.house_edge as u64)?;
        let product = self.bet_value.checked_mul(multiplier)?;
        product.checked_div(self.target as u64 * 100)
    }

    /// Calculate house's take when house wins.
    /// House takes: bet_value - base_win + (base_win * house_edge / 10000)
    /// where base_win = bet_value * 100 / target
    /// This simplifies to: bet_value * (target + house_edge - 100) / target
    pub fn calculate_house_take(&self) -> Option<u64> {
        let base_win = self.bet_value.checked_mul(100)?.checked_div(self.target as u64)?;
        let profit = self.bet_value.saturating_sub(base_win);
        let house_cut = base_win.checked_mul(self.house_edge as u64)?.checked_div(10000)?;
        profit.checked_add(house_cut)
    }
}

// ============================================================================
// PARAMETER TYPES
// ============================================================================

/// Parameters for `Dice::CommitBetV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CommitBetParamsV1 {
    pub player_pub: PublicKey,
    pub bet_value: u64,
    pub target: u8,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub token_id: pallas::Base,
    pub value_commit: pallas::Point,
    pub signature: pallas::Base,
    pub house_edge: u32,
    pub confirmation_depth: u8,  // Player-selected depth for randomness (higher = more secure)
    pub instance_seed: [u8; 32],
}

/// State update for `CommitBetV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CommitBetUpdateV1 {
    pub bet_id: BetId,
    pub player_pub: PublicKey,
    pub bet_value: u64,
    pub target: u8,
    pub secret_nonce_commit: pallas::Base,
    pub blind: pallas::Base,
    pub value_commit: pallas::Point,
    pub token_id: pallas::Base,
    pub house_edge: u32,
    pub confirmation_depth: u8,
    pub settle_block: u64,  // Block at which settlement is allowed
    pub nullifier: BetId,
    pub created_at: u64,
    pub instance_seed: [u8; 32],
}

/// Parameters for `Dice::RevealRollV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevealRollParamsV1 {
    pub bet_id: BetId,
    pub secret_nonce: pallas::Base,
}

/// State update for `RevealRollV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevealRollUpdateV1 {
    pub bet_id: BetId,
    pub roll: u8,
    pub state: BetState,
    pub revealed_at: u64,
}

/// Parameters for `Dice::SettleBetV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleBetParamsV1 {
    pub bet_id: BetId,
    pub proof: Vec<u8>,
    pub roll_hash: pallas::Base,
}

/// State update for `SettleBetV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleBetUpdateV1 {
    pub bet_id: BetId,
    pub state: BetState,
    pub payout: u64,
}

/// Parameters for `Dice::HouseCloseV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HouseCloseParamsV1 {
    /// Bet ID to close
    pub bet_id: BetId,
    /// House public key X coordinate (ZK-verified)
    pub house_pub_x: pallas::Base,
    /// House public key Y coordinate (ZK-verified)
    pub house_pub_y: pallas::Base,
    /// Close nullifier = H(bet_id, house_secret) — replay protection
    pub close_nullifier: pallas::Base,
}

/// State update for `HouseCloseV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HouseCloseUpdateV1 {
    pub bet_id: BetId,
    pub close_nullifier: pallas::Base,
    pub state: BetState,
}

// ============================================================================
// VALIDATION HELPERS
// ============================================================================

pub fn validate_target(target: u8) -> Result<(), DiceError> {
    if target == 0 || target > MAX_TARGET {
        return Err(DiceError::InvalidTarget)
    }
    Ok(())
}

pub fn validate_house_edge(house_edge: u32) -> Result<(), DiceError> {
    if house_edge < MIN_HOUSE_EDGE || house_edge > MAX_HOUSE_EDGE {
        return Err(DiceError::InvalidHouseEdge)
    }
    Ok(())
}

pub fn derive_bet_id(
    player_pub: &PublicKey,
    bet_value: u64,
    target: u8,
    secret_nonce: pallas::Base,
    blind: pallas::Base,
    token_id: pallas::Base,
) -> BetId {
    let (px, py) = player_pub.xy();
    poseidon_hash([
        px,
        py,
        pallas::Base::from(bet_value),
        pallas::Base::from(u64::from(target)),
        secret_nonce,
        blind,
        token_id,
    ])
}

pub fn derive_nullifier(bet_id: BetId, secret_nonce: pallas::Base) -> BetId {
    poseidon_hash([bet_id, secret_nonce])
}

/// Calculate roll using multiple block hashes for enhanced randomness.
/// Uses cumulative PoW entropy - more blocks means exponentially harder to manipulate.
///
/// Security scaling with depth:
/// - K=1: 33% manipulation chance (with 33% hash power)
/// - K=6: ~0.14% (Bitcoin "6 confirmations" standard)
/// - K=10: ~0.005%
pub fn calculate_roll_with_depth(
    block_hashes: &[pallas::Base],
    bet_id: BetId,
    secret_nonce: pallas::Base,
) -> u8 {
    // Combine all block hashes cumulatively using Poseidon
    let combined_hash = combine_block_hashes(block_hashes);
    // Mix in bet_id and secret_nonce for additional entropy
    let final_entropy = mix_entropy(combined_hash, &[bet_id, secret_nonce]);
    let bytes = final_entropy.to_repr();
    ((bytes[0] as u64) % (ROLL_RANGE as u64)) as u8
}

/// Legacy single-block hash roll calculation.
/// Deprecated: Use calculate_roll_with_depth for production gambling.
#[deprecated(since = "0.1.0", note = "Use calculate_roll_with_depth with adjustable confirmation depth")]
pub fn calculate_roll(tx_hash_bytes: [u8; 32], bet_id: BetId, secret_nonce: pallas::Base) -> u8 {
    let block_hash = tx_hash_to_base(&tx_hash_bytes);
    let roll_input = poseidon_hash([block_hash, bet_id, secret_nonce]);
    let bytes = roll_input.to_repr();
    ((bytes[0] as u64) % (ROLL_RANGE as u64)) as u8
}
