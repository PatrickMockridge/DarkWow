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
pub mod capability;

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

/// Keys inside the info tree
pub const DRAIN_PROTECTION_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID: &[u8] = b"promissory_note_cid";
pub const DRAIN_PROTECTION_CONTRACT_PURSE_CONTRACT_ID: &[u8] = b"purse_cid";
pub const DRAIN_PROTECTION_CONTRACT_BOX_CONTRACT_ID: &[u8] = b"box_cid";
pub const DRAIN_PROTECTION_CONTRACT_MULTISIG_CONTRACT_ID: &[u8] = b"multisig_cid";

/// ZK proof namespaces
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_EXIT_NS_V1: &str = "ExitProof";

// V2 circuit namespaces (HAZOP RC3: domain separation)
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_EXECUTE_NS_V2: &str = "ExecuteV2";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_EXIT_NS_V2: &str = "ExitProofV2";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_INITIALIZE_NS_V2: &str = "InitializeV2";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_LOCK_NS_V2: &str = "LockV2";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_PROPOSE_NS_V2: &str = "ProposeV2";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_TRANSFER_NS_V2: &str = "TransferV2";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_UNLOCK_NS_V2: &str = "UnlockV2";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_UPDATE_CONFIG_NS_V2: &str = "UpdateConfigV2";
pub const DRAIN_PROTECTION_CONTRACT_ZKAS_VOTE_NS_V2: &str = "VoteV2";

// ============================================================================
// Deterministic ZK flag
// ============================================================================

/// Thread-safe flag for deterministic ZK proof generation.
/// Set by tests before endpoint exercise to eliminate OsRng from collateral/debt
/// blinds, note encryption, and proof generation, so a chain-replay determinism
/// check (PI-7) produces identical bytes on both chains.
/// Must be set BEFORE any ZK proof is created.
use std::sync::atomic::{AtomicBool, Ordering};
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

/// Enable deterministic ZK proof generation for testing.
/// Replaces OsRng with StdRng::seed_from_u64(0).
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

/// Returns true if deterministic ZK mode is enabled.
pub fn deterministic_zk_enabled() -> bool {
    DETERMINISTIC_ZK.load(Ordering::SeqCst)
}