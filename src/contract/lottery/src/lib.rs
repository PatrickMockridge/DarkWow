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

//! Lottery Contract
//!
//! A privacy-preserving pooled lottery where players pick numbers and win based on matches.
//!
//! Mechanics:
//! 1. House initializes a lottery with configurable parameters (num_picks, range, prize tiers)
//! 2. Players buy tickets by committing to N unique numbers (BuyTicketV1)
//! 3. House draws winning numbers using block hash entropy (DrawWinnersV1)
//! 4. Players reveal their numbers to claim prizes (RevealTicketV1, ClaimPrizeV1)
//! 5. House can expire the lottery to claim unclaimed prizes (ExpireLotteryV1)
//!
//! Money Contract Integration:
//! - BuyTicket should be called as child of promissory_note::transfer_v1 to lock ticket price
//! - ClaimPrize pays out winner's share via promissory_note::transfer_v1 (child call required)
//! - ExpireLottery sends unclaimed to house via promissory_note::transfer_v1 (child call required)

use dwow_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum LotteryFunction {
    InitializeV1 = 0x00,
    BuyTicketV1 = 0x01,
    DrawWinnersV1 = 0x02,
    RevealTicketV1 = 0x03,
    ClaimPrizeV1 = 0x04,
    ExpireLotteryV1 = 0x05,
}

impl TryFrom<u8> for LotteryFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::BuyTicketV1),
            0x02 => Ok(Self::DrawWinnersV1),
            0x03 => Ok(Self::RevealTicketV1),
            0x04 => Ok(Self::ClaimPrizeV1),
            0x05 => Ok(Self::ExpireLotteryV1),
            _ => Err(ContractError::InvalidFunction),
        }
    }
}

/// Capability descriptor for permissioned actions
pub mod capability;

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

/// Stores lottery round details indexed by lottery_id
pub const LOTTERY_CONTRACT_LOTTERIES_TREE: &str = "lotteries";
/// Stores ticket commitments indexed by ticket_id
pub const LOTTERY_CONTRACT_TICKETS_TREE: &str = "tickets";
/// Stores nullifiers to prevent double-spending
pub const LOTTERY_CONTRACT_NULLIFIERS_TREE: &str = "nullifiers";
/// Stores prize claims indexed by ticket_id
pub const LOTTERY_CONTRACT_CLAIMS_TREE: &str = "claims";
/// SMT database for ticket commitments (used for Merkle tree of tickets)
pub const LOTTERY_CONTRACT_TICKETS_SMT_TREE: &str = "tickets_smt";
/// SMT roots database for ticket commitments (historical roots)
pub const LOTTERY_CONTRACT_TICKETS_ROOTS_TREE: &str = "tickets_roots";
/// Stores contract metadata (version, promissory_note CID, etc.)
pub const LOTTERY_CONTRACT_INFO_TREE: &str = "info";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const LOTTERY_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// House public key for receiving house share and unclaimed prizes
pub const LOTTERY_CONTRACT_HOUSE_PUBKEY: &[u8] = b"house_pubkey";
/// Current lottery ID being run
pub const LOTTERY_CONTRACT_CURRENT_LOTTERY: &[u8] = b"current_lottery";
/// Key for latest ticket Merkle root in info database
pub const LOTTERY_CONTRACT_LATEST_TICKET_ROOT: &[u8] = b"latest_ticket_root";
/// Money v3 contract ID for cross-contract validation
pub const LOTTERY_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID: &[u8] = b"promissory_note_cid";

// ============================================================================
// ZK CIRCUIT NAMESPACES
// ============================================================================

/// zkas commit ticket circuit namespace
pub const LOTTERY_CONTRACT_ZKAS_COMMIT_NS: &str = "CommitTicket_V1";
/// zkas reveal ticket circuit namespace
pub const LOTTERY_CONTRACT_ZKAS_REVEAL_NS: &str = "RevealTicket_V1";
pub const LOTTERY_CONTRACT_ZKAS_COMMIT_NS_V2: &str = "CommitTicketV2";
pub const LOTTERY_CONTRACT_ZKAS_REVEAL_NS_V2: &str = "RevealTicketV2";
pub const LOTTERY_CONTRACT_ZKAS_CLAIM_NS_V2: &str = "ClaimPrizeV2";
pub const LOTTERY_CONTRACT_ZKAS_DRAW_NS_V2: &str = "DrawWinnersV2";
pub const LOTTERY_CONTRACT_ZKAS_EXPIRE_NS_V2: &str = "ExpireLotteryV2";
pub const LOTTERY_CONTRACT_ZKAS_INIT_NS_V2: &str = "InitializeV2";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Maximum number of picks allowed
pub const MAX_NUM_PICKS: u8 = 10;
/// Maximum number range allowed
pub const MAX_NUMBER_RANGE: u8 = 100;
/// Default house edge in basis points (2%)
pub const DEFAULT_HOUSE_EDGE_BP: u32 = 200;
/// Maximum house edge (10%)
pub const MAX_HOUSE_EDGE_BP: u32 = 1000;
/// Minimum house edge (0.5%)
pub const MIN_HOUSE_EDGE_BP: u32 = 50;
/// Default ticket price
pub const DEFAULT_TICKET_PRICE: u64 = 100;
/// Default lottery duration in blocks
pub const DEFAULT_LOTTERY_DURATION: u64 = 100;
/// Default claim duration in blocks after draw
pub const DEFAULT_CLAIM_DURATION: u64 = 50;
/// Maximum prize tiers
pub const MAX_PRIZE_TIERS: usize = 10;

// ============================================================================
// STANDARD LOTTERY CONFIGURATIONS
// ============================================================================

/// UK National Lottery style: 6 numbers from 1-59
pub fn uk_lottery_config() -> model::LotteryConfig {
    model::LotteryConfig {
        num_picks: 6,
        number_range: 59,
        house_edge_bp: 2500,
        ticket_price: 200,
        prize_tiers: vec![
            model::PrizeTierConfig { matches_needed: 6, payout_percent: 5000, roll_to_next: true },
            model::PrizeTierConfig { matches_needed: 5, payout_percent: 2500, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 4, payout_percent: 1000, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 3, payout_percent: 250, roll_to_next: false },
        ],
    }
}

/// Pre-configured UK lottery (for convenience in tests)
pub const UK_LOTTERY_CONFIG: fn() -> model::LotteryConfig = uk_lottery_config;

/// Simple neighborhood game: 3 numbers from 1-10
pub fn neighborhood_config() -> model::LotteryConfig {
    model::LotteryConfig {
        num_picks: 3,
        number_range: 10,
        house_edge_bp: 1000,
        ticket_price: 10,
        prize_tiers: vec![
            model::PrizeTierConfig { matches_needed: 3, payout_percent: 7000, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 2, payout_percent: 2000, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 1, payout_percent: 1000, roll_to_next: false },
        ],
    }
}

/// Pre-configured neighborhood game (for convenience in tests)
pub const NEIGHBORHOOD_CONFIG: fn() -> model::LotteryConfig = neighborhood_config;

/// Superenalotto style: 6 numbers from 1-90
pub fn simple_690_config() -> model::LotteryConfig {
    model::LotteryConfig {
        num_picks: 6,
        number_range: 90,
        house_edge_bp: 2000,
        ticket_price: 100,
        prize_tiers: vec![
            model::PrizeTierConfig { matches_needed: 6, payout_percent: 5000, roll_to_next: true },
            model::PrizeTierConfig { matches_needed: 5, payout_percent: 2000, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 4, payout_percent: 1000, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 3, payout_percent: 500, roll_to_next: false },
        ],
    }
}

/// Pre-configured Superenalotto-style (for convenience in tests)
pub const SIMPLE_690_CONFIG: fn() -> model::LotteryConfig = simple_690_config;

/// Powerball style: 5 numbers from 1-69 + Powerball (simplified as single range)
pub fn powerball_config() -> model::LotteryConfig {
    model::LotteryConfig {
        num_picks: 5,
        number_range: 69,
        house_edge_bp: 3000,
        ticket_price: 200,
        prize_tiers: vec![
            model::PrizeTierConfig { matches_needed: 5, payout_percent: 5000, roll_to_next: true },
            model::PrizeTierConfig { matches_needed: 4, payout_percent: 2500, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 3, payout_percent: 1000, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 2, payout_percent: 700, roll_to_next: false },
            model::PrizeTierConfig { matches_needed: 1, payout_percent: 400, roll_to_next: false },
        ],
    }
}

/// Pre-configured Powerball-style (for convenience in tests)
pub const POWERBALL_CONFIG: fn() -> model::LotteryConfig = powerball_config;

/// Thread-safe flag for deterministic ZK proof generation.
/// Set by tests before endpoint exercise to eliminate OsRng from blinds,
/// note encryption, and proof generation, so a chain-replay determinism
/// check (PI-7) produces identical bytes on both chains.
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
