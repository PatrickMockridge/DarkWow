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

//! Data structures for stablecoin (CDP) contract calls
//!
//! ## Design Principles (from Nethermind P2P Oracle)
//!
//! 1. **AMM-based price feed**: TWAP from NETHER/DRK constant-product AMM pool
//! 2. **PI Controller**: Proportional-Integral controller adjusts redemption rate
//! 3. **Privacy**: All positions and amounts hidden via Pedersen commitments + SMT
//! 4. **Self-sovereign**: No trusted price oracles, no governance can freeze

use darkfi_serial::{SerialDecodable, SerialEncodable};
use darkfi_sdk::crypto::{IntentCommitment, IntentNullifier};

/// Namespace for stablecoin intents (used with generic intent primitives)
pub const STABLECOIN_NAMESPACE: u64 = 0x0004;

/// Collateral type identifier
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum CollateralType {
    /// XMR (Monero) collateral
    Xmr,
    /// DRK (DarkFi) collateral
    Drk,
}

/// CDP Engine initialization parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    /// Initial minimum collateralization ratio (basis points)
    pub min_collateralization_ratio: u64,

    /// Initial liquidation threshold (basis points)
    pub liquidation_threshold: u64,

    /// Liquidation penalty (basis points)
    pub liquidation_penalty: u64,

    /// Base stability fee (annual rate in basis points)
    pub base_rate: u64,

    /// PI controller proportional gain
    pub pi_kp: i64,

    /// PI controller integral gain
    pub pi_ki: i64,

    /// Price feed TWAP window in seconds
    pub twap_window: u64,

    /// Price deviation threshold for PI adjustment (basis points)
    pub price_deviation_threshold: u64,
}

/// Open a new CDP (Collateralized Debt Position)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct OpenPositionParams {
    /// Pedersen commitment to collateral amount + debt (uses generic PrivateIntent commitment)
    /// commitment = poseidon_hash([9001, owner_x, owner_y, namespace, payload_hash, expiry, nonce, blind])
    pub position_commitment: IntentCommitment,

    /// Owner's public key for the position
    pub owner_pub_x: [u8; 32],
    pub owner_pub_y: [u8; 32],

    /// Collateral type (XMR or DRK)
    pub collateral_type: CollateralType,

    /// Initial collateral amount (hidden in commitment)
    pub collateral_amount: u64,

    /// Initial debt amount (stablecoin to mint)
    pub debt_amount: u64,

    /// Merkle proof of membership in position tree
    pub merkle_proof: Vec<[u8; 32]>,

    /// Leaf index in Merkle tree (hidden via ZK)
    pub leaf_index: u64,

    /// ZK proof: position commitment valid + collateral sufficient
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Add collateral to an existing CDP
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddCollateralParams {
    /// Nullifier to identify the position (uses generic PrivateIntent nullifier)
    pub position_nullifier: IntentNullifier,

    /// New collateral commitment (cumulative) - uses generic PrivateIntent commitment
    pub new_commitment: IntentCommitment,

    /// Amount of collateral being added (hidden in new commitment)
    pub added_collateral: u64,

    /// Merkle proof of position existence
    pub merkle_proof: Vec<[u8; 32]>,

    /// Current position root
    pub current_root: [u8; 32],

    /// ZK proof: added collateral is valid
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Remove collateral from a CDP (only if collateralization ratio allows)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RemoveCollateralParams {
    /// Nullifier to identify the position (uses generic PrivateIntent nullifier)
    pub position_nullifier: IntentNullifier,

    /// New commitment after removal (uses generic PrivateIntent commitment)
    pub new_commitment: IntentCommitment,

    /// Amount of collateral to remove
    pub removed_collateral: u64,

    /// Current debt (must remain below capacity)
    pub current_debt: u64,

    /// Merkle proof of position
    pub merkle_proof: Vec<[u8; 32]>,

    /// Current position root
    pub current_root: [u8; 32],

    /// ZK proof: removal doesn't violate collateralization ratio
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Mint stablecoin against collateral
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MintStableParams {
    /// Nullifier identifying the position (uses generic PrivateIntent nullifier)
    pub position_nullifier: IntentNullifier,

    /// New commitment with increased debt (uses generic PrivateIntent commitment)
    pub new_commitment: IntentCommitment,

    /// Amount of stablecoin to mint
    pub mint_amount: u64,

    /// Current collateral amount (for ratio check)
    pub current_collateral: u64,

    /// Merkle proof
    pub merkle_proof: Vec<[u8; 32]>,

    /// Current position root
    pub current_root: [u8; 32],

    /// ZK proof: mint doesn't violate min collateralization
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Repay stablecoin debt to unlock collateral
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RepayStableParams {
    /// Nullifier identifying the position (uses generic PrivateIntent nullifier)
    pub position_nullifier: IntentNullifier,

    /// New commitment with reduced debt (uses generic PrivateIntent commitment)
    pub new_commitment: IntentCommitment,

    /// Amount of stablecoin to burn (repay)
    pub repay_amount: u64,

    /// Current collateral (for ratio check)
    pub current_collateral: u64,

    /// Current debt before repayment
    pub current_debt: u64,

    /// Merkle proof
    pub merkle_proof: Vec<[u8; 32]>,

    /// Current position root
    pub current_root: [u8; 32],

    /// ZK proof: repayment is valid
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Liquidate an undercollateralized CDP
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LiquidateParams {
    /// Nullifier of the position being liquidated (uses generic PrivateIntent nullifier)
    pub position_nullifier: IntentNullifier,

    /// New commitment after liquidation (uses generic PrivateIntent commitment)
    pub new_commitment: IntentCommitment,

    /// Current collateral in position
    pub collateral_amount: u64,

    /// Current debt in position
    pub debt_amount: u64,

    /// Current price from TWAP
    pub current_price: u64,

    /// Merkle proof of position
    pub merkle_proof: Vec<[u8; 32]>,

    /// Current position root
    pub current_root: [u8; 32],

    /// ZK proof: position is below liquidation threshold
    pub proof: Vec<u8>,

    /// Liquidation reward to liquidator
    pub liquidation_reward: u64,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Update CDP engine configuration (governance)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParams {
    /// New minimum collateralization ratio
    pub min_collateralization_ratio: u64,

    /// New liquidation threshold
    pub liquidation_threshold: u64,

    /// New liquidation penalty
    pub liquidation_penalty: u64,

    /// New base rate
    pub base_rate: u64,

    /// New PI controller Kp
    pub pi_kp: i64,

    /// New PI controller Ki
    pub pi_ki: i64,

    /// New TWAP window
    pub twap_window: u64,

    /// New price deviation threshold
    pub price_deviation_threshold: u64,
}

/// Stored CDP position record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Position {
    /// Position commitment hash (uses generic PrivateIntent commitment)
    pub commitment: IntentCommitment,

    /// Owner's public key
    pub owner_pub_x: [u8; 32],
    pub owner_pub_y: [u8; 32],

    /// Collateral type
    pub collateral_type: CollateralType,

    /// Collateral amount (hidden in commitment)
    pub collateral_amount: u64,

    /// Debt amount (stablecoin minted, hidden in commitment)
    pub debt_amount: u64,

    /// Accumulated stability fee
    pub accumulated_fee: u64,

    /// Whether position has been liquidated
    pub liquidated: bool,

    /// Creation timestamp
    pub created_at: u64,

    /// Last update timestamp
    pub updated_at: u64,
}

/// Stored liquidation record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Liquidation {
    /// Position nullifier (uses generic PrivateIntent nullifier)
    pub position_nullifier: IntentNullifier,

    /// Liquidator public key
    pub liquidator_pub_x: [u8; 32],
    pub liquidator_pub_y: [u8; 32],

    /// Collateral seized
    pub collateral_seized: u64,

    /// Debt burned
    pub debt_burned: u64,

    /// Liquidation penalty
    pub penalty: u64,

    /// Timestamp
    pub liquidated_at: u64,
}

/// PI Controller state for redemption rate
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PiControllerState {
    /// Current integral term
    pub integral: i64,

    /// Last update timestamp
    pub last_update: u64,

    /// Current redemption rate (basis points per second)
    pub current_rate: u64,

    /// Last known TWAP price
    pub last_twap: u64,
}

/// Price feed data from AMM TWAP
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct PriceFeed {
    /// Current TWAP price
    pub twap: u64,

    /// TWAP window start
    pub window_start: u64,

    /// TWAP window end
    pub window_end: u64,

    /// NETHER/DRK pool reserve0
    pub reserve0: u64,

    /// NETHER/DRK pool reserve1
    pub reserve1: u64,

    /// Block timestamp
    pub timestamp: u64,
}

// ============================================================================
// DESIGN NOTES: How This Differs from Traditional CDPs
// ============================================================================
//
// Traditional CDP (MakerDAO DAI):
// - Governance-controlled parameters
// - Price oracles can be manipulated
// - All positions and amounts public
// - Liquidation auctions can be front-run
//
// This Design (DarkFi Stablecoin):
// - PI Controller replaces governance for rate adjustment
// - AMM TWAP is manipulation-resistant
// - All positions and amounts hidden via ZK
// - Liquidation can be triggered by anyone via ZK proof
//
// Key Innovation: The P2P Oracle uses the NETHER/DRK AMM pool itself as
// the price feed, creating a self-referential stability mechanism where
// the stablecoin's own collateral pool provides price discovery.
//
// ============================================================================