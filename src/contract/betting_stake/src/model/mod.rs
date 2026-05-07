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

//! Betting Stake Contract Model
//!
//! Data structures for capital staking against betting contracts.

use darkfi_sdk::{
    crypto::{PublicKey, schnorr::Signature},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::EARNINGS_BP;

// =============================================================================
// STAKE REGISTRY
// =============================================================================

/// Registry of a single betting table's staking info
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TableStakeRegistry {
    /// Contract ID of the betting contract (Dice, Baccarat, etc.)
    pub betting_contract_id: pallas::Base,
    /// Total capital staked against this table
    pub total_stake: u64,
    /// Accumulated earnings for stakers
    pub accumulated_earnings: u64,
    /// Accumulated losses (payouts that stakers absorbed)
    pub accumulated_losses: u64,
    /// Number of active stakers
    pub staker_count: u64,
    /// House edge of the underlying betting contract (in basis points)
    pub house_edge_bp: u32,
    /// Risk profile (determines risk premium)
    pub risk_profile: u8, // 0=Low, 1=Medium, 2=High
}

impl TableStakeRegistry {
    /// Calculate earnings rate per unit staked
    pub fn earnings_rate_bp(&self) -> u32 {
        // Base house edge - accumulated losses ratio + risk premium
        let base = self.house_edge_bp;
        let losses_ratio = if self.total_stake > 0 {
            self.accumulated_losses
                .checked_mul(EARNINGS_BP as u64)
                .and_then(|v| v.checked_div(self.total_stake))
                .map(|v| v as u32)
                .unwrap_or(0)
        } else {
            0
        };
        // Net earnings rate (can be negative if losses exceed house edge)
        base.saturating_sub(losses_ratio)
    }

    /// Calculate loss absorption capacity
    pub fn loss_absorption_capacity(&self) -> u64 {
        self.total_stake.saturating_sub(self.accumulated_losses)
    }
}

// =============================================================================
// INDIVIDUAL STAKE
// =============================================================================

/// Individual stake position
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Stake {
    /// Unique stake ID
    pub stake_id: pallas::Base,
    /// Associated table registry ID
    pub table_id: pallas::Base,
    /// Staker's public key
    pub staker_pub: PublicKey,
    /// Original stake amount
    pub original_amount: u64,
    /// Current stake amount (decreases with losses)
    pub current_amount: u64,
    /// Accumulated earnings (can be claimed)
    pub accumulated_earnings: u64,
    /// Timestamp of when stake was created
    pub created_at: u64,
    /// Timestamp of when unstake was requested (if any)
    pub unstake_requested_at: Option<u64>,
    /// Whether stake is active
    pub is_active: bool,
}

impl Stake {
    /// Calculate current share of the table's earnings
    pub fn earnings_share(&self, table: &TableStakeRegistry) -> u64 {
        if table.total_stake == 0 {
            return 0
        }
        // Proportional share of earnings based on stake proportion
        (table.accumulated_earnings * self.current_amount) / table.total_stake
    }

    /// Calculate proportional loss
    pub fn loss_share(&self, loss_amount: u64, table: &TableStakeRegistry) -> u64 {
        if table.total_stake == 0 {
            return 0
        }
        (loss_amount * self.current_amount) / table.total_stake
    }

    /// Check if unstake is available (no pending unstake request or lock period passed)
    pub fn can_unstake(&self, lock_blocks: u64, current_block: u64) -> bool {
        if !self.is_active {
            return false
        }
        match self.unstake_requested_at {
            Some(req_at) => current_block >= req_at + lock_blocks,
            None => true, // Can unstake immediately if no pending request
        }
    }
}

// =============================================================================
// PARAMS AND UPDATES
// =============================================================================

/// Parameters for InitializeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParamsV1 {
    /// Contract ID of the betting contract this staking is for
    pub betting_contract_id: pallas::Base,
    /// House edge of the betting contract (in basis points)
    pub house_edge_bp: u32,
    /// Risk profile (0=Low, 1=Medium, 2=High)
    pub risk_profile: u8,
    /// Signature from betting contract verifying these params
    pub signature: Signature,
}

/// Update produced by InitializeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    pub table_id: pallas::Base,
    pub betting_contract_id: pallas::Base,
    pub house_edge_bp: u32,
    pub risk_profile: u8,
}

/// Parameters for StakeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct StakeParamsV1 {
    /// Table ID to stake against
    pub table_id: pallas::Base,
    /// Staker's public key
    pub staker_pub: PublicKey,
    /// Amount to stake
    pub amount: u64,
    /// Signature from staker
    pub signature: Signature,
    /// Spend hook FuncId for money_v3::transfer_v1 callback
    pub spend_hook: pallas::Base,
    /// User data for spend hook callback
    pub user_data: pallas::Base,
}

/// Update produced by StakeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct StakeUpdateV1 {
    pub stake_id: pallas::Base,
    pub table_id: pallas::Base,
    pub staker_pub: PublicKey,
    pub amount: u64,
    pub total_stake: u64,
    pub staker_count: u64,
}

/// Parameters for UnstakeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnstakeParamsV1 {
    /// Stake ID to unstake
    pub stake_id: pallas::Base,
    /// Signature from staker
    pub signature: Signature,
    /// Spend hook FuncId for money_v3::transfer_v1 callback
    pub spend_hook: pallas::Base,
    /// User data for spend hook callback
    pub user_data: pallas::Base,
}

/// Update produced by UnstakeV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UnstakeUpdateV1 {
    pub stake_id: pallas::Base,
    pub payout_amount: u64, // original stake + earnings - losses
    pub unstake_penalty: u64, // any penalty for early unstake
}

/// Parameters for ClaimEarningsV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimEarningsParamsV1 {
    /// Stake ID to claim earnings for
    pub stake_id: pallas::Base,
    /// Signature from staker
    pub signature: Signature,
}

/// Update produced by ClaimEarningsV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimEarningsUpdateV1 {
    pub stake_id: pallas::Base,
    pub claimed_amount: u64,
    pub remaining_earnings: u64,
}

/// Parameters for UpdateRiskV1 (called by betting contracts when payouts occur)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateRiskParamsV1 {
    /// Table ID
    pub table_id: pallas::Base,
    /// Total payout amount that stakers must absorb
    pub payout_amount: u64,
    /// House's share of the payout (if any)
    pub house_share: u64,
}

/// Update produced by UpdateRiskV1
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateRiskUpdateV1 {
    pub table_id: pallas::Base,
    pub total_payout: u64,
    pub staker_loss: u64, // Total loss distributed among stakers
    pub staker_count: u64,
    pub new_total_stake: u64,
}

// =============================================================================
// HELPERS
// =============================================================================

/// Derive table ID from betting contract ID
pub fn derive_table_id(betting_contract_id: pallas::Base, nonce: u64) -> pallas::Base {
    use darkfi_sdk::crypto::poseidon_hash;
    poseidon_hash([betting_contract_id, pallas::Base::from(nonce)])
}

/// Derive stake ID
pub fn derive_stake_id(table_id: pallas::Base, staker_pub: &PublicKey, amount: u64, nonce: u64) -> pallas::Base {
    use darkfi_sdk::crypto::poseidon_hash;
    poseidon_hash([table_id, staker_pub.x(), staker_pub.y(), pallas::Base::from(amount), pallas::Base::from(nonce)])
}
