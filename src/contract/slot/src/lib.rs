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

//! Composable Slot Machine Contract
//!
//! A privacy-preserving slot machine where players bet on spinning reels.
//! Designed to be COMPOSABLE like Baccarat - the core contract handles
//! commitment and settlement, but the game logic (paytables, reel configurations)
//! is modular and swappable.
//!
//! Mechanics:
//! 1. Player commits to a spin via Slot::CommitSpinV1 (hides bet in ZK)
//! 2. Block entropy reveals random positions via Slot::RevealSpinV1
//! 3. Winning combinations calculated via Slot::SettleSpinV1 (ZK constrained)
//! 4. House can close abandoned spins via Slot::CancelSpinV1
//!
//! Composability Design:
//! The slot contract is structured like Baccarat:
//! - Commit phase hides bet parameters (same pattern as Baccarat::CommitBet)
//! - Reveal phase uses block entropy (same pattern as Baccarat::DrawCards)
//! - Settle phase calculates payout constrained by ZK proof
//!
//! Different slot VARIANTS can be created by:
//! - Swapping reel strip configurations (different symbol sets, lengths)
//! - Swapping paytables (different winning combinations, multipliers)
//! - Adding extension circuits for bonus rounds, progressives, etc.
//!
//! This is the COMPOSABILITY PATTERN: core contract is fixed, game logic is modular.
//!
//! Money Contract Integration:
//! - CommitSpin should be called as child of Money::Burn to lock player's bet value
//! - SettleSpin updates state; player-winning spins require separate Money::TokenMint call
//! - CancelSpin collects house's share when spins timeout or are cancelled

use dwow_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum SlotFunction {
    InitializeV1 = 0x00,
    CommitSpinV1 = 0x01,
    RevealSpinV1 = 0x02,
    SettleSpinV1 = 0x03,
    CancelSpinV1 = 0x04,
}

impl TryFrom<u8> for SlotFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::CommitSpinV1),
            0x02 => Ok(Self::RevealSpinV1),
            0x03 => Ok(Self::SettleSpinV1),
            0x04 => Ok(Self::CancelSpinV1),
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
pub mod client;

/// Capability descriptor for wallet resolver
pub mod capability;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Stores spin details indexed by spin_id
pub const SLOT_CONTRACT_SPINS_TREE: &str = "spins";
/// Stores nullifiers to prevent double-spending
pub const SLOT_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Stores contract info (game config, house pubkey, etc.)
pub const SLOT_CONTRACT_CONFIG_TREE: &str = "config";
/// Stores contract metadata (version, promissory_note CID, etc.)
pub const SLOT_CONTRACT_INFO_TREE: &str = "info";
/// Stores accumulated house funds
pub const SLOT_CONTRACT_HOUSE_TREE: &str = "house";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const SLOT_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// House public key for receiving lost bets
pub const SLOT_CONTRACT_HOUSE_PUBKEY: &[u8] = b"house_pubkey";
/// House edge in basis points (e.g., 500 = 5%)
pub const SLOT_CONTRACT_HOUSE_EDGE: &[u8] = b"house_edge";
/// Game type (0=classic, 1=video, etc.)
pub const SLOT_CONTRACT_GAME_TYPE: &[u8] = b"game_type";
/// Bet timeout in blocks (after which house can close)
pub const SLOT_CONTRACT_SPIN_TIMEOUT: &[u8] = b"spin_timeout";
/// Money v3 contract ID for cross-contract validation
pub const SLOT_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID: &[u8] = b"promissory_note_cid";
/// House balance key in house tree
pub const SLOT_CONTRACT_HOUSE_BALANCE: &[u8] = b"balance";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas commit spin circuit namespace
pub const SLOT_CONTRACT_ZKAS_COMMIT_NS: &str = "CommitBet_V1";
/// zkas reveal spin circuit namespace
pub const SLOT_CONTRACT_ZKAS_REVEAL_NS: &str = "RevealSpin_V1";
/// zkas settle spin circuit namespace
pub const SLOT_CONTRACT_ZKAS_SETTLE_NS: &str = "SettleBet_V1";
pub const SLOT_CONTRACT_ZKAS_COMMIT_NS_V2: &str = "CommitBet_V2";
pub const SLOT_CONTRACT_ZKAS_REVEAL_NS_V2: &str = "RevealSpin_V2";
pub const SLOT_CONTRACT_ZKAS_SETTLE_NS_V2: &str = "SettleBet_V2";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default house edge in basis points (~5% for video slots)
pub const DEFAULT_HOUSE_EDGE: u32 = 500;
/// Minimum allowed house edge (1.00%)
pub const MIN_HOUSE_EDGE: u32 = 100;
/// Maximum allowed house edge (10.00%)
pub const MAX_HOUSE_EDGE: u32 = 1000;
/// Default spin timeout in blocks
pub const DEFAULT_SPIN_TIMEOUT: u32 = 10;
/// Maximum confirmation depth
pub const DEFAULT_CONFIRMATION_DEPTH: u8 = 3;
/// Maximum bet value (to prevent overflow)
pub const MAX_BET_VALUE: u64 = 1_000_000_000; // 1 billion tokens
/// Minimum bet value
pub const MIN_BET_VALUE: u64 = 1;

/// Classic slot (3 reels, single line)
pub const GAME_TYPE_CLASSIC: u8 = 0;
/// Video slot (5 reels, multiple paylines)
pub const GAME_TYPE_VIDEO: u8 = 1;

// ============================================================================
// PAYOUT ODDS (as fractions - house edge already in paytable)
// ============================================================================

/// 3x BAR payout (e.g., 100:1)
pub const BAR_PAYOUT_NUM: u32 = 100;
pub const BAR_PAYOUT_DEN: u32 = 1;
/// 3x 7 payout (e.g., 50:1)
pub const SEVEN_PAYOUT_NUM: u32 = 50;
pub const SEVEN_PAYOUT_DEN: u32 = 1;
/// 3x cherry payout (e.g., 20:1)
pub const CHERRY_PAYOUT_NUM: u32 = 20;
pub const CHERRY_PAYOUT_DEN: u32 = 1;
// etc.