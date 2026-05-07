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

//! DarkWow DrainProtection Contract
//!
//! Governance-level protections for endowment/treasury funds:
//! - Rate limiting per block
//! - 2/3 vote thresholds for large withdrawals
//! - Lock/unlock emergency controls
//! - Spend authority management
//! - Member exit with haircut
//!
//! ## Provisional Status
//!
//! This contract is EXPERIMENTAL and has NOT been audited.
//! The protections described here are provisionally specified
//! and require full implementation and security review.

pub mod client;
pub mod error;
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
pub mod entrypoint;

use error::DrainProtectionError;

/// DrainProtection contract functions
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrainProtectionFunction {
    /// Initialize a new protected fund
    InitializeV1 = 0x00,
    /// Propose a vote (large withdrawal, lock, authority change)
    ProposeV1 = 0x01,
    /// Cast a vote on a proposal
    VoteV1 = 0x02,
    /// Execute a concluded proposal
    ExecuteV1 = 0x03,
    /// Exit with haircut (any member, any time)
    ExitV1 = 0x04,
    /// Transfer funds (rate-limited)
    TransferV1 = 0x05,
    /// Lock funds (emergency)
    LockV1 = 0x06,
    /// Unlock funds
    UnlockV1 = 0x07,
    /// Update configuration
    UpdateConfigV1 = 0x08,
}

impl TryFrom<u8> for DrainProtectionFunction {
    type Error = DrainProtectionError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::ProposeV1),
            0x02 => Ok(Self::VoteV1),
            0x03 => Ok(Self::ExecuteV1),
            0x04 => Ok(Self::ExitV1),
            0x05 => Ok(Self::TransferV1),
            0x06 => Ok(Self::LockV1),
            0x07 => Ok(Self::UnlockV1),
            0x08 => Ok(Self::UpdateConfigV1),
            _ => Err(DrainProtectionError::ConfigurationError("Invalid function".to_string()).into()),
        }
    }
}

// ============================================================================
// Database Tree Keys
// ============================================================================

/// Info tree: version, config
pub const DRAIN_PROTECTION_CONTRACT_INFO_TREE: &str = "info";

/// Funds tree: protected fund records
pub const DRAIN_PROTECTION_CONTRACT_FUNDS_TREE: &str = "funds";

/// Proposals tree: pending vote proposals
pub const DRAIN_PROTECTION_CONTRACT_PROPOSALS_TREE: &str = "proposals";

/// Members tree: member weights for exit calculations
pub const DRAIN_PROTECTION_CONTRACT_MEMBERS_TREE: &str = "members";

/// Transfer history tree: for rate limiting
pub const DRAIN_PROTECTION_CONTRACT_TRANSFERS_TREE: &str = "transfers";

/// Exits tree: processed exit requests
pub const DRAIN_PROTECTION_CONTRACT_EXITS_TREE: &str = "exits";

/// Vote history tree: for preventing double-voting
pub const DRAIN_PROTECTION_CONTRACT_VOTES_TREE: &str = "votes";

/// ZK proof namespaces
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_EXIT_NS_V1: &str = "DRAIN_PROTECTION_EXIT_V1";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_PROPOSAL_NS_V1: &str = "DRAIN_PROTECTION_PROPOSAL_V1";