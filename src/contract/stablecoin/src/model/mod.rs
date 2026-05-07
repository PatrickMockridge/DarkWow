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

//! Data structures for stablecoin (Pooled Debt) contract calls
//!
//! ## Architecture: Pooled Debt (Synthetix-style)
//!
//! Unlike individual CDP models (MakerDAO) where each position is tracked separately,
//! this contract uses pooled debt where:
//!
//! - All collateral goes into a global pool
//! - All debt is pooled together
//! - Users hold "debt shares" representing their proportion of total debt
//! - No individual position tracking = simpler privacy
//!
//! ## Why Pooled Debt for Privacy
//!
//! CDP Model (MakerDAO) problems:
//! - Must prove individual position is valid (complex ZK)
//! - Liquidator sees "position ID X was liquidated" (privacy leak)
//! - Individual nullifiers/commitments for each position
//!
//! Pooled Model advantages:
//! - No individual positions to track
//! - Liquidation is "pool had shortfall" not "this person was liquidated"
//! - Simpler ZK circuits
//! - No position IDs that could leak information

use darkfi_serial::{SerialDecodable, SerialEncodable};
use darkfi_sdk::{
    crypto::{IntentCommitment, IntentNullifier},
    pasta::pallas,
};

/// Namespace for stablecoin intents
pub const STABLECOIN_NAMESPACE: u64 = 0x0005;

/// Collateral type identifier
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum CollateralType {
    /// XMR (Monero) collateral
    Xmr,
    /// DRK (DarkWow) collateral
    Drk,
    /// ETH (Ethereum) collateral - large cap, DAI-backed
    Eth,
}

/// Stablecoin model selector - determines the behavior of the stablecoin
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum StablecoinModel {
    /// Synthetix-style pooled debt (default)
    /// All collateral backs all debt. No individual position tracking.
    /// Liquidation is global - pool is either healthy or not.
    PooledDebt,
    /// Liquity-style: Minimum 110% collateralization
    /// Uses stability pool for redemptions. Instant liquidation.
    /// No governance, no stability fee.
    Liquity,
    /// Frax-style: Fractional collateralization (e.g., 80% collateral + 20% algorithmic)
    /// Partial backing with seigniorage share mechanism.
    Fractional,
    /// Individual CDP: Per-position tracking (complex ZK)
    /// Each position has its own collateral/debt tracked separately.
    /// More control but leaks more data.
    IndividualCdp,
}

/// Per-collateral risk parameters (used for multi-collateral support)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CollateralParams {
    /// Collateral type
    pub collateral_type: CollateralType,
    /// Haircut applied to this collateral (basis points, e.g., 9850 = 98.5% value)
    /// Protects against price volatility before liquidation
    pub haircut: u64,
    /// Liquidation threshold for this collateral type (basis points)
    pub liquidation_threshold: u64,
    /// Maximum share of total debt this collateral can back (basis points)
    /// e.g., 30000 = max 30% of debt can be backed by this collateral
    pub max_debt_share: u64,
}

/// What action to take when dead man switch triggers
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum DeadManAction {
    /// Liquidate all positions at current prices (emergency settlement)
    LiquidateAll,
    /// Disable new minting but allow existing positions to remain
    DisableMinting,
    /// Allow free withdrawals without collateralization checks
    EnableFreeWithdrawals,
}

/// Dead man switch configuration - emergency shutdown if no executive action
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeadManSwitchConfig {
    /// Enable dead man switch
    pub enabled: bool,
    /// Timeout in blocks (if no executive action for this many blocks, trigger)
    pub timeout_blocks: u64,
    /// Action to take when triggered
    pub action: DeadManAction,
    /// Last executive action block (tracked internally, not set by user)
    #[doc(hidden)]
    pub last_action_block: u64,
}

/// Pooled Debt Engine initialization parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    /// The stablecoin model to use (PooledDebt, Liquity, Fractional, IndividualCdp)
    pub model: StablecoinModel,

    /// Initial minimum collateralization ratio for pool (basis points, e.g., 15000 = 150%)
    pub min_collateralization_ratio: u64,

    /// Liquidation threshold for pool (basis points)
    pub liquidation_threshold: u64,

    /// Liquidation penalty (basis points)
    pub liquidation_penalty: u64,

    /// Base stability fee (annual rate in basis points)
    /// Note: Liquity model ignores this (no stability fee)
    pub base_rate: u64,

    /// PI controller proportional gain
    pub pi_kp: i64,

    /// PI controller integral gain
    pub pi_ki: i64,

    /// Price feed TWAP window in seconds
    pub twap_window: u64,

    /// Price deviation threshold for PI adjustment (basis points)
    pub price_deviation_threshold: u64,

    /// Per-collateral risk parameters (for multi-collateral support)
    /// If empty, uses default single-collateral (DRK) with above params
    pub collateral_params: Vec<CollateralParams>,

    /// Dead man switch configuration (emergency shutdown)
    pub dead_man_switch: DeadManSwitchConfig,

    /// Authority public key for MoneyV3 token minting authorization
    /// Stablecoin contract needs AuthTokenMint to mint/burn tokens
    pub token_authority_pub: [u8; 32],

    /// Whether to create a new MoneyV3 token for this stablecoin
    pub create_token: bool,

    /// Token symbol for the stablecoin (e.g., "USDx") - used if create_token is true
    pub token_symbol: [u8; 32],
}

/// Deposit collateral into the pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositCollateralParams {
    /// Commitment to the deposit (hides amount)
    pub deposit_commitment: IntentCommitment,

    /// Collateral amount (hidden in commitment)
    pub collateral_amount: u64,

    /// Collateral type
    pub collateral_type: CollateralType,

    /// ZK proof: deposit is valid
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [position_nullifier, position_commitment]
    /// The prover computes these from their secret values
    pub zk_public_inputs: Vec<pallas::Base>,
}

/// Withdraw collateral from the pool (only if collateralization ratio allows)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawCollateralParams {
    /// Nullifier to prove withdrawal is authorized
    pub withdrawal_nullifier: IntentNullifier,

    /// New commitment after withdrawal
    pub new_commitment: IntentCommitment,

    /// Amount of collateral to withdraw
    pub withdraw_amount: u64,

    /// ZK proof: withdrawal doesn't violate pool collateralization
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [nullifier]
    /// The prover computes the nullifier from their secret
    pub zk_public_inputs: Vec<pallas::Base>,
}

/// Mint stablecoin against collateral pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MintStableParams {
    /// Commitment to the mint (hides debt amount)
    pub mint_commitment: IntentCommitment,

    /// Amount of stablecoin to mint
    pub mint_amount: u64,

    /// Current total debt in pool (for ratio check)
    pub total_debt: u64,

    /// Current total collateral in pool (for ratio check)
    pub total_collateral: u64,

    /// ZK proof: mint doesn't violate pool collateralization
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [old_commitment, new_commitment, position_nullifier]
    /// The prover computes these from their secret values
    pub zk_public_inputs: Vec<pallas::Base>,
}

/// Repay stablecoin debt to reduce debt share
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RepayStableParams {
    /// Commitment to the repayment
    pub repay_commitment: IntentCommitment,

    /// Amount of stablecoin to burn (repay)
    pub repay_amount: u64,

    /// ZK proof: repayment is valid
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [commitment]
    /// The prover computes the commitment from their secret values
    pub zk_public_inputs: Vec<pallas::Base>,
}

/// Liquidate pool if undercollateralized
///
/// Note: In pooled model, liquidation is global - the entire pool is either
/// liquidated or not. Individual users' collateral is seized proportionally.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LiquidateParams {
    /// Liquidation commitment
    pub liquidation_commitment: IntentCommitment,

    /// Current total debt in pool
    pub total_debt: u64,

    /// Current total collateral in pool
    pub total_collateral: u64,

    /// Current TWAP price
    pub current_price: u64,

    /// Amount of debt to cover
    pub debt_to_cover: u64,

    /// ZK proof: pool is undercollateralized
    pub proof: Vec<u8>,

    /// Liquidation reward
    pub liquidation_reward: u64,

    /// Fee paid for this operation
    pub fee: u64,

    /// ZK public inputs for proof verification: [old_commitment, new_commitment, position_nullifier]
    /// The prover computes these from their secret values
    pub zk_public_inputs: Vec<pallas::Base>,
}

/// Update pool configuration (governance)
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

/// Update data for configuration changes (sent from instruction to update phase)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigUpdateV1 {
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

/// Update data for adding collateral
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AddCollateralUpdateV1 {
    /// The position commitment (for tracking)
    pub position_commitment: IntentCommitment,
    /// Additional collateral amount
    pub added_collateral: u64,
    /// Collateral type
    pub collateral_type: CollateralType,
}

/// Update data for removing collateral
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RemoveCollateralUpdateV1 {
    /// The position nullifier (proves ownership)
    pub position_nullifier: IntentNullifier,
    /// New commitment after withdrawal
    pub new_commitment: IntentCommitment,
    /// Collateral type
    pub collateral_type: CollateralType,
    /// Amount removed
    pub removed_collateral: u64,
}

/// Update data for minting stablecoin
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct MintStableUpdateV1 {
    /// Position commitment
    pub position_commitment: IntentCommitment,
    /// Amount minted
    pub mint_amount: u64,
    /// New total debt after minting
    pub new_total_debt: u64,
}

/// Update data for repaying stablecoin debt
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RepayStableUpdateV1 {
    /// Position nullifier
    pub position_nullifier: IntentNullifier,
    /// New commitment after repayment
    pub new_commitment: IntentCommitment,
    /// Amount repaid
    pub repay_amount: u64,
    /// New total debt after repayment
    pub new_total_debt: u64,
}

/// Update data for liquidating the pool
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LiquidateUpdateV1 {
    /// Liquidation record
    pub debt_covered: u64,
    /// Collateral seized
    pub collateral_seized: u64,
    /// Liquidation penalty
    pub penalty: u64,
    /// New total debt after liquidation
    pub new_total_debt: u64,
    /// New total collateral after liquidation
    pub new_total_collateral: u64,
}

/// Update data for governance report
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GovernanceReportUpdateV1 {
    /// Reported collateral ratio in basis points
    pub collateral_ratio_bps: u64,
    /// Interest accrued since last report
    pub interest_accrued: u64,
    /// Reporter's public key x
    pub reporter_pub_x: [u8; 32],
    /// Reporter's public key y
    pub reporter_pub_y: [u8; 32],
}

/// Update data for interest accrual
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AccrueInterestUpdateV1 {
    /// Old total debt before accrual
    pub old_total_debt: u64,
    /// New total debt after accrual
    pub new_total_debt: u64,
    /// Interest amount accrued
    pub interest_amount: u64,
    /// Accumulator's public key x
    pub accumulator_pub_x: [u8; 32],
    /// Accumulator's public key y
    pub accumulator_pub_y: [u8; 32],
}

// ============================================================================
// POOLED DEBT STATE (not per-user positions)
// ============================================================================

/// Global debt pool state
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DebtPool {
    /// Total debt in the pool (all stablecoins minted)
    pub total_debt: u64,

    /// Total collateral value (in stablecoin terms)
    pub total_collateral: u64,

    /// Accumulated fees from interest
    pub accumulated_fees: u64,

    /// Last update timestamp
    pub last_update: u64,
}

/// Collateral pool for a specific collateral type
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CollateralPool {
    /// Collateral type
    pub collateral_type: CollateralType,

    /// Total amount of this collateral type deposited
    pub total_deposited: u64,

    /// Current value ratio (to stablecoin)
    pub value_ratio: u64,

    /// Last update timestamp
    pub last_update: u64,
}

/// User's debt share record
///
/// Note: In the pooled model, we don't track individual positions.
/// Instead, users have "debt shares" that represent their proportion
/// of the total debt. Their actual collateral is pooled.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DebtShare {
    /// Owner's public key
    pub owner_pub_x: [u8; 32],
    pub owner_pub_y: [u8; 32],

    /// Amount of stablecoin debt they owe
    pub debt_amount: u64,

    /// Commitment for this debt position
    pub commitment: IntentCommitment,

    /// Creation timestamp
    pub created_at: u64,

    /// Last update timestamp
    pub updated_at: u64,
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

    /// Pool reserve0
    pub reserve0: u64,

    /// Pool reserve1
    pub reserve1: u64,

    /// Block timestamp
    pub timestamp: u64,
}

// ============================================================================
// Cold/Precise Operations (BaseDiv - expensive but accurate)
// ============================================================================

/// Governance report parameters (cold/precise - uses BaseDiv)
/// For monthly governance reporting and precise ratio calculations
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct GovernanceReportParams {
    /// Current total collateral in pool
    pub total_collateral: u64,

    /// Current total debt in pool
    pub total_debt: u64,

    /// Calculated collateral ratio in basis points (collateral/debt * 10000)
    pub collateral_ratio_bps: u64,

    /// Interest accrued since last report
    pub interest_accrued: u64,

    /// Timestamp of this report
    pub report_timestamp: u64,

    /// Reporter's public key
    pub reporter_pub_x: [u8; 32],
    pub reporter_pub_y: [u8; 32],

    /// ZK proof: governance_report_v1.zk
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Accrue interest parameters (cold/precise - uses BaseDiv)
/// For precise interest accrual calculation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct AccrueInterestParams {
    /// Total debt before interest accrual
    pub old_total_debt: u64,

    /// Total debt after interest accrual
    pub new_total_debt: u64,

    /// Calculated interest amount using BaseDiv
    pub interest_amount: u64,

    /// Interest rate per second (in basis points)
    pub rate_per_second: u64,

    /// Time elapsed since last accrual (seconds)
    pub time_elapsed: u64,

    /// Accumulator's public key
    pub accumulator_pub_x: [u8; 32],
    pub accumulator_pub_y: [u8; 32],

    /// ZK proof: accrue_interest_v1.zk
    pub proof: Vec<u8>,

    /// Fee paid for this operation
    pub fee: u64,
}

/// Global liquidation record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct LiquidationRecord {
    /// Total debt covered
    pub debt_covered: u64,

    /// Total collateral seized
    pub collateral_seized: u64,

    /// Liquidation penalty applied
    pub penalty: u64,

    /// Liquidator public key
    pub liquidator_pub_x: [u8; 32],
    pub liquidator_pub_y: [u8; 32],

    /// Timestamp
    pub liquidated_at: u64,
}

// ============================================================================
// DESIGN NOTES: Why Pooled Debt vs Individual CDP
// ============================================================================
//
// INDIVIDUAL CDP MODEL (original DarkWow approach, possible but complex):
//
// Pros:
// - Users have individual positions with specific collateral/debt
// - Can implement partial liquidations
// - More granular control over positions
//
// Cons:
// - Must prove individual position validity (complex ZK circuits)
// - Position IDs leak information: "position 123 was liquidated"
// - Liquidators can see specific positions being liquidated
// - Individual nullifiers/commitments for each position
// - ZK circuits need to verify per-position collateralization
//
// POOLED DEBT MODEL (this implementation, simpler for privacy):
//
// Pros:
// - No individual positions to track
// - Liquidation is "pool had shortfall" - no position IDs leaked
// - Simpler ZK circuits
// - All collateral backs all debt - more capital efficient
//
// Cons:
// - Cannot liquidate individual positions
// - Entire pool must be healthy or entire pool can be liquidated
// - User's collateral is always at risk from others' behavior
//
// The pooled model was chosen for the MVP because:
// 1. Simpler ZK circuits (no per-position proofs)
// 2. Better privacy (no position IDs that could be tracked)
// 3. Faster to implement
// 4. Individual CDP can be added later as a layer on top
//
// TO ADD INDIVIDUAL CDP LATER:
// - Layer individual position tracking on top of pooled debt
// - Use attestation contract to verify individual positions
// - Each user can choose pooled (simpler) or individual (more control)
// ============================================================================
