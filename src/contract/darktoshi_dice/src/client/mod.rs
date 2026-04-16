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

//! DarkToshi Dice Client API
//!
//! This module provides the client-side API for building Dice contract calls.

pub mod commit_bet_v1;

use darkfi_sdk::{
    crypto::{pedersen_commitment_u64, poseidon_hash, PublicKey, ScalarBlind, SecretKey},
    pasta::pallas,
};
use darkfi_sdk::crypto::pasta_prelude::Field;

use crate::model::{derive_bet_id, derive_nullifier, BetId, CommitBetParamsV1};
use crate::{DEFAULT_HOUSE_EDGE, MAX_TARGET};

/// Client-side bet note for tracking bets
#[derive(Debug, Clone)]
pub struct DiceNote {
    pub bet_id: BetId,
    pub bet_value: u64,
    pub target: u8,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub token_id: pallas::Base,
    pub created_at: u64,
}

/// Own bet with secret for claiming
pub struct OwnBet {
    pub note: DiceNote,
    pub secret: SecretKey,
    pub value_commit: pallas::Point,
}

/// Builder for creating commit bet calls
pub struct CommitBetV1Builder {
    player_pub: PublicKey,
    bet_value: u64,
    target: u8,
    secret_nonce: pallas::Base,
    blind: pallas::Base,
    token_id: pallas::Base,
    house_edge: u32,
    confirmation_depth: u8,
}

impl CommitBetV1Builder {
    /// Create a new CommitBet builder
    pub fn new(player_pub: PublicKey, bet_value: u64, target: u8) -> Self {
        Self {
            player_pub,
            bet_value,
            target,
            secret_nonce: pallas::Base::random(&mut rand::thread_rng()),
            blind: pallas::Base::random(&mut rand::thread_rng()),
            token_id: pallas::Base::zero(), // DARK token for now
            house_edge: DEFAULT_HOUSE_EDGE,
            confirmation_depth: 3, // Default 3 blocks for confirmation
        }
    }

    /// Set the secret nonce (for reproducibility)
    pub fn secret_nonce(mut self, secret_nonce: pallas::Base) -> Self {
        self.secret_nonce = secret_nonce;
        self
    }

    /// Set the blinding factor (for reproducibility)
    pub fn blind(mut self, blind: pallas::Base) -> Self {
        self.blind = blind;
        self
    }

    /// Set the token ID
    pub fn token_id(mut self, token_id: pallas::Base) -> Self {
        self.token_id = token_id;
        self
    }

    /// Set the house edge (in basis points, e.g., 200 = 2%)
    pub fn house_edge(mut self, house_edge: u32) -> Self {
        self.house_edge = house_edge;
        self
    }

    /// Set the confirmation depth
    pub fn confirmation_depth(mut self, depth: u8) -> Self {
        self.confirmation_depth = depth;
        self
    }

    /// Build the commit bet parameters
    pub fn build(&self) -> (CommitBetParamsV1, OwnBet) {
        let bet_id = derive_bet_id(
            &self.player_pub,
            self.bet_value,
            self.target,
            self.secret_nonce,
            self.blind,
            self.token_id,
        );

        let nullifier = derive_nullifier(bet_id, self.secret_nonce);

        // Create proper value commitment using Pedersen commitment
        let value_commit = pedersen_commitment_u64(self.bet_value, ScalarBlind::from(self.bet_value));

        // Create signature as poseidon hash of bet parameters
        let signature = poseidon_hash([
            pallas::Base::from(self.bet_value),
            self.secret_nonce,
            self.blind,
        ]);

        let params = CommitBetParamsV1 {
            player_pub: self.player_pub,
            bet_value: self.bet_value,
            target: self.target,
            secret_nonce: self.secret_nonce,
            blind: self.blind,
            token_id: self.token_id,
            value_commit,
            signature,
            house_edge: self.house_edge,
            confirmation_depth: self.confirmation_depth,
        };

        let note = DiceNote {
            bet_id,
            bet_value: self.bet_value,
            target: self.target,
            secret_nonce: self.secret_nonce,
            blind: self.blind,
            token_id: self.token_id,
            created_at: 0, // Filled by contract
        };

        let own_bet = OwnBet { note, secret: SecretKey::random(&mut rand::rngs::OsRng), value_commit };

        (params, own_bet)
    }
}

/// Validate target is in valid range
pub fn validate_target(target: u8) -> Result<(), crate::error::DiceError> {
    if target == 0 || target > MAX_TARGET {
        return Err(crate::error::DiceError::InvalidTarget)
    }
    Ok(())
}
