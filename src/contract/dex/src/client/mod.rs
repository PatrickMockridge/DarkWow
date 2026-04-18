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

pub mod accept_swap_v1;
pub mod cancel_swap_v1;
pub mod create_swap_v1;
pub mod execute_swap_fee_v1;
pub mod execute_swap_slippage_v1;
pub mod execute_swap_v1;

pub use accept_swap_v1::{create_accept_swap_proof, AcceptSwapCallData};
pub use cancel_swap_v1::{create_cancel_swap_proof, CancelSwapCallData};
pub use create_swap_v1::{create_create_swap_proof, CreateSwapCallData};
pub use execute_swap_fee_v1::{create_execute_swap_fee_proof, ExecuteSwapFeeCallData};
pub use execute_swap_slippage_v1::{create_execute_swap_slippage_proof, ExecuteSwapSlippageCallData};
pub use execute_swap_v1::{create_execute_swap_proof, ExecuteSwapCallData};

use darkfi_sdk::{
    crypto::{poseidon_hash, SecretKey},
    pasta::pallas,
};
pub use crate::{model::CreateSwapParams, DexFunction};

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
    secret: Option<pallas::Base>,
    offer_token: Option<pallas::Base>,
    offer_amount: Option<u64>,
    request_token: Option<pallas::Base>,
    request_amount: Option<u64>,
    signature_secret: Option<SecretKey>,
}

impl CreateSwapBuilder {
    pub fn new() -> Self {
        Self {
            secret: None,
            offer_token: None,
            offer_amount: None,
            request_token: None,
            request_amount: None,
            signature_secret: None,
        }
    }

    /// Set the secret for the lock (as pallas::Base)
    pub fn secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Set the token being offered
    pub fn offer_token(&mut self, token: pallas::Base) -> &mut Self {
        self.offer_token = Some(token);
        self
    }

    /// Set the offer amount
    pub fn offer_amount(&mut self, amount: u64) -> &mut Self {
        self.offer_amount = Some(amount);
        self
    }

    /// Set the token being requested
    pub fn request_token(&mut self, token: pallas::Base) -> &mut Self {
        self.request_token = Some(token);
        self
    }

    /// Set the request amount
    pub fn request_amount(&mut self, amount: u64) -> &mut Self {
        self.request_amount = Some(amount);
        self
    }

    /// Set the signature secret key
    pub fn signature_secret(&mut self, secret: SecretKey) -> &mut Self {
        self.signature_secret = Some(secret);
        self
    }

    /// Build the create swap call data
    ///
    /// Returns the call data and the derived public inputs for ZK proof generation.
    pub fn build(&self) -> Result<CreateSwapCallData, DexClientError> {
        let secret = self.secret.ok_or_else(|| DexClientError::MissingField("secret".into()))?;
        let offer_token = self.offer_token.ok_or_else(|| DexClientError::MissingField("offer_token".into()))?;
        let offer_amount = self.offer_amount.ok_or_else(|| DexClientError::MissingField("offer_amount".into()))?;
        let request_token = self.request_token.ok_or_else(|| DexClientError::MissingField("request_token".into()))?;
        let request_amount = self.request_amount.ok_or_else(|| DexClientError::MissingField("request_amount".into()))?;
        let signature_secret = self.signature_secret.ok_or_else(|| DexClientError::MissingField("signature_secret".into()))?;

        Ok(CreateSwapCallData::new(
            secret,
            offer_token,
            offer_amount,
            request_token,
            request_amount,
            signature_secret,
        ))
    }
}

/// Compute lock commitment using poseidon_hash
/// lock_commitment = poseidon_hash([secret, offer_token, offer_amount, token_blind, amount_blind])
pub fn compute_lock_commitment(
    secret: pallas::Base,
    token: pallas::Base,
    amount: pallas::Base,
    token_blind: pallas::Base,
    amount_blind: pallas::Base,
) -> pallas::Base {
    poseidon_hash([secret, token, amount, token_blind, amount_blind])
}

/// Compute swap ID using poseidon_hash
/// swap_id = poseidon_hash([lock_commitment, request_token, request_amount])
pub fn compute_swap_id(
    lock_commitment: pallas::Base,
    request_token: pallas::Base,
    request_amount: pallas::Base,
) -> pallas::Base {
    poseidon_hash([lock_commitment, request_token, request_amount])
}

// ============================================================================
// ACCEPT SWAP BUILDER
// ============================================================================

/// Builder for accepting an atomic swap
pub struct AcceptSwapBuilder {
    swap_id: Option<pallas::Base>,
    proposer_lock_commitment: Option<pallas::Base>,
    secret: Option<pallas::Base>,
    offer_token: Option<pallas::Base>,
    offer_amount: Option<u64>,
    signature_secret: Option<SecretKey>,
}

impl AcceptSwapBuilder {
    pub fn new() -> Self {
        Self {
            swap_id: None,
            proposer_lock_commitment: None,
            secret: None,
            offer_token: None,
            offer_amount: None,
            signature_secret: None,
        }
    }

    /// Set the swap ID to accept
    pub fn swap_id(&mut self, swap_id: pallas::Base) -> &mut Self {
        self.swap_id = Some(swap_id);
        self
    }

    /// Set the proposer's lock commitment
    pub fn proposer_lock_commitment(&mut self, commitment: pallas::Base) -> &mut Self {
        self.proposer_lock_commitment = Some(commitment);
        self
    }

    /// Set the secret for the lock
    pub fn secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    /// Set the token being offered (should match request)
    pub fn offer_token(&mut self, token: pallas::Base) -> &mut Self {
        self.offer_token = Some(token);
        self
    }

    /// Set the offer amount
    pub fn offer_amount(&mut self, amount: u64) -> &mut Self {
        self.offer_amount = Some(amount);
        self
    }

    /// Set the signature secret key
    pub fn signature_secret(&mut self, secret: SecretKey) -> &mut Self {
        self.signature_secret = Some(secret);
        self
    }

    /// Build the accept swap call data
    pub fn build(&self) -> Result<AcceptSwapCallData, DexClientError> {
        let swap_id = self.swap_id.ok_or_else(|| DexClientError::MissingField("swap_id".into()))?;
        let proposer_lock_commitment = self.proposer_lock_commitment.ok_or_else(|| DexClientError::MissingField("proposer_lock_commitment".into()))?;
        let secret = self.secret.ok_or_else(|| DexClientError::MissingField("secret".into()))?;
        let offer_token = self.offer_token.ok_or_else(|| DexClientError::MissingField("offer_token".into()))?;
        let offer_amount = self.offer_amount.ok_or_else(|| DexClientError::MissingField("offer_amount".into()))?;
        let signature_secret = self.signature_secret.ok_or_else(|| DexClientError::MissingField("signature_secret".into()))?;

        Ok(AcceptSwapCallData::new(
            swap_id,
            proposer_lock_commitment,
            secret,
            offer_token,
            offer_amount,
            signature_secret,
        ))
    }
}

// ============================================================================
// EXECUTE SWAP BUILDER
// ============================================================================

/// Builder for executing an atomic swap
pub struct ExecuteSwapBuilder {
    alice_secret: Option<pallas::Base>,
    alice_token: Option<pallas::Base>,
    alice_amount: Option<u64>,
    alice_lock: Option<pallas::Base>,
    bob_secret: Option<pallas::Base>,
    bob_token: Option<pallas::Base>,
    bob_amount: Option<u64>,
    bob_lock: Option<pallas::Base>,
    fill_amount: Option<u64>,
    alice_otc_func_id: Option<pallas::Base>,
    bob_otc_func_id: Option<pallas::Base>,
}

impl ExecuteSwapBuilder {
    pub fn new() -> Self {
        Self {
            alice_secret: None,
            alice_token: None,
            alice_amount: None,
            alice_lock: None,
            bob_secret: None,
            bob_token: None,
            bob_amount: None,
            bob_lock: None,
            fill_amount: None,
            alice_otc_func_id: None,
            bob_otc_func_id: None,
        }
    }

    pub fn alice_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.alice_secret = Some(secret);
        self
    }

    pub fn alice_token(&mut self, token: pallas::Base) -> &mut Self {
        self.alice_token = Some(token);
        self
    }

    pub fn alice_amount(&mut self, amount: u64) -> &mut Self {
        self.alice_amount = Some(amount);
        self
    }

    pub fn alice_lock(&mut self, lock: pallas::Base) -> &mut Self {
        self.alice_lock = Some(lock);
        self
    }

    pub fn bob_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.bob_secret = Some(secret);
        self
    }

    pub fn bob_token(&mut self, token: pallas::Base) -> &mut Self {
        self.bob_token = Some(token);
        self
    }

    pub fn bob_amount(&mut self, amount: u64) -> &mut Self {
        self.bob_amount = Some(amount);
        self
    }

    pub fn bob_lock(&mut self, lock: pallas::Base) -> &mut Self {
        self.bob_lock = Some(lock);
        self
    }

    pub fn fill_amount(&mut self, amount: u64) -> &mut Self {
        self.fill_amount = Some(amount);
        self
    }

    pub fn alice_otc_func_id(&mut self, func_id: pallas::Base) -> &mut Self {
        self.alice_otc_func_id = Some(func_id);
        self
    }

    pub fn bob_otc_func_id(&mut self, func_id: pallas::Base) -> &mut Self {
        self.bob_otc_func_id = Some(func_id);
        self
    }

    /// Build the execute swap call data
    pub fn build(&self) -> Result<ExecuteSwapCallData, DexClientError> {
        let alice_secret = self.alice_secret.ok_or_else(|| DexClientError::MissingField("alice_secret".into()))?;
        let alice_token = self.alice_token.ok_or_else(|| DexClientError::MissingField("alice_token".into()))?;
        let alice_amount = self.alice_amount.ok_or_else(|| DexClientError::MissingField("alice_amount".into()))?;
        let alice_lock = self.alice_lock.ok_or_else(|| DexClientError::MissingField("alice_lock".into()))?;
        let bob_secret = self.bob_secret.ok_or_else(|| DexClientError::MissingField("bob_secret".into()))?;
        let bob_token = self.bob_token.ok_or_else(|| DexClientError::MissingField("bob_token".into()))?;
        let bob_amount = self.bob_amount.ok_or_else(|| DexClientError::MissingField("bob_amount".into()))?;
        let bob_lock = self.bob_lock.ok_or_else(|| DexClientError::MissingField("bob_lock".into()))?;
        let fill_amount = self.fill_amount.ok_or_else(|| DexClientError::MissingField("fill_amount".into()))?;
        let alice_otc_func_id = self.alice_otc_func_id.ok_or_else(|| DexClientError::MissingField("alice_otc_func_id".into()))?;
        let bob_otc_func_id = self.bob_otc_func_id.ok_or_else(|| DexClientError::MissingField("bob_otc_func_id".into()))?;

        Ok(ExecuteSwapCallData::new(
            alice_secret,
            alice_token,
            alice_amount,
            alice_lock,
            bob_secret,
            bob_token,
            bob_amount,
            bob_lock,
            fill_amount,
            alice_otc_func_id,
            bob_otc_func_id,
        ))
    }
}

// ============================================================================
// EXECUTE SWAP WITH SLIPPAGE BUILDER
// ============================================================================

/// Builder for executing an atomic swap with slippage protection
pub struct ExecuteSwapSlippageBuilder {
    inner: ExecuteSwapBuilder,
    slippage_bps: Option<u64>,
}

impl ExecuteSwapSlippageBuilder {
    pub fn new() -> Self {
        Self {
            inner: ExecuteSwapBuilder::new(),
            slippage_bps: None,
        }
    }

    pub fn alice_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.inner.alice_secret(secret);
        self
    }

    pub fn alice_token(&mut self, token: pallas::Base) -> &mut Self {
        self.inner.alice_token(token);
        self
    }

    pub fn alice_amount(&mut self, amount: u64) -> &mut Self {
        self.inner.alice_amount(amount);
        self
    }

    pub fn alice_lock(&mut self, lock: pallas::Base) -> &mut Self {
        self.inner.alice_lock(lock);
        self
    }

    pub fn bob_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.inner.bob_secret(secret);
        self
    }

    pub fn bob_token(&mut self, token: pallas::Base) -> &mut Self {
        self.inner.bob_token(token);
        self
    }

    pub fn bob_amount(&mut self, amount: u64) -> &mut Self {
        self.inner.bob_amount(amount);
        self
    }

    pub fn bob_lock(&mut self, lock: pallas::Base) -> &mut Self {
        self.inner.bob_lock(lock);
        self
    }

    pub fn fill_amount(&mut self, amount: u64) -> &mut Self {
        self.inner.fill_amount(amount);
        self
    }

    /// Set slippage tolerance in basis points (e.g., 50 = 0.5%)
    pub fn slippage_bps(&mut self, bps: u64) -> &mut Self {
        self.slippage_bps = Some(bps);
        self
    }

    /// Build the execute swap with slippage call data
    pub fn build(&self) -> Result<ExecuteSwapSlippageCallData, DexClientError> {
        let inner = self.inner.build()?;
        let slippage_bps = self.slippage_bps.ok_or_else(|| DexClientError::MissingField("slippage_bps".into()))?;

        Ok(ExecuteSwapSlippageCallData::new(
            inner.alice_secret,
            inner.alice_token,
            inner.alice_amount,
            inner.alice_lock,
            inner.bob_secret,
            inner.bob_token,
            inner.bob_amount,
            inner.bob_lock,
            inner.fill_amount,
            pallas::Base::from(slippage_bps),
        ))
    }
}

// ============================================================================
// EXECUTE SWAP WITH FEE BUILDER
// ============================================================================

/// Builder for executing an atomic swap with fee deduction
pub struct ExecuteSwapFeeBuilder {
    inner: ExecuteSwapBuilder,
    fee_bps: Option<u64>,
}

impl ExecuteSwapFeeBuilder {
    pub fn new() -> Self {
        Self {
            inner: ExecuteSwapBuilder::new(),
            fee_bps: None,
        }
    }

    pub fn alice_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.inner.alice_secret(secret);
        self
    }

    pub fn alice_token(&mut self, token: pallas::Base) -> &mut Self {
        self.inner.alice_token(token);
        self
    }

    pub fn alice_amount(&mut self, amount: u64) -> &mut Self {
        self.inner.alice_amount(amount);
        self
    }

    pub fn alice_lock(&mut self, lock: pallas::Base) -> &mut Self {
        self.inner.alice_lock(lock);
        self
    }

    pub fn bob_secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.inner.bob_secret(secret);
        self
    }

    pub fn bob_token(&mut self, token: pallas::Base) -> &mut Self {
        self.inner.bob_token(token);
        self
    }

    pub fn bob_amount(&mut self, amount: u64) -> &mut Self {
        self.inner.bob_amount(amount);
        self
    }

    pub fn bob_lock(&mut self, lock: pallas::Base) -> &mut Self {
        self.inner.bob_lock(lock);
        self
    }

    pub fn fill_amount(&mut self, amount: u64) -> &mut Self {
        self.inner.fill_amount(amount);
        self
    }

    /// Set fee basis points (e.g., 30 = 0.3%)
    pub fn fee_bps(&mut self, bps: u64) -> &mut Self {
        self.fee_bps = Some(bps);
        self
    }

    /// Build the execute swap with fee call data
    pub fn build(&self) -> Result<ExecuteSwapFeeCallData, DexClientError> {
        let inner = self.inner.build()?;
        let fee_bps = self.fee_bps.ok_or_else(|| DexClientError::MissingField("fee_bps".into()))?;

        Ok(ExecuteSwapFeeCallData::new(
            inner.alice_secret,
            inner.alice_token,
            inner.alice_amount,
            inner.alice_lock,
            inner.bob_secret,
            inner.bob_token,
            inner.bob_amount,
            inner.bob_lock,
            inner.fill_amount,
            pallas::Base::from(fee_bps),
        ))
    }
}

// ============================================================================
// CANCEL SWAP BUILDER
// ============================================================================

/// Builder for cancelling an atomic swap
pub struct CancelSwapBuilder {
    swap_id: Option<pallas::Base>,
    lock_commitment: Option<pallas::Base>,
    secret: Option<pallas::Base>,
    token: Option<pallas::Base>,
    amount: Option<u64>,
}

impl CancelSwapBuilder {
    pub fn new() -> Self {
        Self {
            swap_id: None,
            lock_commitment: None,
            secret: None,
            token: None,
            amount: None,
        }
    }

    pub fn swap_id(&mut self, swap_id: pallas::Base) -> &mut Self {
        self.swap_id = Some(swap_id);
        self
    }

    pub fn lock_commitment(&mut self, commitment: pallas::Base) -> &mut Self {
        self.lock_commitment = Some(commitment);
        self
    }

    pub fn secret(&mut self, secret: pallas::Base) -> &mut Self {
        self.secret = Some(secret);
        self
    }

    pub fn token(&mut self, token: pallas::Base) -> &mut Self {
        self.token = Some(token);
        self
    }

    pub fn amount(&mut self, amount: u64) -> &mut Self {
        self.amount = Some(amount);
        self
    }

    /// Build the cancel swap call data
    pub fn build(&self) -> Result<CancelSwapCallData, DexClientError> {
        let swap_id = self.swap_id.ok_or_else(|| DexClientError::MissingField("swap_id".into()))?;
        let lock_commitment = self.lock_commitment.ok_or_else(|| DexClientError::MissingField("lock_commitment".into()))?;
        let secret = self.secret.ok_or_else(|| DexClientError::MissingField("secret".into()))?;
        let token = self.token.ok_or_else(|| DexClientError::MissingField("token".into()))?;
        let amount = self.amount.ok_or_else(|| DexClientError::MissingField("amount".into()))?;

        Ok(CancelSwapCallData::new(swap_id, lock_commitment, secret, token, amount))
    }
}
