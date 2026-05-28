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

//! DarkWow Pool Stake Contract
//!
//! ## Overview
//!
//! Pool staking for relayer shared coverage. Multiple relayers contribute stake
//! to a pool, which provides coverage for guaranteed withdrawals on the bridge.
//!
//! ## How It Works
//!
//! 1. **Create Pool**: A relayer creates a staking pool with configuration
//! 2. **Join Pool**: Other relayers join by staking DAI/NETHER
//! 3. **Allocate Coverage**: When a guaranteed withdrawal is submitted, coverage
//!    is allocated from the pool to cover potential slashing
//! 4. **Execute**: Withdrawal executes on external chain
//! 5. **Release/Slash**: On success, coverage is released back. On failure,
//!    coverage is slashed and given to the user as compensation
//!
//! ## Economic Model
//!
//! - **Stake**: Relayers stake DAI + NETHER for coverage capacity
//! - **Coverage**: Pool can cover up to max_coverage_ratio * total_stake
//! - **Fees**: Relayers earn a share of bridge fees proportional to their stake
//! - **Slash**: Failed guaranteed withdrawals slash pool coverage to repay user
//!
//! ## Pool vs Solo Staking
//!
//! | Aspect | Solo Relayer | Pooled Relayers |
//! |--------|--------------|-----------------|
//! | Stake required | Full coverage amount | Proportional share |
//! | Coverage | Limited to own stake | Combined pool coverage |
//! | Slashing risk | Full loss | Share of loss |
//! | Fees | All bridge fees | Proportional share |

use dwow_sdk::{
    error::ContractError,
};

/// Pool Stake Functions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PoolStakeFunction {
    /// Create a new staking pool
    CreatePoolV1 = 0x00,
    /// Join an existing pool by staking
    JoinPoolV1 = 0x01,
    /// Request to leave pool (after cooldown)
    LeavePoolV1 = 0x02,
    /// Allocate coverage for a guaranteed withdrawal
    AllocateCoverageV1 = 0x03,
    /// Release coverage after successful execution
    ReleaseCoverageV1 = 0x04,
    /// Slash coverage for failed withdrawal (compensation to user)
    SlashCoverageV1 = 0x05,
    /// Claim accumulated fees from the pool
    ClaimFeesV1 = 0x06,
    /// Update pool configuration
    UpdatePoolConfigV1 = 0x07,
    /// Rebalance pool member shares based on attested performance
    RebalancePoolSharesV1 = 0x08,
}

impl TryFrom<u8> for PoolStakeFunction {
    type Error = ContractError;
    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0x00 => Ok(Self::CreatePoolV1),
            0x01 => Ok(Self::JoinPoolV1),
            0x02 => Ok(Self::LeavePoolV1),
            0x03 => Ok(Self::AllocateCoverageV1),
            0x04 => Ok(Self::ReleaseCoverageV1),
            0x05 => Ok(Self::SlashCoverageV1),
            0x06 => Ok(Self::ClaimFeesV1),
            0x07 => Ok(Self::UpdatePoolConfigV1),
            0x08 => Ok(Self::RebalancePoolSharesV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Internal contract errors
pub mod error;

/// Capability descriptor
pub mod capability;

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for interaction with this smart contract
pub mod client;

// Database tree names
/// Pool registry tree - stores pool configurations
pub const POOL_STAKE_REGISTRY_TREE: &str = "pool_registry";
/// Member stakes tree - stores individual member stakes
pub const POOL_STAKE_MEMBERS_TREE: &str = "pool_members";
/// Coverage allocations tree - stores active coverage allocations
pub const POOL_STAKE_ALLOCATIONS_TREE: &str = "pool_allocations";
/// Accumulated fees tree - stores fee allocations per member
pub const POOL_STAKE_FEES_TREE: &str = "pool_fees";
/// Info tree - stores contract info (version, config)
pub const POOL_STAKE_INFO_TREE: &str = "pool_stake_info";

// Database keys
/// Database version key
pub const POOL_STAKE_DB_VERSION: &[u8] = b"db_version";
/// Pool count key
pub const POOL_STAKE_POOL_COUNT: &[u8] = b"pool_count";

// Constants
/// Minimum stake amount to join a pool
pub const POOL_STAKE_MIN_STAKE: u64 = 1_000_000; // 1 DAI equivalent
/// Maximum coverage ratio denominator (10000 = 1:1 stake:coverage)
pub const POOL_STAKE_MAX_COVERAGE_RATIO: u32 = 10000;
/// Default pool cooldown period in blocks before leaving
pub const POOL_STAKE_LEAVE_COOLDOWN_BLOCKS: u64 = 100;
/// Basis points precision for fee calculations
pub const POOL_STAKE_BP_PRECISION: u32 = 10000;

// zkas circuit namespaces
pub const POOL_STAKE_ZKAS_CREATE_POOL_NS_V1: &str = "CreatePool";
pub const POOL_STAKE_ZKAS_JOIN_POOL_NS_V1: &str = "JoinPool";
pub const POOL_STAKE_ZKAS_ALLOCATE_COVERAGE_NS_V1: &str = "AllocateCoverage";
pub const POOL_STAKE_ZKAS_SLASH_COVERAGE_NS_V1: &str = "SlashCoverage";