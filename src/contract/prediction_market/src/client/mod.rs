/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (0xthis0and0that0etc) 2020-2026 Dyne.org foundation
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

//! Prediction Market Client API
//!
//! This module provides the client-side API for building Prediction Market contract calls.

use darkfi_sdk::{
    crypto::{pasta_prelude::{Field, Group}, PublicKey},
    pasta::pallas,
};

use crate::model::{
    derive_market_id, derive_position_id, CreateMarketParamsV1, CreatePositionParamsV1,
};
use crate::{DEFAULT_LP_FEE, DEFAULT_PROTOCOL_FEE};

/// Builder for creating prediction market calls
pub struct CreateMarketV1Builder {
    creator: PublicKey,
    question: Vec<u8>,
    resolve_time: u64,
    betting_closes: u64,
    num_outcomes: u8,
    protocol_fee: u32,
    lp_fee: u32,
    token_id: pallas::Base,
    oracle_pubkey: PublicKey,
}

impl CreateMarketV1Builder {
    /// Create a new market builder
    pub fn new(creator: PublicKey, question: String, resolve_time: u64) -> Self {
        Self {
            creator,
            question: question.into_bytes(),
            resolve_time,
            betting_closes: 0,
            num_outcomes: 2, // Default to YES/NO
            protocol_fee: DEFAULT_PROTOCOL_FEE,
            lp_fee: DEFAULT_LP_FEE,
            token_id: pallas::Base::zero(), // DARK token
            oracle_pubkey: creator, // For MVP, creator is oracle
        }
    }

    /// Set betting closes time
    pub fn betting_closes(mut self, betting_closes: u64) -> Self {
        self.betting_closes = betting_closes;
        self
    }

    /// Set number of outcomes (2 for YES/NO, N for discrete)
    pub fn num_outcomes(mut self, num_outcomes: u8) -> Self {
        self.num_outcomes = num_outcomes;
        self
    }

    /// Set custom protocol fee
    pub fn protocol_fee(mut self, fee: u32) -> Self {
        self.protocol_fee = fee;
        self
    }

    /// Set custom LP fee
    pub fn lp_fee(mut self, fee: u32) -> Self {
        self.lp_fee = fee;
        self
    }

    /// Set token ID for betting
    pub fn token_id(mut self, token_id: pallas::Base) -> Self {
        self.token_id = token_id;
        self
    }

    /// Set oracle public key
    pub fn oracle_pubkey(mut self, oracle_pubkey: PublicKey) -> Self {
        self.oracle_pubkey = oracle_pubkey;
        self
    }

    /// Build the create market parameters
    pub fn build(&self) -> CreateMarketParamsV1 {
        CreateMarketParamsV1 {
            question: self.question.clone(),
            resolve_time: self.resolve_time,
            betting_closes: self.betting_closes,
            num_outcomes: self.num_outcomes,
            protocol_fee: self.protocol_fee,
            lp_fee: self.lp_fee,
            token_id: self.token_id,
            oracle_pubkey: self.oracle_pubkey,
            oracle_signature: pallas::Base::zero(), // TODO: Sign with oracle key
        }
    }

    /// Derive the market ID for this market
    pub fn market_id(&self) -> crate::model::MarketId {
        derive_market_id(
            &self.oracle_pubkey,
            &self.question,
            self.resolve_time,
            self.token_id,
            &self.oracle_pubkey,
        )
    }
}

/// Builder for creating position/bet calls
pub struct CreatePositionV1Builder {
    market_id: crate::model::MarketId,
    owner: PublicKey,
    outcome: u8,
    amount: u64,
    secret_nonce: pallas::Base,
}

impl CreatePositionV1Builder {
    /// Create a new position builder
    pub fn new(market_id: crate::model::MarketId, owner: PublicKey, outcome: u8, amount: u64) -> Self {
        Self {
            market_id,
            owner,
            outcome,
            amount,
            secret_nonce: pallas::Base::random(&mut rand::thread_rng()),
        }
    }

    /// Set secret nonce (for reproducibility)
    pub fn secret_nonce(mut self, nonce: pallas::Base) -> Self {
        self.secret_nonce = nonce;
        self
    }

    /// Build the create position parameters
    pub fn build(&self) -> CreatePositionParamsV1 {
        let value_commit = pallas::Point::identity(); // TODO: Proper commitment

        CreatePositionParamsV1 {
            market_id: self.market_id,
            outcome: self.outcome,
            amount: self.amount,
            owner: self.owner,
            value_commit,
            signature: pallas::Base::zero(), // TODO: Sign the commitment
        }
    }

    /// Derive the position ID
    pub fn position_id(&self) -> crate::model::PositionId {
        derive_position_id(
            self.market_id,
            &self.owner,
            self.outcome,
            self.amount,
            self.secret_nonce,
        )
    }
}

/// Client-side position tracking
#[derive(Debug, Clone)]
pub struct TrackedPosition {
    pub position_id: crate::model::PositionId,
    pub market_id: crate::model::MarketId,
    pub outcome: u8,
    pub amount: u64,
    pub created_at: u64,
}

/// Client-side market tracking
#[derive(Debug, Clone)]
pub struct TrackedMarket {
    pub market_id: crate::model::MarketId,
    pub question: String,
    pub num_outcomes: u8,
    pub total_pool: u64,
    pub state: crate::model::MarketState,
    pub resolved_outcome: Option<u8>,
}
