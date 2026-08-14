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

//! Insurance Market Contract
//!
//! A decentralized insurance marketplace that prices risk using prediction markets
//! and connects underwriters (engineers) with risk buyers.
//!
//! ## Architecture
//!
//! - **Risk Types**: Categories of insurable risks (smart contract hacks, oracle manipulation, etc.)
//! - **Insurance Markets**: Markets for specific risks with premium pricing
//! - **Underwriters**: Engineers who post bonds to underwrite risks they can mitigate
//! - **Coverage**: Insurance policies purchased by risk buyers
//! - **Claims**: Payout requests when covered events occur
//!
//! ## Integration
//!
//! - **Money::Burn** for premium payments
//! - **Money::TokenMint** for claim payouts
//! - **PredictionMarket** for probability pricing

use dwow_sdk::error::ContractError;

/// Functions available in the contract
#[repr(u8)]
#[derive(Debug)]
pub enum InsuranceMarketFunction {
    InitializeV1 = 0x00,
    RegisterRiskTypeV1 = 0x01,
    CreateMarketV1 = 0x02,
    UnderwriteV1 = 0x03,
    PurchaseCoverageV1 = 0x04,
    FileClaimV1 = 0x05,
    ResolveClaimV1 = 0x06,
    WithdrawPremiumV1 = 0x07,
    UpdatePremiumV1 = 0x08,
    // O-Cap enabled functions
    UnderwriteWithCapabilityV1 = 0x09,
    PurchaseCoverageWithCapabilityV1 = 0x0a,
    PurchaseCoverageWithDAGV1 = 0x0b,
    ResolveClaimWithCapabilityV1 = 0x0c,
    DeactivateUnderwriterV1 = 0x0d,
    CloseMarketV1 = 0x0e,
    RetireRiskTypeV1 = 0x0f,
}

impl TryFrom<u8> for InsuranceMarketFunction {
    type Error = ContractError;

    fn try_from(b: u8) -> core::result::Result<Self, Self::Error> {
        match b {
            0x00 => Ok(Self::InitializeV1),
            0x01 => Ok(Self::RegisterRiskTypeV1),
            0x02 => Ok(Self::CreateMarketV1),
            0x03 => Ok(Self::UnderwriteV1),
            0x04 => Ok(Self::PurchaseCoverageV1),
            0x05 => Ok(Self::FileClaimV1),
            0x06 => Ok(Self::ResolveClaimV1),
            0x07 => Ok(Self::WithdrawPremiumV1),
            0x08 => Ok(Self::UpdatePremiumV1),
            // O-Cap enabled functions
            0x09 => Ok(Self::UnderwriteWithCapabilityV1),
            0x0a => Ok(Self::PurchaseCoverageWithCapabilityV1),
            0x0b => Ok(Self::PurchaseCoverageWithDAGV1),
            0x0c => Ok(Self::ResolveClaimWithCapabilityV1),
            0x0d => Ok(Self::DeactivateUnderwriterV1),
            0x0e => Ok(Self::CloseMarketV1),
            0x0f => Ok(Self::RetireRiskTypeV1),
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

/// Stores registered risk types indexed by risk_type_id
pub const INSURANCE_CONTRACT_RISK_TYPES_TREE: &str = "risk_types";
/// Stores insurance markets indexed by market_id
pub const INSURANCE_CONTRACT_MARKETS_TREE: &str = "markets";
/// Stores underwriters indexed by underwriter_id
pub const INSURANCE_CONTRACT_UNDERWRITERS_TREE: &str = "underwriters";
/// Stores coverage policies indexed by coverage_id
pub const INSURANCE_CONTRACT_COVERAGES_TREE: &str = "coverages";
/// Stores claims indexed by claim_id
pub const INSURANCE_CONTRACT_CLAIMS_TREE: &str = "claims";
/// Stores endowment pools for LP capital
pub const INSURANCE_CONTRACT_ENDOWMENT_TREE: &str = "endowment";
/// Stores contract info (version, config)
pub const INSURANCE_CONTRACT_INFO_TREE: &str = "info";
/// Nullifiers tree for replay protection
pub const INSURANCE_MARKET_NULLIFIERS_TREE: &str = "nullifiers";

// ============================================================================
// DATABASE KEYS
// ============================================================================

/// Version key for database migrations
pub const INSURANCE_CONTRACT_DB_VERSION: &[u8] = b"db_version";
/// Money_v3 contract ID key for cross-contract validation
pub const INSURANCE_CONTRACT_PROMISSORY_NOTE_CONTRACT_ID: &[u8] = b"promissory_note_cid";

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default coverage period in blocks (10000 ≈ 1 week)
pub const DEFAULT_COVERAGE_PERIOD: u64 = 10000;
/// Default premium rate in basis points (5%)
pub const DEFAULT_PREMIUM_RATE: u32 = 500;
/// Minimum bond rate in basis points (10%)
pub const MIN_BOND_RATE: u32 = 1000;
/// Default coverage leverage (10x)
pub const DEFAULT_COVERAGE_LEVERAGE: u32 = 10;
/// Maximum coverage leverage (50x)
pub const MAX_COVERAGE_LEVERAGE: u32 = 50;

// zkas circuit namespaces
pub const INSURANCE_MARKET_ZKAS_UNDERWRITE_WITH_CAPABILITY_NS_V1: &str =
    "Underwrite";
pub const INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_WITH_CAPABILITY_NS_V1: &str =
    "PurchaseCoverage";
pub const INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_NS_V1: &str =
    "PurchaseCoverageV1";
pub const INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_WITH_DAG_NS_V1: &str =
    "PurchaseCoverageWithDAG";
pub const INSURANCE_MARKET_ZKAS_UNDERWRITE_WITH_CAPABILITY_NS_V2: &str = "UnderwriteV2";
pub const INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_WITH_CAPABILITY_NS_V2: &str = "PurchaseCoverageWithCapabilityV2";
pub const INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_NS_V2: &str = "PurchaseCoverageV2";
pub const INSURANCE_MARKET_ZKAS_PURCHASE_COVERAGE_WITH_DAG_NS_V2: &str = "PurchaseCoverageWithDAGV2";

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