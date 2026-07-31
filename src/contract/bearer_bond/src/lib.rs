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

//! Bearer Bond — Fixed-Interest Staking Contract
//!
//! A stake coin is a tradeable capital position. The holder provides capital
//! to the issuer and earns a fixed interest rate determined at series creation.
//! Interest is computed deterministically from on-chain state — no issuer
//! profit reporting is needed, preserving privacy for both parties.
//!
//! Maturity is ZK-committed in the coin commitment, making it cryptographically
//! bound to the bond token. Coverage checks are available to bond holders,
//! and if coverage falls below 100% the bond terms are voided.
//!
//! ## Lifecycle
//!
//! | Function | Opcode | Who | Description |
//! |----------|--------|-----|-------------|
//! | IssueStakeV1 | `0x00` | Issuer | Create staking pool, set terms, receive capital, mint stake coins |
//! | TransferStakeV1 | `0x01` | Holder | Transfer stake position to new holder |
//! | RequestInterestV1 | `0x02` | Holder | Request interest payment (prove ownership, provide payment key) |
//! | EmergencyUnstakeV1 | `0x03` | Holder | Exit before maturity when coverage falls below minimum |
//! | UnstakeV1 | `0x04` | Holder | Burn stake coin, receive principal + unclaimed interest at maturity |
//! | BurnStakeV1 | `0x05` | Issuer | Retire staking pool |
//! | ProveCoverageV1 | `0x06` | Issuer/Holder | Submit ZK proof of solvency |
//! | VerifyCoverageV1 | `0x07` | Holder | Read latest coverage report for a series |
//! | PayInterestV1 | `0x08` | Issuer | Pay a pending interest claim with fresh payment coin |
//!
//! ## Interest Formula
//!
//! ```text
//! interest = principal × interest_rate_bps × blocks_elapsed / (10000 × BLOCKS_PER_YEAR)
//! ```

pub use dwow_sdk::error::ContractError;

/// Functions available in the Bearer Bond contract
#[repr(u8)]
#[derive(Debug)]
pub enum BearerBondFunction {
    /// Create a new staking pool
    IssueStakeV1 = 0x00,
    /// Transfer stake position to a new holder
    TransferStakeV1 = 0x01,
    /// Holder requests interest payment (prove ownership, provide payment key)
    RequestInterestV1 = 0x02,
    /// Exit before maturity when coverage falls below minimum
    EmergencyUnstakeV1 = 0x03,
    /// Withdraw principal at maturity
    UnstakeV1 = 0x04,
    /// Retire staking pool (issuer only)
    BurnStakeV1 = 0x05,
    /// Submit ZK proof of solvency (issuer or holder)
    ProveCoverageV1 = 0x06,
    /// Read latest coverage report for a series (read-only query)
    VerifyCoverageV1 = 0x07,
    /// Issuer pays a pending interest claim with fresh payment coin
    PayInterestV1 = 0x08,
}

impl TryFrom<u8> for BearerBondFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::IssueStakeV1),
            0x01 => Ok(Self::TransferStakeV1),
            0x02 => Ok(Self::RequestInterestV1),
            0x03 => Ok(Self::EmergencyUnstakeV1),
            0x04 => Ok(Self::UnstakeV1),
            0x05 => Ok(Self::BurnStakeV1),
            0x06 => Ok(Self::ProveCoverageV1),
            0x07 => Ok(Self::VerifyCoverageV1),
            0x08 => Ok(Self::PayInterestV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// BearerBond-specific errors
pub mod error;

/// Data model types (BondCoin, params, updates)
pub mod model;

/// Capability descriptor for o-cap position resolution
pub mod capability;

/// Cross-contract validation helpers
pub mod validation;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for proof generation
pub mod client;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Stores stake coin data indexed by token_commit
pub const BEARER_BOND_CONTRACT_COINS_TREE: &str = "coins";
/// Stores nullifiers to prevent double-spending
pub const BEARER_BOND_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Stores Merkle tree of all coins
pub const BEARER_BOND_CONTRACT_COIN_MERKLE_TREE: &str = "coin_merkle";
/// Stores contract info
pub const BEARER_BOND_CONTRACT_INFO_TREE: &str = "info";
/// Stores coin roots for historical verification
pub const BEARER_BOND_CONTRACT_COIN_ROOTS_TREE: &str = "coin_roots";
/// Stores nullifier roots for historical verification
pub const BEARER_BOND_CONTRACT_NULLIFIER_ROOTS_TREE: &str = "nullifier_roots";
/// Stores staking pool metadata and profit declarations
pub const BEARER_BOND_CONTRACT_BONDS_INFO_TREE: &str = "bonds_info";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const BEARER_BOND_CONTRACT_DB_VERSION: &[u8] = b"db_version";

// ============================================================================
// EMPTY TREE ROOTS
// ============================================================================

/// Precalculated root hash for an empty coin tree (Poseidon SMT over pallas::Base).
pub const BEARER_BOND_EMPTY_COINS_ROOT: [u8; 32] = [
    0xb8, 0xc1, 0x07, 0x5a, 0x80, 0xa8, 0x09, 0x65, 0xc2, 0x39, 0x8f, 0x71, 0x1f, 0xe7, 0x3e, 0x05,
    0xb4, 0xed, 0xae, 0xde, 0xf1, 0x62, 0xf2, 0x61, 0xd4, 0xee, 0xd7, 0xcd, 0x72, 0x74, 0x8d, 0x17,
];

/// Precalculated root hash for an empty nullifier tree.
pub const BEARER_BOND_EMPTY_NULLIFIER_ROOT: [u8; 32] = [0u8; 32];

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum coins per transaction
pub const BEARER_BOND_MAX_COINS_PER_TX: usize = 16;
/// Maximum principal value
pub const BEARER_BOND_MAX_PRINCIPAL: u64 = 1_000_000_000_000;
/// Minimum principal value
pub const BEARER_BOND_MIN_PRINCIPAL: u64 = 1;

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas burn circuit namespace (nullifier-based spend proof)
pub const BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V1: &str = "Burn_V1";
/// zkas blind output circuit namespace (private output coin formation)
pub const BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V1: &str = "BlindOutput_V1";
/// zkas redeem circuit namespace (zero-value receipt coin formation)
pub const BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V1: &str = "Redeem_V1";
/// zkas prove_coverage circuit namespace (coverage ratio proof)
pub const BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V1: &str = "ProveCoverage_V1";
// V2 domain-separated namespace constants
pub const BEARER_BOND_CONTRACT_ZKAS_BURN_NS_V2: &str = "Burn_V2";
pub const BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_NS_V2: &str = "BlindOutput_V2";
pub const BEARER_BOND_CONTRACT_ZKAS_REDEEM_NS_V2: &str = "Redeem_V2";
pub const BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_NS_V2: &str = "ProveCoverage_V2";

// ============================================================================
// ZK CIRCUIT BINARIES (for client-side proof generation)
// ============================================================================

// ZK circuit binaries moved to client/zkbins.rs behind #[cfg(feature = "client")].
#[cfg(feature = "client")]
pub use crate::client::zkbins::{
    BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_V1_BIN,
    BEARER_BOND_CONTRACT_ZKAS_BURN_V1_BIN,
    BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_V1_BIN,
    BEARER_BOND_CONTRACT_ZKAS_REDEEM_V1_BIN,
};
