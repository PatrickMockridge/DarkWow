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

//! Client API for bridge contract interaction

use darkfi_sdk::error::ClientError;

/// Bridge client errors
#[derive(Debug, thiserror::Error)]
pub enum BridgeClientError {
    #[error("Invalid deposit proof: {0}")]
    InvalidDepositProof(String),

    #[error("Invalid withdrawal proof: {0}")]
    InvalidWithdrawalProof(String),

    #[error("Merkle proof verification failed")]
    MerkleProofFailed,

    #[error("VSS error: {0}")]
    VssError(String),

    #[error("No bridge operators available")]
    NoOperatorsAvailable,
}

/// DepositBuilder constructs a bridge deposit transaction
pub struct DepositBuilder {
    // TODO: Fields for deposit construction
}

impl DepositBuilder {
    /// Create a new deposit builder
    pub fn new() -> Self {
        Self {}
    }

    /// Set the amount to deposit
    pub fn amount(&mut self, _amount: u64) -> &mut Self {
        // TODO: Implement
        self
    }

    /// Set the destination chain
    pub fn chain(&mut self, _chain: u8) -> &mut Self {
        // TODO: Implement
        self
    }

    /// Build and return the deposit call data
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        // TODO: Implement ZK proof generation and call encoding
        Err(ClientError::NotYetImplemented)
    }
}

/// WithdrawBuilder constructs a bridge withdrawal transaction
pub struct WithdrawBuilder {
    // TODO: Fields for withdrawal construction
}

impl WithdrawBuilder {
    /// Create a new withdrawal builder
    pub fn new() -> Self {
        Self {}
    }

    /// Set the deposit nullifier to withdraw
    pub fn nullifier(&mut self, _nullifier: [u8; 32]) -> &mut Self {
        // TODO: Implement
        self
    }

    /// Set the recipient address on external chain
    pub fn recipient(&mut self, _recipient: Vec<u8>) -> &mut Self {
        // TODO: Implement
        self
    }

    /// Set the amount to withdraw
    pub fn amount(&mut self, _amount: u64) -> &mut Self {
        // TODO: Implement
        self
    }

    /// Build and return the withdrawal call data
    pub fn build(&self) -> Result<Vec<u8>, ClientError> {
        // TODO: Implement ZK proof generation and call encoding
        Err(ClientError::NotYetImplemented)
    }
}
