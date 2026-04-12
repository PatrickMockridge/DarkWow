/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! NativeToken - Consensus-First Native Token Contract
//!
//! Design Philosophy: CONSENSUS FIRST, FEES SECOND, PRIVACY THIRD
//!
//! This contract serves as the native token for DarkFi with the following priorities:
//! 1. **Consensus Reward** - Block rewards for PoW mining must be reliable
//! 2. **Network Fees** - Transaction fee payment must be deterministic
//! 3. **Privacy Layer** - Privacy on top, never compromising consensus
//!
//! ## Token Model (Following money_v2 pattern)
//!
//! ```text
//! Coin = pallas::Base  # Hash of coin attributes (pub_x, pub_y, value, token_id, spend_hook, user_data, blind)
//! ```
//!
//! Note: Uses AeadEncryptedNote for recipient-only decryption
//! Nullifier: nullifier = poseidon_hash(spending_key, rho)
//!
//! ## Contract Functions
//!
//! | Function | Opcode | Purpose | Priority |
//! |----------|--------|---------|----------|
//! | FeeV1 | 0x00 | Pay network fees | CONSENSUS |
//! | GenesisMintV1 | 0x01 | Create initial supply | CONSENSUS |
//! | PoWRewardV1 | 0x02 | Distribute block rewards | CONSENSUS |
//! | TransferV1 | 0x03 | Private transfers | PRIVACY |
//! | SpendV1 | 0x04 | Spend with change | PRIVACY |
//! | MeltV1 | 0x05 | Destroy coins | PRIVACY |

use darkfi_sdk::{error::ContractError, pasta::pallas};

/// Functions available in the contract (matches money_v2 function codes)
#[repr(u8)]
#[derive(Debug)]
pub enum NativeTokenFunction {
    FeeV1 = 0x00,
    GenesisMintV1 = 0x01,
    PoWRewardV1 = 0x02,
    TransferV1 = 0x03,
    SpendV1 = 0x04,
    MeltV1 = 0x05,
}

impl TryFrom<u8> for NativeTokenFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::FeeV1),
            0x01 => Ok(Self::GenesisMintV1),
            0x02 => Ok(Self::PoWRewardV1),
            0x03 => Ok(Self::TransferV1),
            0x04 => Ok(Self::SpendV1),
            0x05 => Ok(Self::MeltV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Internal contract errors
pub mod error;

/// Call parameters definitions
pub mod model;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for proof generation
pub mod client;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Stores coin data indexed by coin_id
pub const NATIVE_TOKEN_CONTRACT_COINS_TREE: &str = "coins";
/// Stores nullifiers to prevent double-spending
pub const NATIVE_TOKEN_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Stores Merkle tree of all coins
pub const NATIVE_TOKEN_CONTRACT_MERKLE_TREE: &str = "merkle";
/// Stores contract info
pub const NATIVE_TOKEN_CONTRACT_INFO_TREE: &str = "info";

/// Stores coin roots for historical verification
pub const NATIVE_TOKEN_CONTRACT_COIN_ROOTS_TREE: &str = "coin_roots";
/// Stores nullifier roots for historical verification
pub const NATIVE_TOKEN_CONTRACT_NULLIFIER_ROOTS_TREE: &str = "nullifier_roots";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const NATIVE_TOKEN_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Genesis coin root (initial Merkle root)
pub const NATIVE_TOKEN_CONTRACT_GENESIS_ROOT: &[u8] = b"genesis_root";
/// Total supply tracking key
pub const NATIVE_TOKEN_CONTRACT_TOTAL_SUPPLY: &[u8] = b"total_supply";
/// Latest coin Merkle root
pub const NATIVE_TOKEN_CONTRACT_LATEST_COIN_ROOT: &[u8] = b"last_coin_root";
/// Latest nullifier root
pub const NATIVE_TOKEN_CONTRACT_LATEST_NULLIFIER_ROOT: &[u8] = b"last_nullifier_root";
/// Coin Merkle tree data key
pub const NATIVE_TOKEN_CONTRACT_COIN_MERKLE_TREE: &[u8] = b"coin_merkle_tree";

// ============================================================================
// EMPTY TREE ROOTS
// ============================================================================

/// Precalculated root hash for a tree containing only a single Fp::ZERO coin.
/// Used to save gas.
pub const EMPTY_COINS_TREE_ROOT: [u8; 32] = [
    0xb8, 0xc1, 0x07, 0x5a, 0x80, 0xa8, 0x09, 0x65, 0xc2, 0x39, 0x8f, 0x71, 0x1f, 0xe7, 0x3e, 0x05,
    0xb4, 0xed, 0xae, 0xde, 0xf1, 0x62, 0xf2, 0x61, 0xd4, 0xee, 0xd7, 0xcd, 0x72, 0x74, 0x8d, 0x17,
];

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas mint circuit namespace (for genesis and PoW rewards)
pub const NATIVE_TOKEN_CONTRACT_ZKAS_MINT_NS_V1: &str = "Mint_V1";
/// zkas burn circuit namespace (for spending)
pub const NATIVE_TOKEN_CONTRACT_ZKAS_BURN_NS_V1: &str = "Burn_V1";
/// zkas fee circuit namespace (for network fees)
pub const NATIVE_TOKEN_CONTRACT_ZKAS_FEE_NS_V1: &str = "Fee_V1";

// ============================================================================
// CONSTANTS
// ============================================================================

/// DARK token ID (native token = 0)
pub const DARK_TOKEN_ID: pallas::Base = pallas::Base::zero();

/// Maximum coins per transaction
pub const NATIVE_TOKEN_MAX_COINS_PER_TX: usize = 16;
/// Maximum value per coin (to prevent overflow)
pub const NATIVE_TOKEN_MAX_COIN_VALUE: u64 = 1_000_000_000_000;
/// Minimum coin value
pub const NATIVE_TOKEN_MIN_COIN_VALUE: u64 = 1;