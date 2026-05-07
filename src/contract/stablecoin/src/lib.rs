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

//! DarkWow Stablecoin Contract (CDP)
//!
//! A privacy-preserving collateralized debt position (CDP) stablecoin
//! inspired by Nethermind's P2P oracle design. Key features:
//!
//! - **AMM-based price feed**: TWAP from NETHER/DRK pool, no trusted oracles
//! - **PI Controller**: Algorithmic redemption rate adjustment for stability
//! - **Privacy by default**: All positions, amounts, and identities hidden via ZK
//! - **Pedersen commitments**: Collateral and debt amounts hidden in SMT
//! - **CDP Notes**: Special Money contract coins with spend_hook to CDP Engine

use darkfi_sdk::define_contract_function;

/// Functions available in the stablecoin contract
define_contract_function!(StablecoinFunction {
    InitializeV1 = 0x00,
    OpenPositionV1 = 0x01,
    AddCollateralV1 = 0x02,
    RemoveCollateralV1 = 0x03,
    MintStableV1 = 0x04,
    RepayStableV1 = 0x05,
    LiquidateV1 = 0x06,
    UpdateConfigV1 = 0x07,
    // Precise (cold) operations - use BaseDiv for exact calculations
    // These are expensive (~500 field muls) but accurate
    GovernanceReportV1 = 0x08,  // Precise collateral/debt ratio for governance
    AccrueInterestV1 = 0x09,    // Precise interest accrual calculation
});

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
// CDP (Collateralized Debt Position) Constants
// ============================================================================

/// Minimum collateralization ratio (e.g., 150% = 15000 basis points)
pub const CDP_MIN_COLLATERALIZATION_RATIO: u64 = 15000;
/// Liquidation threshold ratio (e.g., 130% = 13000 basis points)
pub const CDP_LIQUIDATION_THRESHOLD: u64 = 13000;
/// Liquidation penalty percentage (e.g., 10% = 1000 basis points)
pub const CDP_LIQUIDATION_PENALTY: u64 = 1000;

/// Base rate for stability fee (annual rate in basis points)
pub const CDP_BASE_RATE: u64 = 500;

/// PI Controller constants
/// Proportional gain
pub const CDP_PI_KP: i64 = 1000;
/// Integral gain
pub const CDP_PI_KI: i64 = 100;

/// Price feed TWAP window in seconds (e.g., 1 hour)
pub const CDP_PRICE_FEED_TWAP_WINDOW: u64 = 3600;

/// Target price deviation threshold for PI controller adjustment
pub const CDP_PRICE_DEVIATION_THRESHOLD: u64 = 500; // 5%

// ============================================================================
// Model-Specific Constants
// ============================================================================

/// Liquity-style: Minimum collateralization (110% = 11000 basis points)
pub const LIQUIDITY_MIN_COLLATERALIZATION: u64 = 11000;
/// Liquity-style: Default stability pool size (percentage of debt)
pub const LIQUIDITY_STABILITY_POOL_MIN: u64 = 1000; // 10%

/// Frax-style: Default collateral ratio for fractional model (80% = 8000 basis points)
pub const FRAX_DEFAULT_COLLATERAL_RATIO: u64 = 8000;
/// Frax-style: Algorithmically mint limit per epoch
pub const FRAX_ALGO_MINT_LIMIT: u64 = 1_000_000;

/// Individual CDP: Default per-position min collateralization
pub const CDP_INDIVIDUAL_MIN_COLLATERALIZATION: u64 = 15000;

// ============================================================================
// Per-Collateral Default Haircuts (for multi-collateral support)
// ============================================================================

/// ETH haircut (2% buffer for volatility - ETH can move fast)
pub const ETH_HAIRCUT: u64 = 9800; // 98%
/// XMR haircut (1% buffer - Monero more stable)
pub const XMR_HAIRCUT: u64 = 9900; // 99%
/// DRK haircut (no buffer for native token)
pub const DRK_HAIRCUT: u64 = 10000; // 100%

/// Maximum debt share per collateral (prevent over-concentration)
pub const MAX_DEBT_SHARE_PER_COLLATERAL: u64 = 5000; // 50%

// ============================================================================
// Dead Man Switch Constants
// ============================================================================

/// Default dead man timeout (30 days at ~1 block/minute = 43200 blocks)
pub const DEAD_MAN_DEFAULT_TIMEOUT: u64 = 43200;
/// Disabled by default (opt-in for governance)
pub const DEAD_MAN_DEFAULT_ENABLED: bool = false;

// ============================================================================
// Database Tree Names
// ============================================================================

// These are the sled trees that will be created
pub const STABLECOIN_CONTRACT_INFO_TREE: &str = "info";
pub const STABLECOIN_CONTRACT_POSITIONS_TREE: &str = "positions";
pub const STABLECOIN_CONTRACT_POSITION_NULLIFIERS_TREE: &str = "position_nullifiers";
pub const STABLECOIN_CONTRACT_STABLECOIN_TREE: &str = "stablecoin";
pub const STABLECOIN_CONTRACT_COLLATERAL_TREE: &str = "collateral";
pub const STABLECOIN_CONTRACT_LIQUIDATIONS_TREE: &str = "liquidations";

// Keys inside the info tree
pub const STABLECOIN_CONTRACT_DB_VERSION: &[u8] = b"db_version";
pub const STABLECOIN_CONTRACT_POSITION_TREE: &[u8] = b"positions_tree";
pub const STABLECOIN_CONTRACT_LATEST_POSITION_ROOT: &[u8] = b"last_position_root";
pub const STABLECOIN_CONTRACT_STABLECOIN_SUPPLY: &[u8] = b"stablecoin_supply";
pub const STABLECOIN_CONTRACT_PI_CONTROLLER_STATE: &[u8] = b"pi_controller_state";
pub const STABLECOIN_CONTRACT_REDEMPTION_RATE: &[u8] = b"redemption_rate";

// ============================================================================
// zkas Circuit Namespaces
// ============================================================================

/// zkas open position circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_OPEN_NS_V1: &str = "OpenPosition_V1";
/// zkas add collateral circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_ADD_COLLATERAL_NS_V1: &str = "AddCollateral_V1";
/// zkas remove collateral circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_REMOVE_COLLATERAL_NS_V1: &str = "RemoveCollateral_V1";
/// zkas mint stablecoin circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_MINT_STABLE_NS_V1: &str = "MintStable_V1";
/// zkas repay stablecoin circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_REPAY_STABLE_NS_V1: &str = "RepayStable_V1";
/// zkas liquidate circuit namespace
pub const STABLECOIN_CONTRACT_ZKAS_LIQUIDATE_NS_V1: &str = "Liquidate_V1";
/// zkas governance report circuit namespace (precise, uses BaseDiv)
pub const STABLECOIN_CONTRACT_ZKAS_GOVERNANCE_REPORT_NS_V1: &str = "GovernanceReport_V1";
/// zkas interest accrual circuit namespace (precise, uses BaseDiv)
pub const STABLECOIN_CONTRACT_ZKAS_ACCRUE_INTEREST_NS_V1: &str = "AccrueInterest_V1";

// ============================================================================
// XMR Collateral Constants
// ============================================================================

/// Default XMR/USD price feed (fallback when no DEX pool available)
/// This is used as a placeholder until an XMR/USD or XMR/DRK pool exists
/// In production, the price should come from an AMM TWAP
pub const STABLECOIN_XMR_USD_PRICE_FALLBACK: u64 = 150_000_000_000; // ~$150 USD per XMR

/// Maximum price age before using fallback (in seconds)
/// If the last price update is older than this, use fallback
pub const STABLECOIN_XMR_PRICE_MAX_AGE: u64 = 3600; // 1 hour