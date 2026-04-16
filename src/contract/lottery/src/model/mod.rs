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

//! Lottery Contract Model
//!
//! Data structures for configurable lottery games.

use darkfi_sdk::{
    crypto::{draw_unique_range, poseidon_hash, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::error::LotteryError;
use crate::{MAX_NUM_PICKS, MAX_NUMBER_RANGE, MAX_PRIZE_TIERS};

// ============================================================================
// LOTTERY CONFIGURATION
// ============================================================================

/// Prize tier configuration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PrizeTierConfig {
    /// How many matches needed to win this tier (e.g., N for jackpot)
    pub matches_needed: u8,
    /// Payout percentage in basis points (e.g., 5000 = 50%)
    pub payout_percent: u32,
    /// If true, unclaimed prizes roll to next lottery
    pub roll_to_next: bool,
}

/// Configurable lottery parameters (set at deployment)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LotteryConfig {
    /// How many numbers player picks (N)
    pub num_picks: u8,
    /// Upper bound for numbers (M), numbers are 1 to M
    pub number_range: u8,
    /// House edge in basis points
    pub house_edge_bp: u32,
    /// Cost per ticket
    pub ticket_price: u64,
    /// Payout configuration (sorted by matches_needed descending)
    pub prize_tiers: Vec<PrizeTierConfig>,
}

impl LotteryConfig {
    /// Validate the lottery configuration
    pub fn validate(&self) -> Result<(), LotteryError> {
        if self.num_picks == 0 || self.num_picks > MAX_NUM_PICKS {
            return Err(LotteryError::InvalidNumPicks)
        }
        if self.number_range == 0 || self.number_range > MAX_NUMBER_RANGE {
            return Err(LotteryError::InvalidNumberRange)
        }
        if self.num_picks > self.number_range {
            return Err(LotteryError::InvalidNumPicks)
        }
        if self.prize_tiers.len() > MAX_PRIZE_TIERS {
            return Err(LotteryError::InvalidConfig)
        }

        // Ensure tiers are sorted by matches_needed descending
        for i in 1..self.prize_tiers.len() {
            if self.prize_tiers[i].matches_needed >= self.prize_tiers[i - 1].matches_needed {
                return Err(LotteryError::InvalidConfig)
            }
        }

        Ok(())
    }

    /// Get the minimum matches needed to win anything
    pub fn min_matches(&self) -> u8 {
        self.prize_tiers.last().map(|t| t.matches_needed).unwrap_or(0)
    }

    /// Get the maximum matches possible (always equals num_picks for jackpot)
    pub fn max_matches(&self) -> u8 {
        self.num_picks
    }
}

// ============================================================================
// LOTTERY STATE TYPES
// ============================================================================

/// Lottery round state
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum LotteryState {
    Initialized = 0,
    WinnersDrawn = 1,
    Expired = 2,
}

impl LotteryState {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::Initialized),
            1 => Some(Self::WinnersDrawn),
            2 => Some(Self::Expired),
            _ => None,
        }
    }
}

/// Unique lottery identifier
pub type LotteryId = pallas::Base;

/// Unique ticket identifier
pub type TicketId = pallas::Base;

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Lottery round structure stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Lottery {
    /// Unique lottery ID
    pub id: LotteryId,
    /// Lottery configuration
    pub config: LotteryConfig,
    /// House's public key for receiving unclaimed prizes
    pub house_pub: PublicKey,
    /// Current lottery state
    pub state: LotteryState,
    /// Number of tickets sold
    pub ticket_count: u64,
    /// Total prize pool (before house cut)
    pub gross_pool: u64,
    /// House's cut
    pub house_share: u64,
    /// Net prize pool (after house cut)
    pub prize_pool: u64,
    /// Winning numbers (None until drawn)
    pub winning_numbers: Option<Vec<u8>>,
    /// Block at which drawing occurred
    pub draw_block: Option<u64>,
    /// Merkle root of all ticket commitments
    pub ticket_merkle_root: pallas::Base,
    /// Block at which lottery was created
    pub created_at: u64,
    /// Earliest block to draw
    pub draw_block_deadline: u64,
    /// Latest block to claim prizes
    pub claim_deadline: u64,
    /// Rolled-over prize from previous lottery (if applicable)
    pub rolled_over: u64,
}

impl Lottery {
    /// Calculate the prize for a given tier based on current pool
    pub fn calculate_tier_prize(&self, payout_percent: u32, num_winners: u64) -> u64 {
        if num_winners == 0 {
            return 0
        }
        (self.prize_pool * (payout_percent as u64)) / (10000 * num_winners)
    }

    /// Calculate gross pool from ticket sales
    pub fn calculate_gross_pool(&self) -> u64 {
        self.ticket_count * self.config.ticket_price
    }

    /// Calculate house share from gross pool
    pub fn calculate_house_share(&self) -> u64 {
        (self.gross_pool * (self.config.house_edge_bp as u64)) / 10000
    }

    /// Check if lottery is accepting tickets
    pub fn is_active(&self, current_block: u64) -> bool {
        self.state == LotteryState::Initialized && current_block <= self.draw_block_deadline
    }

    /// Check if lottery is in claim period
    pub fn is_claimable(&self, current_block: u64) -> bool {
        self.state == LotteryState::WinnersDrawn && current_block <= self.claim_deadline
    }
}

/// Ticket structure stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Ticket {
    /// Unique ticket ID
    pub id: TicketId,
    /// Associated lottery ID
    pub lottery_id: LotteryId,
    /// Player's public key
    pub player_pub: PublicKey,
    /// Commitment: PoseidonHash(numbers, nonce, lottery_id)
    pub commitment: pallas::Base,
    /// Token ID being used
    pub token_id: pallas::Base,
    /// Value (ticket price)
    pub value: u64,
    /// Nullifier for double-spend prevention
    pub nullifier: TicketId,
    /// Block at which ticket was purchased
    pub created_at: u64,
}

impl Ticket {
    /// Derive the nullifier for this ticket
    pub fn derive_nullifier(&self) -> TicketId {
        poseidon_hash([self.id, self.nullifier])
    }
}

/// Prize claim structure
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Claim {
    /// Ticket ID being claimed
    pub ticket_id: TicketId,
    /// Prize tier won
    pub tier: u8,
    /// Number of matches
    pub matches: u8,
    /// Prize amount claimed
    pub prize: u64,
    /// Block at which claim was made
    pub claimed_at: u64,
}

// ============================================================================
// PARAMS AND UPDATES
// ============================================================================

/// Parameters for InitializeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParamsV1 {
    /// House public key
    pub house_pub: PublicKey,
    /// Lottery configuration
    pub config: LotteryConfig,
    /// Duration in blocks until draw
    pub duration: u64,
    /// Claim duration in blocks after draw
    pub claim_duration: u64,
    /// Rolled over amount from previous lottery (if any)
    pub rolled_over: u64,
}

/// Update produced by InitializeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    pub lottery_id: LotteryId,
    pub config: LotteryConfig,
    pub house_pub: PublicKey,
    pub draw_block_deadline: u64,
    pub claim_deadline: u64,
    pub rolled_over: u64,
    pub state: LotteryState,
}

/// Parameters for BuyTicketV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BuyTicketParamsV1 {
    /// Player's public key
    pub player_pub: PublicKey,
    /// Commitment: PoseidonHash(numbers, nonce, lottery_id)
    pub commitment: pallas::Base,
    /// Token ID
    pub token_id: pallas::Base,
    /// Value (ticket price)
    pub value: u64,
    /// Signature
    pub signature: pallas::Base,
}

/// Update produced by BuyTicketV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct BuyTicketUpdateV1 {
    pub ticket_id: TicketId,
    pub lottery_id: LotteryId,
    pub player_pub: PublicKey,
    pub commitment: pallas::Base,
    pub token_id: pallas::Base,
    pub value: u64,
    pub ticket_count: u64,
    pub gross_pool: u64,
    pub nullifier: TicketId,
    pub created_at: u64,
}

/// Parameters for DrawWinnersV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DrawWinnersParamsV1 {
    /// Lottery ID
    pub lottery_id: LotteryId,
    /// Nonce for randomness
    pub nonce: pallas::Base,
}

/// Update produced by DrawWinnersV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DrawWinnersUpdateV1 {
    pub lottery_id: LotteryId,
    pub winning_numbers: Vec<u8>,
    pub draw_block: u64,
    pub gross_pool: u64,
    pub house_share: u64,
    pub prize_pool: u64,
    pub state: LotteryState,
}

/// Parameters for RevealTicketV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevealTicketParamsV1 {
    /// Ticket ID
    pub ticket_id: TicketId,
    /// The N numbers the player selected
    pub numbers: Vec<u8>,
    /// Secret nonce used in commitment
    pub nonce: pallas::Base,
}

/// Update produced by RevealTicketV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RevealTicketUpdateV1 {
    pub ticket_id: TicketId,
    pub matches: u8,
    pub tier: Option<u8>,
}

/// Parameters for ClaimPrizeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimPrizeParamsV1 {
    /// Ticket ID to claim
    pub ticket_id: TicketId,
    /// ZK proof of reveal
    pub proof: Vec<u8>,
    /// Prize tier won (extracted from ZK proof verification on client)
    pub tier: u8,
    /// Number of matching numbers
    pub matches: u8,
}

/// Update produced by ClaimPrizeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimPrizeUpdateV1 {
    pub ticket_id: TicketId,
    pub tier: u8,
    pub matches: u8,
    pub prize: u64,
    pub claimed_at: u64,
}

/// Parameters for ExpireLotteryV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExpireLotteryParamsV1 {
    /// Lottery ID to expire
    pub lottery_id: LotteryId,
}

/// Update produced by ExpireLotteryV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExpireLotteryUpdateV1 {
    pub lottery_id: LotteryId,
    pub unclaimed_rollover: u64,
    pub house_claim: u64,
    pub state: LotteryState,
}

// ============================================================================
// VALIDATION HELPERS
// ============================================================================

/// Validate numbers selection
pub fn validate_numbers(numbers: &[u8], num_picks: u8, number_range: u8) -> Result<(), LotteryError> {
    if numbers.len() != num_picks as usize {
        return Err(LotteryError::InvalidNumPicks)
    }

    // Check all numbers are in valid range and unique
    let mut seen = [false; 256];
    for &n in numbers {
        if n == 0 || n > number_range {
            return Err(LotteryError::NumberOutOfRange)
        }
        if seen[n as usize] {
            return Err(LotteryError::DuplicateNumbers)
        }
        seen[n as usize] = true;
    }

    Ok(())
}

/// Derive ticket ID from parameters
pub fn derive_ticket_id(
    lottery_id: LotteryId,
    player_pub: &PublicKey,
    commitment: pallas::Base,
    value: u64,
) -> TicketId {
    poseidon_hash([
        lottery_id,
        player_pub.x(),
        player_pub.y(),
        commitment,
        pallas::Base::from(value),
    ])
}

/// Derive nullifier for a ticket
pub fn derive_nullifier(ticket_id: TicketId, nonce: pallas::Base) -> TicketId {
    poseidon_hash([ticket_id, nonce])
}

/// Derive lottery ID from house_pub and creation block
pub fn derive_lottery_id(house_pub: &PublicKey, created_at: u64) -> LotteryId {
    poseidon_hash([house_pub.x(), house_pub.y(), pallas::Base::from(created_at)])
}

/// Count matches between player numbers and winning numbers
pub fn count_matches(player_numbers: &[u8], winning_numbers: &[u8]) -> u8 {
    let mut count = 0u8;
    for &n in player_numbers {
        if winning_numbers.contains(&n) {
            count += 1;
        }
    }
    count
}

/// Determine prize tier based on matches and config
pub fn determine_tier(config: &LotteryConfig, matches: u8) -> Option<usize> {
    for (i, tier) in config.prize_tiers.iter().enumerate() {
        if matches >= tier.matches_needed {
            return Some(i)
        }
    }
    None
}

// ============================================================================
// DRAWING ALGORITHM
// ============================================================================

/// Draw winning numbers using block hash entropy
pub fn draw_winning_numbers(
    block_hash: pallas::Base,
    seed_nonce: u64,
    num_picks: u8,
    number_range: u8,
) -> Vec<u8> {
    draw_unique_range(block_hash, seed_nonce, num_picks, number_range)
}

/// Verify a ticket commitment
pub fn verify_commitment(
    numbers: &[u8],
    nonce: pallas::Base,
    lottery_id: LotteryId,
    commitment: pallas::Base,
) -> bool {
    // Recompute commitment using iterative hashing
    // commitment = PoseidonHash(PoseidonHash(...PoseidonHash(lottery_id, numbers[0])..., numbers[n-1]), nonce)
    let mut state = lottery_id;
    for &n in numbers {
        state = poseidon_hash([state, pallas::Base::from(n as u64)]);
    }
    let computed = poseidon_hash([state, nonce]);
    computed == commitment
}
