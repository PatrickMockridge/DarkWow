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

//! Betting Stake Contract Client API
//!
//! This module provides the client-side API for building Betting Stake contract calls.

pub mod zkbins;

pub mod proof_gen;

use dwow_sdk::{
    crypto::{
        pasta_prelude::Field,
        poseidon_hash,
        schnorr::{SchnorrSecret, Signature},
        PublicKey, SecretKey,
    },
    pasta::pallas,
};
use pasta_curves::group::Group;
use rand::rngs::OsRng;
use rand::SeedableRng;
use dwow_serial::serialize;

use crate::model::{
    ClaimEarningsParamsV1, InitializeParamsV1, StakeParamsV1, UnstakeParamsV1, UpdateRiskParamsV1,
};

/// Client-side stake note for tracking stakes
#[derive(Debug, Clone)]
pub struct StakeNote {
    pub stake_id: pallas::Base,
    pub table_id: pallas::Base,
    pub staker_pub: PublicKey,
    pub original_amount: u64,
    pub current_amount: u64,
    pub accumulated_earnings: u64,
    pub is_active: bool,
}

/// Own stake with secret for signing
pub struct OwnStake {
    pub note: StakeNote,
    pub secret: SecretKey,
}

/// Builder for creating initialize calls (house only)
pub struct InitializeV1Builder {
    betting_contract_id: pallas::Base,
    house_edge_bp: u32,
    risk_profile: u8,
    nonce: pallas::Base,
}

impl InitializeV1Builder {
    /// Create a new InitializeV1 builder
    pub fn new(betting_contract_id: pallas::Base, house_edge_bp: u32, risk_profile: u8) -> Self {
        let nonce = if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            pallas::Base::random(&mut rng)
        } else {
            pallas::Base::random(&mut OsRng)
        };
        Self { betting_contract_id, house_edge_bp, risk_profile, nonce }
    }

    /// Set a specific nonce (default is random)
    pub fn nonce(mut self, nonce: pallas::Base) -> Self {
        self.nonce = nonce;
        self
    }

    /// Build the initialize parameters
    pub fn build(&self) -> InitializeParamsV1 {
        InitializeParamsV1 {
            betting_contract_id: self.betting_contract_id,
            house_edge_bp: self.house_edge_bp,
            risk_profile: self.risk_profile,
            nonce: self.nonce,
            signature: Signature::dummy(), // Filled by house wallet
            instance_seed: [0u8; 32],
        }
    }
}

/// Builder for creating stake calls
pub struct StakeV1Builder {
    table_id: pallas::Base,
    staker_pub: PublicKey,
    staker_secret: SecretKey,
    amount: u64,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    nonce: pallas::Base,
    value_commit: pallas::Point,
}

impl StakeV1Builder {
    /// Create a new StakeV1 builder
    pub fn new(
        table_id: pallas::Base,
        staker_pub: PublicKey,
        staker_secret: SecretKey,
        amount: u64,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
    ) -> Self {
        let nonce = if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            pallas::Base::random(&mut rng)
        } else {
            pallas::Base::random(&mut OsRng)
        };
        Self { table_id, staker_pub, staker_secret, amount, spend_hook, user_data, nonce, value_commit: pallas::Point::identity() }
    }

    /// Set a specific nonce (default is random)
    pub fn nonce(mut self, nonce: pallas::Base) -> Self {
        self.nonce = nonce;
        self
    }

    /// Build the stake parameters and note
    #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
    pub fn build(&self) -> (StakeParamsV1, OwnStake) {
        // Create signature message
        let signature_msg = serialize(&(self.table_id, self.staker_pub.x().expect("pk not identity"), self.staker_pub.y().expect("pk not identity"), self.amount));
        let _ = self.staker_secret.sign(&signature_msg);

        let params = StakeParamsV1 {
            table_id: self.table_id,
            staker_pub: self.staker_pub,
            amount: self.amount,
            nonce: self.nonce,
            value_commit: self.value_commit,
            spend_hook: self.spend_hook,
            user_data: self.user_data,
            instance_seed: [0u8; 32],
            staker_nullifier: pallas::Base::zero(),
        };

        let stake_id = poseidon_hash([
            pallas::Base::from(4),
            self.table_id,
            self.staker_pub.x().expect("pk not identity"),
            self.staker_pub.y().expect("pk not identity"),
            pallas::Base::from(self.amount),
            pallas::Base::from(self.nonce),
        ]);

        let note = StakeNote {
            stake_id,
            table_id: self.table_id,
            staker_pub: self.staker_pub,
            original_amount: self.amount,
            current_amount: self.amount,
            accumulated_earnings: 0,
            is_active: true,
        };

        let own_stake = OwnStake { note, secret: self.staker_secret.clone() };

        (params, own_stake)
    }
}

/// Builder for creating unstake calls
pub struct UnstakeV1Builder {
    stake_id: pallas::Base,
    staker_secret: SecretKey,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
    table_id: pallas::Base,
    staker_pub: PublicKey,
    original_amount: u64,
    nonce: pallas::Base,
    value_commit: pallas::Point,
}

impl UnstakeV1Builder {
    /// Create a new UnstakeV1 builder
    pub fn new(stake_id: pallas::Base, staker_secret: SecretKey, spend_hook: pallas::Base, user_data: pallas::Base) -> Self {
        let (staker_pub, nonce) = if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (PublicKey::from_secret(SecretKey::random(&mut rng)), pallas::Base::random(&mut rng))
        } else {
            (PublicKey::from_secret(SecretKey::random(&mut OsRng)), pallas::Base::random(&mut OsRng))
        };
        Self { stake_id, staker_secret, spend_hook, user_data, table_id: pallas::Base::zero(), staker_pub, original_amount: 0, nonce, value_commit: pallas::Point::identity() }
    }

    /// Set a specific nonce (default is random)
    pub fn nonce(mut self, nonce: pallas::Base) -> Self {
        self.nonce = nonce;
        self
    }

    /// Build the unstake parameters
    pub fn build(&self) -> UnstakeParamsV1 {
        // Create signature message (stake_id)
        let signature_msg = serialize(&self.stake_id);
        let _ = self.staker_secret.sign(&signature_msg);

        UnstakeParamsV1 {
            stake_id: self.stake_id,
            table_id: self.table_id,
            staker_pub: self.staker_pub,
            original_amount: self.original_amount,
            nonce: self.nonce,
            value_commit: self.value_commit,
            spend_hook: self.spend_hook,
            user_data: self.user_data,
            staker_nullifier: pallas::Base::zero(),
        }
    }
}

/// Builder for creating claim earnings calls
pub struct ClaimEarningsV1Builder {
    stake_id: pallas::Base,
    staker_secret: SecretKey,
    table_id: pallas::Base,
    staker_pub: PublicKey,
    current_amount: u64,
    nonce: pallas::Base,
    value_commit: pallas::Point,
}

impl ClaimEarningsV1Builder {
    /// Create a new ClaimEarningsV1 builder
    pub fn new(stake_id: pallas::Base, staker_secret: SecretKey) -> Self {
        let (staker_pub, nonce) = if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            (PublicKey::from_secret(SecretKey::random(&mut rng)), pallas::Base::random(&mut rng))
        } else {
            (PublicKey::from_secret(SecretKey::random(&mut OsRng)), pallas::Base::random(&mut OsRng))
        };
        Self { stake_id, staker_secret, table_id: pallas::Base::zero(), staker_pub, current_amount: 0, nonce, value_commit: pallas::Point::identity() }
    }

    /// Set a specific nonce (default is random)
    pub fn nonce(mut self, nonce: pallas::Base) -> Self {
        self.nonce = nonce;
        self
    }

    /// Build the claim earnings parameters
    pub fn build(&self) -> ClaimEarningsParamsV1 {
        // Create signature message (stake_id)
        let signature_msg = serialize(&self.stake_id);
        let _ = self.staker_secret.sign(&signature_msg);

        ClaimEarningsParamsV1 {
            stake_id: self.stake_id,
            table_id: self.table_id,
            staker_pub: self.staker_pub,
            current_amount: self.current_amount,
            nonce: self.nonce,
            value_commit: self.value_commit,
            staker_nullifier: pallas::Base::zero(),
        }
    }
}

/// Builder for creating update risk calls (house only)
pub struct UpdateRiskV1Builder {
    table_id: pallas::Base,
    payout_amount: u64,
    house_share: u64,
    betting_contract_id: pallas::Base,
    nonce: pallas::Base,
}

impl UpdateRiskV1Builder {
    /// Create a new UpdateRiskV1 builder
    pub fn new(table_id: pallas::Base, payout_amount: u64, house_share: u64) -> Self {
        let nonce = if crate::deterministic_zk_enabled() {
            let mut rng = rand::rngs::StdRng::seed_from_u64(0);
            pallas::Base::random(&mut rng)
        } else {
            pallas::Base::random(&mut OsRng)
        };
        Self { table_id, payout_amount, house_share, betting_contract_id: pallas::Base::zero(), nonce }
    }

    /// Set a specific nonce (default is random)
    pub fn nonce(mut self, nonce: pallas::Base) -> Self {
        self.nonce = nonce;
        self
    }

    /// Build the update risk parameters
    pub fn build(&self) -> UpdateRiskParamsV1 {
        UpdateRiskParamsV1 {
            table_id: self.table_id,
            payout_amount: self.payout_amount,
            house_share: self.house_share,
            betting_contract_id: self.betting_contract_id,
            nonce: self.nonce,
        }
    }
}

/// Validate stake amount
pub fn validate_stake_amount(amount: u64) -> Result<(), crate::error::BettingStakeError> {
    if amount < 100 {
        return Err(crate::error::BettingStakeError::StakeTooSmall)
    }
    if amount > 1_000_000_000 {
        return Err(crate::error::BettingStakeError::ArithmeticOverflow)
    }
    Ok(())
}

/// Validate house edge basis points
pub fn validate_house_edge_bp(house_edge_bp: u32) -> Result<(), crate::error::BettingStakeError> {
    if house_edge_bp > 1000 {
        // Max 10%
        return Err(crate::error::BettingStakeError::StakeExceedsMaxRatio)
    }
    Ok(())
}

/// Calculate potential earnings for a stake
pub fn calculate_potential_earnings(amount: u64, house_edge_bp: u32, blocks_elapsed: u64) -> u64 {
    // Earnings = amount * house_edge_bp * blocks / (10000 * blocks_per_year)
    // Assuming ~10500000 blocks per year (1 block every 30 seconds)
    let blocks_per_year = 10500000u64;
    (amount * (house_edge_bp as u64) * blocks_elapsed) / (10000 * blocks_per_year)
}