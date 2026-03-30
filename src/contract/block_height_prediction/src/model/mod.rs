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

//! Block Height Prediction Contract Data Models
//!
//! This contract allows betting on the canonical block height at a specific time.
//! Resolution uses cumulative PoW entropy from DarkFi's RandomX blockchain.

use darkfi_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::error::BlockHeightPredictionError;
use crate::{MAX_CONFIRMATION_DEPTH, MAX_TOLERANCE};

// ============================================================================
// STATE TYPES
// ============================================================================

/// Unique market identifier (Poseidon hash of market parameters)
pub type MarketId = pallas::Base;

/// Unique position identifier (Poseidon hash of position parameters)
pub type PositionId = pallas::Base;

/// Represents the current state of a market
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum MarketState {
    Active = 0,
    Resolved = 1,
    Cancelled = 2,
}

impl TryFrom<u8> for MarketState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Resolved),
            2 => Ok(Self::Cancelled),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Represents a block height prediction market
///
/// The market resolves at a specific timestamp, using cumulative PoW entropy
/// to determine the "official" block height at that time.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Market {
    /// Unique market identifier
    pub id: MarketId,
    /// Market creator public key
    pub creator: PublicKey,
    /// Target timestamp for resolution (Unix timestamp)
    pub target_time: u64,
    /// Base block height at market creation
    pub base_block_height: u64,
    /// Block height when market was created
    pub created_at: u64,
    /// Cumulative pool size (sum of all bets)
    pub total_pool: u64,
    /// Pool per outcome (below/above/exact ranges)
    pub below_pool: u64,  // Bets that block height < predicted
    pub above_pool: u64,  // Bets that block height > predicted
    pub exact_pool: u64,  // Bets on exact block height
    /// Market state
    pub state: MarketState,
    /// Resolved block height (set at resolution)
    pub resolved_height: Option<u64>,
    /// Resolution block (block used for PoW resolution)
    pub resolution_block: u64,
    /// PoW confirmation depth used for resolution
    pub confirmation_depth: u8,
    /// Protocol fee in basis points
    pub protocol_fee: u32,
    /// Token ID being used for betting
    pub token_id: pallas::Base,
    /// Number of positions
    pub position_count: u64,
}

/// Represents a position/bet in a market
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Position {
    /// Unique position identifier
    pub id: PositionId,
    /// Market this position is for
    pub market_id: MarketId,
    /// Owner of this position
    pub owner: PublicKey,
    /// Predicted block height
    pub predicted_height: u64,
    /// Tolerance range (+/- blocks for "close" payout)
    pub tolerance: u8,
    /// Position type
    pub position_type: PositionType,
    /// Amount wagered
    pub amount: u64,
    /// Payout if won (calculated at resolution)
    pub potential_payout: u64,
    /// Whether winnings have been claimed
    pub claimed: bool,
    /// Block height when position was created
    pub created_at: u64,
}

/// Position type
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum PositionType {
    Below = 0,  // Bet block height < predicted
    Exact = 1,  // Bet block height == predicted
    Above = 2,  // Bet block height > predicted
}

impl TryFrom<u8> for PositionType {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Below),
            1 => Ok(Self::Exact),
            2 => Ok(Self::Above),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// PARAMETER TYPES
// ============================================================================

/// Parameters for `BlockHeightPrediction::CreateMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateMarketParamsV1 {
    /// Market creator public key
    pub creator: PublicKey,
    /// Target timestamp for resolution (Unix timestamp in seconds)
    pub target_time: u64,
    /// Initial predicted block height (market sets initial expectation)
    pub initial_prediction: u64,
    /// PoW confirmation depth (higher = more secure, K blocks)
    pub confirmation_depth: u8,
    /// Protocol fee in basis points (0 = use default)
    pub protocol_fee: u32,
    /// Token ID for betting
    pub token_id: pallas::Base,
}

/// State update for `CreateMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateMarketUpdateV1 {
    pub market_id: MarketId,
    pub creator: PublicKey,
    pub target_time: u64,
    pub base_block_height: u64,
    pub confirmation_depth: u8,
    pub protocol_fee: u32,
    pub token_id: pallas::Base,
    pub created_at: u64,
}

/// Parameters for `BlockHeightPrediction::CreatePositionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreatePositionParamsV1 {
    /// Market ID to bet on
    pub market_id: MarketId,
    /// Predicted block height
    pub predicted_height: u64,
    /// Tolerance range (+/- for "close" payout)
    pub tolerance: u8,
    /// Position type (Below/Exact/Above)
    pub position_type: u8,
    /// Amount to bet
    pub amount: u64,
    /// Owner public key
    pub owner: PublicKey,
    /// Value commitment for the bet amount
    pub value_commit: pallas::Point,
    /// Signature over the bet commitment
    pub signature: pallas::Base,
}

/// State update for `CreatePositionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreatePositionUpdateV1 {
    pub position_id: PositionId,
    pub market_id: MarketId,
    pub owner: PublicKey,
    pub predicted_height: u64,
    pub tolerance: u8,
    pub position_type: PositionType,
    pub amount: u64,
    pub created_at: u64,
}

/// Parameters for `BlockHeightPrediction::ResolveMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveMarketParamsV1 {
    pub market_id: MarketId,
    /// The observed block height at target_time
    pub observed_height: u64,
    /// Resolution proof (ZK proof of PoW hash calculation)
    pub proof: Vec<u8>,
}

/// State update for `ResolveMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveMarketUpdateV1 {
    pub market_id: MarketId,
    pub resolved_height: u64,
    pub resolution_block: u64,
    pub state: MarketState,
}

/// Parameters for `BlockHeightPrediction::ClaimWinningsV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimWinningsParamsV1 {
    pub position_id: PositionId,
    pub market_id: MarketId,
    pub owner: PublicKey,
    pub proof: Vec<u8>,
}

/// State update for `ClaimWinningsV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimWinningsUpdateV1 {
    pub position_id: PositionId,
    pub payout: u64,
    pub claimed: bool,
}

/// Parameters for `BlockHeightPrediction::CancelMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelMarketParamsV1 {
    pub market_id: MarketId,
    pub canceller: PublicKey,
}

/// State update for `CancelMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelMarketUpdateV1 {
    pub market_id: MarketId,
    pub state: MarketState,
    pub refund_amounts: Vec<(PositionId, u64)>,
}

// ============================================================================
// VALIDATION HELPERS
// ============================================================================

/// Validate confirmation depth is within bounds
pub fn validate_confirmation_depth(depth: u8) -> Result<(), BlockHeightPredictionError> {
    if depth == 0 || depth > MAX_CONFIRMATION_DEPTH {
        return Err(BlockHeightPredictionError::InvalidConfirmationDepth)
    }
    Ok(())
}

/// Validate tolerance is within bounds
pub fn validate_tolerance(tolerance: u8) -> Result<(), BlockHeightPredictionError> {
    if tolerance > MAX_TOLERANCE {
        return Err(BlockHeightPredictionError::InvalidTolerance)
    }
    Ok(())
}

/// Validate position amount
pub fn validate_amount(amount: u64) -> Result<(), BlockHeightPredictionError> {
    if amount == 0 {
        return Err(BlockHeightPredictionError::BetValueTooSmall)
    }
    Ok(())
}

// ============================================================================
// IDENTITY DERIVATION
// ============================================================================

/// Derive a market ID from its parameters
pub fn derive_market_id(
    creator: &PublicKey,
    target_time: u64,
    token_id: pallas::Base,
    confirmation_depth: u8,
) -> MarketId {
    let (cx, cy) = creator.xy();
    poseidon_hash([
        cx,
        cy,
        pallas::Base::from(target_time),
        token_id,
        pallas::Base::from(confirmation_depth as u64),
    ])
}

/// Derive a position ID from its parameters
pub fn derive_position_id(
    market_id: MarketId,
    owner: &PublicKey,
    predicted_height: u64,
    position_type: PositionType,
    amount: u64,
    secret_nonce: pallas::Base,
) -> PositionId {
    let (ox, oy) = owner.xy();
    poseidon_hash([
        market_id,
        ox,
        oy,
        pallas::Base::from(predicted_height),
        pallas::Base::from(position_type as u8 as u64),
        pallas::Base::from(amount),
        secret_nonce,
    ])
}

// ============================================================================
// PRICING AND PAYOUT CALCULATIONS
// ============================================================================

/// Calculate payout for a winning position
/// Uses a simple proportional payout model
pub fn calculate_payout(
    position_amount: u64,
    winning_pool: u64,
    total_pool: u64,
    protocol_fee: u32,
) -> Result<u64, BlockHeightPredictionError> {
    if winning_pool == 0 {
        return Ok(0)
    }

    let fee_factor = 10000u64.checked_sub(protocol_fee as u64)
        .ok_or(BlockHeightPredictionError::ArithmeticOverflow)?;

    let share_of_pool = position_amount
        .checked_mul(total_pool)
        .ok_or(BlockHeightPredictionError::ArithmeticOverflow)?
        / winning_pool;

    let product = share_of_pool
        .checked_mul(fee_factor)
        .ok_or(BlockHeightPredictionError::ArithmeticOverflow)?;

    Ok(product / 10000)
}

/// Determine if a position wins given resolved block height
pub fn position_wins(
    position: &Position,
    resolved_height: u64,
) -> PositionOutcome {
    let distance = resolved_height as i64 - position.predicted_height as i64;
    let abs_distance = distance.abs() as u64;

    match position.position_type {
        PositionType::Exact => {
            if distance == 0 {
                PositionOutcome::Exact
            } else if abs_distance <= position.tolerance as u64 {
                PositionOutcome::Close
            } else {
                PositionOutcome::Lost
            }
        }
        PositionType::Below => {
            if distance < 0 {
                // Check if within tolerance for "close" bonus
                if abs_distance <= position.tolerance as u64 {
                    PositionOutcome::Close
                } else {
                    PositionOutcome::Won
                }
            } else {
                PositionOutcome::Lost
            }
        }
        PositionType::Above => {
            if distance > 0 {
                if abs_distance <= position.tolerance as u64 {
                    PositionOutcome::Close
                } else {
                    PositionOutcome::Won
                }
            } else {
                PositionOutcome::Lost
            }
        }
    }
}

/// Outcome of a position after resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionOutcome {
    Won,   // Full payout
    Close, // Partial payout (within tolerance)
    Exact, // Jackpot bonus (exact prediction)
    Lost,  // No payout
}

/// Calculate cumulative PoW hash for resolution
/// Uses multiple block hashes for stronger randomness
pub fn calculate_resolution_hash(
    block_hashes: &[pallas::Base],
) -> pallas::Base {
    let mut combined_hash = pallas::Base::zero();
    for block_hash in block_hashes.iter() {
        combined_hash = poseidon_hash([combined_hash, *block_hash]);
    }
    combined_hash
}

/// Derive block height from PoW entropy
/// This prevents manipulation by using RandomX output as variance source
pub fn derive_height_from_entropy(
    entropy: pallas::Base,
    base_height: u64,
    expected_blocks: u64,
) -> u64 {
    let bytes = entropy.to_repr();
    // Use first byte to derive variance of +/- 50 blocks
    let variance: i64 = (bytes[0] as i64) % 100 - 50;
    let height = (expected_blocks as i64 + variance).max(0) as u64;
    base_height.saturating_add(height)
}
