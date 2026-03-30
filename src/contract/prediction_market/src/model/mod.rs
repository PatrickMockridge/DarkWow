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

//! Prediction Market Contract Data Models
//!
//! This module defines the core data structures for the prediction market.

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::error::PredictionMarketError;
use crate::{MAX_OUTCOMES, MAX_PROTOCOL_FEE, MIN_PROTOCOL_FEE};

// ============================================================================
// STATE TYPES
// ============================================================================

/// Unique market identifier (Poseidon hash of market parameters)
pub type MarketId = pallas::Base;

/// Unique position identifier (Poseidon hash of position parameters)
pub type PositionId = pallas::Base;

/// Unique liquidity provider share identifier
pub type LpShareId = pallas::Base;

/// Represents the current state of a market
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum MarketState {
    Active = 0,
    Frozen = 1,
    Resolved = 2,
    Cancelled = 3,
    Disputed = 4,
}

impl TryFrom<u8> for MarketState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Frozen),
            2 => Ok(Self::Resolved),
            3 => Ok(Self::Cancelled),
            4 => Ok(Self::Disputed),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Represents a prediction market
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Market {
    /// Unique market identifier
    pub id: MarketId,
    /// Market creator public key
    pub creator: PublicKey,
    /// The question/prompt for the prediction (e.g., "Will BTC be > $100k on 2025-01-01?")
    pub question: Vec<u8>,
    /// Timestamp when the market resolves
    pub resolve_time: u64,
    /// Timestamp when betting closes (optional, 0 = same as resolve_time)
    pub betting_closes: u64,
    /// Number of possible outcomes (2 for YES/NO, N for discrete)
    pub num_outcomes: u8,
    /// Total pool size (sum of all bets)
    pub total_pool: u64,
    /// Pool shares per outcome
    pub outcome_pools: Vec<u64>,
    /// Market state
    pub state: MarketState,
    /// Resolved outcome (valid after resolution)
    pub resolved_outcome: Option<u8>,
    /// Protocol fee in basis points
    pub protocol_fee: u32,
    /// Liquidity provider fee in basis points
    pub lp_fee: u32,
    /// Token ID being used for betting
    pub token_id: pallas::Base,
    /// Oracle public key that can resolve this market
    pub oracle_pubkey: PublicKey,
    /// Block height when market was created
    pub created_at: u64,
    /// Block height when market was resolved
    pub resolved_at: u64,
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
    /// The outcome this position represents (0 = first outcome, 1 = second, etc.)
    pub outcome: u8,
    /// Amount of tokens wagered
    pub amount: u64,
    /// Payout if the outcome wins (calculated at resolution)
    pub potential_payout: u64,
    /// Whether winnings have been claimed
    pub claimed: bool,
    /// Block height when position was created
    pub created_at: u64,
}

/// Represents a liquidity provider share
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LpShare {
    /// Unique LP share identifier
    pub id: LpShareId,
    /// Market this LP is for
    pub market_id: MarketId,
    /// LP provider public key
    pub provider: PublicKey,
    /// Number of LP shares owned
    pub shares: u64,
    /// Fees earned (available for withdrawal)
    pub earned_fees: u64,
    /// Block height when LP was created
    pub created_at: u64,
}

// ============================================================================
// PARAMETER TYPES
// ============================================================================

/// Parameters for `PredictionMarket::CreateMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateMarketParamsV1 {
    /// The question/prompt for the prediction
    pub question: Vec<u8>,
    /// Timestamp when the market resolves
    pub resolve_time: u64,
    /// Timestamp when betting closes (0 = same as resolve_time)
    pub betting_closes: u64,
    /// Number of possible outcomes
    pub num_outcomes: u8,
    /// Protocol fee in basis points (0 to use default)
    pub protocol_fee: u32,
    /// LP fee in basis points (0 to use default)
    pub lp_fee: u32,
    /// Token ID for betting
    pub token_id: pallas::Base,
    /// Oracle public key for resolution
    pub oracle_pubkey: PublicKey,
    /// Signature from oracle (to authorize resolution)
    pub oracle_signature: pallas::Base,
}

/// State update for `CreateMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateMarketUpdateV1 {
    pub market_id: MarketId,
    pub creator: PublicKey,
    pub question: Vec<u8>,
    pub resolve_time: u64,
    pub betting_closes: u64,
    pub num_outcomes: u8,
    pub protocol_fee: u32,
    pub lp_fee: u32,
    pub token_id: pallas::Base,
    pub oracle_pubkey: PublicKey,
    pub created_at: u64,
}

/// Parameters for `PredictionMarket::CreatePositionV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreatePositionParamsV1 {
    /// Market ID to bet on
    pub market_id: MarketId,
    /// Which outcome to bet on (0, 1, 2, ...)
    pub outcome: u8,
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
    pub outcome: u8,
    pub amount: u64,
    pub created_at: u64,
}

/// Parameters for `PredictionMarket::AddLiquidityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddLiquidityParamsV1 {
    pub market_id: MarketId,
    pub amount: u64,
    pub provider: PublicKey,
    pub value_commit: pallas::Point,
    pub signature: pallas::Base,
}

/// State update for `AddLiquidityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddLiquidityUpdateV1 {
    pub lp_share_id: LpShareId,
    pub market_id: MarketId,
    pub provider: PublicKey,
    pub shares_minted: u64,
    pub fees_earned: u64,
    pub created_at: u64,
}

/// Parameters for `PredictionMarket::RemoveLiquidityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RemoveLiquidityParamsV1 {
    pub market_id: MarketId,
    pub shares: u64,
    pub provider: PublicKey,
}

/// State update for `RemoveLiquidityV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RemoveLiquidityUpdateV1 {
    pub market_id: MarketId,
    pub provider: PublicKey,
    pub shares_burned: u64,
    pub payout: u64,
    pub fees_withdrawn: u64,
}

/// Parameters for `PredictionMarket::ResolveMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveMarketParamsV1 {
    pub market_id: MarketId,
    /// The winning outcome (0, 1, 2, ...)
    pub outcome: u8,
    /// Oracle attestation data (proof that oracle signed this resolution)
    pub attestation: Vec<u8>,
    /// Oracle signature over the resolution
    pub oracle_signature: pallas::Base,
}

/// State update for `ResolveMarketV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveMarketUpdateV1 {
    pub market_id: MarketId,
    pub outcome: u8,
    pub resolved_at: u64,
}

/// Parameters for `PredictionMarket::ClaimWinningsV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimWinningsParamsV1 {
    pub position_id: PositionId,
    pub market_id: MarketId,
    pub proof: Vec<u8>, // ZK proof of position ownership and winning outcome
}

/// State update for `ClaimWinningsV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimWinningsUpdateV1 {
    pub position_id: PositionId,
    pub payout: u64,
    pub claimed: bool,
}

/// Parameters for `PredictionMarket::CancelMarketV1`
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
    pub refund_amounts: Vec<(PositionId, u64)>, // Position ID and refund amount
}

/// Parameters for `PredictionMarket::WithdrawFeesV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawFeesParamsV1 {
    pub market_id: MarketId,
    pub provider: PublicKey,
}

/// State update for `WithdrawFeesV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawFeesUpdateV1 {
    pub market_id: MarketId,
    pub provider: PublicKey,
    pub amount: u64,
}

// ============================================================================
// VALIDATION HELPERS
// ============================================================================

/// Validate the number of outcomes is within bounds
pub fn validate_num_outcomes(num: u8) -> Result<(), PredictionMarketError> {
    if num == 0 || num > MAX_OUTCOMES {
        return Err(PredictionMarketError::InvalidOutcome)
    }
    Ok(())
}

/// Validate protocol fee is within bounds
pub fn validate_protocol_fee(fee: u32) -> Result<(), PredictionMarketError> {
    if fee != 0 && (fee < MIN_PROTOCOL_FEE || fee > MAX_PROTOCOL_FEE) {
        return Err(PredictionMarketError::InvalidFee)
    }
    Ok(())
}

/// Validate a position amount
pub fn validate_amount(amount: u64) -> Result<(), PredictionMarketError> {
    if amount == 0 {
        return Err(PredictionMarketError::BetValueTooSmall)
    }
    Ok(())
}

// ============================================================================
// IDENTITY DERIVATION
// ============================================================================

/// Derive a market ID from its parameters
pub fn derive_market_id(
    creator: &PublicKey,
    question: &[u8],
    resolve_time: u64,
    token_id: pallas::Base,
    oracle_pubkey: &PublicKey,
) -> MarketId {
    let (cx, cy) = creator.xy();
    let (ox, oy) = oracle_pubkey.xy();
    // Convert question to a field element via first 8 bytes as u64
    let mut bytes = [0u8; 8];
    let q_len = question.len().min(8);
    bytes[..q_len].copy_from_slice(&question[..q_len]);
    let question_field = pallas::Base::from(u64::from_le_bytes(bytes));
    poseidon_hash([
        cx,
        cy,
        question_field,
        pallas::Base::from(resolve_time),
        token_id,
        ox,
        oy,
    ])
}

/// Derive a position ID from its parameters
pub fn derive_position_id(
    market_id: MarketId,
    owner: &PublicKey,
    outcome: u8,
    amount: u64,
    secret_nonce: pallas::Base,
) -> PositionId {
    let (ox, oy) = owner.xy();
    poseidon_hash([
        market_id,
        ox,
        oy,
        pallas::Base::from(outcome as u64),
        pallas::Base::from(amount),
        secret_nonce,
    ])
}

/// Derive an LP share ID
pub fn derive_lp_share_id(
    market_id: MarketId,
    provider: &PublicKey,
    shares: u64,
    secret_nonce: pallas::Base,
) -> LpShareId {
    let (px, py) = provider.xy();
    poseidon_hash([market_id, px, py, pallas::Base::from(shares), secret_nonce])
}

// ============================================================================
// PRICING AND PAYOUT CALCULATIONS
// ============================================================================

/// Calculate the current price of a position given the pool state
/// Uses a constant-product AMM inspired pricing (similar to Uniswap v2)
/// price = pool_other / (pool_self + amount)
pub fn calculate_position_price(
    outcome_pools: &[u64],
    outcome: u8,
    amount: u64,
) -> u64 {
    let total: u64 = outcome_pools.iter().sum();
    if total == 0 {
        return amount // First bet: even odds
    }
    let pool_for_outcome = outcome_pools[outcome as usize];
    let other_pools: u64 = outcome_pools.iter().enumerate()
        .filter(|(i, _)| *i as u8 != outcome)
        .map(|(_, v)| v)
        .sum();

    // Price = (other_pools * amount) / (pool_for_outcome + amount)
    // This ensures price approaches 1 as bets approach even
    (other_pools * amount) / (pool_for_outcome + amount).max(1)
}

/// Calculate payout for a winning position
/// payout = total_pool * (position_amount / winning_pool) * (1 - fees)
pub fn calculate_payout(
    position_amount: u64,
    winning_pool: u64,
    total_pool: u64,
    protocol_fee: u32,
    lp_fee: u32,
) -> u64 {
    let total_fees = protocol_fee as u64 + lp_fee as u64;
    let fee_factor = 10000u64 - total_fees;
    let share_of_pool = (position_amount * total_pool) / winning_pool.max(1);
    (share_of_pool * fee_factor) / 10000
}

/// Calculate how many LP shares to mint for providing liquidity
/// shares = amount * total_shares / total_liquidity
pub fn calculate_lp_shares(
    amount: u64,
    existing_shares: u64,
    existing_liquidity: u64,
) -> u64 {
    if existing_liquidity == 0 {
        amount // First LP gets 1:1 shares
    } else {
        (amount * existing_shares) / existing_liquidity
    }
}

/// Calculate payout for removing liquidity
/// payout = shares * total_pool / total_shares
pub fn calculate_liquidity_payout(
    shares: u64,
    total_pool: u64,
    total_shares: u64,
) -> u64 {
    if total_shares == 0 {
        0
    } else {
        (shares * total_pool) / total_shares
    }
}
