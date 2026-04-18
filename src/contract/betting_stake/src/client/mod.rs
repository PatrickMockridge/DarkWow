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

//! Betting Stake Contract Client API
//!
//! This module provides the client-side API for building Betting Stake contract calls.

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};

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

/// Own stake with secret for claiming
pub struct OwnStake {
    pub note: StakeNote,
    pub secret: SecretKey,
}

/// Builder for creating initialize calls (house only)
pub struct InitializeV1Builder {
    betting_contract_id: pallas::Base,
    house_edge_bp: u32,
    risk_profile: u8,
}

impl InitializeV1Builder {
    /// Create a new InitializeV1 builder
    pub fn new(betting_contract_id: pallas::Base, house_edge_bp: u32, risk_profile: u8) -> Self {
        Self { betting_contract_id, house_edge_bp, risk_profile }
    }

    /// Build the initialize parameters
    pub fn build(&self) -> InitializeParamsV1 {
        InitializeParamsV1 {
            betting_contract_id: self.betting_contract_id,
            house_edge_bp: self.house_edge_bp,
            risk_profile: self.risk_profile,
            signature: pallas::Base::zero(), // Filled by house wallet
        }
    }
}

/// Builder for creating stake calls
pub struct StakeV1Builder {
    table_id: pallas::Base,
    staker_pub: PublicKey,
    amount: u64,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
}

impl StakeV1Builder {
    /// Create a new StakeV1 builder
    pub fn new(
        table_id: pallas::Base,
        staker_pub: PublicKey,
        amount: u64,
        spend_hook: pallas::Base,
        user_data: pallas::Base,
    ) -> Self {
        Self { table_id, staker_pub, amount, spend_hook, user_data }
    }

    /// Build the stake parameters and note
    pub fn build(&self) -> (StakeParamsV1, OwnStake) {
        let signature = poseidon_hash([
            self.table_id,
            self.staker_pub.x(),
            self.staker_pub.y(),
            pallas::Base::from(self.amount),
        ]);

        let params = StakeParamsV1 {
            table_id: self.table_id,
            staker_pub: self.staker_pub,
            amount: self.amount,
            signature,
            spend_hook: self.spend_hook,
            user_data: self.user_data,
        };

        let stake_id =
            poseidon_hash([self.table_id, self.staker_pub.x(), self.staker_pub.y(), pallas::Base::from(self.amount)]);

        let note = StakeNote {
            stake_id,
            table_id: self.table_id,
            staker_pub: self.staker_pub,
            original_amount: self.amount,
            current_amount: self.amount,
            accumulated_earnings: 0,
            is_active: true,
        };

        let own_stake = OwnStake { note, secret: SecretKey::random(&mut rand::rngs::OsRng) };

        (params, own_stake)
    }
}

/// Builder for creating unstake calls
pub struct UnstakeV1Builder {
    stake_id: pallas::Base,
    spend_hook: pallas::Base,
    user_data: pallas::Base,
}

impl UnstakeV1Builder {
    /// Create a new UnstakeV1 builder
    pub fn new(stake_id: pallas::Base, spend_hook: pallas::Base, user_data: pallas::Base) -> Self {
        Self { stake_id, spend_hook, user_data }
    }

    /// Build the unstake parameters
    pub fn build(&self) -> UnstakeParamsV1 {
        UnstakeParamsV1 {
            stake_id: self.stake_id,
            signature: pallas::Base::zero(), // Filled by wallet
            spend_hook: self.spend_hook,
            user_data: self.user_data,
        }
    }
}

/// Builder for creating claim earnings calls
pub struct ClaimEarningsV1Builder {
    stake_id: pallas::Base,
}

impl ClaimEarningsV1Builder {
    /// Create a new ClaimEarningsV1 builder
    pub fn new(stake_id: pallas::Base) -> Self {
        Self { stake_id }
    }

    /// Build the claim earnings parameters
    pub fn build(&self) -> ClaimEarningsParamsV1 {
        ClaimEarningsParamsV1 {
            stake_id: self.stake_id,
            signature: pallas::Base::zero(), // Filled by wallet
        }
    }
}

/// Builder for creating update risk calls (house only)
pub struct UpdateRiskV1Builder {
    table_id: pallas::Base,
    payout_amount: u64,
    house_share: u64,
}

impl UpdateRiskV1Builder {
    /// Create a new UpdateRiskV1 builder
    pub fn new(table_id: pallas::Base, payout_amount: u64, house_share: u64) -> Self {
        Self { table_id, payout_amount, house_share }
    }

    /// Build the update risk parameters
    pub fn build(&self) -> UpdateRiskParamsV1 {
        UpdateRiskParamsV1 {
            table_id: self.table_id,
            payout_amount: self.payout_amount,
            house_share: self.house_share,
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