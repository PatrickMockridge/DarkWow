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

//! Escrow contract client API
//!
//! This module provides builder structs for constructing escrow contract calls.
//! Each state transition has a corresponding builder that handles:
//! - Parameter serialization
//! - ZK proof generation (via zkas circuits)
//! - State update construction
//!
//! ## Usage
//!
//! ```ignore
//! // Create a new escrow
//! let create_builder = CreateEscrowBuilder::new(
//!     buyer_pubkey,
//!     seller_pubkey,
//!     value,
//!     token_id,
//!     timeout,
//! );
//! let (params, proof) = create_builder.build()?;
//!
//! // Fund the escrow
//! let fund_builder = FundEscrowBuilder::new(escrow_id, value_commit);
//! let (params, proof) = fund_builder.build()?;
//!
//! // Seller claims funds
//! let claim_builder = ClaimEscrowBuilder::new(escrow_id, seller_secret);
//! let (params, proof) = claim_builder.build()?;
//!
//! // Buyer refunds after timeout
//! let refund_builder = RefundEscrowBuilder::new(
//!     escrow_id,
//!     buyer_secret,
//!     current_block,
//! );
//! let (params, proof) = refund_builder.build()?;
//! ```

use darkfi_sdk::{
    crypto::{pasta_prelude::Field, poseidon_hash, PublicKey, SecretKey},
    pasta::pallas,
};
use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::model::{
    CancelEscrowParamsV1, ClaimEscrowParamsV1, CreateEscrowParamsV1, EscrowId, FundEscrowParamsV1,
    RefundEscrowParamsV1,
};

// ============================================================================
// NOTE: Placeholder implementations
// ============================================================================
//
// The actual ZK proof generation and client-side building requires the
// zkas circuit binary files (create_escrow_v1.zk.bin, fund_v1.zk.bin, etc.)
// which are compiled from the .zk circuit definitions.
//
// These builders are structured to match the expected API once circuits exist.
// ============================================================================

/// Builder for `Escrow::CreateEscrowV1`
///
/// Creates a new escrow by committing to the terms:
/// - buyer_pubkey, seller_pubkey: parties involved
/// - value, token_id: amount and token type
/// - timeout: block height after which buyer can refund
pub struct CreateEscrowBuilder {
    /// Buyer's public key
    buyer_pubkey: PublicKey,
    /// Seller's public key
    seller_pubkey: PublicKey,
    /// Value to lock in escrow
    value: u64,
    /// Token ID
    token_id: pallas::Base,
    /// Timeout block height
    timeout: u64,
}

impl CreateEscrowBuilder {
    pub fn new(
        buyer_pubkey: PublicKey,
        seller_pubkey: PublicKey,
        value: u64,
        token_id: pallas::Base,
        timeout: u64,
    ) -> Self {
        Self { buyer_pubkey, seller_pubkey, value, token_id, timeout }
    }

    /// Build the create escrow call parameters and proof
    ///
    /// Returns `(params, public_inputs)` for the ZK proof
    pub fn build(&self) -> Result<(CreateEscrowParamsV1, CreateEscrowPublicInputs), &'static str> {
        // Compute escrow ID as commitment to the terms
        // commitment = H(buyer_pub.x, buyer_pub.y, seller_pub.x, seller_pub.y, value, token_id, timeout)
        let (bx, by) = self.buyer_pubkey.xy();
        let (sx, sy) = self.seller_pubkey.xy();
        let commitment = poseidon_hash([
            bx,
            by,
            sx,
            sy,
            pallas::Base::from(self.value),
            self.token_id,
            pallas::Base::from(self.timeout),
        ]);

        let params = CreateEscrowParamsV1 {
            buyer_pubkey: self.buyer_pubkey,
            seller_pubkey: self.seller_pubkey,
            value: self.value,
            token_id: self.token_id,
            timeout: self.timeout,
            commitment,
            merkle_root: Default::default(), // TODO: Merkle tree integration
        };

        let public_inputs = CreateEscrowPublicInputs { commitment };

        Ok((params, public_inputs))
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CreateEscrowPublicInputs {
    pub commitment: EscrowId,
}

/// Builder for `Escrow::FundV1`
///
/// Funds an existing escrow by providing the Pedersen commitment to the value.
/// This transitions the escrow from Created -> Funded.
pub struct FundEscrowBuilder {
    /// Escrow ID
    escrow_id: EscrowId,
    /// Value commitment (Pedersen)
    value_commit: pallas::Point,
}

impl FundEscrowBuilder {
    pub fn new(escrow_id: EscrowId, value_commit: pallas::Point) -> Self {
        Self { escrow_id, value_commit }
    }

    /// Build the fund escrow call parameters and proof
    pub fn build(&self) -> Result<(FundEscrowParamsV1, FundEscrowPublicInputs), &'static str> {
        let params = FundEscrowParamsV1 {
            escrow_id: self.escrow_id,
            value_commit: self.value_commit,
            merkle_proof: vec![], // TODO: Merkle proof integration
            merkle_root: Default::default(),
        };

        let public_inputs = FundEscrowPublicInputs {
            escrow_id: self.escrow_id,
            value_commit_x: self.value_commit.x(),
            value_commit_y: self.value_commit.y(),
        };

        Ok((params, public_inputs))
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct FundEscrowPublicInputs {
    pub escrow_id: EscrowId,
    pub value_commit_x: pallas::Base,
    pub value_commit_y: pallas::Base,
}

/// Builder for `Escrow::ClaimV1`
///
/// Seller claims the escrowed funds by proving knowledge of seller_secret.
/// seller_secret is verified by deriving seller_pubkey = seller_secret * G
/// and checking it matches the escrow's seller_pubkey.
pub struct ClaimEscrowBuilder {
    /// Escrow ID
    escrow_id: EscrowId,
    /// Seller's secret key
    seller_secret: SecretKey,
    /// Recipient for the funds
    recipient_pubkey: PublicKey,
}

impl ClaimEscrowBuilder {
    pub fn new(escrow_id: EscrowId, seller_secret: SecretKey, recipient_pubkey: PublicKey) -> Self {
        Self { escrow_id, seller_secret, recipient_pubkey }
    }

    /// Build the claim escrow call parameters and proof
    pub fn build(&self) -> Result<(ClaimEscrowParamsV1, ClaimEscrowPublicInputs), &'static str> {
        // Derive public key from secret for nullifier
        let seller_pubkey_computed = PublicKey::from_secret(self.seller_secret);
        let spent_nullifier =
            poseidon_hash([self.escrow_id, self.seller_secret.inner()]);

        let params = ClaimEscrowParamsV1 {
            escrow_id: self.escrow_id,
            seller_secret: self.seller_secret.inner(),
            spent_nullifier,
            recipient_pubkey: self.recipient_pubkey,
        };

        let public_inputs = ClaimEscrowPublicInputs {
            escrow_id: self.escrow_id,
            seller_pubkey_x: seller_pubkey_computed.x(),
            seller_pubkey_y: seller_pubkey_computed.y(),
            spent_nullifier,
        };

        Ok((params, public_inputs))
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct ClaimEscrowPublicInputs {
    pub escrow_id: EscrowId,
    pub seller_pubkey_x: pallas::Base,
    pub seller_pubkey_y: pallas::Base,
    pub spent_nullifier: pallas::Base,
}

/// Builder for `Escrow::RefundV1`
///
/// Buyer refunds the escrowed funds after timeout by proving:
/// - current_block >= escrow.timeout (using LessThanStrict)
/// - knowledge of buyer_secret
pub struct RefundEscrowBuilder {
    /// Escrow ID
    escrow_id: EscrowId,
    /// Buyer's secret key
    buyer_secret: SecretKey,
    /// Current block height (proves timeout reached)
    current_block: u64,
    /// Recipient for the refunded funds
    recipient_pubkey: PublicKey,
}

impl RefundEscrowBuilder {
    pub fn new(
        escrow_id: EscrowId,
        buyer_secret: SecretKey,
        current_block: u64,
        recipient_pubkey: PublicKey,
    ) -> Self {
        Self { escrow_id, buyer_secret, current_block, recipient_pubkey }
    }

    /// Build the refund escrow call parameters and proof
    pub fn build(&self) -> Result<(RefundEscrowParamsV1, RefundEscrowPublicInputs), &'static str> {
        let buyer_pubkey_computed = PublicKey::from_secret(self.buyer_secret);
        let spent_nullifier =
            poseidon_hash([self.escrow_id, self.buyer_secret.inner()]);

        let params = RefundEscrowParamsV1 {
            escrow_id: self.escrow_id,
            buyer_secret: self.buyer_secret.inner(),
            spent_nullifier,
            current_block: self.current_block,
            recipient_pubkey: self.recipient_pubkey,
        };

        let public_inputs = RefundEscrowPublicInputs {
            escrow_id: self.escrow_id,
            timeout: 0, // TODO: load from escrow state
            current_block: pallas::Base::from(self.current_block),
            buyer_pubkey_x: buyer_pubkey_computed.x(),
            buyer_pubkey_y: buyer_pubkey_computed.y(),
            spent_nullifier,
        };

        Ok((params, public_inputs))
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct RefundEscrowPublicInputs {
    pub escrow_id: EscrowId,
    pub timeout: u64,
    pub current_block: pallas::Base,
    pub buyer_pubkey_x: pallas::Base,
    pub buyer_pubkey_y: pallas::Base,
    pub spent_nullifier: pallas::Base,
}

/// Builder for `Escrow::CancelV1`
///
/// Buyer cancels the escrow before funding.
/// Only allowed when escrow state is Created.
pub struct CancelEscrowBuilder {
    /// Escrow ID
    escrow_id: EscrowId,
    /// Buyer's secret key
    buyer_secret: SecretKey,
}

impl CancelEscrowBuilder {
    pub fn new(escrow_id: EscrowId, buyer_secret: SecretKey) -> Self {
        Self { escrow_id, buyer_secret }
    }

    /// Build the cancel escrow call parameters and proof
    pub fn build(&self) -> Result<(CancelEscrowParamsV1, CancelEscrowPublicInputs), &'static str> {
        let params = CancelEscrowParamsV1 {
            escrow_id: self.escrow_id,
            buyer_secret: self.buyer_secret.inner(),
        };

        let public_inputs = CancelEscrowPublicInputs { escrow_id: self.escrow_id };

        Ok((params, public_inputs))
    }
}

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct CancelEscrowPublicInputs {
    pub escrow_id: EscrowId,
}
