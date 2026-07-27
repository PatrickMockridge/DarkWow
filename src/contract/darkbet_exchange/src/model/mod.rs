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

//! DarkBet Exchange Contract Models
//!
//! Data structures for the decentralized betting exchange supporting both
//! order-book matching (back/lay) and AMM pool modes.

use dwow_sdk::{
    crypto::{poseidon_hash, schnorr::Signature, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};
use dwow_serial::{SerialDecodable, SerialEncodable};
use dwow_sdk::crypto::pasta_prelude::PrimeField;

// ============================================================================
// ENUMS
// ============================================================================

/// Market states
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum MarketState {
    /// Market created, accepting orders
    Open = 0,
    /// Trading closed, waiting for resolution
    Closed = 1,
    /// Oracle has resolved the outcome
    Resolved = 2,
    /// Winners paid, market settled
    Settled = 3,
    /// Market cancelled (no resolution)
    Cancelled = 4,
}

impl TryFrom<u8> for MarketState {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Open),
            1 => Ok(Self::Closed),
            2 => Ok(Self::Resolved),
            3 => Ok(Self::Settled),
            4 => Ok(Self::Cancelled),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Market type: determines the matching mechanism
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum MarketType {
    /// Order-book style: back/lay orders matched peer-to-peer via DEX
    OrderBook = 0,
    /// AMM pool style: positions priced via constant-product formula
    AmmPool = 1,
}

impl TryFrom<u8> for MarketType {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::OrderBook),
            1 => Ok(Self::AmmPool),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Order types (order-book mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum OrderType {
    /// Bet that outcome WILL happen
    Back = 0,
    /// Bet that outcome will NOT happen
    Lay = 1,
}

impl TryFrom<u8> for OrderType {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Back),
            1 => Ok(Self::Lay),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Outcome types for resolved markets
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum Outcome {
    /// Outcome did not occur (lay wins)
    No = 0,
    /// Outcome occurred (back wins)
    Yes = 1,
    /// Market cancelled or void
    Void = 2,
}

impl TryFrom<u8> for Outcome {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::No),
            1 => Ok(Self::Yes),
            2 => Ok(Self::Void),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Order state (order-book mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum OrderState {
    /// Order is open and waiting to be matched
    Open = 0,
    /// Order has been matched
    Matched = 1,
    /// Order was cancelled
    Cancelled = 2,
    /// Order expired without matching
    Expired = 3,
}

impl TryFrom<u8> for OrderState {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Open),
            1 => Ok(Self::Matched),
            2 => Ok(Self::Cancelled),
            3 => Ok(Self::Expired),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Match state (order-book mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum MatchState {
    /// Match created, waiting for resolution
    Pending = 0,
    /// Outcome resolved, winners paid
    Settled = 1,
    /// Market cancelled, funds refunded
    Cancelled = 2,
}

impl TryFrom<u8> for MatchState {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Pending),
            1 => Ok(Self::Settled),
            2 => Ok(Self::Cancelled),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Position state (AMM mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum PositionState {
    /// Position is active
    Active = 0,
    /// Winnings claimed
    Claimed = 1,
    /// Refunded (market cancelled)
    Refunded = 2,
}

impl TryFrom<u8> for PositionState {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Claimed),
            2 => Ok(Self::Refunded),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// LP share state (AMM mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum LpShareState {
    /// LP shares are active
    Active = 0,
    /// Liquidity removed
    Removed = 1,
}

impl TryFrom<u8> for LpShareState {
    type Error = ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Removed),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// MARKET
// ============================================================================

/// A betting market supporting both order-book and AMM modes
#[derive(Debug, Clone)]
pub struct Market {
    pub version: u8,
    /// Unique market ID (Poseidon hash of market params)
    pub market_id: pallas::Base,
    /// Market creator (house/operator)
    pub creator: PublicKey,
    /// Description of the market (e.g., "Team A vs Team B")
    pub description: String,
    /// Outcomes available (e.g., ["Team_A_Wins", "Team_B_Wins", "Draw"])
    pub outcomes: Vec<String>,
    /// Oracle contract ID for resolution
    pub oracle_id: pallas::Base,
    /// Commission rate in basis points
    pub commission_bp: u32,
    /// Market type: order-book or AMM
    pub market_type: MarketType,
    /// Current state
    pub state: MarketState,
    // ---- Order-book mode fields ----
    /// Total back volume
    pub back_volume: u64,
    /// Total lay volume
    pub lay_volume: u64,
    /// Total matched volume
    pub matched_volume: u64,
    // ---- AMM mode fields ----
    /// Total pool size (sum of all positions)
    pub total_pool: u64,
    /// Total LP shares outstanding
    pub total_lp_shares: u64,
    /// Pool shares per outcome (AMM mode)
    pub outcome_pools: Vec<u64>,
    /// Protocol fee in basis points (AMM mode)
    pub protocol_fee: u32,
    /// LP fee in basis points (AMM mode)
    pub lp_fee: u32,
    // ---- Common fields ----
    /// Market closes at this block
    pub close_block: u64,
    /// Resolved at block (if resolved)
    pub resolved_at: Option<u64>,
    /// Winning outcome (if resolved)
    pub winning_outcome: Option<u8>,
    /// Market created at block
    pub created_at: u64,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

impl Market {
    /// Create a new market (order-book mode)
    pub fn new_order_book(
        creator: PublicKey,
        description: String,
        outcomes: Vec<String>,
        oracle_id: pallas::Base,
        commission_bp: u32,
        close_block: u64,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Self {
        let market_id = poseidon_hash([
            creator.x().expect("pk not identity"),
            creator.y().expect("pk not identity"),
            pallas::Base::from(close_block),
            pallas::Base::from(current_block),
        ]);

        Self {
            version: 0,
            market_id,
            creator,
            description,
            outcomes,
            oracle_id,
            commission_bp,
            market_type: MarketType::OrderBook,
            state: MarketState::Open,
            back_volume: 0,
            lay_volume: 0,
            matched_volume: 0,
            total_pool: 0,
            total_lp_shares: 0,
            outcome_pools: vec![],
            protocol_fee: 0,
            lp_fee: 0,
            close_block,
            resolved_at: None,
            winning_outcome: None,
            created_at: current_block,
            instance_seed,
        }
    }

    /// Create a new AMM pool market
    pub fn new_amm_pool(
        creator: PublicKey,
        description: String,
        outcomes: Vec<String>,
        oracle_id: pallas::Base,
        protocol_fee: u32,
        lp_fee: u32,
        close_block: u64,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Self {
        let num_outcomes = outcomes.len();
        let market_id = poseidon_hash([
            creator.x().expect("pk not identity"),
            creator.y().expect("pk not identity"),
            pallas::Base::from(close_block),
            pallas::Base::from(current_block),
        ]);

        Self {
            version: 0,
            market_id,
            creator,
            description,
            outcomes,
            oracle_id,
            commission_bp: protocol_fee + lp_fee,
            market_type: MarketType::AmmPool,
            state: MarketState::Open,
            back_volume: 0,
            lay_volume: 0,
            matched_volume: 0,
            total_pool: 0,
            total_lp_shares: 0,
            outcome_pools: vec![0; num_outcomes],
            protocol_fee,
            lp_fee,
            close_block,
            resolved_at: None,
            winning_outcome: None,
            created_at: current_block,
            instance_seed,
        }
    }

    /// Check if market can accept orders
    pub fn can_accept_order(&self, current_block: u64) -> Result<(), &'static str> {
        if self.state != MarketState::Open {
            return Err("Market not open")
        }
        if current_block >= self.close_block {
            return Err("Market closed")
        }
        Ok(())
    }

    /// Calculate commission for a matched amount (order-book mode)
    pub fn calculate_commission(&self, matched_amount: u64) -> u64 {
        (matched_amount * (self.commission_bp as u64)) / 10000
    }

    /// Calculate position price using constant-product AMM formula
    /// price = (other_pools * amount) / (pool_for_outcome + amount)
    pub fn calculate_position_price(
        &self,
        outcome: u8,
        amount: u64,
    ) -> Result<u64, &'static str> {
        let total: u64 = self.outcome_pools.iter().sum();
        if total == 0 {
            return Ok(amount) // First bet: even odds
        }

        let pool_for_outcome = self.outcome_pools[outcome as usize];
        let other_pools: u64 =
            self.outcome_pools.iter().enumerate()
                .filter(|(i, _)| *i as u8 != outcome)
                .map(|(_, v)| v)
                .sum();

        // Price = (other_pools * amount) / (pool_for_outcome + amount)
        let product = other_pools.checked_mul(amount).ok_or("Arithmetic overflow")?;
        let denominator = (pool_for_outcome + amount).max(1);
        Ok(product / denominator)
    }

    /// Calculate payout for a winning position (AMM mode)
    pub fn calculate_payout(
        &self,
        position_amount: u64,
        winning_pool: u64,
    ) -> Result<u64, &'static str> {
        let total_fees = (self.protocol_fee + self.lp_fee) as u64;
        let fee_factor = 10000u64.checked_sub(total_fees).ok_or("Arithmetic overflow")?;
        let share_of_pool = position_amount
            .checked_mul(self.total_pool)
            .ok_or("Arithmetic overflow")?
            / winning_pool.max(1);
        let product = share_of_pool.checked_mul(fee_factor).ok_or("Arithmetic overflow")?;
        Ok(product / 10000)
    }

    /// Calculate LP shares to mint for providing liquidity
    pub fn calculate_lp_shares(
        &self,
        amount: u64,
    ) -> Option<u64> {
        if self.total_lp_shares == 0 {
            Some(amount) // First LP gets 1:1 shares
        } else {
            amount.checked_mul(self.total_lp_shares)?.checked_div(self.total_pool)
        }
    }

    /// Calculate payout for removing liquidity
    pub fn calculate_liquidity_payout(
        &self,
        shares: u64,
    ) -> Option<u64> {
        if self.total_lp_shares == 0 {
            Some(0)
        } else {
            shares.checked_mul(self.total_pool)?.checked_div(self.total_lp_shares)
        }
    }
}

// ============================================================================
// ORDER-BOOK MODE: ORDERS
// ============================================================================

/// Base order structure (order-book mode)
#[derive(Debug, Clone)]
pub struct Order {
    pub version: u8,
    /// Unique order ID
    pub order_id: pallas::Base,
    /// Market this order is for
    pub market_id: pallas::Base,
    /// Order type (Back or Lay)
    pub order_type: OrderType,
    /// Outcome being bet on
    pub outcome_index: u8,
    /// Odds (in decimal form, e.g., 2.5 means 2.5:1 payout)
    pub odds: u32, // Stored as basis points (25000 = 2.5)
    /// Amount staked
    pub stake: u64,
    /// Potential liability (for lay orders)
    pub liability: u64,
    /// User placing the order
    pub user_pub: PublicKey,
    /// Order state
    pub state: OrderState,
    /// Created at block
    pub created_at: u64,
    /// Nullifier to prevent double-spending
    pub nullifier: pallas::Base,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

impl Order {
    /// Create a new back order
    pub fn new_back(
        market_id: pallas::Base,
        outcome_index: u8,
        odds: u32,
        stake: u64,
        user_pub: PublicKey,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Self {
        let order_id =
            poseidon_hash([market_id, pallas::Base::from(odds as u64), pallas::Base::from(stake)]);

        let nullifier =
            poseidon_hash([order_id, user_pub.x().expect("pk not identity"), pallas::Base::from(current_block)]);

        Self {
            version: 0,
            order_id,
            market_id,
            order_type: OrderType::Back,
            outcome_index,
            odds,
            stake,
            liability: 0, // Back doesn't have liability
            user_pub,
            state: OrderState::Open,
            created_at: current_block,
            nullifier,
            instance_seed,
        }
    }

    /// Create a new lay order
    pub fn new_lay(
        market_id: pallas::Base,
        outcome_index: u8,
        odds: u32,
        stake: u64,
        user_pub: PublicKey,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Self {
        let order_id =
            poseidon_hash([market_id, pallas::Base::from(odds as u64), pallas::Base::from(stake)]);

        // Liability = stake * (odds - 1)
        let liability = (stake * ((odds - 10000) as u64)) / 10000;

        let nullifier =
            poseidon_hash([order_id, user_pub.x().expect("pk not identity"), pallas::Base::from(current_block)]);

        Self {
            version: 0,
            order_id,
            market_id,
            order_type: OrderType::Lay,
            outcome_index,
            odds,
            stake,
            liability,
            user_pub,
            state: OrderState::Open,
            created_at: current_block,
            nullifier,
            instance_seed,
        }
    }

    /// Calculate potential payout for back order
    pub fn back_payout(&self) -> u64 {
        // Payout = stake * odds (in basis points)
        (self.stake * (self.odds as u64)) / 10000
    }

    /// Check if this order matches another (odds are compatible)
    pub fn matches(&self, other: &Order) -> bool {
        // For a match:
        // - Must be opposite types (back vs lay)
        // - Must be same market and outcome
        // - Lay's odds must be >= back's odds (lay offers worse odds)
        if self.market_id != other.market_id || self.outcome_index != other.outcome_index {
            return false
        }

        if self.order_type == other.order_type {
            return false
        }

        // Find which is back and which is lay
        let (back_odds, lay_odds) = if self.order_type == OrderType::Back {
            (self.odds, other.odds)
        } else {
            (other.odds, self.odds)
        };

        // Back's odds should be >= lay's odds (back gets equal or better)
        lay_odds >= back_odds
    }
}

// ============================================================================
// ORDER-BOOK MODE: MATCHES
// ============================================================================

/// A matched bet between back and lay
#[derive(Debug, Clone)]
pub struct Match {
    pub version: u8,
    /// Unique match ID
    pub match_id: pallas::Base,
    /// Market this match is for
    pub market_id: pallas::Base,
    /// Outcome that was bet on
    pub outcome_index: u8,
    /// Execution odds
    pub odds: u32,
    /// Stake from back side
    pub back_stake: u64,
    /// Liability from lay side
    pub lay_liability: u64,
    /// Back user
    pub back_user: PublicKey,
    /// Lay user
    pub lay_user: PublicKey,
    /// Commission taken by exchange
    pub commission: u64,
    /// Match state
    pub state: MatchState,
    /// Created at block
    pub created_at: u64,
}

impl Match {
    /// Create a new match from back and lay orders
    pub fn new(
        match_id: pallas::Base,
        market_id: pallas::Base,
        outcome_index: u8,
        odds: u32,
        back_order: &Order,
        lay_order: &Order,
        commission: u64,
        current_block: u64,
    ) -> Self {
        Self {
            version: 0,
            match_id,
            market_id,
            outcome_index,
            odds,
            back_stake: back_order.stake,
            lay_liability: lay_order.liability,
            back_user: back_order.user_pub,
            lay_user: lay_order.user_pub,
            commission,
            state: MatchState::Pending,
            created_at: current_block,
        }
    }

    /// Calculate winnings if outcome wins for back
    pub fn back_winnings(&self) -> u64 {
        // Back wins: gets stake back + (stake * odds) - commission
        // Simplified: payout = back_stake * odds / 10000
        (self.back_stake * (self.odds as u64)) / 10000
    }
}

// ============================================================================
// AMM MODE: POSITIONS AND LP SHARES
// ============================================================================

/// A position/bet in an AMM pool market
#[derive(Debug, Clone)]
pub struct Position {
    pub version: u8,
    /// Unique position identifier
    pub position_id: pallas::Base,
    /// Market this position is for
    pub market_id: pallas::Base,
    /// Owner of this position
    pub owner: PublicKey,
    /// The outcome this position represents (0 = first outcome, etc.)
    pub outcome: u8,
    /// Amount of tokens wagered
    pub amount: u64,
    /// Potential payout at time of purchase
    pub potential_payout: u64,
    /// Position state
    pub state: PositionState,
    /// Block height when position was created
    pub created_at: u64,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

impl Position {
    /// Create a new position
    pub fn new(
        market_id: pallas::Base,
        owner: PublicKey,
        outcome: u8,
        amount: u64,
        potential_payout: u64,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Self {
        let position_id = poseidon_hash([
            market_id,
            owner.x().expect("pk not identity"),
            owner.y().expect("pk not identity"),
            pallas::Base::from(outcome as u64),
            pallas::Base::from(amount),
            pallas::Base::from(current_block),
        ]);

        Self {
            version: 0,
            position_id,
            market_id,
            owner,
            outcome,
            amount,
            potential_payout,
            state: PositionState::Active,
            created_at: current_block,
            instance_seed,
        }
    }
}

/// Liquidity provider share in an AMM pool
#[derive(Debug, Clone)]
pub struct LpShare {
    pub version: u8,
    /// Unique LP share identifier
    pub lp_share_id: pallas::Base,
    /// Market this LP is for
    pub market_id: pallas::Base,
    /// LP provider public key
    pub provider: PublicKey,
    /// Number of LP shares owned
    pub shares: u64,
    /// Fees earned (available for withdrawal)
    pub earned_fees: u64,
    /// LP share state
    pub state: LpShareState,
    /// Block height when LP was created
    pub created_at: u64,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

impl LpShare {
    /// Create a new LP share
    pub fn new(
        market_id: pallas::Base,
        provider: PublicKey,
        shares: u64,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Self {
        let lp_share_id = poseidon_hash([
            market_id,
            provider.x().expect("pk not identity"),
            provider.y().expect("pk not identity"),
            pallas::Base::from(shares),
            pallas::Base::from(current_block),
        ]);

        Self {
            version: 0,
            lp_share_id,
            market_id,
            provider,
            shares,
            earned_fees: 0,
            state: LpShareState::Active,
            created_at: current_block,
            instance_seed,
        }
    }
}

// ============================================================================
// PARAMS AND UPDATES
// ============================================================================

/// Parameters for CreateMarketV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateMarketParamsV1 {
    /// Market description
    pub description: String,
    /// Available outcomes
    pub outcomes: Vec<String>,
    /// Oracle contract ID for resolution
    pub oracle_id: pallas::Base,
    /// Commission rate in basis points
    pub commission_bp: u32,
    /// Market type (0 = order-book, 1 = AMM pool)
    pub market_type: u8,
    /// Protocol fee in basis points (AMM mode only, 0 to use default)
    pub protocol_fee: u32,
    /// LP fee in basis points (AMM mode only, 0 to use default)
    pub lp_fee: u32,
    /// Market duration in blocks
    pub duration_blocks: u64,
    /// Creator public key
    pub creator_pub: PublicKey,
    /// Signature from creator over market params
    pub signature: Signature,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

/// Update from CreateMarketV1
#[derive(Debug, Clone)]
pub struct CreateMarketUpdateV1 {
    pub market_id: pallas::Base,
    pub creator: PublicKey,
    pub description: String,
    pub outcomes: Vec<String>,
    pub oracle_id: pallas::Base,
    pub commission_bp: u32,
    pub market_type: MarketType,
    pub protocol_fee: u32,
    pub lp_fee: u32,
    pub close_block: u64,
    pub instance_seed: [u8; 32],
}

// --------------------------------------------------------------------------
// Order-book mode params
// --------------------------------------------------------------------------

/// Parameters for PlaceBackV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceBackParamsV1 {
    /// Market to bet on
    pub market_id: pallas::Base,
    /// Outcome index
    pub outcome_index: u8,
    /// Odds in basis points (e.g., 25000 = 2.5:1)
    pub odds: u32,
    /// Amount to stake
    pub stake: u64,
    /// User public key
    pub user_pub: PublicKey,
    /// Signature over the bet commitment
    pub signature: Signature,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

/// Update from PlaceBackV1
#[derive(Debug, Clone)]
pub struct PlaceBackUpdateV1 {
    pub order_id: pallas::Base,
    pub market_id: pallas::Base,
    pub outcome_index: u8,
    pub odds: u32,
    pub stake: u64,
    pub user_pub: PublicKey,
    pub nullifier: pallas::Base,
    pub instance_seed: [u8; 32],
}

/// Parameters for PlaceLayV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceLayParamsV1 {
    /// Market to bet against
    pub market_id: pallas::Base,
    /// Outcome index
    pub outcome_index: u8,
    /// Odds in basis points (e.g., 25000 = 2.5:1)
    pub odds: u32,
    /// Amount to stake
    pub stake: u64,
    /// User public key
    pub user_pub: PublicKey,
    /// Signature over the bet commitment
    pub signature: Signature,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

/// Update from PlaceLayV1
#[derive(Debug, Clone)]
pub struct PlaceLayUpdateV1 {
    pub order_id: pallas::Base,
    pub market_id: pallas::Base,
    pub outcome_index: u8,
    pub odds: u32,
    pub stake: u64,
    pub liability: u64,
    pub user_pub: PublicKey,
    pub nullifier: pallas::Base,
    pub instance_seed: [u8; 32],
}

/// Parameters for MatchOrdersV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MatchOrdersParamsV1 {
    /// Market ID
    pub market_id: pallas::Base,
    /// Back order ID
    pub back_order_id: pallas::Base,
    /// Lay order ID
    pub lay_order_id: pallas::Base,
    /// Execution odds
    pub odds: u32,
    /// User public key (the matcher)
    pub user_pub: PublicKey,
    /// Signature from matcher
    pub signature: Signature,
}

/// Update from MatchOrdersV1
#[derive(Debug, Clone)]
pub struct MatchOrdersUpdateV1 {
    pub match_id: pallas::Base,
    pub market_id: pallas::Base,
    pub back_order_id: pallas::Base,
    pub lay_order_id: pallas::Base,
    pub odds: u32,
    pub back_stake: u64,
    pub lay_liability: u64,
    pub commission: u64,
}

// --------------------------------------------------------------------------
// AMM mode params
// --------------------------------------------------------------------------

/// Parameters for BuyPositionV1 (AMM mode)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BuyPositionParamsV1 {
    /// Market to buy position in
    pub market_id: pallas::Base,
    /// Which outcome to bet on (0, 1, 2, ...)
    pub outcome: u8,
    /// Amount to spend
    pub amount: u64,
    /// Minimum payout acceptable (slippage protection)
    pub min_payout: u64,
    /// Owner public key
    pub owner: PublicKey,
    /// Value commitment for the bet amount
    pub value_commit: pallas::Point,
    /// Signature over the bet commitment
    pub signature: Signature,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

/// Update from BuyPositionV1
#[derive(Debug, Clone)]
pub struct BuyPositionUpdateV1 {
    pub position_id: pallas::Base,
    pub market_id: pallas::Base,
    pub owner: PublicKey,
    pub outcome: u8,
    pub amount: u64,
    pub payout: u64,
    pub created_at: u64,
    pub instance_seed: [u8; 32],
}

/// Parameters for AddLiquidityV1 (AMM mode)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddLiquidityParamsV1 {
    /// Market to add liquidity to
    pub market_id: pallas::Base,
    /// Amount to add
    pub amount: u64,
    /// Provider public key
    pub provider: PublicKey,
    /// Value commitment
    pub value_commit: pallas::Point,
    /// Signature over liquidity commitment
    pub signature: Signature,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

/// Update from AddLiquidityV1
#[derive(Debug, Clone)]
pub struct AddLiquidityUpdateV1 {
    pub lp_share_id: pallas::Base,
    pub market_id: pallas::Base,
    pub provider: PublicKey,
    pub amount: u64,
    pub shares_minted: u64,
    pub fees_earned: u64,
    pub created_at: u64,
    pub instance_seed: [u8; 32],
}

/// Parameters for RemoveLiquidityV1 (AMM mode)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RemoveLiquidityParamsV1 {
    /// Market to remove liquidity from
    pub market_id: pallas::Base,
    /// LP share ID to remove
    pub lp_share_id: pallas::Base,
    /// Provider public key (access control)
    pub provider: PublicKey,
    /// Signature over removal request
    pub signature: Signature,
}

/// Update from RemoveLiquidityV1
#[derive(Debug, Clone)]
pub struct RemoveLiquidityUpdateV1 {
    pub market_id: pallas::Base,
    pub lp_share_id: pallas::Base,
    pub provider: PublicKey,
    pub shares_burned: u64,
    pub payout: u64,
    pub fees_withdrawn: u64,
}

/// Parameters for ClaimWinningsV1 (AMM mode)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimWinningsParamsV1 {
    /// Position ID to claim
    pub position_id: pallas::Base,
    /// Market ID
    pub market_id: pallas::Base,
    /// Winning outcome index (public input for ZK proof)
    pub winning_outcome: u8,
    /// Owner public key (access control)
    pub owner: PublicKey,
    /// ZK proof of position ownership and winning outcome
    pub proof: Vec<u8>,
}

/// Update from ClaimWinningsV1
#[derive(Debug, Clone)]
pub struct ClaimWinningsUpdateV1 {
    pub position_id: pallas::Base,
    pub payout: u64,
    pub claimed: bool,
}

// --------------------------------------------------------------------------
// Common params
// --------------------------------------------------------------------------

/// Parameters for ResolveMarketV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ResolveMarketParamsV1 {
    /// Market to resolve
    pub market_id: pallas::Base,
    /// Winning outcome index
    pub winning_outcome: u8,
    /// Oracle public key
    pub oracle_pub: PublicKey,
    /// Oracle signature verifying the result
    pub oracle_signature: Signature,
}

/// Update from ResolveMarketV1
#[derive(Debug, Clone)]
pub struct ResolveMarketUpdateV1 {
    pub market_id: pallas::Base,
    pub winning_outcome: u8,
    pub resolved_at_block: u64,
}

/// Parameters for SettleMarketV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleMarketParamsV1 {
    /// Market to settle
    pub market_id: pallas::Base,
    /// Match IDs to settle (order-book mode)
    pub match_ids: Vec<pallas::Base>,
}

/// Update from SettleMarketV1
#[derive(Debug, Clone)]
pub struct SettleMarketUpdateV1 {
    pub market_id: pallas::Base,
    pub match_ids: Vec<pallas::Base>,
    pub settled_count: u64,
    pub total_payout: u64,
    pub total_commission: u64,
}

/// Parameters for CancelOrderV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelOrderParamsV1 {
    /// Order to cancel
    pub order_id: pallas::Base,
    /// User public key
    pub user_pub: PublicKey,
    /// Signature from user
    pub signature: Signature,
}

/// Update from CancelOrderV1
#[derive(Debug, Clone)]
pub struct CancelOrderUpdateV1 {
    pub order_id: pallas::Base,
    pub refund_amount: u64,
}

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default protocol fee in basis points (1%)
pub const DEFAULT_PROTOCOL_FEE: u32 = 100;
/// Default LP fee in basis points (2%)
pub const DEFAULT_LP_FEE: u32 = 200;
/// Minimum protocol fee (0.1%)
pub const MIN_PROTOCOL_FEE: u32 = 10;
/// Maximum protocol fee (10%)
pub const MAX_PROTOCOL_FEE: u32 = 1000;
/// Maximum number of outcomes in a market
pub const MAX_OUTCOMES: u8 = 20;

// ============================================================================
// RHO-CALCULUS EXPLICIT ENCODE/DECODE
// ============================================================================
// Per type-system.md §2.2: bytes round-trip is forbidden.
// Per contract-wasm-type-system.md §3.1: SHALL use explicit encode/decode.

impl Position {
    pub const ENCODED_SIZE: usize = 155;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(155);
        b.push(self.version);
        b.extend_from_slice(&self.position_id.to_repr());
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.owner.to_bytes());
        b.push(self.outcome);
        b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.potential_payout.to_le_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.instance_seed);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 155 { return Err(ContractError::IoError(format!("Position: expected 155 bytes, got {}", data.len()))); }
        Ok(Position {
            version: data[0],
            position_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Position: invalid position_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Position: invalid market_id".into()))?,
            owner: PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Position: invalid owner: {}", e)))?,
            outcome: data[97],
            amount: u64::from_le_bytes(data[98..106].try_into().unwrap()),
            potential_payout: u64::from_le_bytes(data[106..114].try_into().unwrap()),
            state: PositionState::try_from(data[114])?,
            created_at: u64::from_le_bytes(data[115..123].try_into().unwrap()),
            instance_seed: data[123..155].try_into().unwrap(),
        })
    }
}

impl LpShare {
    pub const ENCODED_SIZE: usize = 154;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(154);
        b.push(self.version);
        b.extend_from_slice(&self.lp_share_id.to_repr());
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.provider.to_bytes());
        b.extend_from_slice(&self.shares.to_le_bytes());
        b.extend_from_slice(&self.earned_fees.to_le_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.instance_seed);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 154 { return Err(ContractError::IoError(format!("LpShare: expected 154 bytes, got {}", data.len()))); }
        Ok(LpShare {
            version: data[0],
            lp_share_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("LpShare: invalid lp_share_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("LpShare: invalid market_id".into()))?,
            provider: PublicKey::from_bytes(data[65..97].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("LpShare: invalid provider: {}", e)))?,
            shares: u64::from_le_bytes(data[97..105].try_into().unwrap()),
            earned_fees: u64::from_le_bytes(data[105..113].try_into().unwrap()),
            state: LpShareState::try_from(data[113])?,
            created_at: u64::from_le_bytes(data[114..122].try_into().unwrap()),
            instance_seed: data[122..154].try_into().unwrap(),
        })
    }
}

impl Market {
    pub fn encode(&self) -> Vec<u8> {
        let outcomes_bytes: usize = self.outcomes.iter().map(|s| 1 + s.len()).sum();
        let cap = 203 + self.description.len() + outcomes_bytes + self.outcome_pools.len() * 8;
        let mut b = Vec::with_capacity(cap);
        b.push(self.version);
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.creator.to_bytes());
        // description (u8-prefixed String)
        b.push(self.description.len() as u8);
        b.extend_from_slice(self.description.as_bytes());
        // outcomes (u8 count, each u8-prefixed String)
        b.push(self.outcomes.len() as u8);
        for s in &self.outcomes { b.push(s.len() as u8); b.extend_from_slice(s.as_bytes()); }
        b.extend_from_slice(&self.oracle_id.to_repr());
        b.extend_from_slice(&self.commission_bp.to_le_bytes());
        b.push(self.market_type as u8);
        b.push(self.state as u8);
        b.extend_from_slice(&self.back_volume.to_le_bytes());
        b.extend_from_slice(&self.lay_volume.to_le_bytes());
        b.extend_from_slice(&self.matched_volume.to_le_bytes());
        b.extend_from_slice(&self.total_pool.to_le_bytes());
        b.extend_from_slice(&self.total_lp_shares.to_le_bytes());
        b.push(self.outcome_pools.len() as u8);
        for v in &self.outcome_pools { b.extend_from_slice(&v.to_le_bytes()); }
        b.extend_from_slice(&self.protocol_fee.to_le_bytes());
        b.extend_from_slice(&self.lp_fee.to_le_bytes());
        b.extend_from_slice(&self.close_block.to_le_bytes());
        b.push(self.resolved_at.is_some() as u8);
        if let Some(r) = self.resolved_at { b.extend_from_slice(&r.to_le_bytes()); }
        b.push(self.winning_outcome.is_some() as u8);
        if let Some(w) = self.winning_outcome { b.push(w); }
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.instance_seed);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 203 { return Err(ContractError::IoError(format!("Market: expected at least 203 bytes, got {}", data.len()))); }
        let version = data[0];
        let market_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Market: invalid market_id".into()))?;
        let creator = PublicKey::from_bytes(data[33..65].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Market: invalid creator: {}", e)))?;
        let desc_len = data[65] as usize;
        if data.len() < 66 + desc_len { return Err(ContractError::IoError("Market: data too short for description".into())); }
        let description = String::from_utf8(data[66..66 + desc_len].to_vec()).map_err(|e| ContractError::IoError(format!("Market: invalid description: {}", e)))?;
        let out_pos = 66 + desc_len;
        let out_count = data[out_pos] as usize;
        let mut pos = out_pos + 1;
        let mut outcomes = Vec::with_capacity(out_count);
        for _ in 0..out_count {
            if data.len() < pos + 1 { return Err(ContractError::IoError("Market: data too short for outcome".into())); }
            let slen = data[pos] as usize;
            pos += 1;
            if data.len() < pos + slen { return Err(ContractError::IoError("Market: outcome data truncated".into())); }
            outcomes.push(String::from_utf8(data[pos..pos + slen].to_vec()).map_err(|e| ContractError::IoError(format!("Market: invalid outcome: {}", e)))?);
            pos += slen;
        }
        if data.len() < pos + 32 + 4 + 1 + 1 + 40 { return Err(ContractError::IoError("Market: data too short for numeric fields".into())); }
        let oracle_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[pos..pos+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Market: invalid oracle_id".into()))?; pos += 32;
        let commission_bp = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()); pos += 4;
        let market_type = MarketType::try_from(data[pos])?; pos += 1;
        let state = MarketState::try_from(data[pos])?; pos += 1;
        let back_volume = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let lay_volume = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let matched_volume = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let total_pool = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let total_lp_shares = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let pool_count = data[pos] as usize; pos += 1;
        if data.len() < pos + pool_count * 8 { return Err(ContractError::IoError("Market: data too short for outcome_pools".into())); }
        let mut outcome_pools = Vec::with_capacity(pool_count);
        for _ in 0..pool_count { outcome_pools.push(u64::from_le_bytes(data[pos..pos+8].try_into().unwrap())); pos += 8; }
        if data.len() < pos + 4 + 4 + 8 + 1 + 1 + 8 + 32 { return Err(ContractError::IoError("Market: data too short for trailing fields".into())); }
        let protocol_fee = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()); pos += 4;
        let lp_fee = u32::from_le_bytes(data[pos..pos+4].try_into().unwrap()); pos += 4;
        let close_block = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let has_resolved = data[pos] != 0; pos += 1;
        let resolved_at = if has_resolved { let v = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8; Some(v) } else { None };
        let has_winner = data[pos] != 0; pos += 1;
        let winning_outcome = if has_winner { let v = data[pos]; pos += 1; Some(v) } else { None };
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let instance_seed: [u8; 32] = data[pos..pos+32].try_into().unwrap();
        Ok(Market { version, market_id, creator, description, outcomes, oracle_id, commission_bp, market_type, state, back_volume, lay_volume, matched_volume, total_pool, total_lp_shares, outcome_pools, protocol_fee, lp_fee, close_block, resolved_at, winning_outcome, created_at, instance_seed })
    }
}

impl Order {
    pub const ENCODED_SIZE: usize = 192;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(192);
        b.push(self.version);
        b.extend_from_slice(&self.order_id.to_repr());
        b.extend_from_slice(&self.market_id.to_repr());
        b.push(self.order_type as u8);
        b.push(self.outcome_index);
        b.extend_from_slice(&self.odds.to_le_bytes());
        b.extend_from_slice(&self.stake.to_le_bytes());
        b.extend_from_slice(&self.liability.to_le_bytes());
        b.extend_from_slice(&self.user_pub.to_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.nullifier.to_repr());
        b.extend_from_slice(&self.instance_seed);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 192 { return Err(ContractError::IoError(format!("Order: expected 192 bytes, got {}", data.len()))); }
        Ok(Order {
            version: data[0],
            order_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Order: invalid order_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Order: invalid market_id".into()))?,
            order_type: OrderType::try_from(data[65])?,
            outcome_index: data[66],
            odds: u32::from_le_bytes(data[67..71].try_into().unwrap()),
            stake: u64::from_le_bytes(data[71..79].try_into().unwrap()),
            liability: u64::from_le_bytes(data[79..87].try_into().unwrap()),
            user_pub: PublicKey::from_bytes(data[87..119].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Order: invalid user_pub: {}", e)))?,
            state: OrderState::try_from(data[119])?,
            created_at: u64::from_le_bytes(data[120..128].try_into().unwrap()),
            nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[128..160].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Order: invalid nullifier".into()))?,
            instance_seed: data[160..192].try_into().unwrap(),
        })
    }
}

impl Match {
    pub const ENCODED_SIZE: usize = 167;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(167);
        b.push(self.version);
        b.extend_from_slice(&self.match_id.to_repr());
        b.extend_from_slice(&self.market_id.to_repr());
        b.push(self.outcome_index);
        b.extend_from_slice(&self.odds.to_le_bytes());
        b.extend_from_slice(&self.back_stake.to_le_bytes());
        b.extend_from_slice(&self.lay_liability.to_le_bytes());
        b.extend_from_slice(&self.back_user.to_bytes());
        b.extend_from_slice(&self.lay_user.to_bytes());
        b.extend_from_slice(&self.commission.to_le_bytes());
        b.push(self.state as u8);
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 167 { return Err(ContractError::IoError(format!("Match: expected 167 bytes, got {}", data.len()))); }
        Ok(Match {
            version: data[0],
            match_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[1..33].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Match: invalid match_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[33..65].try_into().unwrap())).ok_or_else(|| ContractError::IoError("Match: invalid market_id".into()))?,
            outcome_index: data[65],
            odds: u32::from_le_bytes(data[66..70].try_into().unwrap()),
            back_stake: u64::from_le_bytes(data[70..78].try_into().unwrap()),
            lay_liability: u64::from_le_bytes(data[78..86].try_into().unwrap()),
            back_user: PublicKey::from_bytes(data[86..118].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Match: invalid back_user: {}", e)))?,
            lay_user: PublicKey::from_bytes(data[118..150].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("Match: invalid lay_user: {}", e)))?,
            commission: u64::from_le_bytes(data[150..158].try_into().unwrap()),
            state: MatchState::try_from(data[158])?,
            created_at: u64::from_le_bytes(data[159..167].try_into().unwrap()),
        })
    }
}

// --- Bridge update structs ---

impl CancelOrderUpdateV1 {
    pub const ENCODED_SIZE: usize = 40;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(40); b.extend_from_slice(&self.order_id.to_repr()); b.extend_from_slice(&self.refund_amount.to_le_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 40 { return Err(ContractError::IoError(format!("CancelOrderUpdateV1: expected 40 bytes, got {}", data.len()))); }
        Ok(CancelOrderUpdateV1 { order_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CancelOrderUpdateV1: invalid order_id".into()))?, refund_amount: u64::from_le_bytes(data[32..40].try_into().unwrap()) })
    }
}

impl ResolveMarketUpdateV1 {
    pub const ENCODED_SIZE: usize = 41;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(41); b.extend_from_slice(&self.market_id.to_repr()); b.push(self.winning_outcome); b.extend_from_slice(&self.resolved_at_block.to_le_bytes()); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 41 { return Err(ContractError::IoError(format!("ResolveMarketUpdateV1: expected 41 bytes, got {}", data.len()))); }
        Ok(ResolveMarketUpdateV1 { market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ResolveMarketUpdateV1: invalid market_id".into()))?, winning_outcome: data[32], resolved_at_block: u64::from_le_bytes(data[33..41].try_into().unwrap()) })
    }
}

impl ClaimWinningsUpdateV1 {
    pub const ENCODED_SIZE: usize = 41;
    pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(41); b.extend_from_slice(&self.position_id.to_repr()); b.extend_from_slice(&self.payout.to_le_bytes()); b.push(self.claimed as u8); b }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 41 { return Err(ContractError::IoError(format!("ClaimWinningsUpdateV1: expected 41 bytes, got {}", data.len()))); }
        Ok(ClaimWinningsUpdateV1 { position_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("ClaimWinningsUpdateV1: invalid position_id".into()))?, payout: u64::from_le_bytes(data[32..40].try_into().unwrap()), claimed: data[40] != 0 })
    }
}

impl SettleMarketUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(56 + self.match_ids.len() * 32);
        b.extend_from_slice(&self.market_id.to_repr());
        b.push(self.match_ids.len() as u8);
        for id in &self.match_ids { b.extend_from_slice(&id.to_repr()); }
        b.extend_from_slice(&self.settled_count.to_le_bytes());
        b.extend_from_slice(&self.total_payout.to_le_bytes());
        b.extend_from_slice(&self.total_commission.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 56 { return Err(ContractError::IoError(format!("SettleMarketUpdateV1: expected at least 56 bytes, got {}", data.len()))); }
        let market_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("SettleMarketUpdateV1: invalid market_id".into()))?;
        let count = data[32] as usize;
        let expected = 33 + count * 32 + 24;
        if data.len() != expected { return Err(ContractError::IoError(format!("SettleMarketUpdateV1: expected {} bytes for {} matches, got {}", expected, count, data.len()))); }
        let mut match_ids = Vec::with_capacity(count);
        for i in 0..count { let s = 33 + i * 32; match_ids.push(Option::<pallas::Base>::from(pallas::Base::from_repr(data[s..s+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError(format!("SettleMarketUpdateV1: invalid match_id[{}]", i)))?); }
        let pos = 33 + count * 32;
        let settled_count = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        let total_payout = u64::from_le_bytes(data[pos+8..pos+16].try_into().unwrap());
        let total_commission = u64::from_le_bytes(data[pos+16..pos+24].try_into().unwrap());
        Ok(SettleMarketUpdateV1 { market_id, match_ids, settled_count, total_payout, total_commission })
    }
}

impl CreateMarketUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let outcomes_bytes: usize = self.outcomes.iter().map(|s| 1 + s.len()).sum();
        let cap = 107 + self.description.len() + outcomes_bytes;
        let mut b = Vec::with_capacity(cap);
        b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.creator.to_bytes());
        b.push(self.description.len() as u8);
        b.extend_from_slice(self.description.as_bytes());
        b.push(self.outcomes.len() as u8);
        for s in &self.outcomes { b.push(s.len() as u8); b.extend_from_slice(s.as_bytes()); }
        b.extend_from_slice(&self.oracle_id.to_repr());
        b.extend_from_slice(&self.commission_bp.to_le_bytes());
        b.push(self.market_type as u8);
        b.extend_from_slice(&self.protocol_fee.to_le_bytes());
        b.extend_from_slice(&self.lp_fee.to_le_bytes());
        b.extend_from_slice(&self.close_block.to_le_bytes());
        b.extend_from_slice(&self.instance_seed);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 107 { return Err(ContractError::IoError(format!("CreateMarketUpdateV1: expected at least 107 bytes, got {}", data.len()))); }
        let market_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CreateMarketUpdateV1: invalid market_id".into()))?;
        let creator = PublicKey::from_bytes(data[32..64].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("CreateMarketUpdateV1: invalid creator: {}", e)))?;
        let desc_len = data[64] as usize;
        if data.len() < 65 + desc_len { return Err(ContractError::IoError("CreateMarketUpdateV1: data too short for description".into())); }
        let description = String::from_utf8(data[65..65+desc_len].to_vec()).map_err(|e| ContractError::IoError(format!("CreateMarketUpdateV1: invalid description: {}", e)))?;
        let pos = 65 + desc_len;
        let out_count = data[pos] as usize;
        let mut p = pos + 1;
        let mut outcomes = Vec::with_capacity(out_count);
        for _ in 0..out_count {
            if data.len() < p + 1 { return Err(ContractError::IoError("CreateMarketUpdateV1: outcome data truncated".into())); }
            let slen = data[p] as usize; p += 1;
            outcomes.push(String::from_utf8(data[p..p+slen].to_vec()).map_err(|e| ContractError::IoError(format!("CreateMarketUpdateV1: invalid outcome: {}", e)))?);
            p += slen;
        }
        if data.len() < p + 75 { return Err(ContractError::IoError("CreateMarketUpdateV1: data too short for tail".into())); }
        let oracle_id = Option::<pallas::Base>::from(pallas::Base::from_repr(data[p..p+32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("CreateMarketUpdateV1: invalid oracle_id".into()))?; p += 32;
        let commission_bp = u32::from_le_bytes(data[p..p+4].try_into().unwrap()); p += 4;
        let market_type = MarketType::try_from(data[p])?; p += 1;
        let protocol_fee = u32::from_le_bytes(data[p..p+4].try_into().unwrap()); p += 4;
        let lp_fee = u32::from_le_bytes(data[p..p+4].try_into().unwrap()); p += 4;
        let close_block = u64::from_le_bytes(data[p..p+8].try_into().unwrap()); p += 8;
        let instance_seed: [u8; 32] = data[p..p+32].try_into().unwrap();
        Ok(CreateMarketUpdateV1 { market_id, creator, description, outcomes, oracle_id, commission_bp, market_type, protocol_fee, lp_fee, close_block, instance_seed })
    }
}

impl PlaceBackUpdateV1 {
    pub const ENCODED_SIZE: usize = 173;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(173);
        b.extend_from_slice(&self.order_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr());
        b.push(self.outcome_index); b.extend_from_slice(&self.odds.to_le_bytes());
        b.extend_from_slice(&self.stake.to_le_bytes()); b.extend_from_slice(&self.user_pub.to_bytes());
        b.extend_from_slice(&self.nullifier.to_repr()); b.extend_from_slice(&self.instance_seed);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 173 { return Err(ContractError::IoError(format!("PlaceBackUpdateV1: expected 173 bytes, got {}", data.len()))); }
        Ok(PlaceBackUpdateV1 {
            order_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PlaceBackUpdateV1: invalid order_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PlaceBackUpdateV1: invalid market_id".into()))?,
            outcome_index: data[64], odds: u32::from_le_bytes(data[65..69].try_into().unwrap()),
            stake: u64::from_le_bytes(data[69..77].try_into().unwrap()),
            user_pub: PublicKey::from_bytes(data[77..109].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PlaceBackUpdateV1: invalid user_pub: {}", e)))?,
            nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[109..141].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PlaceBackUpdateV1: invalid nullifier".into()))?,
            instance_seed: data[141..173].try_into().unwrap(),
        })
    }
}

impl PlaceLayUpdateV1 {
    pub const ENCODED_SIZE: usize = 181;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(181);
        b.extend_from_slice(&self.order_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr());
        b.push(self.outcome_index); b.extend_from_slice(&self.odds.to_le_bytes());
        b.extend_from_slice(&self.stake.to_le_bytes()); b.extend_from_slice(&self.liability.to_le_bytes());
        b.extend_from_slice(&self.user_pub.to_bytes()); b.extend_from_slice(&self.nullifier.to_repr());
        b.extend_from_slice(&self.instance_seed); b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 181 { return Err(ContractError::IoError(format!("PlaceLayUpdateV1: expected 181 bytes, got {}", data.len()))); }
        Ok(PlaceLayUpdateV1 {
            order_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PlaceLayUpdateV1: invalid order_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PlaceLayUpdateV1: invalid market_id".into()))?,
            outcome_index: data[64], odds: u32::from_le_bytes(data[65..69].try_into().unwrap()),
            stake: u64::from_le_bytes(data[69..77].try_into().unwrap()), liability: u64::from_le_bytes(data[77..85].try_into().unwrap()),
            user_pub: PublicKey::from_bytes(data[85..117].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("PlaceLayUpdateV1: invalid user_pub: {}", e)))?,
            nullifier: Option::<pallas::Base>::from(pallas::Base::from_repr(data[117..149].try_into().unwrap())).ok_or_else(|| ContractError::IoError("PlaceLayUpdateV1: invalid nullifier".into()))?,
            instance_seed: data[149..181].try_into().unwrap(),
        })
    }
}

impl MatchOrdersUpdateV1 {
    pub const ENCODED_SIZE: usize = 156;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(156);
        b.extend_from_slice(&self.match_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.back_order_id.to_repr()); b.extend_from_slice(&self.lay_order_id.to_repr());
        b.extend_from_slice(&self.odds.to_le_bytes()); b.extend_from_slice(&self.back_stake.to_le_bytes());
        b.extend_from_slice(&self.lay_liability.to_le_bytes()); b.extend_from_slice(&self.commission.to_le_bytes());
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 156 { return Err(ContractError::IoError(format!("MatchOrdersUpdateV1: expected 156 bytes, got {}", data.len()))); }
        Ok(MatchOrdersUpdateV1 {
            match_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("MatchOrdersUpdateV1: invalid match_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("MatchOrdersUpdateV1: invalid market_id".into()))?,
            back_order_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[64..96].try_into().unwrap())).ok_or_else(|| ContractError::IoError("MatchOrdersUpdateV1: invalid back_order_id".into()))?,
            lay_order_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[96..128].try_into().unwrap())).ok_or_else(|| ContractError::IoError("MatchOrdersUpdateV1: invalid lay_order_id".into()))?,
            odds: u32::from_le_bytes(data[128..132].try_into().unwrap()), back_stake: u64::from_le_bytes(data[132..140].try_into().unwrap()),
            lay_liability: u64::from_le_bytes(data[140..148].try_into().unwrap()), commission: u64::from_le_bytes(data[148..156].try_into().unwrap()),
        })
    }
}

impl BuyPositionUpdateV1 {
    pub const ENCODED_SIZE: usize = 153;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(153);
        b.extend_from_slice(&self.position_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.owner.to_bytes()); b.push(self.outcome);
        b.extend_from_slice(&self.amount.to_le_bytes()); b.extend_from_slice(&self.payout.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes()); b.extend_from_slice(&self.instance_seed); b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 153 { return Err(ContractError::IoError(format!("BuyPositionUpdateV1: expected 153 bytes, got {}", data.len()))); }
        Ok(BuyPositionUpdateV1 {
            position_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BuyPositionUpdateV1: invalid position_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("BuyPositionUpdateV1: invalid market_id".into()))?,
            owner: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("BuyPositionUpdateV1: invalid owner: {}", e)))?,
            outcome: data[96], amount: u64::from_le_bytes(data[97..105].try_into().unwrap()),
            payout: u64::from_le_bytes(data[105..113].try_into().unwrap()), created_at: u64::from_le_bytes(data[113..121].try_into().unwrap()),
            instance_seed: data[121..153].try_into().unwrap(),
        })
    }
}

impl AddLiquidityUpdateV1 {
    pub const ENCODED_SIZE: usize = 160;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(160);
        b.extend_from_slice(&self.lp_share_id.to_repr()); b.extend_from_slice(&self.market_id.to_repr());
        b.extend_from_slice(&self.provider.to_bytes()); b.extend_from_slice(&self.amount.to_le_bytes());
        b.extend_from_slice(&self.shares_minted.to_le_bytes()); b.extend_from_slice(&self.fees_earned.to_le_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes()); b.extend_from_slice(&self.instance_seed); b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 160 { return Err(ContractError::IoError(format!("AddLiquidityUpdateV1: expected 160 bytes, got {}", data.len()))); }
        Ok(AddLiquidityUpdateV1 {
            lp_share_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("AddLiquidityUpdateV1: invalid lp_share_id".into()))?,
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("AddLiquidityUpdateV1: invalid market_id".into()))?,
            provider: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("AddLiquidityUpdateV1: invalid provider: {}", e)))?,
            amount: u64::from_le_bytes(data[96..104].try_into().unwrap()), shares_minted: u64::from_le_bytes(data[104..112].try_into().unwrap()),
            fees_earned: u64::from_le_bytes(data[112..120].try_into().unwrap()), created_at: u64::from_le_bytes(data[120..128].try_into().unwrap()),
            instance_seed: data[128..160].try_into().unwrap(),
        })
    }
}

impl RemoveLiquidityUpdateV1 {
    pub const ENCODED_SIZE: usize = 120;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(120);
        b.extend_from_slice(&self.market_id.to_repr()); b.extend_from_slice(&self.lp_share_id.to_repr());
        b.extend_from_slice(&self.provider.to_bytes()); b.extend_from_slice(&self.shares_burned.to_le_bytes());
        b.extend_from_slice(&self.payout.to_le_bytes()); b.extend_from_slice(&self.fees_withdrawn.to_le_bytes()); b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != 120 { return Err(ContractError::IoError(format!("RemoveLiquidityUpdateV1: expected 120 bytes, got {}", data.len()))); }
        Ok(RemoveLiquidityUpdateV1 {
            market_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[0..32].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RemoveLiquidityUpdateV1: invalid market_id".into()))?,
            lp_share_id: Option::<pallas::Base>::from(pallas::Base::from_repr(data[32..64].try_into().unwrap())).ok_or_else(|| ContractError::IoError("RemoveLiquidityUpdateV1: invalid lp_share_id".into()))?,
            provider: PublicKey::from_bytes(data[64..96].try_into().unwrap()).map_err(|e| ContractError::IoError(format!("RemoveLiquidityUpdateV1: invalid provider: {}", e)))?,
            shares_burned: u64::from_le_bytes(data[96..104].try_into().unwrap()), payout: u64::from_le_bytes(data[104..112].try_into().unwrap()),
            fees_withdrawn: u64::from_le_bytes(data[112..120].try_into().unwrap()),
        })
    }
}
