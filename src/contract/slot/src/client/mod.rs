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

//! Slot Contract Client API
//!
//! This module provides the client-side API for building Slot contract calls.

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};

use crate::model::{
    CancelSpinParamsV1, CommitSpinParamsV1, Paytable, PaytableEntry, ReelStrip,
    RevealSpinParamsV1, SettleSpinParamsV1, SpinId, Symbol,
};

/// Client-side spin note for tracking spins
#[derive(Debug, Clone)]
pub struct SpinNote {
    pub spin_id: SpinId,
    pub player_pub: PublicKey,
    pub bet_value: u64,
    pub paylines_played: u32,
    pub house_edge: u32,
    pub token_id: pallas::Base,
    pub value_commit: pallas::Point,
    pub state: u8,
    pub settle_block: u64,
    pub payout: u64,
}

/// Own spin with secrets for reveal/claim
pub struct OwnSpin {
    pub note: SpinNote,
    pub secret_nonce: SecretKey,
    pub blind: SecretKey,
}

/// Builder for creating commit spin calls
pub struct CommitSpinV1Builder {
    player_pub: PublicKey,
    bet_value: u64,
    paylines_played: u32,
    secret_nonce: pallas::Base,
    blind: pallas::Base,
    house_edge: u32,
    confirmation_depth: u8,
    token_id: pallas::Base,
}

impl CommitSpinV1Builder {
    /// Create a new CommitSpinV1 builder
    pub fn new(player_pub: PublicKey, bet_value: u64, token_id: pallas::Base) -> Self {
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        let blind_secret = SecretKey::random(&mut rand::rngs::OsRng);
        Self {
            player_pub,
            bet_value,
            paylines_played: 1,
            secret_nonce: secret.inner(),
            blind: blind_secret.inner(),
            house_edge: 500, // Default 5%
            confirmation_depth: 3,
            token_id,
        }
    }

    /// Set number of paylines to play
    pub fn paylines(mut self, paylines: u32) -> Self {
        self.paylines_played = paylines;
        self
    }

    /// Set house edge in basis points
    pub fn house_edge(mut self, house_edge: u32) -> Self {
        self.house_edge = house_edge;
        self
    }

    /// Set confirmation depth
    pub fn confirmation_depth(mut self, depth: u8) -> Self {
        self.confirmation_depth = depth;
        self
    }

    /// Set secret nonce (for reproducibility)
    pub fn secret_nonce(mut self, secret_nonce: pallas::Base) -> Self {
        self.secret_nonce = secret_nonce;
        self
    }

    /// Set blind (for reproducibility)
    pub fn blind(mut self, blind: pallas::Base) -> Self {
        self.blind = blind;
        self
    }

    /// Build the commit spin parameters
    pub fn build(&self) -> (CommitSpinParamsV1, OwnSpin) {
        // Derive spin_id
        let spin_id = poseidon_hash([
            self.player_pub.x(),
            self.player_pub.y(),
            pallas::Base::from(self.bet_value),
            pallas::Base::from(self.paylines_played as u64),
            self.secret_nonce,
            self.blind,
            self.token_id,
        ]);

        let params = CommitSpinParamsV1 {
            player_pub: self.player_pub,
            bet_value: self.bet_value,
            paylines_played: self.paylines_played,
            secret_nonce: self.secret_nonce,
            blind: self.blind,
            house_edge: self.house_edge,
            confirmation_depth: self.confirmation_depth,
            token_id: self.token_id,
            value_commit: pallas::Point::identity(), // Filled by ZK proof
        };

        let note = SpinNote {
            spin_id,
            player_pub: self.player_pub,
            bet_value: self.bet_value,
            paylines_played: self.paylines_played,
            house_edge: self.house_edge,
            token_id: self.token_id,
            value_commit: pallas::Point::identity(),
            state: 0, // Committed
            settle_block: 0, // Filled after commit
            payout: 0,
        };

        let own_spin = OwnSpin {
            note,
            secret_nonce: SecretKey::from(self.secret_nonce),
            blind: SecretKey::from(self.blind),
        };

        (params, own_spin)
    }
}

/// Builder for creating reveal spin calls
pub struct RevealSpinV1Builder {
    spin_id: SpinId,
    secret_nonce: pallas::Base,
}

impl RevealSpinV1Builder {
    /// Create a new RevealSpinV1 builder
    pub fn new(spin_id: SpinId) -> Self {
        Self { spin_id, secret_nonce: pallas::Base::zero() }
    }

    /// Set the secret nonce
    pub fn secret_nonce(mut self, secret_nonce: pallas::Base) -> Self {
        self.secret_nonce = secret_nonce;
        self
    }

    /// Build the reveal spin parameters
    pub fn build(&self) -> RevealSpinParamsV1 {
        RevealSpinParamsV1 { spin_id: self.spin_id, secret_nonce: self.secret_nonce }
    }
}

/// Builder for creating settle spin calls (house only)
pub struct SettleSpinV1Builder {
    spin_id: SpinId,
}

impl SettleSpinV1Builder {
    /// Create a new SettleSpinV1 builder
    pub fn new(spin_id: SpinId) -> Self {
        Self { spin_id }
    }

    /// Build the settle spin parameters
    pub fn build(&self) -> SettleSpinParamsV1 {
        SettleSpinParamsV1 { spin_id: self.spin_id }
    }
}

/// Builder for creating cancel spin calls (house only)
pub struct CancelSpinV1Builder {
    spin_id: SpinId,
}

impl CancelSpinV1Builder {
    /// Create a new CancelSpinV1 builder
    pub fn new(spin_id: SpinId) -> Self {
        Self { spin_id }
    }

    /// Build the cancel spin parameters
    pub fn build(&self) -> CancelSpinParamsV1 {
        CancelSpinParamsV1 { spin_id: self.spin_id }
    }
}

/// Validate bet value is within limits
pub fn validate_bet_value(bet_value: u64) -> Result<(), crate::error::SlotError> {
    if bet_value < 1 {
        return Err(crate::error::SlotError::InvalidBetValue)
    }
    if bet_value > 1_000_000_000 {
        return Err(crate::error::SlotError::BetValueOverflow)
    }
    Ok(())
}

/// Validate paylines is within limits
pub fn validate_paylines(
    paylines: u32,
    max_paylines: u32,
    game_type: u8,
) -> Result<(), crate::error::SlotError> {
    if paylines == 0 {
        return Err(crate::error::SlotError::InvalidPaylines)
    }
    // Classic slots typically have 1-3 paylines
    // Video slots can have up to 100
    if paylines > max_paylines {
        return Err(crate::error::SlotError::InvalidPaylines)
    }
    Ok(())
}

/// Validate house edge is within limits
pub fn validate_house_edge(house_edge: u32) -> Result<(), crate::error::SlotError> {
    if house_edge < 100 {
        // Minimum 1%
        return Err(crate::error::SlotError::InvalidHouseEdge)
    }
    if house_edge > 1000 {
        // Maximum 10%
        return Err(crate::error::SlotError::InvalidHouseEdge)
    }
    Ok(())
}

/// Calculate maximum potential payout
pub fn calculate_max_payout(bet_value: u64, paylines: u32, max_multiplier: u64) -> u64 {
    bet_value * (paylines as u64) * max_multiplier
}

/// Calculate total bet (bet * paylines)
pub fn calculate_total_bet(bet_value: u64, paylines: u32) -> u64 {
    bet_value * (paylines as u64)
}