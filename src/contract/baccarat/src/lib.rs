/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
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

//! Baccarat Contract (Punto Banco)
//!
//! A privacy-preserving Baccarat implementation where players bet on Player/Banker/Tie outcomes.
//!
//! Mechanics:
//! 1. Player commits to a bet (bet_type + value + secret nonce) via Baccarat::CommitBetV1
//! 2. Cards are dealt using block hash entropy via Baccarat::DrawCardsV1
//! 3. Drawing rules applied to determine winner via Baccarat::SettleBetV1
//! 4. House edge built into payout ratios (default ~1.5%)
//!
//! Baccarat Rules:
//! - Hand value = sum of cards % 10 (0-9)
//! - Face cards (10, J, Q, K) = 0, Ace = 1, 2-9 = face value
//! - Player draws on 0-5, stands on 6-9
//! - Banker draws based on player's third card (complex rules)
//!
//! Money Contract Integration:
//! - CommitBet should be called as child of Money::Burn to lock player's bet value
//! - SettleBet updates state; player-winning bets require separate Money::TokenMint call
//! - HouseClose collects house's share when bets timeout or are cancelled

use darkfi_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum BaccaratFunction {
    InitializeV1 = 0x00,
    CommitBetV1 = 0x01,
    DrawCardsV1 = 0x02,
    SettleBetV1 = 0x03,
    HouseCloseV1 = 0x04,
}

impl TryFrom<u8> for BaccaratFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::CommitBetV1),
            0x02 => Ok(Self::DrawCardsV1),
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
pub const BACCARAT_CONTRACT_BETS_TREE: &str = "bets";
/// Stores nullifiers to prevent double-spending
pub const BACCARAT_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Stores contract info (house pubkey, house edge, etc.)
pub const BACCARAT_CONTRACT_INFO_TREE: &str = "info";
/// Stores accumulated house funds
pub const BACCARAT_CONTRACT_HOUSE_TREE: &str = "house";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const BACCARAT_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// House public key for receiving lost bets
pub const BACCARAT_CONTRACT_HOUSE_PUBKEY: &[u8] = b"house_pubkey";
/// House edge in basis points (e.g., 150 = 1.50%)
pub const BACCARAT_CONTRACT_HOUSE_EDGE: &[u8] = b"house_edge";
/// Bet timeout in blocks (after which house can close)
pub const BACCARAT_CONTRACT_BET_TIMEOUT: &[u8] = b"bet_timeout";
/// House balance key in house tree
pub const BACCARAT_CONTRACT_HOUSE_BALANCE: &[u8] = b"balance";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas commit bet circuit namespace
pub const BACCARAT_CONTRACT_ZKAS_COMMIT_NS: &str = "CommitBet_V1";
/// zkas settle bet circuit namespace
pub const BACCARAT_CONTRACT_ZKAS_SETTLE_NS: &str = "SettleBet_V1";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default house edge in basis points (~1.5% average)
pub const DEFAULT_HOUSE_EDGE: u32 = 150;
/// Minimum allowed house edge (1.00%)
pub const MIN_HOUSE_EDGE: u32 = 100;
/// Maximum allowed house edge (3.00%)
pub const MAX_HOUSE_EDGE: u32 = 300;
/// Default bet timeout in blocks
pub const DEFAULT_BET_TIMEOUT: u32 = 10;
/// Maximum confirmation depth
pub const MAX_CONFIRMATION_DEPTH: u8 = 10;
/// Number of card ranks (2-10, J, Q, K, A = 13)
pub const CARD_RANKS: u8 = 13;
/// Number of suits (Clubs, Diamonds, Hearts, Spades = 4)
pub const CARD_SUITS: u8 = 4;
/// Total cards in deck (52)
pub const DECK_SIZE: u8 = CARD_RANKS * CARD_SUITS;

// Payout odds (as fractions)
/// Player bet payout: 1:1 (100/100)
pub const PLAYER_PAYOUT_NUM: u32 = 100;
pub const PLAYER_PAYOUT_DEN: u32 = 100;
/// Banker bet payout: 0.95:1 (95/100), house takes 5%
pub const BANKER_PAYOUT_NUM: u32 = 95;
pub const BANKER_PAYOUT_DEN: u32 = 100;
/// Tie bet payout: 8:1 (800/100)
pub const TIE_PAYOUT_NUM: u32 = 800;
pub const TIE_PAYOUT_DEN: u32 = 100;
