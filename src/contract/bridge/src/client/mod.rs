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

//! Client API for bridge contract interaction
//!
//! This module provides transaction builders for the bridge contract.
//! It shows how users construct bridge deposits and withdrawals.
//!
//! ## How Deposits Work
//!
//! 1. User derives bridge_address = H(recipient_identity, nonce)
//! 2. User deposits funds to bridge_address on external chain
//! 3. User waits for confirmations
//! 4. User constructs ZK proof demonstrating:
//!    - Deposit exists on external chain (Merkle proof)
//!    - User knows secret
//!    - Commitment is correctly formed
//! 5. User submits DepositV1 to DarkWow
//! 6. DarkWow verifies proof, provides note from pool
//!
//! ## How Withdrawals Work
//!
//! 1. User computes nullifier = H(secret)
//! 2. User burns tokens on DarkWow
//! 3. User constructs ZK proof demonstrating:
//!    - User knows secret for a deposited commitment
//!    - Commitment is in bridge's Merkle tree
//!    - Amount is valid
//! 4. User submits WithdrawV1 to DarkWow
//! 5. DarkWow verifies proof, marks nullifier spent
//! 6. Relayer broadcasts withdrawal to external chain

pub mod zkbins;

use dwow_sdk::error::ContractError;

// ============================================================================
// ZK Proof Generation Modules
// ============================================================================

pub mod deposit;
pub mod withdraw;
pub mod ltc_deposit;
pub mod xmr_deposit;
pub mod azt_deposit;
pub mod zec_deposit;

/// Bridge client errors
#[derive(Debug, thiserror::Error)]
pub enum BridgeClientError {
    #[error("Invalid deposit proof: {0}")]
    InvalidDepositProof(String),

    #[error("Invalid withdrawal proof: {0}")]
    InvalidWithdrawalProof(String),

    #[error("Merkle proof verification failed")]
    MerkleProofFailed,

    #[error("No VSS - using Object Capability model")]
    OCapError(String),

    #[error("No bridge operators available")]
    NoOperatorsAvailable,

    #[error("{0}")]
    ContractError(String),
}

impl From<ContractError> for BridgeClientError {
    fn from(e: ContractError) -> Self {
        BridgeClientError::ContractError(format!("{:?}", e))
    }
}

// ============================================================================
// DEPOSIT BUILDER
// ============================================================================

/// DepositBuilder constructs a bridge deposit transaction
///
/// # Example: How to Bridge ETH to DarkWow
///
/// ```ignore
/// // 1. Derive bridge address for recipient
/// let recipient_pub = user.public_key();
/// let nonce = generate_fresh_nonce();
/// let bridge_address = derive_bridge_address(recipient_pub, nonce);
///
/// // 2. User deposits ETH to bridge_address on Ethereum
/// //    This is done via external wallet/interface
///
/// // 3. Wait for confirmations, get Merkle proof from indexer
/// let merkle_proof = indexer.get_deposit_proof(tx_hash).await?;
///
/// // 4. Build the deposit transaction
/// let deposit = DepositBuilder::new()
///     .secret(secret)
///     .amount(eth_amount)
///     .recipient_pub(recipient_pub)
///     .nonce(nonce)
///     .merkle_proof(merkle_proof)
///     .external_block_hash(block_hash)
///     .build()?;
///
/// // 5. Submit to DarkWow bridge contract
/// client.submit(deposit).await?;
/// ```
pub struct DepositBuilder {
    /// User's secret for the deposit
    secret: Option<[u8; 32]>,
    /// Amount being deposited
    amount: Option<u64>,
    /// Recipient's public key on DarkWow
    recipient_pub_x: Option<[u8; 32]>,
    recipient_pub_y: Option<[u8; 32]>,
    /// Nonce for temporal privacy (fresh address per deposit)
    bridge_nonce: Option<u64>,
    /// External chain being bridged from
    chain: Option<u8>,
    /// Merkle proof of deposit on external chain
    merkle_proof: Option<Vec<[u8; 32]>>,
    /// Block hash containing the deposit
    external_block_hash: Option<[u8; 32]>,
    /// State root of external chain at that block
    external_state_root: Option<[u8; 32]>,
    /// Fee for the bridge service
    fee: Option<u64>,
}

impl DepositBuilder {
    /// Create a new deposit builder
    pub fn new() -> Self {
        Self {
            secret: None,
            amount: None,
            recipient_pub_x: None,
            recipient_pub_y: None,
            bridge_nonce: None,
            chain: None,
            merkle_proof: None,
            external_block_hash: None,
            external_state_root: None,
            fee: None,
        }
    }

    /// Set the secret (known only to user)
    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Set the amount to deposit
    pub fn amount(&mut self, amount: u64) -> &mut Self {
        self.amount = Some(amount);
        self
    }

    /// Set the recipient public key on DarkWow
    pub fn recipient_pub(&mut self, pub_x: [u8; 32], pub_y: [u8; 32]) -> &mut Self {
        self.recipient_pub_x = Some(pub_x);
        self.recipient_pub_y = Some(pub_y);
        self
    }

    /// Set the bridge nonce (for temporal privacy)
    pub fn nonce(&mut self, nonce: u64) -> &mut Self {
        self.bridge_nonce = Some(nonce);
        self
    }

    /// Set the external chain
    pub fn chain(&mut self, chain: u8) -> &mut Self {
        self.chain = Some(chain);
        self
    }

    /// Set the Merkle proof from external chain indexer
    pub fn merkle_proof(&mut self, proof: Vec<[u8; 32]>) -> &mut Self {
        self.merkle_proof = Some(proof);
        self
    }

    /// Set the external block hash
    pub fn external_block_hash(&mut self, hash: [u8; 32]) -> &mut Self {
        self.external_block_hash = Some(hash);
        self
    }

    /// Set the external state root
    pub fn external_state_root(&mut self, root: [u8; 32]) -> &mut Self {
        self.external_state_root = Some(root);
        self
    }

    /// Set the bridge fee
    pub fn fee(&mut self, fee: u64) -> &mut Self {
        self.fee = Some(fee);
        self
    }

    /// Build the deposit call data
    ///
    /// This constructs:
    /// 1. bridge_address = H(recipient_pub, nonce)
    /// 2. commitment = H(secret, amount, bridge_address)
    /// 3. ZK proof that proves deposit exists and commitment is valid
    /// 4. Encoded call data for the bridge contract
    pub fn build(&self) -> Result<Vec<u8>, BridgeClientError> {
        // Validate all required fields
        let secret = self.secret.ok_or_else(|| BridgeClientError::InvalidDepositProof("secret required".into()))?;
        let amount = self.amount.ok_or_else(|| BridgeClientError::InvalidDepositProof("amount required".into()))?;
        let recipient_pub_x = self.recipient_pub_x.ok_or_else(|| BridgeClientError::InvalidDepositProof("recipient_pub_x required".into()))?;
        let recipient_pub_y = self.recipient_pub_y.ok_or_else(|| BridgeClientError::InvalidDepositProof("recipient_pub_y required".into()))?;
        let nonce = self.bridge_nonce.ok_or_else(|| BridgeClientError::InvalidDepositProof("nonce required".into()))?;
        let chain = self.chain.ok_or_else(|| BridgeClientError::InvalidDepositProof("chain required".into()))?;
        let merkle_proof = self.merkle_proof.clone().ok_or_else(|| BridgeClientError::InvalidDepositProof("merkle_proof required".into()))?;
        let block_hash = self.external_block_hash.ok_or_else(|| BridgeClientError::InvalidDepositProof("external_block_hash required".into()))?;
        let state_root = self.external_state_root.ok_or_else(|| BridgeClientError::InvalidDepositProof("external_state_root required".into()))?;
        let fee = self.fee.unwrap_or(1000); // Default fee

        // Step 1: Derive bridge_address
        // bridge_secret = poseidon_hash(recipient_pub_x, recipient_pub_y, nonce)
        // bridge_pub = bridge_secret * G
        // bridge_address = poseidon_hash(bridge_pub.x, bridge_pub.y)
        let bridge_address = derive_bridge_address(recipient_pub_x, recipient_pub_y, nonce);

        // Step 2: Compute commitment
        // commitment = H(secret, amount, bridge_address)
        let commitment = compute_commitment(secret, amount, bridge_address);

        // Step 3: Generate ZK proof
        // The proof demonstrates:
        // - User knows secret
        // - Deposit exists in external chain (Merkle proof verified)
        // - commitment = H(secret, amount, bridge_address)
        //
        // In production: call the zkas proving system
        // let proof = generate_deposit_proof(secret, amount, bridge_address, merkle_proof)?;
        let proof = vec![0u8; 64]; // Placeholder for ZK proof

        // Step 4: Encode call data
        // The bridge contract expects:
        // [function_id, commitment, recipient_pub_x, recipient_pub_y, nonce, chain, block_hash, merkle_proof, state_root, fee, proof]
        let mut call_data = Vec::new();
        call_data.push(0x01); // DepositV1 function ID
        call_data.extend_from_slice(&commitment);
        call_data.extend_from_slice(&recipient_pub_x);
        call_data.extend_from_slice(&recipient_pub_y);
        call_data.extend_from_slice(&nonce.to_le_bytes());
        call_data.push(chain);
        call_data.extend_from_slice(&block_hash);
        // Merkle proof length and data
        call_data.extend_from_slice(&(merkle_proof.len() as u32).to_le_bytes());
        for proof_element in &merkle_proof {
            call_data.extend_from_slice(proof_element);
        }
        call_data.extend_from_slice(&state_root);
        call_data.extend_from_slice(&fee.to_le_bytes());
        // Proof length and data
        call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&proof);

        Ok(call_data)
    }
}

/// Derive bridge address from recipient identity and nonce
fn derive_bridge_address(recipient_pub_x: [u8; 32], recipient_pub_y: [u8; 32], nonce: u64) -> [u8; 32] {
    use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::{crypto::{pasta_prelude::PrimeField}, pasta::pallas};

    // Derive bridge_secret = poseidon_hash(recipient_pub_x, recipient_pub_y, nonce)
    // Using poseidon ensures ZK-friendly derivation
    let recipient_x = pallas::Base::from_repr(recipient_pub_x.into()).unwrap();
    let recipient_y = pallas::Base::from_repr(recipient_pub_y.into()).unwrap();
    let bridge_secret = poseidon_hash([recipient_x, recipient_y, pallas::Base::from(nonce)]);

    // Return poseidon hash of the secret as the bridge address
    let address_hash = poseidon_hash([bridge_secret]);
    address_hash.to_repr()
}

/// Compute commitment from secret, amount, and bridge address
fn compute_commitment(secret: [u8; 32], amount: u64, bridge_address: [u8; 32]) -> [u8; 32] {
    use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::{crypto::{pasta_prelude::PrimeField}, pasta::pallas};

    // commitment = poseidon_hash(secret, amount, bridge_address)
    // Using poseidon ensures ZK-friendly derivation
    let secret_base = pallas::Base::from_repr(secret.into()).unwrap();
    let addr_base = pallas::Base::from_repr(bridge_address.into()).unwrap();
    let commitment = poseidon_hash([secret_base, pallas::Base::from(amount), addr_base]);
    commitment.to_repr()
}

// ============================================================================
// WITHDRAWAL BUILDER
// ============================================================================

/// WithdrawBuilder constructs a bridge withdrawal transaction
///
/// # Example: How to Withdraw from DarkWow to Ethereum
///
/// ```ignore
/// // 1. User has a note from a previous deposit
/// let note = user.get_bridged_note();
///
/// // 2. Compute nullifier = H(secret)
/// let nullifier = compute_nullifier(note.secret);
///
/// // 3. Determine recipient on Ethereum
/// let recipient_hash = hash(ethereum_address);
///
/// // 4. Build withdrawal
/// let withdrawal = WithdrawBuilder::new()
///     .nullifier(nullifier)
///     .recipient_hash(recipient_hash)
///     .amount(withdraw_amount)
///     .build()?;
///
/// // 5. Submit to DarkWow bridge contract
/// client.submit(withdrawal).await?;
///
/// // 6. Relayer sees event, broadcasts ETH tx to Ethereum
/// ```
pub struct WithdrawBuilder {
    /// Nullifier = H(secret) - proves ownership without revealing secret
    nullifier: Option<[u8; 32]>,
    /// Recipient address hash on external chain
    recipient_hash: Option<[u8; 32]>,
    /// Amount to withdraw
    amount: Option<u64>,
    /// Fee for the bridge service
    fee: Option<u64>,
}

impl WithdrawBuilder {
    /// Create a new withdrawal builder
    pub fn new() -> Self {
        Self {
            nullifier: None,
            recipient_hash: None,
            amount: None,
            fee: None,
        }
    }

    /// Set the nullifier (computed from deposit secret)
    pub fn nullifier(&mut self, nullifier: [u8; 32]) -> &mut Self {
        self.nullifier = Some(nullifier);
        self
    }

    /// Set the recipient address hash on external chain
    pub fn recipient_hash(&mut self, hash: [u8; 32]) -> &mut Self {
        self.recipient_hash = Some(hash);
        self
    }

    /// Set the amount to withdraw
    pub fn amount(&mut self, amount: u64) -> &mut Self {
        self.amount = Some(amount);
        self
    }

    /// Set the bridge fee
    pub fn fee(&mut self, fee: u64) -> &mut Self {
        self.fee = Some(fee);
        self
    }

    /// Build the withdrawal call data
    ///
    /// This constructs:
    /// 1. ZK proof demonstrating:
    ///    - User knows secret for a commitment in the Merkle tree
    ///    - Deposit hasn't been spent (nullifier not in spent tree)
    ///    - Amount is valid (<= deposited amount)
    /// 2. Encoded call data for the bridge contract
    pub fn build(&self) -> Result<Vec<u8>, BridgeClientError> {
        let nullifier = self.nullifier.ok_or_else(|| BridgeClientError::InvalidWithdrawalProof("nullifier required".into()))?;
        let recipient_hash = self.recipient_hash.ok_or_else(|| BridgeClientError::InvalidWithdrawalProof("recipient_hash required".into()))?;
        let amount = self.amount.ok_or_else(|| BridgeClientError::InvalidWithdrawalProof("amount required".into()))?;
        let fee = self.fee.unwrap_or(1000);

        // Generate ZK proof
        // The proof demonstrates:
        // - User knows secret S where nullifier = H(S)
        // - There exists a commitment C = H(S, amount, address) in the deposit Merkle tree
        // - The nullifier hasn't been spent
        // - Amount is valid
        //
        // In production: call the zkas proving system
        // let proof = generate_withdrawal_proof(secret, amount, recipient_hash)?;
        let proof = vec![0u8; 64]; // Placeholder for ZK proof

        // Encode call data
        let mut call_data = Vec::new();
        call_data.push(0x02); // WithdrawV1 function ID
        call_data.extend_from_slice(&nullifier);
        call_data.extend_from_slice(&recipient_hash);
        call_data.extend_from_slice(&amount.to_le_bytes());
        call_data.extend_from_slice(&fee.to_le_bytes());
        // Proof length and data
        call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&proof);

        Ok(call_data)
    }
}

/// Compute nullifier from secret
pub fn compute_nullifier(secret: [u8; 32]) -> [u8; 32] {
    use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::{crypto::{pasta_prelude::PrimeField}, pasta::pallas};

    // nullifier = poseidon_hash(secret)
    let secret_base = pallas::Base::from_repr(secret.into()).unwrap();
    let nullifier = poseidon_hash([secret_base]);
    nullifier.to_repr()
}

// ============================================================================
// HELPER FUNCTIONS
// ============================================================================

/// Derive a bridge address for receiving bridged funds
///
/// # Arguments
/// * `user_pub_x` - User's DarkWow public key X coordinate
/// * `user_pub_y` - User's DarkWow public key Y coordinate
/// * `nonce` - Fresh nonce for this deposit (ensures unlinkability)
///
/// # Returns
/// The bridge address on the external chain
///
/// # Example
/// ```ignore
/// let bridge_addr = derive_bridge_address_external(user_pub_x, user_pub_y, nonce);
/// // User sends ETH to bridge_addr on Ethereum
/// ```
pub fn derive_bridge_address_external(
    user_pub_x: [u8; 32],
    user_pub_y: [u8; 32],
    nonce: u64,
) -> [u8; 32] {
    use dwow_sdk::crypto::poseidon_hash;
use dwow_sdk::{crypto::{pasta_prelude::PrimeField}, pasta::pallas};

    // Use poseidon for ZK-friendly hashing
    let pub_x = pallas::Base::from_repr(user_pub_x.into()).unwrap();
    let pub_y = pallas::Base::from_repr(user_pub_y.into()).unwrap();
    let cap_hash = poseidon_hash([pub_x, pub_y, pallas::Base::from(nonce)]);
    cap_hash.to_repr()
}