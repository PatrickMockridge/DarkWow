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

//! Data structures for bridge contract calls
//!
//! Security Model: Object Capability Security (No VSS)
//!
//! Unlike VSS-based bridges, this design uses deterministic address derivation:
//! - Bridge address = H(recipient_identity, nonce)
//! - No secret sharing between bridge nodes
//! - User alone controls withdrawal via their secret

use darkfi_serial::{SerialDecodable, SerialEncodable};

/// External chain identifier
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub enum ExternalChain {
    Ethereum,
    // Future chains can be added here
    // Bitcoin,
    // Aztec,
}

/// Bridge deposit parameters
///
/// Security: Deposit creates a commitment H(secret, amount, bridge_address).
/// Only the depositor knows `secret`, so only they can later withdraw.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct DepositParams {
    /// Commitment hash from user's secret
    /// commitment = H(secret, amount, bridge_address)
    pub commitment: [u8; 32],

    /// Recipient public key for address derivation
    /// Used to compute: bridge_address = H(H(secret)*G, recipient_pub)
    pub recipient_pub_x: [u8; 32],
    pub recipient_pub_y: [u8; 32],

    /// Nonce ensures fresh address per deposit (temporal privacy)
    pub bridge_nonce: u64,

    /// The external chain where the deposit was made
    pub chain: ExternalChain,

    /// Hash of the external block containing the deposit
    pub external_block_hash: [u8; 32],

    /// Merkle proof of deposit inclusion in external chain
    pub merkle_proof: Vec<[u8; 32]>,

    /// Merkle root of external chain state at block
    pub external_state_root: [u8; 32],

    /// Bridge fee paid by depositor
    pub fee: u64,

    /// ZK proof demonstrating:
    /// 1. Knowledge of secret
    /// 2. Deposit exists in external chain
    /// 3. Commitment is correctly computed
    pub proof: Vec<u8>,
}

/// Bridge withdrawal parameters
///
/// Security: Withdrawal is authorized by the depositor alone via their secret.
/// No VSS/threshold signing required.
///
/// Nullifier = H(secret) proves deposit ownership without revealing secret.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct WithdrawParams {
    /// Nullifier = H(secret) - proves deposit exists and hasn't been withdrawn
    /// Double-spend prevention without revealing which deposit
    pub nullifier: [u8; 32],

    /// Recipient address hash on external chain
    /// Hash of actual address for privacy
    pub recipient_hash: [u8; 32],

    /// Amount to withdraw
    pub amount: u64,

    /// ZK proof demonstrating:
    /// 1. Knowledge of secret corresponding to a registered deposit
    /// 2. Deposit is in the bridge's Merkle tree
    /// 3. Amount is valid (<= deposited amount)
    /// 4. Recipient hash matches
    pub proof: Vec<u8>,

    /// Bridge fee paid by withdrawer
    pub fee: u64,
}

/// Bridge configuration update parameters
///
/// Security: Only callable by authorized governance (DAO).
/// This doesn't affect user funds - only operational parameters.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParams {
    /// New deposit fee
    pub deposit_fee: u64,

    /// New withdrawal fee
    pub withdrawal_fee: u64,

    /// Minimum confirmations required on external chain
    pub min_confirmations: u32,

    /// Maximum deposit amount (anti-money laundering)
    pub max_deposit: u64,

    /// Maximum withdrawal amount
    pub max_withdrawal: u64,
}

/// Stored deposit record
///
/// This record tracks deposits registered in the bridge.
/// The actual proof of deposit ownership is via the commitment
/// which requires knowledge of secret to claim.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Deposit {
    /// Commitment hash
    pub commitment: [u8; 32],

    /// Amount deposited
    pub amount: u64,

    /// External chain of origin
    pub chain: ExternalChain,

    /// Block height on external chain
    pub external_height: u64,

    /// Whether deposit has been claimed (withdrawn)
    pub claimed: bool,

    /// Timestamp of registration
    pub registered_at: u64,
}

/// Stored withdrawal record
///
/// Records successful withdrawals for audit trail.
/// Note: Withdrawal doesn't reveal which deposit was withdrawn,
/// only that some deposit was spent.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct Withdrawal {
    /// Nullifier (proves deposit was spent)
    pub nullifier: [u8; 32],

    /// Recipient on external chain (hashed)
    pub recipient_hash: [u8; 32],

    /// Amount withdrawn
    pub amount: u64,

    /// Whether withdrawal has been executed on external chain
    pub executed: bool,

    /// Transaction hash on external chain (if executed)
    pub external_tx_hash: Option<[u8; 32]>,

    /// Timestamp of withdrawal
    pub withdrawn_at: u64,
}

// ================================================================
// OBJECT CAPABILITY SECURITY MODEL
// ================================================================
//
// Capability Derivation (No VSS):
//
//   bridge_secret = poseidon_hash(recipient_pub_x, recipient_pub_y, bridge_nonce)
//   bridge_pub = bridge_secret * G
//   bridge_address = poseidon_hash(ec_get_x(bridge_pub), ec_get_y(bridge_pub))
//
// Deposit Authorization:
//
//   commitment = poseidon_hash(secret, amount, bridge_address)
//
// Withdrawal Authorization:
//
//   nullifier = poseidon_hash(secret)
//
// The bridge contract never sees bridge_secret. Only the user knows it.
// To withdraw, user proves knowledge of secret via ZK proof.
//
// Security Properties:
//
// 1. Bridge nodes cannot steal funds (no VSS shards)
// 2. User alone authorizes withdrawals (no threshold)
// 3. Fresh addresses per deposit (temporal privacy)
// 4. Double-spend prevention via nullifiers
//
// ================================================================
