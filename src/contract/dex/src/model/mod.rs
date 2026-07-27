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
//!
//! ## Trusted Setup with Money Contract
//!
//! The DEX contract requires a trusted setup with the money contract to verify
//! lock_proofs. This is a TEMPORARY WORKAROUND due to the absence of proper
//! cross-contract composable opcodes in the ZK circuit execution environment.
//!
//! ### The Problem
//!
//! Ideally, the DEX would:
//! 1. Call the money contract to verify lock_proofs
//! 2. Use cross-contract state proofs
//! 3. Have atomic composition of ZK proofs across contracts
//!
//! Without these opcodes, we rely on a TRUSTED Merkle root that is set during
//! initialization and assumed to be valid.
//!
//! ### Current Implementation
//!
//! - `InitializeParams.trusted_money_merkle_root`: Set during contract initialization
//! - This root is used to verify `lock_proof` in CreateSwap and AcceptSwap
//! - The root should be obtained from the money contract's current Merkle root
//!
//! ### Security Note
//!
//! This trusted setup is a SIGNIFICANT SECURITY TRADE-OFF:
//! - If the trusted root is wrong or outdated, an attacker could:
//!   - Create swaps with invalid lock_proofs
//!   - Claim swaps for funds they haven't locked
//!
//! Proper solution requires:
//! - Cross-contract ZK proof composition opcodes
//! - On-chain Merkle root verification
//! - Event-based state synchronization between contracts

use dwow_serial::{SerialDecodable, SerialEncodable};
use dwow_sdk::crypto::{IntentCommitment, IntentNullifier, PublicKey};
use dwow_sdk::error::ContractError;
use dwow_sdk::pasta::pallas;

/// Namespace for DEX intents (used with generic intent primitives)
pub const DEX_NAMESPACE: u64 = 0x0003;

/// Initialize contract parameters
///
/// # Trusted Setup
///
/// This includes a `trusted_money_merkle_root` which is used to verify lock_proofs.
/// See module-level documentation for security considerations.
///
/// # Transparency Configuration
///
/// The `transparency_config` determines what data is revealed at each level:
/// - Dark: Nothing revealed (MVP)
/// - Aggregate: Price ranges and volume bands only
/// - Anonymized: Anonymized trade data
/// - Full: Everything revealed
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct InitializeParams {
    /// Swap timeout in blocks
    pub timeout: u32,
    /// DEX fee (basis points)
    pub fee: u64,
    /// Trusted Merkle root of the money contract's coin tree
    ///
    /// # Security Warning
    ///
    /// This root is TRUSTED and not verified. It should match the money contract's
    /// current Merkle root at initialization time. If incorrect, the DEX cannot
    /// detect invalid lock_proofs.
    ///
    /// This is a workaround for lack of cross-contract ZK composition opcodes.
    pub trusted_money_merkle_root: [u8; 32],
    /// Transparency configuration for this DEX deployment
    pub transparency_config: TransparencyConfig,
}

/// Create swap proposal parameters
///
/// SECURITY NOTE: The prover MUST compute nullifier externally:
/// - nullifier = poseidon_hash([secret, lock_commitment])
///
/// This nullifier is passed in this struct to allow the contract to:
/// 1. Verify it as a public input to the ZK proof
/// 2. Track it for double-spend prevention
///
/// ## Signature Verification
///
/// The proposer signs the swap parameters using their secret key. The signature
/// is verified at the host level before contract execution. The ZK circuit
/// constrains the signature_public coordinates, binding the proof to the signer's
/// public key.
///
/// The signature commits to: swap_id || offer_token || offer_amount || request_token || request_amount
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

    /// Proposer's nullifier: poseidon_hash([secret, lock_commitment])
    /// MUST be computed by the prover before submitting
    pub nullifier: IntentNullifier,

    /// Merkle proof that lock commitment is valid
    pub lock_proof: Vec<[u8; 32]>,

    /// Proposer's public key for signature verification
    /// The ZK circuit constrains this public key's coordinates
    pub signature_public: PublicKey,

    /// Fee paid for swap creation
    pub fee: u64,

    /// If true, anyone can execute this swap after acceptance (no secret needed)
    /// WARNING: Reveals Alice's secret to the network. Use only with trusted acceptors.
    /// Default: false (standard atomic swap flow)
    pub open_execution: bool,
}

/// Accept swap parameters
///
/// SECURITY NOTE: The prover MUST compute nullifier externally:
/// - nullifier = poseidon_hash([secret, lock_commitment])
///
/// This nullifier is passed in this struct to allow the contract to:
/// 1. Verify it as a public input to the ZK proof
/// 2. Track it for double-spend prevention
///
/// ## Signature Verification
///
/// The acceptor signs the acceptance using their secret key. The signature
/// is verified at the host level before contract execution. The ZK circuit
/// constrains the signature_public coordinates, binding the proof to the signer's
/// public key.
///
/// The signature commits to: swap_id || lock_commitment
#[derive(Debug, Clone, SerialDecodable, SerialEncodable)]
pub struct AcceptSwapParams {
    /// Swap ID being accepted
    pub swap_id: [u8; 32],

    /// Commitment that Bob's funds are locked (uses generic PrivateIntent commitment)
    pub lock_commitment: IntentCommitment,

    /// Acceptor's nullifier: poseidon_hash([secret, lock_commitment])
    /// MUST be computed by the prover before submitting
    pub nullifier: IntentNullifier,

    /// Merkle proof that Bob's lock is valid
    pub lock_proof: Vec<[u8; 32]>,

    /// Acceptor's public key for signature verification
    /// The ZK circuit constrains this public key's coordinates
    pub signature_public: PublicKey,

    /// Fee paid for acceptance
    pub fee: u64,

    /// If true and swap has open_execution=true, immediately execute after acceptance.
    /// This enables "immediate fill" - Bob accepts and the swap executes in the same tx.
    /// Default: false
    pub immediate_execute: bool,
}

/// Execute swap parameters
///
/// SECURITY NOTE: The prover MUST compute nullifiers externally:
/// - alice_nullifier = poseidon_hash([alice_secret, alice_lock])
/// - bob_nullifier = poseidon_hash([bob_secret, bob_lock])
///
/// These nullifiers are passed in this struct to allow the contract to:
/// 1. Verify them as public inputs to the ZK proof
/// 2. Check them against on-chain state to prevent double-execution
///
/// SECURITY: The prover MUST also provide alice_lock and bob_lock which must
/// match the proposer's stored lock for the ZK proof to be valid.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteSwapParams {
    /// Swap ID to execute
    pub swap_id: [u8; 32],

    /// Prover's secret for Alice's lock
    pub alice_secret: [u8; 32],

    /// Prover's secret for Bob's lock
    pub bob_secret: [u8; 32],

    /// Alice's lock commitment (must match proposer's stored lock)
    /// This is verified by the ZK circuit
    pub alice_lock: IntentCommitment,

    /// Bob's lock commitment (must match acceptor's stored lock)
    /// This is verified by the ZK circuit
    pub bob_lock: IntentCommitment,

    /// Alice's nullifier: poseidon_hash([alice_secret, alice_lock])
    /// MUST be computed by the prover before submitting
    pub alice_nullifier: IntentNullifier,

    /// Bob's nullifier: poseidon_hash([bob_secret, bob_lock])
    /// MUST be computed by the prover before submitting
    pub bob_nullifier: IntentNullifier,

    /// ZK proof that swap is valid
    pub proof: Vec<u8>,

    /// Fee paid for execution
    pub fee: u64,
}

/// Cancel swap parameters
///
/// SECURITY NOTE: The prover MUST compute the nullifier externally:
/// - nullifier = poseidon_hash([secret, lock_commitment])
///
/// This nullifier is passed in this struct to allow the contract to:
/// 1. Verify it as a public input to the ZK proof
/// 2. Check it against on-chain state to prevent double-cancellation
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelSwapParams {
    /// Swap ID to cancel
    pub swap_id: [u8; 32],

    /// Secret to unlock the lock
    pub secret: [u8; 32],

    /// Nullifier: poseidon_hash([secret, lock_commitment])
    /// MUST be computed by the prover before submitting
    pub nullifier: IntentNullifier,

    /// ZK proof of ownership
    pub proof: Vec<u8>,

    /// Fee paid for cancellation
    pub fee: u64,
}

/// Execute swap with fee parameters
///
/// This allows executing a swap with a fee deducted from the received amount.
/// Fee calculation: fee = fill_amount * fee_bps / 10000
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteSwapFeeParams {
    /// Swap ID to execute
    pub swap_id: [u8; 32],

    /// Prover's secret for Alice's lock
    pub alice_secret: [u8; 32],

    /// Prover's secret for Bob's lock
    pub bob_secret: [u8; 32],

    /// Alice's lock commitment
    pub alice_lock: IntentCommitment,

    /// Bob's lock commitment
    pub bob_lock: IntentCommitment,

    /// Alice's nullifier
    pub alice_nullifier: IntentNullifier,

    /// Bob's nullifier
    pub bob_nullifier: IntentNullifier,

    /// Fee basis points (e.g., 30 = 0.3%)
    pub fee_bps: u64,

    /// ZK proof
    pub proof: Vec<u8>,

    /// Fee paid for execution
    pub fee: u64,
}

/// Execute swap with slippage tolerance parameters
///
/// This allows executing a swap where the received amount can vary
/// within a slippage tolerance.
/// Slippage tolerance: received >= min_expected * (1 - slippage_bps / 10000)
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ExecuteSwapSlippageParams {
    /// Swap ID to execute
    pub swap_id: [u8; 32],

    /// Prover's secret for Alice's lock
    pub alice_secret: [u8; 32],

    /// Prover's secret for Bob's lock
    pub bob_secret: [u8; 32],

    /// Alice's lock commitment
    pub alice_lock: IntentCommitment,

    /// Bob's lock commitment
    pub bob_lock: IntentCommitment,

    /// Alice's nullifier
    pub alice_nullifier: IntentNullifier,

    /// Bob's nullifier
    pub bob_nullifier: IntentNullifier,

    /// Slippage tolerance in basis points (e.g., 50 = 0.5%)
    pub slippage_bps: u64,

    /// ZK proof
    pub proof: Vec<u8>,

    /// Fee paid for execution
    pub fee: u64,
}

/// Update configuration parameters
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct UpdateConfigParams {
    /// New timeout (in blocks)
    pub timeout: u32,
    /// New fee (basis points)
    pub fee: u64,
    /// Governance public key X (ZK-verified)
    pub gov_pub_x: pallas::Base,
    /// Governance public key Y (ZK-verified)
    pub gov_pub_y: pallas::Base,
    /// Governance nullifier for ZK replay protection
    pub gov_nullifier: pallas::Base,
}

/// Set transparency level parameters
///
/// Allows governance to change transparency level post-deployment.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SetTransparencyLevelParams {
    /// New transparency level
    pub level: TransparencyLevel,
    /// Governance public key X (ZK-verified)
    pub gov_pub_x: pallas::Base,
    /// Governance public key Y (ZK-verified)
    pub gov_pub_y: pallas::Base,
    /// Governance nullifier for ZK replay protection
    pub gov_nullifier: pallas::Base,
}

/// Set full transparency configuration parameters
///
/// Allows governance to change transparency level AND parameters post-deployment.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct SetTransparencyConfigParams {
    /// New transparency configuration
    pub config: TransparencyConfig,
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

impl From<SwapState> for u8 {
    fn from(s: SwapState) -> u8 {
        match s {
            SwapState::Created => 0,
            SwapState::Accepted => 1,
            SwapState::Executed => 2,
            SwapState::Cancelled => 3,
        }
    }
}

impl TryFrom<u8> for SwapState {
    type Error = ContractError;

    fn try_from(v: u8) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(SwapState::Created),
            1 => Ok(SwapState::Accepted),
            2 => Ok(SwapState::Executed),
            3 => Ok(SwapState::Cancelled),
            _ => Err(ContractError::IoError(format!("Invalid SwapState variant: {}", v))),
        }
    }
}

/// Stored swap record
#[derive(Debug, Clone)]
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

    /// Proposer's nullifier for double-spend prevention
    pub proposer_nullifier: IntentNullifier,

    /// Acceptor's lock commitment (set when accepted)
    pub acceptor_lock: IntentCommitment,

    /// Acceptor's nullifier for double-spend prevention (set when accepted)
    pub acceptor_nullifier: IntentNullifier,

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

impl Swap {
    /// Exact encoded size: 11×[u8;32] + 4×u64 + bool + SwapState = 386 bytes.
    pub const ENCODED_SIZE: usize = 386;

    /// Rho-calculus deterministic encode.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.swap_id);
        b.extend_from_slice(&self.proposer_pub_x);
        b.extend_from_slice(&self.proposer_pub_y);
        b.extend_from_slice(&self.acceptor_pub_x);
        b.extend_from_slice(&self.acceptor_pub_y);
        b.extend_from_slice(&self.offer_token);
        b.extend_from_slice(&self.offer_amount.to_le_bytes());
        b.extend_from_slice(&self.request_token);
        b.extend_from_slice(&self.request_amount.to_le_bytes());
        b.extend_from_slice(&self.proposer_lock.to_bytes());
        b.extend_from_slice(&self.proposer_nullifier.to_bytes());
        b.extend_from_slice(&self.acceptor_lock.to_bytes());
        b.extend_from_slice(&self.acceptor_nullifier.to_bytes());
        b.push(u8::from(self.state.clone()));
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.expires_at.to_le_bytes());
        b.push(self.open_execution as u8);
        b
    }

    /// Rho-calculus deterministic decode.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "Swap::decode buffer too short: {} < {}",
                data.len(),
                Self::ENCODED_SIZE,
            )))
        }
        let d = &data[..Self::ENCODED_SIZE];

        let swap_id: [u8; 32] = d[0..32].try_into().unwrap();
        let proposer_pub_x: [u8; 32] = d[32..64].try_into().unwrap();
        let proposer_pub_y: [u8; 32] = d[64..96].try_into().unwrap();
        let acceptor_pub_x: [u8; 32] = d[96..128].try_into().unwrap();
        let acceptor_pub_y: [u8; 32] = d[128..160].try_into().unwrap();
        let offer_token: [u8; 32] = d[160..192].try_into().unwrap();
        let offer_amount = u64::from_le_bytes(d[192..200].try_into().unwrap());
        let request_token: [u8; 32] = d[200..232].try_into().unwrap();
        let request_amount = u64::from_le_bytes(d[232..240].try_into().unwrap());

        let proposer_lock = IntentCommitment::from_bytes(d[240..272].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Swap::decode proposer_lock: {}", e)))?;
        let proposer_nullifier = IntentNullifier::from_bytes(d[272..304].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Swap::decode proposer_nullifier: {}", e)))?;
        let acceptor_lock = IntentCommitment::from_bytes(d[304..336].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Swap::decode acceptor_lock: {}", e)))?;
        let acceptor_nullifier = IntentNullifier::from_bytes(d[336..368].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("Swap::decode acceptor_nullifier: {}", e)))?;

        let state = SwapState::try_from(d[368])?;
        let created_at = u64::from_le_bytes(d[369..377].try_into().unwrap());
        let expires_at = u64::from_le_bytes(d[377..385].try_into().unwrap());
        let open_execution = d[385] != 0;

        Ok(Self {
            swap_id,
            proposer_pub_x,
            proposer_pub_y,
            acceptor_pub_x,
            acceptor_pub_y,
            offer_token,
            offer_amount,
            request_token,
            request_amount,
            proposer_lock,
            proposer_nullifier,
            acceptor_lock,
            acceptor_nullifier,
            state,
            created_at,
            expires_at,
            open_execution,
        })
    }
}

// ============================================================================
// UPDATE STRUCTS (for state transitions)
// ============================================================================

/// Update struct for CreateSwapV1
#[derive(Debug, Clone)]
pub struct CreateSwapUpdateV1 {
    /// The swap ID
    pub swap_id: [u8; 32],
    /// Proposer's public key x
    pub proposer_pub_x: [u8; 32],
    /// Proposer's public key y
    pub proposer_pub_y: [u8; 32],
    /// Token being offered
    pub offer_token: [u8; 32],
    /// Amount being offered
    pub offer_amount: u64,
    /// Token being requested
    pub request_token: [u8; 32],
    /// Amount being requested
    pub request_amount: u64,
    /// Proposer's lock commitment
    pub proposer_lock: IntentCommitment,
    /// Proposer's nullifier for double-spend prevention
    pub proposer_nullifier: IntentNullifier,
    /// Creation timestamp
    pub created_at: u64,
    /// Expiration timestamp
    pub expires_at: u64,
    /// Open execution flag
    pub open_execution: bool,
}

impl CreateSwapUpdateV1 {
    /// Exact encoded size: 8×[u8;32] + 4×u64 + bool = 289 bytes.
    pub const ENCODED_SIZE: usize = 289;

    /// Rho-calculus deterministic encode.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.swap_id);
        b.extend_from_slice(&self.proposer_pub_x);
        b.extend_from_slice(&self.proposer_pub_y);
        b.extend_from_slice(&self.offer_token);
        b.extend_from_slice(&self.offer_amount.to_le_bytes());
        b.extend_from_slice(&self.request_token);
        b.extend_from_slice(&self.request_amount.to_le_bytes());
        b.extend_from_slice(&self.proposer_lock.to_bytes());
        b.extend_from_slice(&self.proposer_nullifier.to_bytes());
        b.extend_from_slice(&self.created_at.to_le_bytes());
        b.extend_from_slice(&self.expires_at.to_le_bytes());
        b.push(self.open_execution as u8);
        b
    }

    /// Rho-calculus deterministic decode.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "CreateSwapUpdateV1::decode buffer too short: {} < {}",
                data.len(),
                Self::ENCODED_SIZE,
            )))
        }
        let d = &data[..Self::ENCODED_SIZE];

        let swap_id: [u8; 32] = d[0..32].try_into().unwrap();
        let proposer_pub_x: [u8; 32] = d[32..64].try_into().unwrap();
        let proposer_pub_y: [u8; 32] = d[64..96].try_into().unwrap();
        let offer_token: [u8; 32] = d[96..128].try_into().unwrap();
        let offer_amount = u64::from_le_bytes(d[128..136].try_into().unwrap());
        let request_token: [u8; 32] = d[136..168].try_into().unwrap();
        let request_amount = u64::from_le_bytes(d[168..176].try_into().unwrap());

        let proposer_lock = IntentCommitment::from_bytes(d[176..208].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("CreateSwapUpdateV1::decode proposer_lock: {}", e)))?;
        let proposer_nullifier = IntentNullifier::from_bytes(d[208..240].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("CreateSwapUpdateV1::decode proposer_nullifier: {}", e)))?;

        let created_at = u64::from_le_bytes(d[240..248].try_into().unwrap());
        let expires_at = u64::from_le_bytes(d[248..256].try_into().unwrap());
        let open_execution = d[256] != 0;

        Ok(Self {
            swap_id,
            proposer_pub_x,
            proposer_pub_y,
            offer_token,
            offer_amount,
            request_token,
            request_amount,
            proposer_lock,
            proposer_nullifier,
            created_at,
            expires_at,
            open_execution,
        })
    }
}

/// Update struct for AcceptSwapV1
#[derive(Debug, Clone)]
pub struct AcceptSwapUpdateV1 {
    /// The swap ID
    pub swap_id: [u8; 32],
    /// Acceptor's public key x
    pub acceptor_pub_x: [u8; 32],
    /// Acceptor's public key y
    pub acceptor_pub_y: [u8; 32],
    /// Acceptor's lock commitment
    pub acceptor_lock: IntentCommitment,
    /// Acceptor's nullifier for double-spend prevention
    pub acceptor_nullifier: IntentNullifier,
}

impl AcceptSwapUpdateV1 {
    /// Exact encoded size: 5×[u8;32] = 160 bytes.
    pub const ENCODED_SIZE: usize = 160;

    /// Rho-calculus deterministic encode.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.swap_id);
        b.extend_from_slice(&self.acceptor_pub_x);
        b.extend_from_slice(&self.acceptor_pub_y);
        b.extend_from_slice(&self.acceptor_lock.to_bytes());
        b.extend_from_slice(&self.acceptor_nullifier.to_bytes());
        b
    }

    /// Rho-calculus deterministic decode.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "AcceptSwapUpdateV1::decode buffer too short: {} < {}",
                data.len(),
                Self::ENCODED_SIZE,
            )))
        }
        let d = &data[..Self::ENCODED_SIZE];

        let swap_id: [u8; 32] = d[0..32].try_into().unwrap();
        let acceptor_pub_x: [u8; 32] = d[32..64].try_into().unwrap();
        let acceptor_pub_y: [u8; 32] = d[64..96].try_into().unwrap();
        let acceptor_lock = IntentCommitment::from_bytes(d[96..128].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("AcceptSwapUpdateV1::decode acceptor_lock: {}", e)))?;
        let acceptor_nullifier = IntentNullifier::from_bytes(d[128..160].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("AcceptSwapUpdateV1::decode acceptor_nullifier: {}", e)))?;

        Ok(Self { swap_id, acceptor_pub_x, acceptor_pub_y, acceptor_lock, acceptor_nullifier })
    }
}

/// Update struct for ExecuteSwapV1
#[derive(Debug, Clone)]
pub struct ExecuteSwapUpdateV1 {
    /// The swap ID
    pub swap_id: [u8; 32],
    /// Alice's nullifier
    pub alice_nullifier: IntentNullifier,
    /// Bob's nullifier
    pub bob_nullifier: IntentNullifier,
}

impl ExecuteSwapUpdateV1 {
    /// Exact encoded size: 3×[u8;32] = 96 bytes.
    pub const ENCODED_SIZE: usize = 96;

    /// Rho-calculus deterministic encode.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.swap_id);
        b.extend_from_slice(&self.alice_nullifier.to_bytes());
        b.extend_from_slice(&self.bob_nullifier.to_bytes());
        b
    }

    /// Rho-calculus deterministic decode.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "ExecuteSwapUpdateV1::decode buffer too short: {} < {}",
                data.len(),
                Self::ENCODED_SIZE,
            )))
        }
        let d = &data[..Self::ENCODED_SIZE];

        let swap_id: [u8; 32] = d[0..32].try_into().unwrap();
        let alice_nullifier = IntentNullifier::from_bytes(d[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("ExecuteSwapUpdateV1::decode alice_nullifier: {}", e)))?;
        let bob_nullifier = IntentNullifier::from_bytes(d[64..96].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("ExecuteSwapUpdateV1::decode bob_nullifier: {}", e)))?;

        Ok(Self { swap_id, alice_nullifier, bob_nullifier })
    }
}

/// Update struct for CancelSwapV1
#[derive(Debug, Clone)]
pub struct CancelSwapUpdateV1 {
    /// The swap ID
    pub swap_id: [u8; 32],
    /// Nullifier for the cancelled lock
    pub nullifier: IntentNullifier,
    /// Whether the proposer cancelled (true) or acceptor (false)
    pub is_proposer: bool,
}

impl CancelSwapUpdateV1 {
    /// Exact encoded size: 2×[u8;32] + bool = 65 bytes.
    pub const ENCODED_SIZE: usize = 65;

    /// Rho-calculus deterministic encode.
    pub fn encode(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(Self::ENCODED_SIZE);
        b.extend_from_slice(&self.swap_id);
        b.extend_from_slice(&self.nullifier.to_bytes());
        b.push(self.is_proposer as u8);
        b
    }

    /// Rho-calculus deterministic decode.
    pub fn decode(data: &[u8]) -> Result<Self, ContractError> {
        if data.len() < Self::ENCODED_SIZE {
            return Err(ContractError::IoError(format!(
                "CancelSwapUpdateV1::decode buffer too short: {} < {}",
                data.len(),
                Self::ENCODED_SIZE,
            )))
        }
        let d = &data[..Self::ENCODED_SIZE];

        let swap_id: [u8; 32] = d[0..32].try_into().unwrap();
        let nullifier = IntentNullifier::from_bytes(d[32..64].try_into().unwrap())
            .map_err(|e| ContractError::IoError(format!("CancelSwapUpdateV1::decode nullifier: {}", e)))?;
        let is_proposer = d[64] != 0;

        Ok(Self { swap_id, nullifier, is_proposer })
    }
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

// ============================================================================
// TRANSPARENCY CONFIGURATION
// ============================================================================

/// Transparency levels for the DEX
///
/// Different DEX deployments serve different users with different
/// privacy/compliance needs. The transparency level is set at
/// deployment time and can be adjusted by governance post-deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub enum TransparencyLevel {
    /// Level 0: Complete darkness - no amounts, parties, or trade data revealed
    Dark = 0,
    /// Level 1: Aggregate market data only - price ranges, volume bands
    Aggregate = 1,
    /// Level 2: Anonymized trades - unlinkable trade data
    Anonymized = 2,
    /// Level 3: Full transparency - opt-in full disclosure
    Full = 3,
}

/// Default price band size for Aggregate level (in token units)
/// e.g., 100 means prices are bucketed into $100 bands
const DEFAULT_PRICE_BAND_SIZE: u64 = 100;

/// Default volume bucket size for Aggregate level (in token units)
/// e.g., 1000 means volume is bucketed into 1000-token buckets
const DEFAULT_VOLUME_BUCKET_SIZE: u64 = 1000;

/// Default anonymity group size for Anonymized level
/// e.g., 10 means trades are grouped in batches of 10
const DEFAULT_ANONYMITY_GROUP_SIZE: u64 = 10;

/// Transparency configuration for the DEX
///
/// This config determines what data is emitted in events and what
/// circuit capabilities are enabled at each transparency level.
#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct TransparencyConfig {
    /// Current transparency level
    pub level: TransparencyLevel,
    /// Price band size for Aggregate level (e.g., 100 = $100 bands)
    pub price_band_size: u64,
    /// Volume bucket size for Aggregate level (e.g., 1000 = 1000 token buckets)
    pub volume_bucket_size: u64,
    /// Anonymity group size for Anonymized level (e.g., 10 = groups of 10)
    pub anonymity_group_size: u64,
}

impl Default for TransparencyConfig {
    fn default() -> Self {
        Self {
            level: TransparencyLevel::Dark,
            price_band_size: DEFAULT_PRICE_BAND_SIZE,
            volume_bucket_size: DEFAULT_VOLUME_BUCKET_SIZE,
            anonymity_group_size: DEFAULT_ANONYMITY_GROUP_SIZE,
        }
    }
}