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

//! Client-side transaction builders for DEX atomic swap contract
//!
//! ## How to Create and Execute an Atomic Swap
//!
//! ```ignore
//! // Alice creates swap proposal
//! let create = CreateSwapBuilder::new()
//!     .secret(alice_secret)
//!     .offer_token(drk_token)
//!     .offer_amount(1000)
//!     .request_token(eth_token)
//!     .request_amount(1)
//!     .build()?;
//!
//! // Bob accepts swap
//! let accept = AcceptSwapBuilder::new()
//!     .swap_id(create.swap_id())
//!     .secret(bob_secret)
//!     .build()?;
//!
//! // Anyone executes (with both parties' secrets)
//! let execute = ExecuteSwapBuilder::new()
//!     .swap_id(swap_id)
//!     .alice_secret(alice_secret)
//!     .bob_secret(bob_secret)
//!     .build()?;
//! ```

use darkfi_sdk::error::ClientError;

/// DEX client errors
#[derive(Debug, thiserror::Error)]
pub enum DexClientError {
    #[error("Invalid swap: {0}")]
    InvalidSwap(String),

    #[error("Invalid lock: {0}")]
    InvalidLock(String),

    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Invalid ZK proof: {0}")]
    InvalidProof(String),
}

// ============================================================================
// CREATE SWAP BUILDER
// ============================================================================

/// Builder for creating an atomic swap proposal
pub struct CreateSwapBuilder {
    secret: Option<[u8; 32]>,
    offer_token: Option<[u8; 32]>,
    offer_amount: Option<u64>,
    request_token: Option<[u8; 32]>,
    request_amount: Option<u64>,
}

impl CreateSwapBuilder {
    pub fn new() -> Self {
        Self {
            secret: None,
            offer_token: None,
            offer_amount: None,
            request_token: None,
            request_amount: None,
        }
    }

    /// Set the secret for the lock
    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Set the token being offered
    pub fn offer_token(&mut self, token: [u8; 32]) -> &mut Self {
        self.offer_token = Some(token);
        self
    }

    /// Set the offer amount
    pub fn offer_amount(&mut self, amount: u64) -> &mut Self {
        self.offer_amount = Some(amount);
        self
    }

    /// Set the token being requested
    pub fn request_token(&mut self, token: [u8; 32]) -> &mut Self {
        self.request_token = Some(token);
        self
    }

    /// Set the request amount
    pub fn request_amount(&mut self, amount: u64) -> &mut Self {
        self.request_amount = Some(amount);
        self
    }

    /// Build the create swap transaction
    ///
    /// lock_commitment = H(secret, offer_token, offer_amount)
    /// swap_id = H(lock_commitment, request_token, request_amount, nonce)
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let secret = self.secret.ok_or_else(|| ClientError::InvalidInput("secret required".into()))?;
        let offer_token = self.offer_token.ok_or_else(|| ClientError::InvalidInput("offer_token required".into()))?;
        let offer_amount = self.offer_amount.ok_or_else(|| ClientError::InvalidInput("offer_amount required".into()))?;
        let request_token = self.request_token.ok_or_else(|| ClientError::InvalidInput("request_token required".into()))?;
        let request_amount = self.request_amount.ok_or_else(|| ClientError::InvalidInput("request_amount required".into()))?;

        // Compute lock commitment
        let lock_commitment = compute_lock_commitment(secret, offer_token, offer_amount);

        // Compute swap ID
        let swap_id = compute_swap_id(lock_commitment, request_token, request_amount);

        // TODO: Generate Merkle proof for lock
        let lock_proof = vec![];

        // TODO: Generate signature
        let signature = vec![];

        // Encode call data
        let mut call_data = Vec::new();
        call_data.push(0x01); // CreateSwapV1
        call_data.extend_from_slice(&swap_id);
        call_data.extend_from_slice(&offer_token);
        call_data.extend_from_slice(&offer_amount.to_le_bytes());
        call_data.extend_from_slice(&request_token);
        call_data.extend_from_slice(&request_amount.to_le_bytes());
        call_data.extend_from_slice(&lock_commitment);
        call_data.extend_from_slice(&(lock_proof.len() as u32).to_le_bytes());
        for p in &lock_proof {
            call_data.extend_from_slice(p);
        }
        call_data.extend_from_slice(&(signature.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&signature);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

/// Compute lock commitment
fn compute_lock_commitment(secret: [u8; 32], token: [u8; 32], amount: u64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"lock_commitment");
    hasher.update(&secret);
    hasher.update(&token);
    hasher.update(&amount.to_le_bytes());
    *hasher.finalize().as_bytes()
}

/// Compute swap ID
fn compute_swap_id(
    lock_commitment: [u8; 32],
    request_token: [u8; 32],
    request_amount: u64,
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"swap_id");
    hasher.update(&lock_commitment);
    hasher.update(&request_token);
    hasher.update(&request_amount.to_le_bytes());
    *hasher.finalize().as_bytes()
}

// ============================================================================
// ACCEPT SWAP BUILDER
// ============================================================================

/// Builder for accepting an atomic swap
pub struct AcceptSwapBuilder {
    swap_id: Option<[u8; 32]>,
    secret: Option<[u8; 32]>,
    offer_token: Option<[u8; 32]>,
    offer_amount: Option<u64>,
}

impl AcceptSwapBuilder {
    pub fn new() -> Self {
        Self {
            swap_id: None,
            secret: None,
            offer_token: None,
            offer_amount: None,
        }
    }

    /// Set the swap ID to accept
    pub fn swap_id(&mut self, swap_id: [u8; 32]) -> &mut Self {
        self.swap_id = Some(swap_id);
        self
    }

    /// Set the secret for the lock
    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Set the token being offered (should match request)
    pub fn offer_token(&mut self, token: [u8; 32]) -> &mut Self {
        self.offer_token = Some(token);
        self
    }

    /// Set the offer amount
    pub fn offer_amount(&mut self, amount: u64) -> &mut Self {
        self.offer_amount = Some(amount);
        self
    }

    /// Build the accept swap transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let swap_id = self.swap_id.ok_or_else(|| ClientError::InvalidInput("swap_id required".into()))?;
        let secret = self.secret.ok_or_else(|| ClientError::InvalidInput("secret required".into()))?;
        let offer_token = self.offer_token.ok_or_else(|| ClientError::InvalidInput("offer_token required".into()))?;
        let offer_amount = self.offer_amount.ok_or_else(|| ClientError::InvalidInput("offer_amount required".into()))?;

        let lock_commitment = compute_lock_commitment(secret, offer_token, offer_amount);

        // TODO: Generate Merkle proof
        let lock_proof = vec![];

        // TODO: Generate signature
        let signature = vec![];

        let mut call_data = Vec::new();
        call_data.push(0x02); // AcceptSwapV1
        call_data.extend_from_slice(&swap_id);
        call_data.extend_from_slice(&lock_commitment);
        call_data.extend_from_slice(&(lock_proof.len() as u32).to_le_bytes());
        for p in &lock_proof {
            call_data.extend_from_slice(p);
        }
        call_data.extend_from_slice(&(signature.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&signature);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

// ============================================================================
// EXECUTE SWAP BUILDER
// ============================================================================

/// Builder for executing an atomic swap
pub struct ExecuteSwapBuilder {
    swap_id: Option<[u8; 32]>,
    alice_secret: Option<[u8; 32]>,
    bob_secret: Option<[u8; 32]>,
}

impl ExecuteSwapBuilder {
    pub fn new() -> Self {
        Self {
            swap_id: None,
            alice_secret: None,
            bob_secret: None,
        }
    }

    pub fn swap_id(&mut self, swap_id: [u8; 32]) -> &mut Self {
        self.swap_id = Some(swap_id);
        self
    }

    pub fn alice_secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.alice_secret = Some(secret);
        self
    }

    pub fn bob_secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.bob_secret = Some(secret);
        self
    }

    /// Build the execute swap transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let swap_id = self.swap_id.ok_or_else(|| ClientError::InvalidInput("swap_id required".into()))?;
        let alice_secret = self.alice_secret.ok_or_else(|| ClientError::InvalidInput("alice_secret required".into()))?;
        let bob_secret = self.bob_secret.ok_or_else(|| ClientError::InvalidInput("bob_secret required".into()))?;

        // TODO: Generate ZK proof
        let proof = vec![0u8; 64];

        let mut call_data = Vec::new();
        call_data.push(0x03); // ExecuteSwapV1
        call_data.extend_from_slice(&swap_id);
        call_data.extend_from_slice(&alice_secret);
        call_data.extend_from_slice(&bob_secret);
        call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&proof);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}

// ============================================================================
// CANCEL SWAP BUILDER
// ============================================================================

/// Builder for cancelling an atomic swap
pub struct CancelSwapBuilder {
    swap_id: Option<[u8; 32]>,
    secret: Option<[u8; 32]>,
}

impl CancelSwapBuilder {
    pub fn new() -> Self {
        Self {
            swap_id: None,
            secret: None,
        }
    }

    pub fn swap_id(&mut self, swap_id: [u8; 32]) -> &mut Self {
        self.swap_id = Some(swap_id);
        self
    }

    pub fn secret(&mut self, secret: [u8; 32]) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Build the cancel swap transaction
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        let swap_id = self.swap_id.ok_or_else(|| ClientError::InvalidInput("swap_id required".into()))?;
        let secret = self.secret.ok_or_else(|| ClientError::InvalidInput("secret required".into()))?;

        // TODO: Generate ZK proof
        let proof = vec![0u8; 64];

        let mut call_data = Vec::new();
        call_data.push(0x04); // CancelSwapV1
        call_data.extend_from_slice(&swap_id);
        call_data.extend_from_slice(&secret);
        call_data.extend_from_slice(&(proof.len() as u32).to_le_bytes());
        call_data.extend_from_slice(&proof);
        call_data.extend_from_slice(&0u64.to_le_bytes()); // fee

        Ok(call_data)
    }
}