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

//! Simple UTXO Token Contract
//!
//! A minimal Bitcoin-like UTXO token designed as a baseline.
//! No Pedersen commitments, no encrypted notes, no EC multiplication in circuits.
//! Privacy can be layered on top later.
//!
//! ## Design Principles
//!
//! 1. **Simple**: No complex ZK circuits required for basic transfer
//! 2. **Auditable**: Easy to understand the token logic
//! 3. **Baseline**: Works as plain public token, privacy optional
//! 4. **Compatible**: Uses existing DarkFi primitives (Merkle proofs, signatures)
//!
//! ## Token Model
//!
//! ```text
//! Coin = {
//!     owner: PublicKey,     // Who owns
//!     value: u64,           // How much
//!     token_id: TokenId,    // Which token
//!     nonce: u64,           // Uniqueness
//! }
//!
//! coin_id = poseidon_hash(owner, value, token_id, nonce)
//! nullifier = poseidon_hash(coin_id)
//! ```
//!
//! ## Contract Functions
//!
//! - `GenesisV1` - Create initial coin supply
//! - `TransferV1` - Send coins to another party (signature-based, no ZK)
//! - `SpendV1` - Consume coins, create change outputs
//! - `MeltV1` - Destroy coins (for fees)
//!
//! ## Differences from MoneyV2
//!
//! | Feature | MoneyV2 | SimpleCoin |
//! |---------|---------|------------|
//! | Value privacy | Pedersen commitments | Public values |
//! | Note encryption | AEAD encrypted | Plaintext |
//! | Token creation | AuthTokenMint + TokenMint | Direct mint |
//! | EC operations | ec_mul_base in circuits | None in baseline |
//! | ZK proofs | Required for all ops | Optional, signature-based |

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum SimplecoinFunction {
    GenesisV1 = 0x00,
    TransferV1 = 0x01,
    SpendV1 = 0x02,
    MeltV1 = 0x03,
}

impl TryFrom<u8> for SimplecoinFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::GenesisV1),
            0x01 => Ok(Self::TransferV1),
            0x02 => Ok(Self::SpendV1),
            0x03 => Ok(Self::MeltV1),
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
/// Client API
pub mod client;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Stores coin data indexed by coin_id
pub const SIMPLECOIN_CONTRACT_COINS_TREE: &str = "coins";
/// Stores nullifiers to prevent double-spending
pub const SIMPLECOIN_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Stores Merkle tree of all coins
pub const SIMPLECOIN_CONTRACT_MERKLE_TREE: &str = "merkle";
/// Stores contract info
pub const SIMPLECOIN_CONTRACT_INFO_TREE: &str = "info";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const SIMPLECOIN_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Genesis coin root (initial Merkle root)
pub const SIMPLECOIN_CONTRACT_GENESIS_ROOT: &[u8] = b"genesis_root";
/// Total supply tracking key
pub const SIMPLECOIN_CONTRACT_TOTAL_SUPPLY: &[u8] = b"total_supply";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas mint circuit namespace (for genesis and future minting)
pub const SIMPLECOIN_CONTRACT_ZKAS_MINT_NS_V1: &str = "Mint_V1";
/// zkas spend circuit namespace (for spending with ZK proofs)
pub const SIMPLECOIN_CONTRACT_ZKAS_SPEND_NS_V1: &str = "Spend_V1";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum coins per transaction
pub const SIMPLECOIN_MAX_COINS_PER_TX: usize = 16;
/// Maximum value per coin (to prevent overflow)
pub const SIMPLECOIN_MAX_COIN_VALUE: u64 = 1_000_000_000_000;
/// Minimum coin value
pub const SIMPLECOIN_MIN_COIN_VALUE: u64 = 1;