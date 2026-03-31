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
 * You have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Roulette Contract Model
//!
//! Data structures for the roulette game.

use darkfi_sdk::{
    crypto::{draw_single, pasta_prelude::PrimeField, poseidon_hash, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::{EUROPEAN_HOUSE_EDGE_BP, EUROPEAN_WHEEL_SIZE, AMERICAN_HOUSE_EDGE_BP, AMERICAN_WHEEL_SIZE};

// ============================================================================
// BET TYPES
// ============================================================================

/// Roulette bet types with their payouts
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
#[repr(u8)]
pub enum BetType {
    /// Single number (35:1)
    Straight = 0,
    /// Two adjacent numbers (17:1)
    Split = 1,
    /// Three numbers in a row (11:1)
    Street = 2,
    /// Four numbers in a corner (8:1)
    Corner = 3,
    /// Six numbers (two rows) (5:1)
    SixLine = 4,
    /// Dozen: 1-12, 13-24, 25-36 (2:1)
    Dozen = 5,
    /// Column: left, middle, right (2:1)
    Column = 6,
    /// Even money: Red/Black, Odd/Even, Low/High (1:1)
    EvenMoney = 7,
}

impl BetType {
    /// Get payout ratio (excludes original stake)
    pub fn payout_ratio(&self) -> u32 {
        match self {
            Self::Straight => 35,
            Self::Split => 17,
            Self::Street => 11,
            Self::Corner => 8,
            Self::SixLine => 5,
            Self::Dozen | Self::Column => 2,
            Self::EvenMoney => 1,
        }
    }

    /// Get the house edge in basis points
    pub fn house_edge_bp(&self, wheel_size: u8) -> u32 {
        // House edge = (fair_payout - actual_payout) / fair_payout * 10000
        // For straight bet: fair_payout = wheel_size - 1, actual_payout = 35
        let fair_payout = (wheel_size - 1) as u32;
        let actual_payout = Self::Straight.payout_ratio();
        let house_edge_per_spin = (fair_payout - actual_payout) * 10000 / fair_payout;

        match wheel_size {
            37 => EUROPEAN_HOUSE_EDGE_BP,
            38 => AMERICAN_HOUSE_EDGE_BP,
            _ => house_edge_per_spin,
        }
    }
}

// ============================================================================
// ROULETTE TABLE
// ============================================================================

/// Roulette table configuration and state
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RouletteTable {
    /// Unique table ID
    pub table_id: pallas::Base,
    /// House public key
    pub house_pub: PublicKey,
    /// Whether using American (38) or European (37) wheel
    pub wheel_size: u8,
    /// Current house edge in basis points
    pub house_edge_bp: u32,
    /// Table's capital for paying winners
    pub house_capital: u64,
    /// Maximum bet per single number (straight)
    pub max_straight_bet: u64,
    /// Maximum total bet per spin
    pub max_total_bet: u64,
    /// Current state
    pub state: RouletteTableState,
    /// Current spin number (for tracking)
    pub spin_count: u64,
    /// Current winning number (None until spun)
    pub winning_number: Option<u8>,
    /// Block when bets close
    pub bets_close_block: u64,
    /// Block when spin occurred
    pub spun_at_block: Option<u64>,
    /// Created at block
    pub created_at: u64,
}

impl RouletteTable {
    /// Create a new European table
    pub fn new_european(
        table_id: pallas::Base,
        house_pub: PublicKey,
        house_capital: u64,
        max_straight_bet: u64,
        duration_blocks: u64,
        current_block: u64,
    ) -> Self {
        Self {
            table_id,
            house_pub,
            wheel_size: EUROPEAN_WHEEL_SIZE,
            house_edge_bp: EUROPEAN_HOUSE_EDGE_BP,
            house_capital,
            max_straight_bet,
            max_total_bet: max_straight_bet * 36, // Approximate max exposure
            state: RouletteTableState::Active,
            spin_count: 0,
            winning_number: None,
            bets_close_block: current_block + duration_blocks,
            spun_at_block: None,
            created_at: current_block,
        }
    }

    /// Create a new American table
    pub fn new_american(
        table_id: pallas::Base,
        house_pub: PublicKey,
        house_capital: u64,
        max_straight_bet: u64,
        duration_blocks: u64,
        current_block: u64,
    ) -> Self {
        Self {
            table_id,
            house_pub,
            wheel_size: AMERICAN_WHEEL_SIZE,
            house_edge_bp: AMERICAN_HOUSE_EDGE_BP,
            house_capital,
            max_straight_bet,
            max_total_bet: max_straight_bet * 36,
            state: RouletteTableState::Active,
            spin_count: 0,
            winning_number: None,
            bets_close_block: current_block + duration_blocks,
            spun_at_block: None,
            created_at: current_block,
        }
    }

    /// Check if table can accept a bet
    pub fn can_accept_bet(&self, bet: &Bet, current_block: u64) -> Result<(), &'static str> {
        if self.state != RouletteTableState::Active {
            return Err("Table not active")
        }
        if current_block >= self.bets_close_block {
            return Err("Bets closed")
        }
        if bet.amount > self.max_straight_bet {
            return Err("Bet exceeds max")
        }
        Ok(())
    }

    /// Calculate maximum payout for a bet
    pub fn max_payout(&self, bet: &Bet) -> u64 {
        bet.amount * (bet.bet_type.payout_ratio() as u64)
    }
}

/// Table state
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum RouletteTableState {
    /// Table is open for bets
    Active = 0,
    /// Waiting for spin
    WaitingForSpin = 1,
    /// Spin in progress
    Spun = 2,
    /// Table closed by house
    Closed = 3,
}

// ============================================================================
// BET
// ============================================================================

/// Individual bet
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Bet {
    /// Unique bet ID
    pub bet_id: pallas::Base,
    /// Table this bet is on
    pub table_id: pallas::Base,
    /// Player public key
    pub player_pub: PublicKey,
    /// Bet type
    pub bet_type: BetType,
    /// Numbers bet on
    pub numbers: Vec<u8>,
    /// Amount wagered
    pub amount: u64,
    /// Potential payout (amount * payout_ratio)
    pub payout: u64,
    /// Whether won
    pub won: Option<bool>,
    /// Actual payout received
    pub actual_payout: u64,
    /// Spin number when bet was placed
    pub spin_number: u64,
    /// Block when placed
    pub placed_at: u64,
    /// Nullifier for double-spend prevention
    pub nullifier: pallas::Base,
}

impl Bet {
    /// Create a new bet
    pub fn new(
        table_id: pallas::Base,
        player_pub: PublicKey,
        bet_type: BetType,
        numbers: Vec<u8>,
        amount: u64,
        spin_number: u64,
        current_block: u64,
    ) -> Self {
        let bet_id =
            poseidon_hash([table_id, player_pub.x(), player_pub.y(), pallas::Base::from(amount)]);
        let nullifier = poseidon_hash([bet_id, pallas::Base::from(current_block)]);

        Self {
            bet_id,
            table_id,
            player_pub,
            bet_type,
            numbers,
            amount,
            payout: amount * (bet_type.payout_ratio() as u64),
            won: None,
            actual_payout: 0,
            spin_number,
            placed_at: current_block,
            nullifier,
        }
    }

    /// Check if this bet wins given the winning number
    pub fn check_win(&self, winning_number: u8) -> bool {
        self.numbers.contains(&winning_number)
    }
}

// ============================================================================
// PARAMS AND UPDATES
// ============================================================================

/// Parameters for InitializeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParamsV1 {
    /// House public key
    pub house_pub: PublicKey,
    /// Use American wheel (38 numbers) vs European (37)
    pub american_wheel: bool,
    /// Initial house capital
    pub house_capital: u64,
    /// Maximum straight bet
    pub max_straight_bet: u64,
    /// Duration in blocks before spin
    pub duration_blocks: u64,
}

/// Update from InitializeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    pub table_id: pallas::Base,
    pub wheel_size: u8,
    pub house_edge_bp: u32,
    pub house_capital: u64,
    pub max_straight_bet: u64,
    pub bets_close_block: u64,
}

/// Parameters for PlaceBetV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceBetParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
    /// Player public key
    pub player_pub: PublicKey,
    /// Bet type
    pub bet_type: BetType,
    /// Numbers to bet on
    pub numbers: Vec<u8>,
    /// Amount to bet
    pub amount: u64,
    /// Signature
    pub signature: pallas::Base,
}

/// Update from PlaceBetV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PlaceBetUpdateV1 {
    pub bet_id: pallas::Base,
    pub table_id: pallas::Base,
    pub player_pub: PublicKey,
    pub bet_type: BetType,
    pub numbers: Vec<u8>,
    pub amount: u64,
    pub payout: u64,
    pub spin_number: u64,
    pub nullifier: pallas::Base,
    pub table_house_capital: u64,
    pub total_bets: u64,
}

/// Parameters for SpinWheelV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SpinWheelParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
    /// Nonce for randomness
    pub nonce: pallas::Base,
}

/// Update from SpinWheelV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SpinWheelUpdateV1 {
    pub table_id: pallas::Base,
    pub winning_number: u8,
    pub spin_number: u64,
    pub spun_at_block: u64,
}

/// Parameters for SettleBetsV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleBetsParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
    /// Bet IDs to settle
    pub bet_ids: Vec<pallas::Base>,
}

/// Update from SettleBetsV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleBetsUpdateV1 {
    pub table_id: pallas::Base,
    pub winning_number: u8,
    pub settled_count: u64,
    pub house_payout: u64,
    pub house_new_capital: u64,
}

/// Parameters for HouseCloseV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HouseCloseParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
}

/// Update from HouseCloseV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HouseCloseUpdateV1 {
    pub table_id: pallas::Base,
    pub remaining_capital: u64,
}

// ============================================================================
// HELPERS
// ============================================================================

/// Derive table ID from house pub and creation block
pub fn derive_table_id(house_pub: &PublicKey, created_at: u64) -> pallas::Base {
    poseidon_hash([house_pub.x(), house_pub.y(), pallas::Base::from(created_at)])
}

/// Draw winning number from block hash
pub fn draw_winning_number(
    block_hash: pallas::Base,
    nonce: pallas::Base,
    wheel_size: u8,
) -> u8 {
    draw_single(block_hash, nonce, wheel_size)
}
