/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

//! Plain Subscription Contract Model
//!
//! # Privacy Notice
//!
//! This contract uses **partial transparency** - state is public on-chain.
//! See [`PRIVACY_TRADEOFFS.md`](PRIVACY_TRADEOFFS.md) for full details.
//!
//! # ZK vs Native Operations
//!
//! | Operation | Method | Reason |
//! |-----------|--------|--------|
//! | Signature verification | ZK (Schnorr) | Constrainable, sound |
//! | Subscription commitment | ZK (Poseidon) | Privacy-preserving |
//! | Access bitmask checking | Native Rust | Needs `base_div` (not implemented) |
//! | Rate limit calculation | Native Rust | Needs `base_div` (not implemented) |

use darkfi_sdk::{
    crypto::PublicKey,
    pasta::pallas,
};
use darkfi_sdk::crypto::schnorr::Signature;
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// ACCESS RIGHTS (Bitmask-based - Native Rust, visible on-chain)
// ============================================================================

/// Access rights as bitmask values
/// PRIVACY NOTICE: These are PUBLIC in the plain version
pub const ACCESS_NONE: u32 = 0b0000;
pub const ACCESS_READ: u32 = 0b0001;
pub const ACCESS_WRITE: u32 = 0b0010;
pub const ACCESS_ADMIN: u32 = 0b0100;

/// Tier definitions with bitmask values
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubscriptionTier {
    Basic = ACCESS_READ as isize,
    Premium = (ACCESS_READ | ACCESS_WRITE) as isize,
    Admin = (ACCESS_READ | ACCESS_WRITE | ACCESS_ADMIN) as isize,
}

impl SubscriptionTier {
    pub fn from_u32(val: u32) -> Option<Self> {
        if val == (ACCESS_READ | ACCESS_WRITE | ACCESS_ADMIN) {
            Some(SubscriptionTier::Admin)
        } else if val == (ACCESS_READ | ACCESS_WRITE) {
            Some(SubscriptionTier::Premium)
        } else if val == ACCESS_READ {
            Some(SubscriptionTier::Basic)
        } else {
            None
        }
    }

    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Subscription state
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum SubscriptionState {
    Active = 0,
    Cancelled = 1,
    Expired = 2,
}

// ============================================================================
// SUBSCRIPTION (Plain - all fields visible)
// ============================================================================

/// Subscription record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Subscription {
    /// Unique subscription ID (Poseidon hash)
    pub id: pallas::Base,
    /// Subscriber public key
    pub subscriber: PublicKey,
    /// Service provider public key
    pub provider: PublicKey,
    /// Tier/permissions bitmask
    /// PRIVACY NOTICE: This is PUBLIC in plain version
    pub tier: u32,
    /// Subscription state
    pub state: SubscriptionState,
    /// Uses remaining (for rate-limited subscriptions)
    pub uses_remaining: u64,
    /// Total uses allowed in period
    pub uses_allowed: u64,
    /// Rate period in blocks
    pub rate_period: u64,
    /// Subscription start block
    pub start_block: u64,
    /// Subscription expiry block
    pub expiry_block: u64,
    /// Last access block (for rate limiting)
    pub last_access_block: u64,
    /// Accumulated uses in current period
    pub period_uses: u64,
}

impl Subscription {
    /// Check if subscription is active
    pub fn is_active(&self, current_block: u64) -> bool {
        self.state == SubscriptionState::Active && current_block < self.expiry_block
    }

    /// OPCODE PLACEHOLDER: When base_div is available in ZK, this could be ZK-verified
    /// Currently uses native Rust division (visible on-chain)
    ///
    /// PRIVACY NOTICE: Rate limit ratio is visible
    pub fn check_rate_limit(&self, current_block: u64) -> bool {
        // If we've crossed a period boundary, reset counter
        let blocks_since_last = current_block.saturating_sub(self.last_access_block);

        if blocks_since_last >= self.rate_period {
            // New period - uses reset
            return self.uses_allowed > 0
        }

        // Within same period
        self.period_uses < self.uses_allowed
    }

    /// OPCODE PLACEHOLDER: When base_div is available in ZK
    /// Currently uses native Rust division (visible on-chain)
    ///
    /// PRIVACY NOTICE: Rate calculation is visible
    pub fn update_usage(&mut self, current_block: u64) {
        let blocks_since_last = current_block.saturating_sub(self.last_access_block);

        if blocks_since_last >= self.rate_period {
            // New period - reset
            self.period_uses = 1;
        } else {
            self.period_uses += 1;
        }

        self.last_access_block = current_block;
        self.uses_remaining = self.uses_remaining.saturating_sub(1);
    }
}

/// Subscription parameters for creation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubscribeParamsV1 {
    /// Subscriber public key
    pub subscriber: PublicKey,
    /// Service provider public key
    pub provider: PublicKey,
    /// Tier/permissions bitmask
    pub tier: u32,
    /// Uses allowed per period
    pub uses_allowed: u64,
    /// Rate period in blocks
    pub rate_period: u64,
    /// Duration in blocks
    pub duration_blocks: u64,
    /// Subscriber signature over subscription params
    /// ZK: Schnorr signature verified in ZK to constrain subscriber
    pub signature: Signature,
}

/// Parameters for access verification
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyAccessParamsV1 {
    /// Subscription ID
    pub subscription_id: pallas::Base,
    /// Required access bitmask
    /// PRIVACY NOTICE: This is PUBLIC in plain version
    pub required_access: u32,
    /// Current block for rate limit check
    pub current_block: u64,
}

/// Parameters for cancellation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelParamsV1 {
    /// Subscription ID
    pub subscription_id: pallas::Base,
    /// Subscriber public key for authorization
    pub subscriber: PublicKey,
    /// Subscriber signature for authorization
    /// ZK: Schnorr signature verified in ZK to constrain subscriber
    pub signature: Signature,
}

/// Update produced by access verification
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct VerifyAccessUpdateV1 {
    pub subscription_id: pallas::Base,
    pub access_granted: bool,
    pub uses_remaining: u64,
}

/// Update produced by subscription creation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SubscribeUpdateV1 {
    pub subscription_id: pallas::Base,
    pub subscriber: PublicKey,
    pub provider: PublicKey,
    pub tier: u32,
    pub uses_allowed: u64,
    pub rate_period: u64,
    pub start_block: u64,
    pub expiry_block: u64,
}

/// Update produced by cancellation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelUpdateV1 {
    pub subscription_id: pallas::Base,
    pub refunded_amount: u64,
}
