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

//! Subscription contract data structures
//!
//! ## Composability with DAO-Escrow
//!
//! This contract integrates with DAO-Escrow for tiered membership benefits:
//! - DAO-Escrow members get discounted subscription rates
//! - Membership note verified via Merkle proof
//! - Nullifier check prevents double-spending of membership
//!
//! ## State Machine
//!
//! ```text
//! Active ──[Cancel]──> Cancelled ──[Expiry]──> Expired
//!    │                                          │
//!    └──[Renew]──> Active                       │
//! ```
//!
//! - **Active**: Subscription is valid, lock_until_block > current_block
//! - **Cancelled**: User cancelled, refund available
//! - **Expired**: Time lock expired, refund available

use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, PublicKey},
    pasta::pallas,
};
use dwow_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// STATE TYPES
// ============================================================================

/// Subscription unique identifier (Poseidon hash of subscription data)
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct SubscriptionId(pub pallas::Base);
impl SubscriptionId {
    pub fn inner(&self) -> pallas::Base { self.0 }
    pub fn to_bytes(&self) -> [u8; 32] { self.0.to_repr() }
    pub fn from_bytes(x: [u8; 32]) -> Option<Self> {
        pallas::Base::from_repr(x).into_option().map(Self)
    }
    pub fn is_zero(&self) -> bool { self.0 == pallas::Base::zero() }
    pub fn zero() -> Self { Self(pallas::Base::zero()) }
}

/// Represents the current state of a subscription
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum SubscriptionState {
    /// Subscription is active
    Active = 0,
    /// User cancelled, refund available at lock_until_block
    Cancelled = 1,
    /// Subscription expired (lock_until_block reached), refund available
    Expired = 2,
}

impl TryFrom<u8> for SubscriptionState {
    type Error = dwow_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Active),
            1 => Ok(Self::Cancelled),
            2 => Ok(Self::Expired),
            _ => Err(dwow_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Core subscription data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Subscription {
    pub version: u8,
    /// Subscription identifier (commitment)
    pub id: SubscriptionId,
    /// Subscriber's public key
    pub subscriber_pubkey: PublicKey,
    /// Plan ID
    pub plan_id: u32,
    /// Block height when subscription expires (time lock)
    pub lock_until_block: u64,
    /// Deposit amount (held in escrow)
    pub deposit: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// Current state
    pub state: SubscriptionState,
    /// Nullifier for the subscription (prevents double-cancel/renew)
    pub spent_nullifier: pallas::Base,
    /// Block height when subscription was created
    pub created_at: u64,
    /// DAO-Escrow bulla (if member of insurance pool)
    pub dao_escrow_bulla: Option<pallas::Base>,
    /// DAO-Escrow membership note (verifies insurance membership)
    pub dao_membership_note: Option<pallas::Base>,
    // ========================================================================
    // Rate Limiting Fields
    // ========================================================================
    /// Total uses allowed in a period (for rate-limited subscriptions)
    pub uses_allowed: u64,
    /// Rate period in blocks (resets after this many blocks)
    pub rate_period: u64,
    /// Accumulated uses in current period
    pub period_uses: u64,
    /// Block height of last access (for rate limiting)
    pub last_access_block: u64,
    /// Remaining uses in current period
    pub uses_remaining: u64,
    pub instance_seed: [u8; 32],
}

impl Subscription {
    /// Compute the nullifier that prevents double-cancel or double-renew
    pub fn compute_nullifier(&self, secret: pallas::Base) -> pallas::Base {
        poseidon_hash([self.id, secret])
    }
}

/// Subscription plan definition
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Plan {
    pub version: u8,
    /// Plan unique identifier
    pub id: u32,
    /// Plan name (Poseidon hash for privacy)
    pub name_hash: pallas::Base,
    /// Price per period (subscription fee)
    pub price: u64,
    /// Token ID for payment
    pub token_id: pallas::Base,
    /// Duration in blocks
    pub duration_blocks: u64,
    /// DAO treasury share (percentage * 10000, e.g., 1000 = 10%)
    pub treasury_share: u32,
    /// Endowment share (percentage * 10000)
    pub endowment_share: u32,
    /// Whether the plan is active
    pub active: bool,
    /// DAO-Escrow discount (percentage * 10000, e.g., 2000 = 20% off for members)
    pub dao_escrow_discount: u32,
    /// Required DAO-Escrow bulla for discount (optional)
    pub required_dao_escrow: Option<pallas::Base>,
}

/// Capability derived from subscription for access control
/// This implements the Object Capability pattern
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubscriptionCapability {
    /// Subscriber's public key
    pub subscriber: PublicKey,
    /// Plan ID
    pub plan_id: u32,
    /// Subscription ID (commitment)
    pub subscription_id: SubscriptionId,
    /// Permissions bitmask
    pub permissions: u8,
    /// Expiry block
    pub expires_at: u64,
    /// Nonce for unpredictability
    pub nonce: pallas::Base,
}

impl SubscriptionCapability {
    /// Derive the capability digest for access control
    #[allow(dead_code)]
    pub fn derive_capability(
        subscriber: &PublicKey,
        plan_id: u32,
        subscription_id: SubscriptionId,
        permissions: u8,
        expires_at: u64,
        nonce: pallas::Base,
    ) -> pallas::Base {
        let (bx, by) = subscriber.xy().expect("pk not identity");
        poseidon_hash([
            bx,
            by,
            pallas::Base::from(plan_id as u64),
            subscription_id,
            pallas::Base::from(permissions as u64),
            pallas::Base::from(expires_at),
            nonce,
        ])
    }
}

// ============================================================================
// PARAMETER TYPES (for contract calls)
// ============================================================================

/// Parameters for `Subscription::SubscribeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubscribeParamsV1 {
    /// Plan ID to subscribe to
    pub plan_id: u32,
    /// Subscriber's public key
    pub subscriber_pubkey: PublicKey,
    /// Commitment to subscription parameters
    pub commitment: SubscriptionId,
    /// Value commitment (Pedersen)
    pub value_commit: pallas::Point,
    /// Merkle proof of the commitment
    pub merkle_proof: Vec<pallas::Base>,
    /// Merkle root for the plan
    pub merkle_root: pallas::Base,
    /// DAO-Escrow bulla (optional - for insurance tier discount)
    pub dao_escrow_bulla: Option<pallas::Base>,
    /// DAO-Escrow membership note (verifies valid insurance member)
    pub dao_membership_note: Option<pallas::Base>,
    /// DAO-Escrow Merkle root (for verifying membership note)
    pub dao_escrow_merkle_root: Option<pallas::Base>,
    /// DAO-Escrow membership Merkle proof
    pub dao_merkle_proof: Option<Vec<pallas::Base>>,
    /// DAO-Escrow membership leaf position
    pub dao_leaf_pos: Option<u32>,
    pub instance_seed: [u8; 32],
}

/// State update for `Subscription::SubscribeV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubscribeUpdateV1 {
    /// The full subscription object
    pub subscription: Subscription,
}

/// Parameters for `Subscription::CancelV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelParamsV1 {
    /// Subscription ID
    pub subscription_id: SubscriptionId,
    /// Subscriber's secret (proves ownership)
    pub subscriber_secret: pallas::Base,
    /// Nullifier revealing the subscription is cancelled
    pub spent_nullifier: pallas::Base,
    /// Current block height
    pub current_block: u64,
    /// Recipient public key for refund
    pub recipient_pubkey: PublicKey,
}

/// State update for `Subscription::CancelV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelUpdateV1 {
    /// The cancelled subscription ID
    pub subscription_id: SubscriptionId,
    /// Nullifier for the cancelled subscription
    pub spent_nullifier: pallas::Base,
    /// The updated subscription with Cancelled state
    pub updated_subscription: Subscription,
}

/// Parameters for `Subscription::RenewV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RenewParamsV1 {
    /// Existing subscription ID
    pub subscription_id: SubscriptionId,
    /// Subscriber's secret (proves ownership)
    pub subscriber_secret: pallas::Base,
    /// New lock_until_block
    pub new_lock_until_block: u64,
    /// Nullifier for the old subscription
    pub spent_nullifier: pallas::Base,
    /// Value commitment for renewal payment
    pub value_commit: pallas::Point,
    /// Merkle proof of the commitment
    pub merkle_proof: Vec<pallas::Base>,
}

/// State update for `Subscription::RenewV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RenewUpdateV1 {
    /// The renewed subscription ID (new commitment)
    pub subscription_id: SubscriptionId,
    /// Nullifier for the old subscription
    pub spent_nullifier: pallas::Base,
    /// The new subscription object
    pub new_subscription: Subscription,
}

/// Parameters for `Subscription::VerifyAccessV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyAccessParamsV1 {
    /// Subscription ID being verified
    pub subscription_id: SubscriptionId,
    /// Capability derived from subscription
    pub capability: pallas::Base,
    /// Nonce for the proof
    pub nonce: pallas::Base,
}

/// Parameters for `Subscription::UpdateUsageV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateUsageParamsV1 {
    /// Subscription ID
    pub subscription_id: SubscriptionId,
    /// Subscriber's public key x-coordinate
    pub subscriber_pub_x: pallas::Base,
    /// Subscriber's public key y-coordinate
    pub subscriber_pub_y: pallas::Base,
    /// Subscriber's secret (proves ownership)
    pub subscriber_secret: pallas::Base,
    /// Current block height (usage_timestamp in ZK circuit)
    pub current_block: u64,
    /// Nonce for ZK circuit witness
    pub nonce: pallas::Base,
    /// Spent nullifier for this subscription
    pub spent_nullifier: pallas::Base,
    /// Merkle proof of the subscription state
    pub merkle_proof: Vec<pallas::Base>,
}

/// State update for `Subscription::UpdateUsageV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateUsageUpdateV1 {
    /// The subscription ID
    pub subscription_id: SubscriptionId,
    /// Updated period uses
    pub period_uses: u64,
    /// Updated last access block
    pub last_access_block: u64,
    /// Updated uses remaining
    pub uses_remaining: u64,
    /// Whether this was a period reset
    pub is_new_period: bool,
}

/// Parameters for `Subscription::DaoControlV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum DaoControlParamsV1 {
    /// Update plan parameters
    UpdatePlan(Plan),
    /// Activate/deactivate a plan
    SetPlanActive { plan_id: u32, active: bool },
    /// Emergency pause all subscriptions
    EmergencyPause { pause: bool, reason: pallas::Base },
    /// Withdraw from endowment fund
    EndowmentWithdraw { amount: u64, recipient: PublicKey },
    /// Slash a subscription (governance enforcement)
    Slash { subscription_id: SubscriptionId, reason: pallas::Base },
}

/// State update for `Subscription::DaoControlV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DaoControlUpdateV1 {
    /// The DAO action performed
    pub action: DaoControlAction,
}

/// Actions resulting from DAO control
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum DaoControlAction {
    /// Plan was updated
    PlanUpdated(u32),
    /// Plan active status changed
    PlanStatusChanged { plan_id: u32, active: bool },
    /// Emergency pause toggled
    EmergencyPauseToggled(bool),
    /// Endowment withdrawn
    EndowmentWithdrawn { amount: u64, recipient: PublicKey },
    /// Subscription slashed
    SubscriptionSlashed(SubscriptionId),
}

// ============================================================================
// ACCESS CONTROL CONSTANTS
// ============================================================================

/// Permission bitmask for subscription capabilities
pub mod permissions {
    /// Read access - can verify membership
    pub const READ: u8 = 0b0000_0001;
    /// Write access - can modify subscription
    pub const WRITE: u8 = 0b0000_0010;
    /// Cancel access - can cancel subscription
    pub const CANCEL: u8 = 0b0000_0100;
    /// Renew access - can renew subscription
    pub const RENEW: u8 = 0b0000_1000;
    /// Admin access - DAO governance
    pub const ADMIN: u8 = 0b1000_0000;
}