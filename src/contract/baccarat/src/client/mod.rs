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

//! Baccarat Client API
//!
//! This module provides the client-side API for building Baccarat contract calls.

pub mod zkbins;

pub mod commit_bet;
pub mod draw_cards;
pub mod house_close;
pub mod settle_bet;

use dwow_sdk::{
    crypto::{pedersen_commitment_u64, ContractId, PublicKey, ScalarBlind, SecretKey},
    error::ContractError,
    pasta::pallas,
};
use dwow_sdk::crypto::pasta_prelude::Field;
use rand::RngCore;

use crate::model::{derive_bet_id, BetId, BetType, CommitBetParamsV1};

/// Client-side bet note for tracking bets
#[derive(Debug, Clone)]
pub struct BaccaratNote {
    pub bet_id: BetId,
    pub bet_value: u64,
    pub bet_type: BetType,
    pub secret_nonce: pallas::Base,
    pub blind: pallas::Base,
    pub token_id: pallas::Base,
    pub created_at: u64,
}

/// Own bet with secret for claiming
pub struct OwnBet {
    pub note: BaccaratNote,
    pub secret: SecretKey,
    pub value_commit: pallas::Point,
}

/// Builder for creating commit bet calls
pub struct CommitBetV1Builder {
    wallet_secret: SecretKey,
    contract_id: ContractId,
    bet_value: u64,
    bet_type: BetType,
    secret_nonce: pallas::Base,
    blind: pallas::Base,
    token_id: pallas::Base,
    house_edge: u32,
    confirmation_depth: u8,
    instance_seed: [u8; 32],
}

impl CommitBetV1Builder {
    /// Create a new CommitBet builder
    pub fn new(wallet_secret: SecretKey, contract_id: ContractId, bet_value: u64, bet_type: BetType) -> Self {
        let mut instance_seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut instance_seed);
        Self {
            wallet_secret,
            contract_id,
            bet_value,
            bet_type,
            secret_nonce: pallas::Base::random(&mut rand::thread_rng()),
            blind: pallas::Base::random(&mut rand::thread_rng()),
            token_id: pallas::Base::zero(), // DRKW token — native consensus asset
            house_edge: crate::DEFAULT_HOUSE_EDGE,
            confirmation_depth: 3, // Default 3 blocks for confirmation
            instance_seed,
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

    /// Set the house edge (in basis points, e.g., 150 = 1.5%)
    pub fn house_edge(mut self, house_edge: u32) -> Self {
        self.house_edge = house_edge;
        self
    }

    /// Set the confirmation depth
    pub fn confirmation_depth(mut self, depth: u8) -> Self {
        self.confirmation_depth = depth;
        self
    }

    /// Set the instance seed (for reproducibility)
    pub fn instance_seed(mut self, instance_seed: [u8; 32]) -> Self {
        self.instance_seed = instance_seed;
        self
    }

    /// Build the commit bet parameters
    pub fn build(&self) -> Result<(CommitBetParamsV1, OwnBet), ContractError> {
        let instance_secret = self.wallet_secret.derive_instance(&self.contract_id, &self.instance_seed)?;
        let player_pub = PublicKey::from_secret(instance_secret.clone());

        let bet_id = derive_bet_id(
            &player_pub,
            self.bet_type as u8,
            self.bet_value,
            self.secret_nonce,
            self.blind,
            self.token_id,
        );

        // Create proper value commitment using Pedersen commitment
        let value_commit = pedersen_commitment_u64(self.bet_value, ScalarBlind::from_u64(self.bet_value));

        let params = CommitBetParamsV1 {
            player_pub,
            bet_type: self.bet_type as u8,
            bet_value: self.bet_value,
            secret_nonce: self.secret_nonce,
            blind: self.blind,
            token_id: self.token_id,
            house_edge: self.house_edge,
            confirmation_depth: self.confirmation_depth,
            value_commit,
            instance_seed: self.instance_seed,
        };

        let note = BaccaratNote {
            bet_id,
            bet_value: self.bet_value,
            bet_type: self.bet_type,
            secret_nonce: self.secret_nonce,
            blind: self.blind,
            token_id: self.token_id,
            created_at: 0, // Filled by contract
        };

        let own_bet = OwnBet { note, secret: instance_secret, value_commit };

        Ok((params, own_bet))
    }
}

/// Validate bet type is valid
pub fn validate_bet_type(bet_type: u8) -> Result<(), crate::error::BaccaratError> {
    match BetType::from_u8(bet_type) {
        Some(_) => Ok(()),
        None => Err(crate::error::BaccaratError::InvalidBetType),
    }
}
