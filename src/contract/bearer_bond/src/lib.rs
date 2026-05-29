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

//! Bearer Bond — Profit-Share Staking Contract
//!
//! A stake coin is a tradeable capital position. The holder provides capital
//! to the issuer, the issuer does work, and profits are shared pro-rata.
//! If there are no profits, there are no payouts — risk is shared between
//! capital provider and entrepreneur.
//!
//! Unclaimed profit distributions travel with the stake coin. On transfer,
//! the new coin preserves `last_claim_block`, so the new holder inherits
//! the right to claim all unpaid profits.
//!
//! ## Lifecycle
//!
//! | Function | Opcode | Who | Description |
//! |----------|--------|-----|-------------|
//! | IssueStakeV1 | `0x00` | Issuer | Create staking pool, set terms, receive capital, mint stake coins |
//! | TransferStakeV1 | `0x01` | Holder | Transfer stake position to new holder |
//! | DeclareProfitsV1 | `0x02` | Issuer | Declare profit distribution for a series |
//! | ClaimProfitsV1 | `0x03` | Holder | Claim pro-rata share of declared profits |
//! | UnstakeV1 | `0x04` | Holder | Burn stake coin, receive principal + unclaimed profits |
//! | BurnStakeV1 | `0x05` | Issuer | Retire staking pool |
//!
//! ## Profit Share Formula
//!
//! ```text
//! share = staked_principal × declared_profit / total_staked_in_series
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
    /// Declare a profit distribution for a series
    DeclareProfitsV1 = 0x02,
    /// Claim pro-rata share of declared profits
    ClaimProfitsV1 = 0x03,
    /// Withdraw principal at maturity
    UnstakeV1 = 0x04,
    /// Retire staking pool (issuer only)
    BurnStakeV1 = 0x05,
    /// Prove reserves cover outstanding stake (governance)
    ProveCoverageV1 = 0x06,
}

impl TryFrom<u8> for BearerBondFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::IssueStakeV1),
            0x01 => Ok(Self::TransferStakeV1),
            0x02 => Ok(Self::DeclareProfitsV1),
            0x03 => Ok(Self::ClaimProfitsV1),
            0x04 => Ok(Self::UnstakeV1),
            0x05 => Ok(Self::BurnStakeV1),
            0x06 => Ok(Self::ProveCoverageV1),
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

// ============================================================================
// ZK CIRCUIT BINARIES (for client-side proof generation)
// ============================================================================

/// Burn_V1 zkas circuit binary
pub const BEARER_BOND_CONTRACT_ZKAS_BURN_V1_BIN: &[u8] =
    include_bytes!("../proof/burn_v1.zk.bin");
/// BlindOutput_V1 zkas circuit binary
pub const BEARER_BOND_CONTRACT_ZKAS_BLIND_OUTPUT_V1_BIN: &[u8] =
    include_bytes!("../proof/blind_output_v1.zk.bin");
/// Redeem_V1 zkas circuit binary
pub const BEARER_BOND_CONTRACT_ZKAS_REDEEM_V1_BIN: &[u8] =
    include_bytes!("../proof/redeem_v1.zk.bin");
/// ProveCoverage_V1 zkas circuit binary
pub const BEARER_BOND_CONTRACT_ZKAS_PROVE_COVERAGE_V1_BIN: &[u8] =
    include_bytes!("../proof/prove_coverage_v1.zk.bin");
