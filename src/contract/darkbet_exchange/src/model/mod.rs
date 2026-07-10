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
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

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

/// Market type: determines the matching mechanism
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum MarketType {
    /// Order-book style: back/lay orders matched peer-to-peer via DEX
    OrderBook = 0,
    /// AMM pool style: positions priced via constant-product formula
    AmmPool = 1,
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

/// LP share state (AMM mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum LpShareState {
    /// LP shares are active
    Active = 0,
    /// Liquidity removed
    Removed = 1,
}

// ============================================================================
// MARKET
// ============================================================================

/// A betting market supporting both order-book and AMM modes
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
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
