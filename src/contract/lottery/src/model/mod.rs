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

//! Lottery Contract Model
//!
//! Data structures for configurable lottery games.

use dwow_sdk::{
    crypto::{draw_unique_range, poseidon_hash, pasta_prelude::PrimeField, PublicKey},
    error::ContractError,
    pasta::{group::GroupEncoding, pallas},
};

use crate::error::LotteryError;
use crate::{MAX_NUM_PICKS, MAX_NUMBER_RANGE, MAX_PRIZE_TIERS};

// ============================================================================
// LOTTERY CONFIGURATION
// ============================================================================

/// Prize tier configuration
#[derive(Debug, Clone)]
pub struct PrizeTierConfig {
    /// How many matches needed to win this tier (e.g., N for jackpot)
    pub matches_needed: u8,
    /// Payout percentage in basis points (e.g., 5000 = 50%)
    pub payout_percent: u32,
    /// If true, unclaimed prizes roll to next lottery
    pub roll_to_next: bool,
}

impl PrizeTierConfig {
    /// Fixed canonical byte size: matches_needed(1) + payout_percent(4) + roll_to_next(1)
    pub const ENCODED_SIZE: usize = 6;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.matches_needed);
        buf.extend_from_slice(&self.payout_percent.to_le_bytes());
        buf.push(self.roll_to_next as u8);
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "PrizeTierConfig: expected {} bytes, got {}",
                Self::ENCODED_SIZE,
                data.len()
            )))
        }
        let matches_needed = data[0];
        let payout_percent = u32::from_le_bytes(data[1..5].try_into().unwrap());
        let roll_to_next = data[5] != 0;
        Ok(PrizeTierConfig { matches_needed, payout_percent, roll_to_next })
    }
}

/// Configurable lottery parameters (set at deployment)
#[derive(Debug, Clone)]
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

/// Encode LotteryConfig to bytes.
pub fn encode_config(config: &LotteryConfig) -> Vec<u8> {
    config.encode()
}

/// Decode LotteryConfig from bytes.
pub fn decode_config(data: &[u8]) -> Result<LotteryConfig, ContractError> {
    LotteryConfig::decode(data)
}

impl LotteryConfig {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(15 + self.prize_tiers.len() * PrizeTierConfig::ENCODED_SIZE);
        buf.push(self.num_picks);
        buf.push(self.number_range);
        buf.extend_from_slice(&self.house_edge_bp.to_le_bytes());
        buf.extend_from_slice(&self.ticket_price.to_le_bytes());
        buf.push(self.prize_tiers.len() as u8);
        for tier in &self.prize_tiers { buf.extend_from_slice(&tier.encode()); }
        buf
    }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 15 { return Err(ContractError::IoError(format!("LotteryConfig: expected >=15 got {}", data.len()))); }
        let num_picks = data[0];
        let number_range = data[1];
        let house_edge_bp = u32::from_le_bytes(data[2..6].try_into().unwrap());
        let ticket_price = u64::from_le_bytes(data[6..14].try_into().unwrap());
        let tier_count = data[14] as usize;
        let tiers_size = tier_count * PrizeTierConfig::ENCODED_SIZE;
        if data.len() != 15 + tiers_size { return Err(ContractError::IoError(format!("LotteryConfig: expected {} got {}", 15+tiers_size, data.len()))); }
        let mut prize_tiers = Vec::with_capacity(tier_count);
        for i in 0..tier_count {
            prize_tiers.push(PrizeTierConfig::decode(&data[15+i*PrizeTierConfig::ENCODED_SIZE..15+(i+1)*PrizeTierConfig::ENCODED_SIZE])?);
        }
        Ok(LotteryConfig { num_picks, number_range, house_edge_bp, ticket_price, prize_tiers })
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
#[derive(Debug, Clone)]
pub struct Lottery {
    pub version: u8,
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
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

impl Lottery {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Format: version(1) + id(32) + config(var) + house_pub(32) + state(1) +
    ///   ticket_count(8) + gross_pool(8) + house_share(8) + prize_pool(8) +
    ///   winning_numbers: Option<Vec<u8>> (Pattern 4: 1-byte flag + u8 count + N bytes) +
    ///   draw_block: Option<u64> (Pattern 4: 1-byte flag + 8 bytes LE) +
    ///   ticket_merkle_root(32) + created_at(8) + draw_block_deadline(8) +
    ///   claim_deadline(8) + rolled_over(8) + instance_seed(32)
    pub fn encode(&self) -> Vec<u8> {
        let config_bytes = encode_config(&self.config);
        let cap = 1 + 32 + config_bytes.len() + 32 + 1 + 8 + 8 + 8 + 8
            + 1 + self.winning_numbers.as_ref().map_or(0, |w| 1 + w.len())
            + 1 + self.draw_block.map_or(0, |_| 8)
            + 32 + 8 + 8 + 8 + 8 + 32;
        let mut buf = Vec::with_capacity(cap);
        buf.push(self.version);
        buf.extend_from_slice(&self.id.to_repr());
        buf.extend_from_slice(&config_bytes);
        buf.extend_from_slice(&self.house_pub.to_bytes());
        buf.push(match self.state {
            LotteryState::Initialized => 0u8,
            LotteryState::WinnersDrawn => 1u8,
            LotteryState::Expired => 2u8,
        });
        buf.extend_from_slice(&self.ticket_count.to_le_bytes());
        buf.extend_from_slice(&self.gross_pool.to_le_bytes());
        buf.extend_from_slice(&self.house_share.to_le_bytes());
        buf.extend_from_slice(&self.prize_pool.to_le_bytes());
        // winning_numbers: Option<Vec<u8>> — Pattern 4
        if let Some(ref nums) = self.winning_numbers {
            buf.push(1u8);
            buf.push(nums.len() as u8);
            buf.extend_from_slice(nums);
        } else {
            buf.push(0u8);
        }
        // draw_block: Option<u64> — Pattern 4
        if let Some(db) = self.draw_block {
            buf.push(1u8);
            buf.extend_from_slice(&db.to_le_bytes());
        } else {
            buf.push(0u8);
        }
        buf.extend_from_slice(&self.ticket_merkle_root.to_repr());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.draw_block_deadline.to_le_bytes());
        buf.extend_from_slice(&self.claim_deadline.to_le_bytes());
        buf.extend_from_slice(&self.rolled_over.to_le_bytes());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 1 + 32 + 15 + 32 + 1 + 32 + 1 + 1 + 32 + 32 {
            return Err(ContractError::IoError(format!(
                "Lottery: data too short ({} bytes)", data.len()
            )))
        }
        let mut pos = 0;
        let version = data[pos]; pos += 1;
        let id_bytes: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(id_bytes))
            .ok_or_else(|| ContractError::IoError("Lottery: invalid id".into()))?;

        // Parse LotteryConfig
        if data.len() < pos + 15 {
            return Err(ContractError::IoError("Lottery: data too short for config prefix".into()))
        }
        let tier_count = data[pos + 14] as usize;
        let config_end = pos + 15 + tier_count * PrizeTierConfig::ENCODED_SIZE;
        if data.len() < config_end {
            return Err(ContractError::IoError(format!(
                "Lottery: data too short for config (need {} bytes, have {})", config_end, data.len()
            )))
        }
        let config = decode_config(&data[pos..config_end])?;
        pos = config_end;

        let house_pub_bytes: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let house_pub = PublicKey::from_bytes(house_pub_bytes)
            .map_err(|e| ContractError::IoError(format!("Lottery: invalid house_pub: {}", e)))?;
        let state = LotteryState::from_u8(data[pos])
            .ok_or_else(|| ContractError::IoError(format!("Lottery: invalid state {}", data[pos])))?;
        pos += 1;
        let ticket_count = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let gross_pool = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let house_share = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let prize_pool = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        // winning_numbers: Option<Vec<u8>> — Pattern 4
        let wn_flag = data[pos]; pos += 1;
        let winning_numbers = if wn_flag != 0 {
            let count = data[pos] as usize; pos += 1;
            let nums = data[pos..pos+count].to_vec(); pos += count;
            Some(nums)
        } else { None };
        // draw_block: Option<u64> — Pattern 4
        let db_flag = data[pos]; pos += 1;
        let draw_block = if db_flag != 0 {
            let val = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
            Some(val)
        } else { None };
        let mr_bytes: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let ticket_merkle_root = Option::<pallas::Base>::from(pallas::Base::from_repr(mr_bytes))
            .ok_or_else(|| ContractError::IoError("Lottery: invalid ticket_merkle_root".into()))?;
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let draw_block_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let claim_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let rolled_over = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let instance_seed: [u8; 32] = data[pos..pos+32].try_into().unwrap();
        Ok(Lottery {
            version, id, config, house_pub, state, ticket_count, gross_pool,
            house_share, prize_pool, winning_numbers, draw_block, ticket_merkle_root,
            created_at, draw_block_deadline, claim_deadline, rolled_over, instance_seed,
        })
    }

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
#[derive(Debug, Clone)]
pub struct Ticket {
    pub version: u8,
    /// Unique ticket ID
    pub id: TicketId,
    /// Associated lottery ID
    pub lottery_id: LotteryId,
    /// Player's public key
    pub player_pub: PublicKey,
    /// Commitment: PoseidonHash(numbers, nonce, lottery_id)
    pub commitment: pallas::Base,
    /// Token ID being used
    pub asset_id: pallas::Base,
    /// Value (ticket price)
    pub value: u64,
    /// Nullifier for double-spend prevention
    pub nullifier: TicketId,
    /// Block at which ticket was purchased
    pub created_at: u64,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

impl Ticket {
    /// Fixed canonical byte size: version(1) + id(32) + lottery_id(32) + player_pub(32) +
    ///   commitment(32) + asset_id(32) + value(8) + nullifier(32) + created_at(8) + instance_seed(32)
    pub const ENCODED_SIZE: usize = 241;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.id.to_repr());
        buf.extend_from_slice(&self.lottery_id.to_repr());
        buf.extend_from_slice(&self.player_pub.to_bytes());
        buf.extend_from_slice(&self.commitment.to_repr());
        buf.extend_from_slice(&self.asset_id.to_repr());
        buf.extend_from_slice(&self.value.to_le_bytes());
        buf.extend_from_slice(&self.nullifier.to_repr());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.instance_seed);
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Ticket: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )))
        }
        let version = data[0];
        let id_bytes: [u8; 32] = data[1..33].try_into().unwrap();
        let id = Option::<pallas::Base>::from(pallas::Base::from_repr(id_bytes))
            .ok_or_else(|| ContractError::IoError("Ticket: invalid id".into()))?;
        let lid_bytes: [u8; 32] = data[33..65].try_into().unwrap();
        let lottery_id = Option::<pallas::Base>::from(pallas::Base::from_repr(lid_bytes))
            .ok_or_else(|| ContractError::IoError("Ticket: invalid lottery_id".into()))?;
        let pk_bytes: [u8; 32] = data[65..97].try_into().unwrap();
        let player_pub = PublicKey::from_bytes(pk_bytes)
            .map_err(|e| ContractError::IoError(format!("Ticket: invalid player_pub: {}", e)))?;
        let cm_bytes: [u8; 32] = data[97..129].try_into().unwrap();
        let commitment = Option::<pallas::Base>::from(pallas::Base::from_repr(cm_bytes))
            .ok_or_else(|| ContractError::IoError("Ticket: invalid commitment".into()))?;
        let tid_bytes: [u8; 32] = data[129..161].try_into().unwrap();
        let asset_id = Option::<pallas::Base>::from(pallas::Base::from_repr(tid_bytes))
            .ok_or_else(|| ContractError::IoError("Ticket: invalid asset_id".into()))?;
        let value = u64::from_le_bytes(data[161..169].try_into().unwrap());
        let nf_bytes: [u8; 32] = data[169..201].try_into().unwrap();
        let nullifier = Option::<pallas::Base>::from(pallas::Base::from_repr(nf_bytes))
            .ok_or_else(|| ContractError::IoError("Ticket: invalid nullifier".into()))?;
        let created_at = u64::from_le_bytes(data[201..209].try_into().unwrap());
        let instance_seed: [u8; 32] = data[209..241].try_into().unwrap();
        Ok(Ticket {
            version, id, lottery_id, player_pub, commitment, asset_id,
            value, nullifier, created_at, instance_seed,
        })
    }
}

impl Ticket {
    /// Derive the nullifier for this ticket
    pub fn derive_nullifier(&self) -> TicketId {
        poseidon_hash([self.id, self.nullifier])
    }
}

/// Prize claim structure
#[derive(Debug, Clone)]
pub struct Claim {
    pub version: u8,
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

impl Claim {
    /// Fixed canonical byte size: version(1) + ticket_id(32) + tier(1) + matches(1) + prize(8) + claimed_at(8)
    pub const ENCODED_SIZE: usize = 51;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.push(self.version);
        buf.extend_from_slice(&self.ticket_id.to_repr());
        buf.push(self.tier);
        buf.push(self.matches);
        buf.extend_from_slice(&self.prize.to_le_bytes());
        buf.extend_from_slice(&self.claimed_at.to_le_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Claim: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )))
        }
        let version = data[0];
        let tid_bytes: [u8; 32] = data[1..33].try_into().unwrap();
        let ticket_id = Option::<pallas::Base>::from(pallas::Base::from_repr(tid_bytes))
            .ok_or_else(|| ContractError::IoError("Claim: invalid ticket_id".into()))?;
        let tier = data[33];
        let matches = data[34];
        let prize = u64::from_le_bytes(data[35..43].try_into().unwrap());
        let claimed_at = u64::from_le_bytes(data[43..51].try_into().unwrap());
        Ok(Claim { version, ticket_id, tier, matches, prize, claimed_at })
    }
}

// ============================================================================
// PARAMS AND UPDATES
// ============================================================================

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
fn read_base(data: &[u8]) -> Result<pallas::Base, ContractError> { Option::<pallas::Base>::from(pallas::Base::from_repr(data.try_into().unwrap())).ok_or_else(|| ContractError::IoError("invalid base".into())) }

/// Parameters for InitializeV1
#[derive(Debug, Clone,)]
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
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
}

#[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
impl InitializeParamsV1 { pub fn encode(&self) -> Vec<u8> { let cfg = encode_config(&self.config); let mut b = Vec::with_capacity(89+cfg.len()); b.extend_from_slice(&self.house_pub.to_bytes()); b.extend_from_slice(&cfg); b.extend_from_slice(&self.duration.to_le_bytes()); b.extend_from_slice(&self.claim_duration.to_le_bytes()); b.extend_from_slice(&self.rolled_over.to_le_bytes()); b.extend_from_slice(&self.instance_seed); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 89 { return Err(ContractError::IoError(format!("InitializeParamsV1: too short ({} bytes)", data.len()))); } let hp_bytes: [u8;32] = data[0..32].try_into().unwrap(); let house_pub = PublicKey::from_bytes(hp_bytes).map_err(|e| ContractError::IoError(format!("InitializeParamsV1: invalid house_pub: {}", e)))?; let tier_count = data[46] as usize; let cfg_end = 47 + tier_count * PrizeTierConfig::ENCODED_SIZE; let config = decode_config(&data[32..cfg_end])?; let pos = cfg_end; let duration = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); let claim_duration = u64::from_le_bytes(data[pos+8..pos+16].try_into().unwrap()); let rolled_over = u64::from_le_bytes(data[pos+16..pos+24].try_into().unwrap()); let instance_seed: [u8;32] = data[pos+24..pos+56].try_into().unwrap(); Ok(InitializeParamsV1 { house_pub, config, duration, claim_duration, rolled_over, instance_seed }) } }

/// Update produced by InitializeV1
#[derive(Debug, Clone)]
pub struct InitializeUpdateV1 {
    pub lottery_id: LotteryId,
    pub config: LotteryConfig,
    pub house_pub: PublicKey,
    pub draw_block_deadline: u64,
    pub claim_deadline: u64,
    pub rolled_over: u64,
    pub state: LotteryState,
    pub instance_seed: [u8; 32],
    pub created_at: u64,
}

impl InitializeUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Format: lottery_id(32) + config(var) + house_pub(32) + draw_block_deadline(8) +
    ///   claim_deadline(8) + rolled_over(8) + state(1) + instance_seed(32) + created_at(8)
    pub fn encode(&self) -> Vec<u8> {
        let config_bytes = encode_config(&self.config);
        let cap = 32 + config_bytes.len() + 32 + 8 + 8 + 8 + 1 + 32 + 8;
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.lottery_id.to_repr());
        buf.extend_from_slice(&config_bytes);
        buf.extend_from_slice(&self.house_pub.to_bytes());
        buf.extend_from_slice(&self.draw_block_deadline.to_le_bytes());
        buf.extend_from_slice(&self.claim_deadline.to_le_bytes());
        buf.extend_from_slice(&self.rolled_over.to_le_bytes());
        buf.push(match self.state {
            LotteryState::Initialized => 0u8,
            LotteryState::WinnersDrawn => 1u8,
            LotteryState::Expired => 2u8,
        });
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 + 15 + 32 + 8 + 8 + 8 + 1 + 32 + 8 {
            return Err(ContractError::IoError(format!(
                "InitializeUpdateV1: data too short ({} bytes)", data.len()
            )))
        }
        let mut pos = 0;
        let lid_bytes: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let lottery_id = Option::<pallas::Base>::from(pallas::Base::from_repr(lid_bytes))
            .ok_or_else(|| ContractError::IoError("InitializeUpdateV1: invalid lottery_id".into()))?;
        // Parse LotteryConfig
        let tier_count = data[pos + 14] as usize;
        let config_end = pos + 15 + tier_count * PrizeTierConfig::ENCODED_SIZE;
        if data.len() < config_end {
            return Err(ContractError::IoError(format!(
                "InitializeUpdateV1: data too short for config (need {}, have {})",
                config_end, data.len()
            )))
        }
        let config = decode_config(&data[pos..config_end])?;
        pos = config_end;
        let hp_bytes: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let house_pub = PublicKey::from_bytes(hp_bytes)
            .map_err(|e| ContractError::IoError(format!("InitializeUpdateV1: invalid house_pub: {}", e)))?;
        let draw_block_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let claim_deadline = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let rolled_over = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let state = LotteryState::from_u8(data[pos])
            .ok_or_else(|| ContractError::IoError(format!(
                "InitializeUpdateV1: invalid state {}", data[pos]
            )))?;
        pos += 1;
        let instance_seed: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap());
        Ok(InitializeUpdateV1 {
            lottery_id, config, house_pub, draw_block_deadline,
            claim_deadline, rolled_over, state, instance_seed, created_at,
        })
    }
}

/// Parameters for BuyTicketV1
#[derive(Debug, Clone,)]
pub struct BuyTicketParamsV1 {
    /// Player's public key
    pub player_pub: PublicKey,
    /// Commitment: PoseidonHash(numbers, nonce, lottery_id)
    pub commitment: pallas::Base,
    /// Token ID
    pub asset_id: pallas::Base,
    /// Value (ticket price)
    pub value: u64,
    /// Value commitment point
    pub value_commit: pallas::Point,
    /// Signature
    pub signature: pallas::Base,
    /// Per-instance seed for deriving capability-scoped keys
    pub instance_seed: [u8; 32],
    /// Lottery this ticket belongs to (commit_ticket.zk witness)
    pub lottery_id: LotteryId,
    /// Secret nonce used in the ticket_id derivation and commitment (commit_ticket.zk witness)
    pub nonce: pallas::Base,
}

impl BuyTicketParamsV1 {
    pub const ENCODED_SIZE: usize = 264;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.player_pub.to_bytes());
        b.extend_from_slice(&self.commitment.to_repr());
        b.extend_from_slice(&self.asset_id.to_repr());
        b.extend_from_slice(&self.value.to_le_bytes());
        b.extend_from_slice(&self.value_commit.to_bytes());
        b.extend_from_slice(&self.signature.to_repr());
        b.extend_from_slice(&self.instance_seed);
        b.extend_from_slice(&self.lottery_id.to_repr());
        b.extend_from_slice(&self.nonce.to_repr());
        b
    }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "BuyTicketParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )))
        }
        let player_pub = PublicKey::from_bytes(data[0..32].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("BuyTicketParamsV1: invalid player_pub: {}", e)))?;
        let commitment = read_base(&data[32..64])?;
        let asset_id = read_base(&data[64..96])?;
        let value = u64::from_le_bytes(data[96..104].try_into().unwrap());
        let value_commit = Option::<pallas::Point>::from(pallas::Point::from_bytes(data[104..136].try_into().unwrap()))
            .ok_or_else(|| ContractError::IoError("BuyTicketParamsV1: invalid value_commit".into()))?;
        let signature = read_base(&data[136..168])?;
        let instance_seed: [u8; 32] = data[168..200].try_into().unwrap();
        let lottery_id = read_base(&data[200..232])?;
        let nonce = read_base(&data[232..264])?;
        Ok(BuyTicketParamsV1 { player_pub, commitment, asset_id, value, value_commit, signature, instance_seed, lottery_id, nonce })
    }
}

/// Update produced by BuyTicketV1
#[derive(Debug, Clone)]
pub struct BuyTicketUpdateV1 {
    pub ticket_id: TicketId,
    pub lottery_id: LotteryId,
    pub player_pub: PublicKey,
    pub commitment: pallas::Base,
    pub asset_id: pallas::Base,
    pub value: u64,
    pub nullifier: TicketId,
    pub created_at: u64,
    pub instance_seed: [u8; 32],
    /// Carried lottery record (exec advances ticket_count/gross_pool; apply re-stores it
    /// without a db_get-in-apply).
    pub lottery: Lottery,
}

impl BuyTicketUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Format: ticket_id(32) + lottery_id(32) + player_pub(32) + commitment(32) +
    ///   asset_id(32) + value(8) + nullifier(32) + created_at(8) + instance_seed(32) + lottery(var)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(240 + self.lottery.encode().len());
        buf.extend_from_slice(&self.ticket_id.to_repr());
        buf.extend_from_slice(&self.lottery_id.to_repr());
        buf.extend_from_slice(&self.player_pub.to_bytes());
        buf.extend_from_slice(&self.commitment.to_repr());
        buf.extend_from_slice(&self.asset_id.to_repr());
        buf.extend_from_slice(&self.value.to_le_bytes());
        buf.extend_from_slice(&self.nullifier.to_repr());
        buf.extend_from_slice(&self.created_at.to_le_bytes());
        buf.extend_from_slice(&self.instance_seed);
        buf.extend_from_slice(&self.lottery.encode());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 240 {
            return Err(ContractError::IoError(format!(
                "BuyTicketUpdateV1: data too short ({} bytes)", data.len()
            )))
        }
        let mut pos = 0;
        let f32 = |p: &mut usize, d: &[u8]| -> Result<pallas::Base, ContractError> {
            let b: [u8; 32] = d[*p..*p+32].try_into().unwrap(); *p += 32;
            Option::<pallas::Base>::from(pallas::Base::from_repr(b))
                .ok_or_else(|| ContractError::IoError("BuyTicketUpdateV1: invalid pallas::Base".into()))
        };
        let ticket_id = f32(&mut pos, data)?;
        let lottery_id = f32(&mut pos, data)?;
        let pk_bytes: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let player_pub = PublicKey::from_bytes(pk_bytes)
            .map_err(|e| ContractError::IoError(format!("BuyTicketUpdateV1: invalid player_pub: {}", e)))?;
        let commitment = f32(&mut pos, data)?;
        let asset_id = f32(&mut pos, data)?;
        let value = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let nullifier = f32(&mut pos, data)?;
        let created_at = u64::from_le_bytes(data[pos..pos+8].try_into().unwrap()); pos += 8;
        let instance_seed: [u8; 32] = data[pos..pos+32].try_into().unwrap(); pos += 32;
        let lottery = Lottery::decode(&data[pos..])?;
        Ok(BuyTicketUpdateV1 {
            ticket_id, lottery_id, player_pub, commitment, asset_id,
            value, nullifier, created_at, instance_seed, lottery,
        })
    }
}

/// Parameters for DrawWinnersV1
#[derive(Debug, Clone,)]
pub struct DrawWinnersParamsV1 {
    /// Lottery ID
    pub lottery_id: LotteryId,
    /// Nonce for randomness
    pub nonce: pallas::Base,
    /// House public key (draw_winners.zk instance)
    pub house_pub: PublicKey,
    /// House nullifier (draw_winners.zk instance)
    pub house_nullifier: pallas::Base,
}

impl DrawWinnersParamsV1 {
    pub const ENCODED_SIZE: usize = 128;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.lottery_id.to_repr());
        b.extend_from_slice(&self.nonce.to_repr());
        b.extend_from_slice(&self.house_pub.to_bytes());
        b.extend_from_slice(&self.house_nullifier.to_repr());
        b
    }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "DrawWinnersParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )))
        }
        let hp_bytes: [u8; 32] = data[64..96].try_into().unwrap();
        let house_pub = PublicKey::from_bytes(hp_bytes)
            .map_err(|e| ContractError::IoError(format!("DrawWinnersParamsV1: invalid house_pub: {}", e)))?;
        Ok(DrawWinnersParamsV1 {
            lottery_id: read_base(&data[0..32])?,
            nonce: read_base(&data[32..64])?,
            house_pub,
            house_nullifier: read_base(&data[96..128])?,
        })
    }
}

/// Update produced by DrawWinnersV1
#[derive(Debug, Clone)]
pub struct DrawWinnersUpdateV1 {
    pub lottery_id: LotteryId,
    /// Carried lottery record (exec sets winning_numbers/draw_block/pools/state; apply re-stores
    /// it without a db_get-in-apply).
    pub lottery: Lottery,
}

impl DrawWinnersUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Format: lottery_id(32) + lottery(var)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + self.lottery.encode().len());
        buf.extend_from_slice(&self.lottery_id.to_repr());
        buf.extend_from_slice(&self.lottery.encode());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 {
            return Err(ContractError::IoError(format!(
                "DrawWinnersUpdateV1: data too short ({} bytes)", data.len()
            )))
        }
        let lid_bytes: [u8; 32] = data[0..32].try_into().unwrap();
        let lottery_id = Option::<pallas::Base>::from(pallas::Base::from_repr(lid_bytes))
            .ok_or_else(|| ContractError::IoError("DrawWinnersUpdateV1: invalid lottery_id".into()))?;
        let lottery = Lottery::decode(&data[32..])?;
        Ok(DrawWinnersUpdateV1 { lottery_id, lottery })
    }
}

/// Parameters for RevealTicketV1
#[derive(Debug, Clone,)]
pub struct RevealTicketParamsV1 {
    /// Ticket ID
    pub ticket_id: TicketId,
    /// The N numbers the player selected
    pub numbers: Vec<u8>,
    /// Secret nonce used in commitment
    pub nonce: pallas::Base,
    /// Revealed commitment (public input for ZK proof)
    pub revealed_commitment: pallas::Base,
    /// Number of matches (public input for ZK proof)
    pub matches: u8,
}

impl RevealTicketParamsV1 { pub fn encode(&self) -> Vec<u8> { let mut b = Vec::with_capacity(66+self.numbers.len()); b.extend_from_slice(&self.ticket_id.to_repr()); b.push(self.numbers.len() as u8); b.extend_from_slice(&self.numbers); b.extend_from_slice(&self.nonce.to_repr()); b.extend_from_slice(&self.revealed_commitment.to_repr()); b.push(self.matches); b } pub fn decode(data: &[u8]) -> Result<Self, ContractError> { if data.len() < 66 { return Err(ContractError::IoError(format!("RevealTicketParamsV1: too short ({} bytes)", data.len()))); } let ticket_id = read_base(&data[0..32])?; let num_len = data[32] as usize; let end = 33+num_len; if data.len() < end+33 { return Err(ContractError::IoError("RevealTicketParamsV1: numbers truncated".into())); } let numbers = data[33..end].to_vec(); let nonce = read_base(&data[end..end+32])?; let revealed_commitment = read_base(&data[end+32..end+64])?; let matches = data[end+64]; Ok(RevealTicketParamsV1 { ticket_id, numbers, nonce, revealed_commitment, matches }) } }

/// Update produced by RevealTicketV1
#[derive(Debug, Clone)]
pub struct RevealTicketUpdateV1 {
    pub ticket_id: TicketId,
    pub matches: u8,
    pub tier: Option<u8>,
}

impl RevealTicketUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Format: ticket_id(32) + matches(1) + tier: Option<u8> (Pattern 4: 1-byte flag + value if Some)
    pub fn encode(&self) -> Vec<u8> {
        let cap = 32 + 1 + 1 + self.tier.map_or(0, |_| 1);
        let mut buf = Vec::with_capacity(cap);
        buf.extend_from_slice(&self.ticket_id.to_repr());
        buf.push(self.matches);
        // tier: Option<u8> — Pattern 4
        if let Some(t) = self.tier {
            buf.push(1u8);
            buf.push(t);
        } else {
            buf.push(0u8);
        }
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 34 {
            return Err(ContractError::IoError(format!(
                "RevealTicketUpdateV1: data too short ({} bytes, need at least 34)", data.len()
            )))
        }
        let tid_bytes: [u8; 32] = data[0..32].try_into().unwrap();
        let ticket_id = Option::<pallas::Base>::from(pallas::Base::from_repr(tid_bytes))
            .ok_or_else(|| ContractError::IoError("RevealTicketUpdateV1: invalid ticket_id".into()))?;
        let matches = data[32];
        // tier: Option<u8> — Pattern 4
        let tier_flag = data[33];
        let (tier, pos) = if tier_flag != 0 {
            if data.len() < 35 {
                return Err(ContractError::IoError(
                    "RevealTicketUpdateV1: expected tier byte".into()
                ))
            }
            (Some(data[34]), 35)
        } else {
            if data.len() != 34 {
                return Err(ContractError::IoError(format!(
                    "RevealTicketUpdateV1: expected 34 bytes (no tier), got {}",
                    data.len()
                )))
            }
            (None, 34)
        };
        if pos != data.len() {
            return Err(ContractError::IoError(format!(
                "RevealTicketUpdateV1: trailing bytes (expected {} bytes, got {})",
                pos, data.len()
            )))
        }
        Ok(RevealTicketUpdateV1 { ticket_id, matches, tier })
    }
}

/// Parameters for ClaimPrizeV1
#[derive(Debug, Clone,)]
pub struct ClaimPrizeParamsV1 {
    /// Ticket ID to claim
    pub ticket_id: TicketId,
    /// ZK proof of reveal
    pub proof: Vec<u8>,
    /// Prize tier won (extracted from ZK proof verification on client)
    pub tier: u8,
    /// Number of matching numbers
    pub matches: u8,
    /// Ticket-ownership commitment: poseidon_hash(4, ticket_id, ticket_secret)
    /// (claim_prize.zk instance — the contract cannot recompute it without the secret)
    pub computed_commit: pallas::Base,
}

impl ClaimPrizeParamsV1 {
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(67 + self.proof.len());
        b.extend_from_slice(&self.ticket_id.to_repr());
        b.push(self.proof.len() as u8);
        b.extend_from_slice(&self.proof);
        b.extend_from_slice(&self.computed_commit.to_repr());
        b.push(self.tier);
        b.push(self.matches);
        b
    }
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 67 {
            return Err(ContractError::IoError("ClaimPrizeParamsV1: too short".into()))
        }
        let ticket_id = read_base(&data[0..32])?;
        let proof_len = data[32] as usize;
        let end = 33 + proof_len;
        if data.len() != end + 34 {
            return Err(ContractError::IoError(format!(
                "ClaimPrizeParamsV1: expected {} bytes, got {}", end + 34, data.len()
            )))
        }
        let proof = data[33..end].to_vec();
        let computed_commit = read_base(&data[end..end+32])?;
        let tier = data[end+32];
        let matches = data[end+33];
        Ok(ClaimPrizeParamsV1 { ticket_id, proof, tier, matches, computed_commit })
    }
}

/// Update produced by ClaimPrizeV1
#[derive(Debug, Clone)]
pub struct ClaimPrizeUpdateV1 {
    pub ticket_id: TicketId,
    pub tier: u8,
    pub matches: u8,
    pub prize: u64,
    pub claimed_at: u64,
}

impl ClaimPrizeUpdateV1 {
    /// Fixed canonical byte size: ticket_id(32) + tier(1) + matches(1) + prize(8) + claimed_at(8)
    pub const ENCODED_SIZE: usize = 50;

    /// Encode to canonical bytes (ρ-calculus: quote).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(Self::ENCODED_SIZE);
        buf.extend_from_slice(&self.ticket_id.to_repr());
        buf.push(self.tier);
        buf.push(self.matches);
        buf.extend_from_slice(&self.prize.to_le_bytes());
        buf.extend_from_slice(&self.claimed_at.to_le_bytes());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "ClaimPrizeUpdateV1: expected {} bytes, got {}",
                Self::ENCODED_SIZE, data.len()
            )))
        }
        let tid_bytes: [u8; 32] = data[0..32].try_into().unwrap();
        let ticket_id = Option::<pallas::Base>::from(pallas::Base::from_repr(tid_bytes))
            .ok_or_else(|| ContractError::IoError("ClaimPrizeUpdateV1: invalid ticket_id".into()))?;
        let tier = data[32];
        let matches = data[33];
        let prize = u64::from_le_bytes(data[34..42].try_into().unwrap());
        let claimed_at = u64::from_le_bytes(data[42..50].try_into().unwrap());
        Ok(ClaimPrizeUpdateV1 { ticket_id, tier, matches, prize, claimed_at })
    }
}

/// Parameters for ExpireLotteryV1
#[derive(Debug, Clone,)]
pub struct ExpireLotteryParamsV1 {
    /// Lottery ID to expire
    pub lottery_id: LotteryId,
    /// House public key (expire_lottery.zk instance)
    pub house_pub: PublicKey,
    /// House nullifier (expire_lottery.zk instance)
    pub house_nullifier: pallas::Base,
}

impl ExpireLotteryParamsV1 {
    pub const ENCODED_SIZE: usize = 96;
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.lottery_id.to_repr());
        b.extend_from_slice(&self.house_pub.to_bytes());
        b.extend_from_slice(&self.house_nullifier.to_repr());
        b
    }
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() != Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "ExpireLotteryParamsV1: expected {} bytes, got {}", Self::ENCODED_SIZE, data.len()
            )))
        }
        let hp_bytes: [u8; 32] = data[32..64].try_into().unwrap();
        let house_pub = PublicKey::from_bytes(hp_bytes)
            .map_err(|e| ContractError::IoError(format!("ExpireLotteryParamsV1: invalid house_pub: {}", e)))?;
        Ok(ExpireLotteryParamsV1 {
            lottery_id: read_base(&data[0..32])?,
            house_pub,
            house_nullifier: read_base(&data[64..96])?,
        })
    }
}

/// Update produced by ExpireLotteryV1
#[derive(Debug, Clone)]
pub struct ExpireLotteryUpdateV1 {
    pub lottery_id: LotteryId,
    /// Carried lottery record (exec sets state=Expired; apply re-stores it without a
    /// db_get-in-apply).
    pub lottery: Lottery,
}

impl ExpireLotteryUpdateV1 {
    /// Encode to canonical bytes (ρ-calculus: quote).
    /// Format: lottery_id(32) + lottery(var)
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(32 + self.lottery.encode().len());
        buf.extend_from_slice(&self.lottery_id.to_repr());
        buf.extend_from_slice(&self.lottery.encode());
        buf
    }

    /// Decode from canonical bytes (ρ-calculus: eval).
    #[expect(clippy::unwrap_used, reason = "internally-consistent serialized data")]
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < 32 {
            return Err(ContractError::IoError(format!(
                "ExpireLotteryUpdateV1: data too short ({} bytes)", data.len()
            )))
        }
        let lid_bytes: [u8; 32] = data[0..32].try_into().unwrap();
        let lottery_id = Option::<pallas::Base>::from(pallas::Base::from_repr(lid_bytes))
            .ok_or_else(|| ContractError::IoError("ExpireLotteryUpdateV1: invalid lottery_id".into()))?;
        let lottery = Lottery::decode(&data[32..])?;
        Ok(ExpireLotteryUpdateV1 { lottery_id, lottery })
    }
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

/// Derive ticket ID from parameters (V2 — domain-separated, matches commit_ticket.zk).
///
/// Circuit: `computed_ticket_id = poseidon_hash(4, lottery_id, ticket_pub_x, ticket_pub_y,
/// amount, nonce)`.
#[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
pub fn derive_ticket_id(
    lottery_id: LotteryId,
    player_pub: &PublicKey,
    amount: u64,
    nonce: pallas::Base,
) -> TicketId {
    poseidon_hash([
        pallas::Base::from(4u64),
        lottery_id,
        player_pub.x().expect("pk not identity"),
        player_pub.y().expect("pk not identity"),
        pallas::Base::from(amount),
        nonce,
    ])
}

/// Derive nullifier for a ticket
pub fn derive_nullifier(ticket_id: TicketId, nonce: pallas::Base) -> TicketId {
    poseidon_hash([ticket_id, nonce])
}

/// Derive lottery ID from house_pub and creation block
#[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
pub fn derive_lottery_id(house_pub: &PublicKey, created_at: u64) -> LotteryId {
    poseidon_hash([house_pub.x().expect("pk not identity"), house_pub.y().expect("pk not identity"), pallas::Base::from(created_at)])
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
