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

//! DarkToshi Dice Contract
//!
//! A privacy-preserving Satoshi Dice clone where players bet on random rolls.
//!
//! Mechanics:
//! 1. Player commits to a bet (value + target + secret nonce) via Dice::CommitBetV1
//! 2. Roll is derived from block hash + bet commitment via Dice::RevealRollV1
//! 3. If roll < target, player wins (payout = bet_value * (10000 - house_edge_bp) / (target * 100))
//! 4. House edge is built in (default 2% = 200 basis points)
//!
//! Money Contract Integration:
//! - CommitBet should be called as child of Money::Burn to lock player's bet value
//! - SettleBet updates state; player-winning bets require separate Money::TokenMint call
//! - HouseClose collects house's share when bets timeout or are cancelled

use dwow_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum DiceFunction {
    InitializeV1 = 0x00,
    CommitBetV1 = 0x01,
    RevealRollV1 = 0x02,
    SettleBetV1 = 0x03,
    HouseCloseV1 = 0x04,
}

impl TryFrom<u8> for DiceFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::CommitBetV1),
            0x02 => Ok(Self::RevealRollV1),
            0x03 => Ok(Self::SettleBetV1),
            0x04 => Ok(Self::HouseCloseV1),
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
/// Client API for interaction with this smart contract
pub mod client;

// ============================================================================
// DATABASE TREES
// ============================================================================

/// Stores bet details indexed by bet_id
pub const DICE_CONTRACT_BETS_TREE: &str = "bets";
/// Stores nullifiers to prevent double-spending
pub const DICE_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Stores contract info (house pubkey, house edge, etc.)
pub const DICE_CONTRACT_INFO_TREE: &str = "info";
/// Stores accumulated house funds
pub const DICE_CONTRACT_HOUSE_TREE: &str = "house";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const DICE_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// House public key for receiving lost bets
pub const DICE_CONTRACT_HOUSE_PUBKEY: &[u8] = b"house_pubkey";
/// House edge in basis points (e.g., 200 = 2.00%)
pub const DICE_CONTRACT_HOUSE_EDGE: &[u8] = b"house_edge";
/// Roll timeout in blocks (after which house can close)
pub const DICE_CONTRACT_ROLL_TIMEOUT: &[u8] = b"roll_timeout";
/// House balance key in house tree
pub const DICE_CONTRACT_HOUSE_BALANCE: &[u8] = b"balance";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas commit bet circuit namespace
pub const DICE_CONTRACT_ZKAS_COMMIT_NS: &str = "CommitBet_V1";
/// zkas settle bet circuit namespace
pub const DICE_CONTRACT_ZKAS_SETTLE_NS: &str = "SettleBet_V1";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default house edge in basis points (2.00%)
pub const DEFAULT_HOUSE_EDGE: u32 = 200;
/// Minimum allowed house edge (1.00%)
pub const MIN_HOUSE_EDGE: u32 = 100;
/// Maximum allowed house edge (5.00%)
pub const MAX_HOUSE_EDGE: u32 = 500;
/// Default roll timeout in blocks
pub const DEFAULT_ROLL_TIMEOUT: u32 = 10;
/// Maximum target number (1-99 valid)
pub const MAX_TARGET: u8 = 99;
/// Number of possible outcomes (0-99)
pub const ROLL_RANGE: u8 = 100;
