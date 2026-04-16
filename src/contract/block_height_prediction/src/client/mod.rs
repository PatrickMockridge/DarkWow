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

//! Block Height Prediction Contract Client API
//!
//! This module provides the client-side API for building Block Height Prediction contract calls.

pub mod create_market_v1;
pub mod create_position_v1;

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};
use darkfi_sdk::crypto::pasta_prelude::Group;

use crate::model::{
    CancelMarketParamsV1, ClaimWinningsParamsV1, CreateMarketParamsV1, CreatePositionParamsV1,
    MarketId, PositionId, PositionType,
};

/// Client-side market note for tracking markets
#[derive(Debug, Clone)]
pub struct MarketNote {
    pub market_id: MarketId,
    pub creator: PublicKey,
    pub target_time: u64,
    pub base_block_height: u64,
    pub total_pool: u64,
    pub state: u8,
    pub token_id: pallas::Base,
}

/// Client-side position note for tracking bets
#[derive(Debug, Clone)]
pub struct PositionNote {
    pub position_id: PositionId,
    pub market_id: MarketId,
    pub owner: PublicKey,
    pub predicted_height: u64,
    pub tolerance: u8,
    pub position_type: PositionType,
    pub amount: u64,
    pub potential_payout: u64,
    pub claimed: bool,
}

/// Own position with secret for claiming
pub struct OwnPosition {
    pub note: PositionNote,
    pub secret: SecretKey,
}

/// Builder for creating market calls
pub struct CreateMarketV1Builder {
    creator: PublicKey,
    target_time: u64,
    initial_prediction: u64,
    confirmation_depth: u8,
    protocol_fee: u32,
    token_id: pallas::Base,
}

impl CreateMarketV1Builder {
    /// Create a new CreateMarketV1 builder
    pub fn new(creator: PublicKey, target_time: u64, token_id: pallas::Base) -> Self {
        Self {
            creator,
            target_time,
            initial_prediction: 0,
            confirmation_depth: 6,
            protocol_fee: 100,
            token_id,
        }
    }

    /// Set initial prediction (expected block height at target time)
    pub fn initial_prediction(mut self, height: u64) -> Self {
        self.initial_prediction = height;
        self
    }

    /// Set confirmation depth (higher = more secure)
    pub fn confirmation_depth(mut self, depth: u8) -> Self {
        self.confirmation_depth = depth;
        self
    }

    /// Set protocol fee in basis points
    pub fn protocol_fee(mut self, fee_bp: u32) -> Self {
        self.protocol_fee = fee_bp;
        self
    }

    /// Build the create market parameters
    pub fn build(&self) -> CreateMarketParamsV1 {
        CreateMarketParamsV1 {
            creator: self.creator,
            target_time: self.target_time,
            initial_prediction: self.initial_prediction,
            confirmation_depth: self.confirmation_depth,
            protocol_fee: self.protocol_fee,
            token_id: self.token_id,
        }
    }
}

/// Builder for creating position/bet calls
pub struct CreatePositionV1Builder {
    market_id: MarketId,
    predicted_height: u64,
    tolerance: u8,
    position_type: PositionType,
    amount: u64,
    owner: PublicKey,
    secret_nonce: pallas::Base,
}

impl CreatePositionV1Builder {
    /// Create a new CreatePositionV1 builder
    pub fn new(market_id: MarketId, predicted_height: u64, position_type: PositionType, amount: u64, owner: PublicKey) -> Self {
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        Self {
            market_id,
            predicted_height,
            tolerance: 5,
            position_type,
            amount,
            owner,
            secret_nonce: secret.inner(),
        }
    }

    /// Set tolerance range (+/- blocks for "close" payout)
    pub fn tolerance(mut self, tolerance: u8) -> Self {
        self.tolerance = tolerance;
        self
    }

    /// Set secret nonce (for reproducibility)
    pub fn secret_nonce(mut self, nonce: pallas::Base) -> Self {
        self.secret_nonce = nonce;
        self
    }

    /// Build the create position parameters
    pub fn build(&self) -> (CreatePositionParamsV1, OwnPosition) {
        let position_id = poseidon_hash([
            self.market_id,
            self.owner.x(),
            self.owner.y(),
            pallas::Base::from(self.predicted_height),
            pallas::Base::from(self.position_type as u8 as u64),
            pallas::Base::from(self.amount),
            self.secret_nonce,
        ]);

        let signature = poseidon_hash([
            self.market_id,
            self.owner.x(),
            self.owner.y(),
            pallas::Base::from(self.amount),
        ]);

        let params = CreatePositionParamsV1 {
            market_id: self.market_id,
            predicted_height: self.predicted_height,
            tolerance: self.tolerance,
            position_type: self.position_type as u8,
            amount: self.amount,
            owner: self.owner,
            value_commit: pallas::Point::identity(), // Filled by ZK
            signature,
        };

        let note = PositionNote {
            position_id,
            market_id: self.market_id,
            owner: self.owner,
            predicted_height: self.predicted_height,
            tolerance: self.tolerance,
            position_type: self.position_type,
            amount: self.amount,
            potential_payout: 0, // Calculated at resolution
            claimed: false,
        };

        let own_position = OwnPosition {
            note,
            secret: SecretKey::from(self.secret_nonce),
        };

        (params, own_position)
    }
}

/// Builder for claiming winnings
pub struct ClaimWinningsV1Builder {
    position_id: PositionId,
    market_id: MarketId,
    owner: PublicKey,
}

impl ClaimWinningsV1Builder {
    /// Create a new ClaimWinningsV1 builder
    pub fn new(position_id: PositionId, market_id: MarketId, owner: PublicKey) -> Self {
        Self { position_id, market_id, owner }
    }

    /// Build the claim winnings parameters
    pub fn build(&self) -> ClaimWinningsParamsV1 {
        ClaimWinningsParamsV1 {
            position_id: self.position_id,
            market_id: self.market_id,
            owner: self.owner,
            proof: vec![], // Filled by ZK proof
        }
    }
}

/// Builder for cancelling a market
pub struct CancelMarketV1Builder {
    market_id: MarketId,
    canceller: PublicKey,
}

impl CancelMarketV1Builder {
    /// Create a new CancelMarketV1 builder
    pub fn new(market_id: MarketId, canceller: PublicKey) -> Self {
        Self { market_id, canceller }
    }

    /// Build the cancel market parameters
    pub fn build(&self) -> CancelMarketParamsV1 {
        CancelMarketParamsV1 { market_id: self.market_id, canceller: self.canceller }
    }
}

/// Validate confirmation depth
pub fn validate_confirmation_depth(depth: u8) -> Result<(), crate::error::BlockHeightPredictionError> {
    if depth == 0 || depth > 10 {
        return Err(crate::error::BlockHeightPredictionError::InvalidConfirmationDepth)
    }
    Ok(())
}

/// Validate tolerance
pub fn validate_tolerance(tolerance: u8) -> Result<(), crate::error::BlockHeightPredictionError> {
    if tolerance > 50 {
        return Err(crate::error::BlockHeightPredictionError::InvalidTolerance)
    }
    Ok(())
}

/// Validate bet amount
pub fn validate_amount(amount: u64) -> Result<(), crate::error::BlockHeightPredictionError> {
    if amount == 0 {
        return Err(crate::error::BlockHeightPredictionError::BetValueTooSmall)
    }
    Ok(())
}

/// Calculate potential payout (estimate before resolution)
pub fn estimate_payout(amount: u64, pool_ratio: u64) -> u64 {
    // Simplified: assume equal distribution
    amount * pool_ratio / 10000
}