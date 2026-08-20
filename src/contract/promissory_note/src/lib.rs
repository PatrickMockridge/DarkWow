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

//! Promissory Note - DeFi Token Contract (ERC-20 style)
//!
//! Design Philosophy: PRIVACY FIRST, COMPOSABILITY SECOND, SIMPLICITY THIRD
//!
//! PromissoryNote is the privacy-focused token contract for DeFi use cases:
//! - Wrapped tokens (wBTC, wETH, etc.)
//! - Stablecoins (USD, EUR, etc.)
//! - ERC-20 style tokens
//!
//! NativeToken handles consensus (PoW rewards, fees).
//! PromissoryNote handles DeFi tokens (no consensus responsibility).
//!
//! ## Token Model
//!
//! Unlike NativeToken which has a single native token (DRKW),
//! PromissoryNote supports MULTIPLE tokens via token registration.
//!
//! ## Key Differences from NativeToken
//!
//! | Aspect | NativeToken | PromissoryNote |
//! |--------|-------------|---------|
//! | Purpose | Consensus (PoW rewards, fees) | DeFi tokens |
//! | Tokens | Single (DRKW) | Multiple (via TokenMint) |
//! | Authorization | None | Backing capability proof |
//! | Privacy | Full privacy | Full privacy |
//!
//! ## Value Commitments
//!
//! Promissory Note uses Pedersen commitments for value (additively homomorphic).
//! This enables cross-proof value conservation: the entrypoint sums input
//! value_commits and output value_commits per token_commit group, and
//! verifies they are equal — preventing value inflation without revealing
//! plaintext values.
//!
//! ## Privacy Architecture
//!
//! ```text
//! TokenMint: asset_id = poseidon_hash(auth_parent, user_data, blind)
//! Mint:      C = poseidon_hash(pub, value, asset_id, spend_hook, user_data, blind)
//! Burn:      N = poseidon_hash(secret, C)  // Nullifier breaks mint<->burn link
//! Value:     V = pedersen_commit(value, blind) // Homomorphic for conservation
//! ```
//!
//! ## Contract Functions
//!
//! | Function | Opcode | Purpose |
//! |----------|--------|---------|
//! | RegisterTypeV1 | 0x00 | Create new token type (capability type registration) |
//! | RedeemV1 | 0x01 | Redeem a coin, destroying monetary value, creating a receipt |
//! | IssueV1 | 0x02 | Issue tokens of existing token type (capability issuance) |
//! | RevokeV1 | 0x03 | Revoke/destroy tokens (capability revocation) |
//! | TransferV1 | 0x04 | Private token transfer (capability transfer) |
//! | OtcSwapV1 | 0x05 | Atomic OTC token swap |

pub use dwow_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum PromissoryNoteFunction {
    /// Create a new token type — capability type registration
    RegisterTypeV1 = 0x00,
    /// Redeem a coin, destroying its monetary value and creating a receipt
    RedeemV1 = 0x01,
    /// Issue tokens of an existing token type — capability issuance
    IssueV1 = 0x02,
    /// Revoke/destroy tokens — capability revocation
    RevokeV1 = 0x03,
    /// Private token transfer — capability transfer
    TransferV1 = 0x04,
    /// Atomic OTC swap (swap tokens between two parties)
    OtcSwapV1 = 0x05,
}

impl TryFrom<u8> for PromissoryNoteFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::RegisterTypeV1),
            0x01 => Ok(Self::RedeemV1),
            0x02 => Ok(Self::IssueV1),
            0x03 => Ok(Self::RevokeV1),
            0x04 => Ok(Self::TransferV1),
            0x05 => Ok(Self::OtcSwapV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Internal contract errors
pub mod error;

/// Call parameters definitions
pub mod model;

/// Capability descriptor for o-cap position resolution
pub mod capability;

/// Cross-contract validation helpers (always compiled)
pub mod validation;

#[cfg(not(feature = "no-entrypoint"))]
/// WASM entrypoint functions
pub mod entrypoint;

#[cfg(feature = "client")]
/// Client API for proof generation
pub mod client;

// ── Circuit Self-Registration (REMOVED, D2) ──────────────────────────
// The circuit_registry crate is deleted. Proofs are built by the generic
// prover (wallet.md §6.4.1, Phase 6) from the zkas binary + manifest
// witness_map — no compiled-in per-contract builder is required.

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Stores coin data indexed by coin_id
pub const PROMISSORY_NOTE_CONTRACT_COINS_TREE: &str = "coins";
/// Stores nullifiers to prevent double-spending
pub const PROMISSORY_NOTE_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Stores Merkle tree of all coins
pub const PROMISSORY_NOTE_CONTRACT_MERKLE_TREE: &str = "merkle";
/// Stores contract info
pub const PROMISSORY_NOTE_CONTRACT_INFO_TREE: &str = "info";

/// Stores coin roots for historical verification
pub const PROMISSORY_NOTE_CONTRACT_COIN_ROOTS_TREE: &str = "coin_roots";
/// Stores nullifier roots for historical verification
pub const PROMISSORY_NOTE_CONTRACT_NULLIFIER_ROOTS_TREE: &str = "nullifier_roots";
/// Stores registered token IDs (prevents unauthorized minting)
pub const PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_TREE: &str = "token_registry";
/// Stores token registry roots for historical verification
pub const PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_ROOTS_TREE: &str = "token_registry_roots";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const PROMISSORY_NOTE_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Genesis coin root (initial Merkle root)
pub const PROMISSORY_NOTE_CONTRACT_GENESIS_ROOT: &[u8] = b"genesis_root";
/// Total supply tracking key
pub const PROMISSORY_NOTE_CONTRACT_TOTAL_SUPPLY: &[u8] = b"total_supply";
/// Latest coin Merkle root
pub const PROMISSORY_NOTE_CONTRACT_LATEST_COIN_ROOT: &[u8] = b"last_coin_root";
/// Latest nullifier root
pub const PROMISSORY_NOTE_CONTRACT_LATEST_NULLIFIER_ROOT: &[u8] = b"last_nullifier_root";
/// Coin Merkle tree data key
pub const PROMISSORY_NOTE_CONTRACT_COIN_MERKLE_TREE: &[u8] = b"coin_merkle_tree";
/// Latest token registry root
pub const PROMISSORY_NOTE_CONTRACT_LATEST_TOKEN_REGISTRY_ROOT: &[u8] = b"last_token_registry_root";
/// Token registry Merkle tree data key
pub const PROMISSORY_NOTE_CONTRACT_TOKEN_REGISTRY_MERKLE_TREE: &[u8] = b"token_registry_merkle_tree";

// ============================================================================
// EMPTY TREE ROOTS
// ============================================================================

/// Precalculated root hash for a tree containing only a single Fp::ZERO coin.
/// Used to save gas.
pub const EMPTY_COINS_TREE_ROOT: [u8; 32] = [
    0xb8, 0xc1, 0x07, 0x5a, 0x80, 0xa8, 0x09, 0x65, 0xc2, 0x39, 0x8f, 0x71, 0x1f, 0xe7, 0x3e, 0x05,
    0xb4, 0xed, 0xae, 0xde, 0xf1, 0x62, 0xf2, 0x61, 0xd4, 0xee, 0xd7, 0xcd, 0x72, 0x74, 0x8d, 0x17,
];

/// Precalculated root hash for an empty token registry tree.
/// Same as EMPTY_COINS_TREE_ROOT since both are empty Poseidon SMTs over pallas::Base.
pub const EMPTY_TOKEN_REGISTRY_TREE_ROOT: [u8; 32] = EMPTY_COINS_TREE_ROOT;

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

pub const PROMISSORY_NOTE_CONTRACT_ZKAS_REGISTER_TYPE_NS_V1: &str = "RegisterType_V1";
/// zkas issue circuit namespace (issue new capability coins)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V1: &str = "Issue_V1";
/// zkas revoke circuit namespace (for exercising capabilities)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_REVOKE_NS_V1: &str = "Revoke_V1";
/// zkas transfer circuit namespace (capability transfer between holders)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_TRANSFER_NS_V1: &str = "Transfer_V1";
/// zkas redeem circuit namespace (receipt coin formation, value=0 via is_notequal)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_REDEEM_NS_V1: &str = "Redeem_V1";

// V2 circuit namespaces (HAZOP RC3: domain separation)
/// zkas register type circuit namespace V2 (domain-separated)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_REGISTER_TYPE_NS_V2: &str = "RegisterType_V2";
/// zkas issue circuit namespace V2 (domain-separated)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_ISSUE_NS_V2: &str = "Issue_V2";
/// zkas revoke circuit namespace V2 (domain-separated)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_REVOKE_NS_V2: &str = "Revoke_V2";
/// zkas transfer circuit namespace V2 (domain-separated)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_TRANSFER_NS_V2: &str = "Transfer_V2";
/// zkas redeem circuit namespace V2 (domain-separated)
pub const PROMISSORY_NOTE_CONTRACT_ZKAS_REDEEM_NS_V2: &str = "Redeem_V2";

// ============================================================================
// ZK CIRCUIT BINARIES (for client-side proof generation)
// ============================================================================

// V1 ZK circuit binaries removed (rc3 Batch 4) — V1 .zk source and .zk.bin files deleted.

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum coins per transaction
pub const PROMISSORY_NOTE_MAX_COINS_PER_TX: usize = 16;
/// Maximum value per coin (to prevent overflow)
pub const PROMISSORY_NOTE_MAX_COIN_VALUE: u64 = 1_000_000_000_000;
/// Minimum coin value
pub const PROMISSORY_NOTE_MIN_COIN_VALUE: u64 = 1;

/// Thread-safe flag for deterministic ZK proof generation.
/// Set by tests before endpoint exercise to eliminate OsRng from value/user-data
/// blinds, AEAD note encryption, and proof generation, so a chain-replay
/// determinism check (PI-7) produces identical bytes on both chains.
/// Must be set BEFORE any ZK proof is created.
#[cfg(feature = "deterministic-zk")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "deterministic-zk")]
static DETERMINISTIC_ZK: AtomicBool = AtomicBool::new(false);

/// Enable deterministic ZK proof generation for testing.
/// Replaces OsRng with StdRng::seed_from_u64(0).
#[cfg(feature = "deterministic-zk")]
pub fn enable_deterministic_zk() {
    DETERMINISTIC_ZK.store(true, Ordering::SeqCst);
}

/// Returns true if deterministic ZK mode is enabled. Always `false` unless the
/// `deterministic-zk` feature is enabled (test builds only — heavyweight-spec.md §7.4 DZ-4).
pub fn deterministic_zk_enabled() -> bool {
    #[cfg(feature = "deterministic-zk")]
    {
        DETERMINISTIC_ZK.load(Ordering::SeqCst)
    }
    #[cfg(not(feature = "deterministic-zk"))]
    {
        false
    }
}