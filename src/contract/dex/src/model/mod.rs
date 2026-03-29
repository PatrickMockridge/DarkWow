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

//! Data structures for DEX atomic swap contract
//!
//! ## Atomic Swap Flow
//!
//! ```text
//! 1. Alice creates swap: lock(A_token, amount_A, B_token, amount_B)
//!    → swap_id = H(lock_proof, params)
//!
//! 2. Bob accepts swap: lock(B_token, amount_B, swap_id)
//!    → swap marked as "accepted"
//!
//! 3. Execute: verify both locks valid
//!    → Atomic: Alice gets B_token, Bob gets A_token
//!
//! 4. OR Cancel/Timeout: refund both
//! ```

use darkfi_serial::{SerialDecodable, SerialEncodable};
use darkfi_sdk::crypto::{IntentCommitment, IntentNullifier};

/// Namespace for DEX intents (used with generic intent primitives)
pub const DEX_NAMESPACE: u64 = 0x0003;

/// Initialize contract parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    /// Swap timeout in blocks
    pub timeout: u32,
    /// DEX fee (basis points)
    pub fee: u64,
}

/// Create swap proposal parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateSwapParams {
    /// Swap ID (computed from commitment)
    pub swap_id: [u8; 32],

    /// Token Alice is offering
    pub offer_token: [u8; 32],

    /// Amount Alice is offering
    pub offer_amount: u64,

    /// Token Alice wants in return
    pub request_token: [u8; 32],

    /// Amount Alice wants in return
    pub request_amount: u64,

    /// Commitment that Alice's funds are locked (uses generic PrivateIntent commitment)
    /// lock_commitment = poseidon_hash([9001, owner_x, owner_y, namespace, payload_hash, expiry, nonce, blind])
    pub lock_commitment: IntentCommitment,

    /// Merkle proof that lock commitment is valid
    pub lock_proof: Vec<[u8; 32]>,

    /// Signature authorizing swap creation
    pub signature: Vec<u8>,

    /// Fee paid for swap creation
    pub fee: u64,

    /// If true, anyone can execute this swap after acceptance (no secret needed)
    /// WARNING: Reveals Alice's secret to the network. Use only with trusted acceptors.
    /// Default: false (standard atomic swap flow)
    pub open_execution: bool,
}

/// Accept swap parameters
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct AcceptSwapParams {
    /// Swap ID being accepted
    pub swap_id: [u8; 32],

    /// Commitment that Bob's funds are locked (uses generic PrivateIntent commitment)
    pub lock_commitment: IntentCommitment,

    /// Merkle proof that Bob's lock is valid
    pub lock_proof: Vec<[u8; 32]>,

    /// Signature authorizing acceptance
    pub signature: Vec<u8>,

    /// Fee paid for acceptance
    pub fee: u64,

    /// If true and swap has open_execution=true, immediately execute after acceptance.
    /// This enables "immediate fill" - Bob accepts and the swap executes in the same tx.
    /// Default: false
    pub immediate_execute: bool,
}

/// Execute swap parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteSwapParams {
    /// Swap ID to execute
    pub swap_id: [u8; 32],

    /// Prover's secret for Alice's lock
    pub alice_secret: [u8; 32],

    /// Prover's secret for Bob's lock
    pub bob_secret: [u8; 32],

    /// ZK proof that swap is valid
    pub proof: Vec<u8>,

    /// Fee paid for execution
    pub fee: u64,
}

/// Cancel swap parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelSwapParams {
    /// Swap ID to cancel
    pub swap_id: [u8; 32],

    /// Secret to unlock the lock
    pub secret: [u8; 32],

    /// ZK proof of ownership
    pub proof: Vec<u8>,

    /// Fee paid for cancellation
    pub fee: u64,
}

/// Update configuration parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParams {
    /// New timeout (in blocks)
    pub timeout: u32,
    /// New fee (basis points)
    pub fee: u64,
}

/// Swap state
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum SwapState {
    /// Swap created, waiting for acceptor
    Created,
    /// Acceptor has locked funds
    Accepted,
    /// Swap executed successfully
    Executed,
    /// Swap cancelled or timed out
    Cancelled,
}

/// Stored swap record
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Swap {
    /// Unique swap ID
    pub swap_id: [u8; 32],

    /// Proposer's public key
    pub proposer_pub_x: [u8; 32],
    pub proposer_pub_y: [u8; 32],

    /// Acceptor's public key (set when accepted)
    pub acceptor_pub_x: [u8; 32],
    pub acceptor_pub_y: [u8; 32],

    /// Swap details
    pub offer_token: [u8; 32],
    pub offer_amount: u64,
    pub request_token: [u8; 32],
    pub request_amount: u64,

    /// Proposer's lock commitment (uses generic PrivateIntent commitment)
    pub proposer_lock: IntentCommitment,

    /// Acceptor's lock commitment (set when accepted)
    pub acceptor_lock: IntentCommitment,

    /// Current state
    pub state: SwapState,

    /// Creation timestamp
    pub created_at: u64,

    /// Expiration timestamp
    pub expires_at: u64,

    /// If true, anyone can execute this swap (no Alice secret needed)
    /// Set by Alice at swap creation time
    pub open_execution: bool,
}

// ============================================================================
// COMMITMENTS AND NULLIFIERS
// ============================================================================
//
// Lock Commitment:
//   lock_commitment = H(secret, token, amount)
//
// Swap ID:
//   swap_id = H(proposer_lock, request_token, request_amount, nonce)
//
// Nullifier (for cancellation):
//   nullifier = H(secret)
//
// ============================================================================