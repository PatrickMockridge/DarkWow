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

//! Data structures for relayer_endowment contract calls

use dwow_serial::{SerialDecodable, SerialEncodable};
use dwow_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};

/// Relayer's endowment account - tracks total deployed capital and fee distribution
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RelayerEndowmentAccount {
    pub version: u8,
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Relayer ID (public key)
    pub relayer_pub: PublicKey,
    /// Total deployed capital from all backers
    pub total_deployed: u64,
    /// Active deployments count
    pub active_deployments: u64,
    /// Accumulated fees to be distributed to backers
    pub accumulated_fees: u64,
    /// Default fee cut for backers (basis points)
    pub default_backer_cut_bp: u32,
    /// Block when account was created
    pub created_at: u64,
    /// Last block height when fees were settled (for force-settlement timeout)
    pub last_settlement_height: u64,
    /// Total fees collected since last settlement (for backer audit)
    pub total_collected_fees_log: u64,
    /// Whether account is active
    pub is_active: bool,
    /// Total amount slashed from this relayer (Phase 2d hardening)
    pub total_slashed: u64,
    /// Total successful withdrawals processed (Phase 2d hardening)
    pub total_successful: u64,
}

/// Individual deployment from a backer to a relayer
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EndowmentDeployment {
    pub version: u8,
    /// Unique deployment identifier
    pub deployment_id: pallas::Base,
    /// Relayer this deployment is for
    pub relayer_pub: PublicKey,
    /// Backer who deployed capital
    pub backer_pub: PublicKey,
    /// Amount deployed
    pub amount: u64,
    /// Backer's cut of relayer fees (basis points)
    pub backer_cut_bp: u32,
    /// Accumulated fees claimable by backer
    pub accumulated_fees: u64,
    /// Block when deployment was made
    pub deployed_at: u64,
    /// Block when withdrawal was requested (if requested)
    pub withdraw_requested_at: Option<u64>,
    /// Whether deployment has been withdrawn
    pub withdrawn: bool,
}

// ============================================================================
// PARAMETER STRUCTS
// ============================================================================

/// Parameters for initializing a relayer endowment account
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Default fee cut for backers (basis points)
    pub default_backer_cut_bp: u32,
    /// Public key of the relayer (from transaction signature)
    pub signature_public: PublicKey,
}

/// Update returned after initializing endowment
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub relayer_pub: PublicKey,
    pub default_backer_cut_bp: u32,
    pub created_at: u64,
}

/// Parameters for deploying capital to a relayer's endowment
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeployCapitalParamsV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    /// Relayer to deploy capital to
    pub relayer_pub: PublicKey,
    /// Amount to deploy
    pub amount: u64,
    /// Backer's desired cut of relayer fees (basis points)
    pub backer_cut_bp: u32,
    /// Backer's public key (from transaction signature)
    pub signature_public: PublicKey,
    /// Value commitment point (public input for ZK proof)
    pub value_commit: pallas::Point,
    /// Optional minimum success rate threshold (basis points, e.g. 8000 = 80%)
    pub min_success_rate_bp: Option<u64>,
    /// Optional maximum slash count threshold
    pub max_slash_count: Option<u64>,
}

/// Update returned after deploying capital
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeployCapitalUpdateV1 {
    /// Instance seed for per-capability key derivation
    pub instance_seed: [u8; 32],
    pub deployment_id: pallas::Base,
    pub relayer_pub: PublicKey,
    pub backer_pub: PublicKey,
    pub amount: u64,
    pub backer_cut_bp: u32,
    pub total_deployed: u64,
    pub active_deployments: u64,
}

/// Parameters for withdrawing a deployment
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawDeploymentParamsV1 {
    /// Deployment ID to withdraw
    pub deployment_id: pallas::Base,
}

/// Update returned after withdrawing deployment
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawDeploymentUpdateV1 {
    pub deployment_id: pallas::Base,
    pub payout_amount: u64,
    pub fees_claimed: u64,
}

/// Parameters for claiming accumulated fees
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimFeesParamsV1 {
    /// Deployment ID to claim fees for
    pub deployment_id: pallas::Base,
    /// Backer's public key X coordinate
    pub backer_pub_x: [u8; 32],
    /// Backer's public key Y coordinate
    pub backer_pub_y: [u8; 32],
    /// Fee share allocated to this backer
    pub fee_share: u64,
}

/// Update returned after claiming fees
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimFeesUpdateV1 {
    pub deployment_id: pallas::Base,
    pub claimed_amount: u64,
    pub remaining_fees: u64,
}

/// Per-deployment fee allocation for SettleFees
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FeeAllocation {
    /// Deployment receiving fees
    pub deployment_id: pallas::Base,
    /// Fee amount allocated to this deployment
    pub fee_amount: u64,
}

/// Parameters for settling fees to deployments
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleFeesParamsV1 {
    /// Relayer to settle fees for
    pub relayer_pub: PublicKey,
    /// Total fees to distribute
    pub total_fees: u64,
    /// Per-deployment fee allocations
    pub allocations: Vec<FeeAllocation>,
    /// Public key of the relayer (from transaction signature)
    pub signature_public: PublicKey,
}

/// Update returned after settling fees
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SettleFeesUpdateV1 {
    pub relayer_pub: PublicKey,
    pub total_fees_settled: u64,
    pub deployments_updated: u64,
    /// Per-deployment fee allocations applied
    pub allocations: Vec<FeeAllocation>,
}

/// Parameters for updating fee configuration
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParamsV1 {
    /// Relayer to update config for
    pub relayer_pub: PublicKey,
    /// New default backer cut (basis points)
    pub default_backer_cut_bp: u32,
}

/// Update returned after updating config
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigUpdateV1 {
    pub relayer_pub: PublicKey,
    pub default_backer_cut_bp: u32,
}

/// Parameters for backer-initiated force settlement
///
/// If a relayer hasn't settled fees within `FORCE_SETTLEMENT_TIMEOUT` blocks,
/// any backer with active deployment can force a pro-rata settlement.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ForceSettleParamsV1 {
    /// Relayer whose fees are being force-settled
    pub relayer_pub: PublicKey,
    /// Deployment ID to force-settle for
    pub deployment_id: pallas::Base,
    /// Current block height for timeout verification
    pub current_block: u64,
    /// Backer's public key (from transaction signature)
    pub signature_public: PublicKey,
}

/// Update returned after force settlement
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ForceSettleUpdateV1 {
    pub deployment_id: pallas::Base,
    pub relayer_pub: PublicKey,
    pub force_settled_amount: u64,
}

/// Parameters for deactivating an endowment account
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeactivateEndowmentParamsV1 {
    pub relayer_pub: PublicKey,
}

/// Update returned after deactivating an endowment
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DeactivateEndowmentUpdateV1 {
    pub relayer_pub: PublicKey,
}