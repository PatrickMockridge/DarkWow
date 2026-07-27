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

//! Roulette Contract Model
//!
//! Data structures for the roulette game.

use dwow_sdk::{
    crypto::{draw_single, pasta_prelude::PrimeField, poseidon_hash, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};
use dwow_serial::{SerialDecodable, SerialEncodable};

use crate::{EUROPEAN_HOUSE_EDGE_BP, EUROPEAN_WHEEL_SIZE, AMERICAN_HOUSE_EDGE_BP, AMERICAN_WHEEL_SIZE};

// ============================================================================
// BET TYPES
// ============================================================================

/// Roulette bet types with their payouts
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

impl TryFrom<u8> for BetType {
    type Error = ContractError;

    fn try_from(v: u8) -> core::result::Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Straight),
            1 => Ok(Self::Split),
            2 => Ok(Self::Street),
            3 => Ok(Self::Corner),
            4 => Ok(Self::SixLine),
            5 => Ok(Self::Dozen),
            6 => Ok(Self::Column),
            7 => Ok(Self::EvenMoney),
            _ => Err(ContractError::IoError("BetType: invalid discriminant".into())),
        }
    }
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
#[derive(Debug, Clone)]
pub struct RouletteTable {
    pub version: u8,
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
    pub instance_seed: [u8; 32],
}

impl RouletteTable {
    /// Canonical byte size of an encoded RouletteTable.
    pub const ENCODED_SIZE: usize = 162;
    // version(1) + table_id(32) + house_pub(32) + wheel_size(1) + house_edge_bp(4)
    // + house_capital(8) + max_straight_bet(8) + max_total_bet(8) + state(1)
    // + spin_count(8) + winning_number(2: presence+value) + bets_close_block(8)
    // + spun_at_block(9: presence+value) + created_at(8) + instance_seed(32)

    /// Encode to canonical fixed-offset bytes.
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.house_pub.to_bytes());
        buf.push(self.wheel_size);
        buf.extend_from_slice(&self.house_edge_bp.to_le_bytes());
        buf.extend_from_slice(&self.house_capital.to_le_bytes());
        buf.extend_from_slice(&self.max_straight_bet.to_le_bytes());
        buf.extend_from_slice(&self.max_total_bet.to_le_bytes());
        buf.push(self.state as u8);
        buf.extend_from_slice(&self.spin_count.to_le_bytes());
        // winning_number: Option<u8> — presence byte + value
        match self.winning_number {
            Some(n) => { buf.push(1u8); buf.push(n); }
            None => { buf.push(0u8); buf.push(0u8); }
        }
        buf.extend_from_slice(&self.bets_close_block.to_le_bytes());
        // spun_at_block: Option<u64> — presence byte + value
        match self.spun_at_block {
            Some(b) => { buf.push(1u8); buf.extend_from_slice(&b.to_le_bytes()); }
            None => { buf.push(0u8); buf.extend_from_slice(&0u64.to_le_bytes()); }
        }
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    /// Decode from canonical fixed-offset bytes.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "RouletteTable: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let version = data[0];
        let table_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[1..33].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("RouletteTable: invalid table_id".into()))?;
        let house_pub = Option::<PublicKey>::from(
            PublicKey::from_bytes(data[33..65].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("RouletteTable: invalid house_pub".into()))?;
        let wheel_size = data[65];
        let house_edge_bp = u32::from_le_bytes(data[66..70].try_into().unwrap());
        let house_capital = u64::from_le_bytes(data[70..78].try_into().unwrap());
        let max_straight_bet = u64::from_le_bytes(data[78..86].try_into().unwrap());
        let max_total_bet = u64::from_le_bytes(data[86..94].try_into().unwrap());
        let state = RouletteTableState::try_from(data[94])?;
        let spin_count = u64::from_le_bytes(data[95..103].try_into().unwrap());
        let winning_number = if data[103] != 0 {
            Some(data[104])
        } else {
            None
        };
        let bets_close_block = u64::from_le_bytes(data[105..113].try_into().unwrap());
        let spun_at_block = if data[113] != 0 {
            Some(u64::from_le_bytes(data[114..122].try_into().unwrap()))
        } else {
            None
        };
        let created_at = u64::from_le_bytes(data[122..130].try_into().unwrap());
        let instance_seed = data[130..162].try_into().unwrap();
        Ok(RouletteTable {
            version, table_id, house_pub, wheel_size, house_edge_bp,
            house_capital, max_straight_bet, max_total_bet, state,
            spin_count, winning_number, bets_close_block, spun_at_block,
            created_at, instance_seed,
        })
    }

    /// Create a new European table
    pub fn new_european(
        table_id: pallas::Base,
        house_pub: PublicKey,
        house_capital: u64,
        max_straight_bet: u64,
        duration_blocks: u64,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Option<Self> {
        Some(Self {
            version: 0,
            table_id,
            house_pub,
            wheel_size: EUROPEAN_WHEEL_SIZE,
            house_edge_bp: EUROPEAN_HOUSE_EDGE_BP,
            house_capital,
            max_straight_bet,
            max_total_bet: max_straight_bet.checked_mul(36)?, // Approximate max exposure
            state: RouletteTableState::Active,
            spin_count: 0,
            winning_number: None,
            bets_close_block: current_block + duration_blocks,
            spun_at_block: None,
            created_at: current_block,
            instance_seed,
        })
    }

    /// Create a new American table
    pub fn new_american(
        table_id: pallas::Base,
        house_pub: PublicKey,
        house_capital: u64,
        max_straight_bet: u64,
        duration_blocks: u64,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Option<Self> {
        Some(Self {
            version: 0,
            table_id,
            house_pub,
            wheel_size: AMERICAN_WHEEL_SIZE,
            house_edge_bp: AMERICAN_HOUSE_EDGE_BP,
            house_capital,
            max_straight_bet,
            max_total_bet: max_straight_bet.checked_mul(36)?,
            state: RouletteTableState::Active,
            spin_count: 0,
            winning_number: None,
            bets_close_block: current_block + duration_blocks,
            spun_at_block: None,
            created_at: current_block,
            instance_seed,
        })
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
    pub fn max_payout(&self, bet: &Bet) -> Option<u64> {
        bet.amount.checked_mul(bet.bet_type.payout_ratio() as u64)
    }
}

/// Table state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouletteTableState {
    /// Table is open for bets
    Active = 0,
    /// Waiting for spin
    WaitingForSpin = 1,
    /// Spin in progress
    Spun = 2,
    /// Bets settled after spin
    Settled = 3,
    /// Table closed by house
    Closed = 4,
}

impl TryFrom<u8> for RouletteTableState {
    type Error = ContractError;

    fn try_from(v: u8) -> core::result::Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Active),
            1 => Ok(Self::WaitingForSpin),
            2 => Ok(Self::Spun),
            3 => Ok(Self::Settled),
            4 => Ok(Self::Closed),
            _ => Err(ContractError::IoError("RouletteTableState: invalid discriminant".into())),
        }
    }
}

// ============================================================================
// BET
// ============================================================================

/// Individual bet
#[derive(Debug, Clone)]
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
    pub instance_seed: [u8; 32],
}

// Fixed prefix (before numbers): bet_id(32) + table_id(32) + player_pub(32) + bet_type(1) + numbers_len(1) = 98
// Fixed suffix (after numbers): amount(8) + payout(8) + won(2: presence+value) + actual_payout(8) + spin_number(8) + placed_at(8) + nullifier(32) + instance_seed(32) = 106
// Total: 204 + numbers_len

impl Bet {
    /// Encode to variable-length bytes with length-prefixed numbers field.
    pub fn encode(&self) -> Vec<u8> {
        let n = self.numbers.len() as u8;
        let total = 204usize + n as usize;
        let mut buf = Vec::with_capacity(total);
        buf.extend_from_slice(&self.bet_id.to_repr());
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.player_pub.to_bytes());
        buf.push(self.bet_type as u8);
        buf.push(n);
        buf.extend_from_slice(&self.numbers);
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.payout.to_le_bytes());
        // won: Option<bool> — presence byte + bool byte (Pattern 4)
        match self.won {
            Some(true) => { buf.push(1u8); buf.push(1u8); }
            Some(false) => { buf.push(1u8); buf.push(0u8); }
            None => { buf.push(0u8); buf.push(0u8); }
        }
        buf.extend_from_slice(&self.actual_payout.to_le_bytes());
        buf.extend_from_slice(&self.spin_number.to_le_bytes());
        buf.extend_from_slice(&self.placed_at.to_le_bytes());
        buf.extend_from_slice(&self.nullifier.to_repr());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    /// Decode from variable-length bytes with length-prefixed numbers field.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        const FIXED: usize = 204; // fixed fields excluding numbers
        if data.len() < 98 {
            return Err(ContractError::IoError(format!(
                "Bet: too short (need at least 98 bytes, got {})", data.len()
            )));
        }
        let n = data[97] as usize;
        let expected = FIXED + n;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "Bet: expected {} bytes ({} numbers), got {}", expected, n, data.len()
            )));
        }
        let bet_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[0..32].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Bet: invalid bet_id".into()))?;
        let table_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[32..64].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Bet: invalid table_id".into()))?;
        let player_pub = Option::<PublicKey>::from(
            PublicKey::from_bytes(data[64..96].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Bet: invalid player_pub".into()))?;
        let bet_type = BetType::try_from(data[96])?;
        // data[97] is numbers_len, already read as n
        let numbers = data[98..98 + n].to_vec();
        let off = 98 + n;
        let amount = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
        let payout = u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap());
        let won = if data[off + 16] != 0 {
            Some(data[off + 17] != 0)
        } else {
            None
        };
        let actual_payout = u64::from_le_bytes(data[off + 18..off + 26].try_into().unwrap());
        let spin_number = u64::from_le_bytes(data[off + 26..off + 34].try_into().unwrap());
        let placed_at = u64::from_le_bytes(data[off + 34..off + 42].try_into().unwrap());
        let nullifier = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[off + 42..off + 74].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("Bet: invalid nullifier".into()))?;
        let instance_seed = data[off + 74..off + 106].try_into().unwrap();
        Ok(Bet {
            bet_id, table_id, player_pub, bet_type, numbers,
            amount, payout, won, actual_payout, spin_number, placed_at,
            nullifier, instance_seed,
        })
    }

    /// Create a new bet
    pub fn new(
        table_id: pallas::Base,
        player_pub: PublicKey,
        bet_type: BetType,
        numbers: Vec<u8>,
        amount: u64,
        spin_number: u64,
        current_block: u64,
        instance_seed: [u8; 32],
    ) -> Option<Self> {
        let bet_id =
            poseidon_hash([table_id, player_pub.x().expect("pk not identity"), player_pub.y().expect("pk not identity"), pallas::Base::from(amount)]);
        let nullifier = poseidon_hash([bet_id, pallas::Base::from(current_block)]);

        Some(Self {
            bet_id,
            table_id,
            player_pub,
            bet_type,
            numbers,
            amount,
            payout: amount.checked_mul(bet_type.payout_ratio() as u64)?,
            won: None,
            actual_payout: 0,
            spin_number,
            placed_at: current_block,
            nullifier,
            instance_seed,
        })
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
    pub instance_seed: [u8; 32],
}

/// Update from InitializeV1
#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    pub table_id: pallas::Base,
    pub house_pub: PublicKey,
    pub wheel_size: u8,
    pub house_edge_bp: u32,
    pub house_capital: u64,
    pub max_straight_bet: u64,
    pub bets_close_block: u64,
    pub instance_seed: [u8; 32],
}

impl InitializeUpdateV1 {
    // table_id(32) + house_pub(32) + wheel_size(1) + house_edge_bp(4)
    // + house_capital(8) + max_straight_bet(8) + bets_close_block(8) + instance_seed(32)
    pub const ENCODED_SIZE: usize = 125;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.house_pub.to_bytes());
        buf.push(self.wheel_size);
        buf.extend_from_slice(&self.house_edge_bp.to_le_bytes());
        buf.extend_from_slice(&self.house_capital.to_le_bytes());
        buf.extend_from_slice(&self.max_straight_bet.to_le_bytes());
        buf.extend_from_slice(&self.bets_close_block.to_le_bytes());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "InitializeUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let table_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[0..32].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("InitializeUpdateV1: invalid table_id".into()))?;
        let house_pub = Option::<PublicKey>::from(
            PublicKey::from_bytes(data[32..64].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("InitializeUpdateV1: invalid house_pub".into()))?;
        let wheel_size = data[64];
        let house_edge_bp = u32::from_le_bytes(data[65..69].try_into().unwrap());
        let house_capital = u64::from_le_bytes(data[69..77].try_into().unwrap());
        let max_straight_bet = u64::from_le_bytes(data[77..85].try_into().unwrap());
        let bets_close_block = u64::from_le_bytes(data[85..93].try_into().unwrap());
        let instance_seed = data[93..125].try_into().unwrap();
        Ok(InitializeUpdateV1 {
            table_id, house_pub, wheel_size, house_edge_bp, house_capital,
            max_straight_bet, bets_close_block, instance_seed,
        })
    }
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
    pub instance_seed: [u8; 32],
}

/// Update from PlaceBetV1
#[derive(Debug, Clone)]
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
    pub instance_seed: [u8; 32],
}

// Fixed prefix (before numbers): bet_id(32)+table_id(32)+player_pub(32)+bet_type(1)+numbers_len(1) = 98
// Fixed suffix (after numbers): amount(8)+payout(8)+spin_number(8)+nullifier(32)+table_house_capital(8)+total_bets(8)+instance_seed(32) = 104
// Total: 202 + numbers_len

impl PlaceBetUpdateV1 {
    pub fn encode(&self) -> Vec<u8> {
        let n = self.numbers.len() as u8;
        let total = 202usize + n as usize;
        let mut buf = Vec::with_capacity(total);
        buf.extend_from_slice(&self.bet_id.to_repr());
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.player_pub.to_bytes());
        buf.push(self.bet_type as u8);
        buf.push(n);
        buf.extend_from_slice(&self.numbers);
        buf.extend_from_slice(&self.amount.to_le_bytes());
        buf.extend_from_slice(&self.payout.to_le_bytes());
        buf.extend_from_slice(&self.spin_number.to_le_bytes());
        buf.extend_from_slice(&self.nullifier.to_repr());
        buf.extend_from_slice(&self.table_house_capital.to_le_bytes());
        buf.extend_from_slice(&self.total_bets.to_le_bytes());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        const FIXED: usize = 202; // fixed fields excluding numbers
        if data.len() < 98 {
            return Err(ContractError::IoError(format!(
                "PlaceBetUpdateV1: too short (need at least 98 bytes, got {})", data.len()
            )));
        }
        let n = data[97] as usize;
        let expected = FIXED + n;
        if data.len() != expected {
            return Err(ContractError::IoError(format!(
                "PlaceBetUpdateV1: expected {} bytes ({} numbers), got {}", expected, n, data.len()
            )));
        }
        let bet_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[0..32].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("PlaceBetUpdateV1: invalid bet_id".into()))?;
        let table_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[32..64].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("PlaceBetUpdateV1: invalid table_id".into()))?;
        let player_pub = Option::<PublicKey>::from(
            PublicKey::from_bytes(data[64..96].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("PlaceBetUpdateV1: invalid player_pub".into()))?;
        let bet_type = BetType::try_from(data[96])?;
        // data[97] is numbers_len, already read as n
        let numbers = data[98..98 + n].to_vec();
        let off = 98 + n;
        let amount = u64::from_le_bytes(data[off..off + 8].try_into().unwrap());
        let payout = u64::from_le_bytes(data[off + 8..off + 16].try_into().unwrap());
        let spin_number = u64::from_le_bytes(data[off + 16..off + 24].try_into().unwrap());
        let nullifier = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[off + 24..off + 56].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("PlaceBetUpdateV1: invalid nullifier".into()))?;
        let table_house_capital = u64::from_le_bytes(data[off + 56..off + 64].try_into().unwrap());
        let total_bets = u64::from_le_bytes(data[off + 64..off + 72].try_into().unwrap());
        let instance_seed = data[off + 72..off + 104].try_into().unwrap();
        Ok(PlaceBetUpdateV1 {
            bet_id, table_id, player_pub, bet_type, numbers,
            amount, payout, spin_number, nullifier,
            table_house_capital, total_bets, instance_seed,
        })
    }
}

/// Parameters for SpinWheelV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SpinWheelParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
    /// Nonce for randomness
    pub nonce: pallas::Base,
    /// House public key X coordinate (ZK-verified)
    pub house_pub_x: pallas::Base,
    /// House public key Y coordinate (ZK-verified)
    pub house_pub_y: pallas::Base,
    /// Spin nullifier = H(table_id, house_secret) — replay protection
    pub spin_nullifier: pallas::Base,
}

/// Update from SpinWheelV1
#[derive(Debug, Clone)]
pub struct SpinWheelUpdateV1 {
    pub table_id: pallas::Base,
    pub winning_number: u8,
    pub spin_number: u64,
    pub spun_at_block: u64,
    pub spin_nullifier: pallas::Base,
}

impl SpinWheelUpdateV1 {
    // table_id(32) + winning_number(1) + spin_number(8) + spun_at_block(8) + spin_nullifier(32)
    pub const ENCODED_SIZE: usize = 81;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.push(self.winning_number);
        buf.extend_from_slice(&self.spin_number.to_le_bytes());
        buf.extend_from_slice(&self.spun_at_block.to_le_bytes());
        buf.extend_from_slice(&self.spin_nullifier.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "SpinWheelUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let table_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[0..32].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("SpinWheelUpdateV1: invalid table_id".into()))?;
        let winning_number = data[32];
        let spin_number = u64::from_le_bytes(data[33..41].try_into().unwrap());
        let spun_at_block = u64::from_le_bytes(data[41..49].try_into().unwrap());
        let spin_nullifier = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[49..81].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("SpinWheelUpdateV1: invalid spin_nullifier".into()))?;
        Ok(SpinWheelUpdateV1 { table_id, winning_number, spin_number, spun_at_block, spin_nullifier })
    }
}

/// Parameters for SettleBetsV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleBetsParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
    /// Bet IDs to settle
    pub bet_ids: Vec<pallas::Base>,
    /// Total payout amount (public input for ZK proof)
    pub payout: u64,
}

/// Update from SettleBetsV1
#[derive(Debug, Clone)]
pub struct SettleBetsUpdateV1 {
    pub table_id: pallas::Base,
    pub winning_number: u8,
    pub settled_count: u64,
    pub house_payout: u64,
    pub house_new_capital: u64,
    pub state: RouletteTableState,
}

impl SettleBetsUpdateV1 {
    // table_id(32) + winning_number(1) + settled_count(8) + house_payout(8)
    // + house_new_capital(8) + state(1)
    pub const ENCODED_SIZE: usize = 58;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.push(self.winning_number);
        buf.extend_from_slice(&self.settled_count.to_le_bytes());
        buf.extend_from_slice(&self.house_payout.to_le_bytes());
        buf.extend_from_slice(&self.house_new_capital.to_le_bytes());
        buf.push(self.state as u8);
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "SettleBetsUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let table_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[0..32].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("SettleBetsUpdateV1: invalid table_id".into()))?;
        let winning_number = data[32];
        let settled_count = u64::from_le_bytes(data[33..41].try_into().unwrap());
        let house_payout = u64::from_le_bytes(data[41..49].try_into().unwrap());
        let house_new_capital = u64::from_le_bytes(data[49..57].try_into().unwrap());
        let state = RouletteTableState::try_from(data[57])?;
        Ok(SettleBetsUpdateV1 {
            table_id, winning_number, settled_count, house_payout, house_new_capital, state,
        })
    }
}

/// Parameters for HouseCloseV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct HouseCloseParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
    /// House public key X coordinate (ZK-verified)
    pub house_pub_x: pallas::Base,
    /// House public key Y coordinate (ZK-verified)
    pub house_pub_y: pallas::Base,
    /// Close nullifier = H(table_id, house_secret) — replay protection
    pub close_nullifier: pallas::Base,
}

/// Update from HouseCloseV1
#[derive(Debug, Clone)]
pub struct HouseCloseUpdateV1 {
    pub table_id: pallas::Base,
    pub remaining_capital: u64,
    pub close_nullifier: pallas::Base,
}

impl HouseCloseUpdateV1 {
    // table_id(32) + remaining_capital(8) + close_nullifier(32)
    pub const ENCODED_SIZE: usize = 72;

    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.table_id.to_repr());
        buf.extend_from_slice(&self.remaining_capital.to_le_bytes());
        buf.extend_from_slice(&self.close_nullifier.to_repr());
        buf
    }

    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "HouseCloseUpdateV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )));
        }
        let table_id = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[0..32].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("HouseCloseUpdateV1: invalid table_id".into()))?;
        let remaining_capital = u64::from_le_bytes(data[32..40].try_into().unwrap());
        let close_nullifier = Option::<pallas::Base>::from(
            pallas::Base::from_repr(data[40..72].try_into().unwrap())
        ).ok_or_else(|| ContractError::IoError("HouseCloseUpdateV1: invalid close_nullifier".into()))?;
        Ok(HouseCloseUpdateV1 { table_id, remaining_capital, close_nullifier })
    }
}

// ============================================================================
// HELPERS
// ============================================================================

/// Derive table ID from house pub and creation block
pub fn derive_table_id(house_pub: &PublicKey, created_at: u64) -> pallas::Base {
    poseidon_hash([house_pub.x().expect("pk not identity"), house_pub.y().expect("pk not identity"), pallas::Base::from(created_at)])
}

/// Draw winning number from block hash
pub fn draw_winning_number(
    block_hash: pallas::Base,
    nonce: pallas::Base,
    wheel_size: u8,
) -> u8 {
    draw_single(block_hash, nonce, wheel_size)
}
