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

//! Atomic Swap contract data structures
//!
//! ## Hashed Timelock Contract (HTLC) Pattern
//!
//! ```text
//! Alice (Chain A)                          Bob (Chain B)
//! ───────────────────────────────────      ───────────────────────────────────
//! 1. Create HTLC                           1. Create HTLC
//!    hash = SHA256(secret)                    hash = SHA256(secret)
//!    timelock = T                             timelock = T + δ
//!    amount = X                                amount = Y
//! 2. Send hash to Bob             ───────────────────────────────────────►
//!                                             2. Verify hash matches
//!                                             3. Lock funds
//! 4. Wait for Bob's confirmation   ◄───────────────────────────────────────
//! 5. Reveal secret (on-chain)       ───────────────────────────────────────►
//! 6. Bob claims on Chain B         ◄───────────────────────────────────────
//! 7. Alice claims on Chain A        ───────────────────────────────────────►
//! ```
//!
//! ## Security Properties
//!
//! - **Atomic**: Both sides complete, or neither
//! - **Hashlock**: Only secret holder can claim
//! - **Timelock**: Refund after expiration
//! - **Non-custodial**: No third-party holds funds

use darkfi_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

// ============================================================================
// STATE TYPES
// ============================================================================

/// Atomic swap unique identifier (hash of swap parameters)
pub type SwapId = pallas::Base;

/// Represents the current state of a swap
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum SwapState {
    /// Swap created, funds locked
    Created = 0,
    /// Claimed by the other party (secret revealed)
    Claimed = 1,
    /// Refunded after timelock expiration
    Refunded = 2,
    /// Both sides completed (final state)
    Completed = 3,
}

impl TryFrom<u8> for SwapState {
    type Error = darkfi_sdk::error::ContractError;

    fn try_from(b: u8) -> Result<Self, Self::Error> {
        match b {
            0 => Ok(Self::Created),
            1 => Ok(Self::Claimed),
            2 => Ok(Self::Refunded),
            3 => Ok(Self::Completed),
            _ => Err(darkfi_sdk::error::ContractError::InvalidFunction),
        }
    }
}

// ============================================================================
// CORE DATA STRUCTURES
// ============================================================================

/// Core swap data stored on-chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Swap {
    /// Swap identifier (commitment)
    pub id: SwapId,
    /// The hash of the secret (same on both chains)
    pub hash: pallas::Base,
    /// Timelock block height (before which refund is not allowed)
    pub timelock: u64,
    /// Current state
    pub state: SwapState,
    /// Which side: 0 = Alice (initiator), 1 = Bob (responder)
    pub side: u8,
    /// External chain identifier (0 = Ethereum, 1 = Bitcoin, etc.)
    pub external_chain: u8,
    /// External chain address (receiver on external chain)
    pub external_receiver: pallas::Base,
    /// DarkFi receiver public key
    pub darkfi_receiver: PublicKey,
    /// Swap amount
    pub amount: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Blinding factor for the swap commitment
    pub blind: pallas::Base,
    /// Created at block
    pub created_at: u64,
}

impl Swap {
    /// Derive the swap ID from parameters
    #[allow(dead_code)]
    pub fn derive_id(
        hash: pallas::Base,
        timelock: u64,
        darkfi_receiver: &PublicKey,
        amount: u64,
        token_id: pallas::Base,
        side: u8,
        blind: pallas::Base,
    ) -> SwapId {
        let (dx, dy) = darkfi_receiver.xy();
        poseidon_hash([
            hash,
            pallas::Base::from(timelock),
            dx,
            dy,
            pallas::Base::from(amount),
            token_id,
            pallas::Base::from(side),
            blind,
        ])
    }
}

/// Represents an HTLC on an external chain
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExternalHtlc {
    /// Hash of the secret (same as internal swap.hash)
    pub hash: pallas::Base,
    /// Timelock block on external chain
    pub external_timelock: u64,
    /// External chain ID (0 = Ethereum, 1 = Bitcoin, etc.)
    pub chain_id: u8,
    /// Receiver address on external chain
    pub receiver: pallas::Base,
    /// Sender address on external chain
    pub sender: pallas::Base,
    /// Amount locked
    pub amount: u64,
    /// Whether the secret has been revealed
    pub secret_revealed: bool,
}

// ============================================================================
// PARAMETER TYPES (for contract calls)
// ============================================================================

/// Parameters for `AtomicSwap::CreateSwapV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateSwapParamsV1 {
    /// The hash of the secret (same on both chains)
    pub hash: pallas::Base,
    /// Timelock block height (refund not allowed before)
    pub timelock: u64,
    /// Which side: 0 = Alice (initiator), 1 = Bob (responder)
    pub side: u8,
    /// External chain ID
    pub external_chain: u8,
    /// External chain receiver address
    pub external_receiver: pallas::Base,
    /// DarkFi receiver public key
    pub darkfi_receiver: PublicKey,
    /// Swap amount
    pub amount: u64,
    /// Token ID
    pub token_id: pallas::Base,
    /// Blinding factor
    pub blind: pallas::Base,
    /// Commitment to the swap
    pub commitment: SwapId,
}

/// State update for `AtomicSwap::CreateSwapV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateSwapUpdateV1 {
    /// The created swap ID
    pub swap_id: SwapId,
};

/// Parameters for `AtomicSwap::ClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimParamsV1 {
    /// Swap ID
    pub swap_id: SwapId,
    /// The secret (revealed on external chain)
    pub secret: pallas::Base,
    /// Nullifier for the swap
    pub nullifier: pallas::Base,
}

/// State update for `AtomicSwap::ClaimV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimUpdateV1 {
    /// The claimed swap ID
    pub swap_id: SwapId,
    /// Nullifier for the spent swap
    pub nullifier: pallas::Base,
    /// The revealed secret (cleared after use)
    pub secret: pallas::Base,
};

/// Parameters for `AtomicSwap::RefundV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RefundParamsV1 {
    /// Swap ID
    pub swap_id: SwapId,
    /// Current block (for timelock verification)
    pub current_block: u64,
    /// Nullifier for the swap
    pub nullifier: pallas::Base,
    /// Recipient for refund
    pub recipient: PublicKey,
}

/// State update for `AtomicSwap::RefundV1`
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RefundUpdateV1 {
    /// The refunded swap ID
    pub swap_id: SwapId,
    /// Nullifier for the refunded swap
    pub nullifier: pallas::Base,
};

// ============================================================================
// CROSS-CHAIN CONSTANTS
// ============================================================================

/// External chain identifiers
pub mod chains {
    /// Ethereum
    pub const CHAIN_ETHEREUM: u8 = 0;
    /// Bitcoin
    pub const CHAIN_BITCOIN: u8 = 1;
    /// Solana
    pub const CHAIN_SOLANA: u8 = 2;
}