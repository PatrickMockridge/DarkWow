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

//! Roulette Client API
//!
//! This module provides the client-side API for building Roulette contract calls.

pub mod zkbins;

use dwow_sdk::{
    crypto::{poseidon_hash, ContractId, PublicKey, SecretKey},
    error::ContractError,
    pasta::pallas,
};
use rand::RngCore;
use crate::model::{
    BetType, InitializeParamsV1, PlaceBetParamsV1, SpinWheelParamsV1,
    SettleBetsParamsV1, HouseCloseParamsV1,
};

/// Client-side bet note for tracking bets
#[derive(Debug, Clone)]
pub struct RouletteBetNote {
    pub bet_id: pallas::Base,
    pub table_id: pallas::Base,
    pub player_pub: PublicKey,
    pub bet_type: BetType,
    pub numbers: Vec<u8>,
    pub amount: u64,
    pub payout: u64,
    pub spin_number: u64,
    pub nullifier: pallas::Base,
    pub placed_at: u64,
}

/// Own bet with secret for claiming
pub struct OwnRouletteBet {
    pub note: RouletteBetNote,
    pub secret: SecretKey,
}

/// Builder for creating initialize table calls (house only)
pub struct InitializeV1Builder {
    wallet_secret: SecretKey,
    contract_id: ContractId,
    american_wheel: bool,
    house_capital: u64,
    max_straight_bet: u64,
    duration_blocks: u64,
    instance_seed: [u8; 32],
}

impl InitializeV1Builder {
    /// Create a new InitializeV1 builder
    pub fn new(wallet_secret: SecretKey, contract_id: ContractId, house_capital: u64) -> Self {
        let mut instance_seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut instance_seed);
        Self {
            wallet_secret,
            contract_id,
            american_wheel: false,
            house_capital,
            max_straight_bet: 1_000_000, // Default 1 DARK
            duration_blocks: 10,
            instance_seed,
        }
    }

    /// Set whether to use American wheel (38 numbers vs European 37)
    pub fn american_wheel(mut self, american_wheel: bool) -> Self {
        self.american_wheel = american_wheel;
        self
    }

    /// Set the maximum straight bet amount
    pub fn max_straight_bet(mut self, max_straight_bet: u64) -> Self {
        self.max_straight_bet = max_straight_bet;
        self
    }

    /// Set duration in blocks before spin
    pub fn duration_blocks(mut self, duration_blocks: u64) -> Self {
        self.duration_blocks = duration_blocks;
        self
    }

    /// Set the instance seed (for reproducibility)
    pub fn instance_seed(mut self, instance_seed: [u8; 32]) -> Self {
        self.instance_seed = instance_seed;
        self
    }

    /// Build the initialize parameters
    pub fn build(&self) -> Result<InitializeParamsV1, ContractError> {
        let instance_secret = self.wallet_secret.derive_instance(&self.contract_id, &self.instance_seed)?;
        let house_pub = PublicKey::from_secret(instance_secret);
        Ok(InitializeParamsV1 {
            house_pub,
            american_wheel: self.american_wheel,
            house_capital: self.house_capital,
            max_straight_bet: self.max_straight_bet,
            duration_blocks: self.duration_blocks,
            instance_seed: self.instance_seed,
        })
    }
}

/// Builder for creating place bet calls
pub struct PlaceBetV1Builder {
    table_id: pallas::Base,
    wallet_secret: SecretKey,
    contract_id: ContractId,
    bet_type: BetType,
    numbers: Vec<u8>,
    amount: u64,
    secret_nonce: pallas::Base,
    instance_seed: [u8; 32],
}

impl PlaceBetV1Builder {
    /// Create a new PlaceBetV1 builder
    pub fn new(
        table_id: pallas::Base,
        wallet_secret: SecretKey,
        contract_id: ContractId,
        bet_type: BetType,
        numbers: Vec<u8>,
        amount: u64,
    ) -> Self {
        // Generate random nonce using SecretKey::random
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        let mut instance_seed = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut instance_seed);
        Self {
            table_id,
            wallet_secret,
            contract_id,
            bet_type,
            numbers,
            amount,
            secret_nonce: *secret.inner(),
            instance_seed,
        }
    }

    /// Set the secret nonce (for reproducibility)
    pub fn secret_nonce(mut self, secret_nonce: pallas::Base) -> Self {
        self.secret_nonce = secret_nonce;
        self
    }

    /// Set the instance seed (for reproducibility)
    pub fn instance_seed(mut self, instance_seed: [u8; 32]) -> Self {
        self.instance_seed = instance_seed;
        self
    }

    /// Build the place bet parameters and note
    /// Note: signature must be created by the client wallet
    pub fn build(&self) -> Result<(PlaceBetParamsV1, OwnRouletteBet), ContractError> {
        let instance_secret = self.wallet_secret.derive_instance(&self.contract_id, &self.instance_seed)?;
        let player_pub = PublicKey::from_secret(instance_secret);

        let signature = poseidon_hash([
            self.table_id,
            player_pub.x().expect("pk not identity"),
            player_pub.y().expect("pk not identity"),
            pallas::Base::from(self.amount),
        ]);

        let params = PlaceBetParamsV1 {
            table_id: self.table_id,
            player_pub,
            bet_type: self.bet_type,
            numbers: self.numbers.clone(),
            amount: self.amount,
            signature,
            instance_seed: self.instance_seed,
        };

        // Calculate payout
        let payout = self.amount * (self.bet_type.payout_ratio() as u64);

        // Create bet_id similar to Bet::new() but without the full context
        let bet_id =
            poseidon_hash([self.table_id, player_pub.x().expect("pk not identity"), player_pub.y().expect("pk not identity"), pallas::Base::from(self.amount)]);

        // Create note for client tracking
        let note = RouletteBetNote {
            bet_id,
            table_id: self.table_id,
            player_pub,
            bet_type: self.bet_type,
            numbers: self.numbers.clone(),
            amount: self.amount,
            payout,
            spin_number: 0, // Filled by contract
            nullifier: pallas::Base::zero(), // Filled by contract
            placed_at: 0, // Filled by contract
        };

        let own_bet = OwnRouletteBet { note, secret: SecretKey::from_base(self.secret_nonce) };

        Ok((params, own_bet))
    }
}

/// Builder for creating spin wheel calls (house only)
pub struct SpinWheelV1Builder {
    table_id: pallas::Base,
    house_pub: PublicKey,
    nonce: pallas::Base,
}

impl SpinWheelV1Builder {
    /// Create a new SpinWheelV1 builder
    pub fn new(table_id: pallas::Base, house_pub: PublicKey) -> Self {
        // Generate random nonce using SecretKey::random
        let secret = SecretKey::random(&mut rand::rngs::OsRng);
        Self {
            table_id,
            house_pub,
            nonce: *secret.inner(),
        }
    }

    /// Set the nonce for randomness
    pub fn nonce(mut self, nonce: pallas::Base) -> Self {
        self.nonce = nonce;
        self
    }

    /// Build the spin wheel parameters
    /// Note: ZK proof must be created by the house wallet
    pub fn build(&self) -> SpinWheelParamsV1 {
        let (hx, hy) = self.house_pub.xy().expect("pk not identity");
        SpinWheelParamsV1 {
            table_id: self.table_id,
            nonce: self.nonce,
            house_pub_x: hx,
            house_pub_y: hy,
            spin_nullifier: pallas::Base::zero(),
        }
    }
}

/// Builder for creating settle bets calls (house only)
pub struct SettleBetsV1Builder {
    table_id: pallas::Base,
    bet_ids: Vec<pallas::Base>,
    payout: u64,
}

impl SettleBetsV1Builder {
    /// Create a new SettleBetsV1 builder
    pub fn new(table_id: pallas::Base) -> Self {
        Self { table_id, bet_ids: Vec::new(), payout: 0 }
    }

    /// Add a bet ID to settle
    pub fn add_bet(mut self, bet_id: pallas::Base) -> Self {
        self.bet_ids.push(bet_id);
        self
    }

    /// Add multiple bet IDs to settle
    pub fn add_bets(mut self, bet_ids: Vec<pallas::Base>) -> Self {
        self.bet_ids.extend(bet_ids);
        self
    }

    /// Build the settle bets parameters
    pub fn build(&self) -> SettleBetsParamsV1 {
        SettleBetsParamsV1 { table_id: self.table_id, bet_ids: self.bet_ids.clone(), payout: self.payout }
    }
}

/// Builder for creating house close calls (house only)
pub struct HouseCloseV1Builder {
    table_id: pallas::Base,
    house_pub: PublicKey,
}

impl HouseCloseV1Builder {
    /// Create a new HouseCloseV1 builder
    pub fn new(table_id: pallas::Base, house_pub: PublicKey) -> Self {
        Self { table_id, house_pub }
    }

    /// Build the house close parameters
    /// Note: ZK proof must be created by the house wallet
    pub fn build(&self) -> HouseCloseParamsV1 {
        let (hx, hy) = self.house_pub.xy().expect("pk not identity");
        HouseCloseParamsV1 {
            table_id: self.table_id,
            house_pub_x: hx,
            house_pub_y: hy,
            close_nullifier: pallas::Base::zero(),
        }
    }
}

/// Validate bet type is valid (0-7)
pub fn validate_bet_type(bet_type: u8) -> Result<(), crate::error::RouletteError> {
    match bet_type {
        0..=7 => Ok(()),
        _ => Err(crate::error::RouletteError::InvalidBetType),
    }
}

/// Validate numbers are valid for a given bet type
pub fn validate_numbers(
    bet_type: BetType,
    numbers: &[u8],
    wheel_size: u8,
) -> Result<(), crate::error::RouletteError> {
    // Check wheel bounds
    for n in numbers {
        if *n >= wheel_size {
            return Err(crate::error::RouletteError::InvalidNumbers)
        }
    }

    match bet_type {
        BetType::Straight => {
            if numbers.len() != 1 {
                return Err(crate::error::RouletteError::InvalidNumbers)
            }
        }
        BetType::Split => {
            if numbers.len() != 2 {
                return Err(crate::error::RouletteError::InvalidNumbers)
            }
        }
        BetType::Street => {
            if numbers.len() != 3 {
                return Err(crate::error::RouletteError::InvalidNumbers)
            }
        }
        BetType::Corner => {
            if numbers.len() != 4 {
                return Err(crate::error::RouletteError::InvalidNumbers)
            }
        }
        BetType::SixLine => {
            if numbers.len() != 6 {
                return Err(crate::error::RouletteError::InvalidNumbers)
            }
        }
        BetType::Dozen | BetType::Column | BetType::EvenMoney => {
            if numbers.len() != 1 {
                return Err(crate::error::RouletteError::InvalidNumbers)
            }
        }
    }

    Ok(())
}

/// Calculate potential payout for a bet
pub fn calculate_payout(amount: u64, bet_type: BetType) -> u64 {
    amount * (bet_type.payout_ratio() as u64)
}

// ZK proof generation modules
pub mod place_bet;
pub mod spin_wheel;
pub mod house_close;
pub mod settle_bet;
