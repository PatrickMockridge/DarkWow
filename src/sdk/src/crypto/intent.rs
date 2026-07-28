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

//! Generic private intent primitives for DarkWow smart contracts.
//!
//! This module provides application-agnostic private authorization primitives
//! that can be reused across identity, bridge, DEX, stablecoin, and other
//! privacy-preserving contracts.
//!
//! ## The Private Intent Pattern
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                 Private Intent Lifecycle                               │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                   │
//! │  1. CREATE                                                       │
//! │     User creates PrivateIntent with:                              │
//! │       - owner: PublicKey (who can consume)                        │
//! │       - namespace: Base (scopes to specific app)                │
//! │       - payload_hash: Base (commits to app-specific data)        │
//! │       - expiry: u64 (when intent expires)                         │
//! │       - nonce: Base (prevents replay)                             │
//! │       - blind: Base (additional blinding)                         │
//! │                                                                   │
//! │  2. COMMIT                                                        │
//! │     commitment = poseidon_hash([                                   │
//! │       9001,  // domain separator for intent commitment              │
//! │       owner_x, owner_y,                                          │
//! │       namespace, payload_hash, expiry, nonce, blind              │
//! │     ])                                                            │
//! │     → On-chain: commitment stored in Merkle tree                   │
//! │                                                                   │
//! │  3. CONSUME                                                       │
//! │     nullifier = poseidon_hash([                                   │
//! │       9002,  // domain separator for intent nullifier               │
//! │       owner_secret,                                              │
//! │       namespace, nonce,                                           │
//! │       commitment                                                   │
//! │     ])                                                            │
//! │     → On-chain: nullifier stored to prevent reuse                 │
//! │                                                                   │
//! │  4. EXPIRE (automatic)                                             │
//! │     After expiry height, intent cannot be consumed                │
//! │                                                                   │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Namespace Constants
//!
//! Each application should define its own namespace constant:
//! - Identity credentials: `0x0001` (example)
//! - DEX swaps: `0x0002` (example)
//! - Bridge deposits: `0x0003` (example)
//! - Stablecoin positions: `0x0004` (example)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use dwow_sdk::crypto::{PrivateIntent, IntentCommitment, IntentNullifier};
//!
//! // Create an intent
//! let intent = PrivateIntent::new(
//!     owner_pubkey,
//!     namespace,       // e.g., IDENTITY_NAMESPACE
//!     payload_hash,    // e.g., H(credential data)
//!     expiry,          // block height when expires
//!     nonce,           // fresh random value
//!     blind,           // additional blinding
//! );
//!
//! // Get commitment for on-chain storage
//! let commitment = intent.commitment();
//!
//! // Derive nullifier when consuming
//! let nullifier = intent.derive_nullifier(owner_secret)?;
//! ```

use core::str::FromStr;

use dwow_serial::{SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::PrimeField, pallas};

use super::{poseidon_hash, PublicKey, SecretKey};
use crate::{fp_from_bs58, fp_to_bs58, ty_from_fp, ContractError};

/// Domain separator for intent commitment hash
const INTENT_COMMITMENT_DOMAIN: u64 = 9001;
/// Domain separator for intent nullifier hash
const INTENT_NULLIFIER_DOMAIN: u64 = 9002;

/// A generic private intent state object.
///
/// This is deliberately app-agnostic:
/// - `namespace` scopes intents per application/protocol
/// - `payload_hash` commits to application-specific order/intent data
/// - `expiry` is block-height based and can be checked in-circuit or in contract logic
#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct PrivateIntent {
    /// Owner public key (who can consume this intent)
    pub owner: PublicKey,
    /// Application namespace to scope this intent
    pub namespace: pallas::Base,
    /// Hash of application-specific payload
    pub payload_hash: pallas::Base,
    /// Block height when this intent expires
    pub expiry: u64,
    /// Nonce to prevent nullifier replay
    pub nonce: pallas::Base,
    /// Blinding factor for the commitment
    pub blind: pallas::Base,
}

impl PrivateIntent {
    /// Construct a new `PrivateIntent`.
    pub fn new(
        owner: PublicKey,
        namespace: pallas::Base,
        payload_hash: pallas::Base,
        expiry: u64,
        nonce: pallas::Base,
        blind: pallas::Base,
    ) -> Self {
        Self { owner, namespace, payload_hash, expiry, nonce, blind }
    }

    /// Convenience constructor that derives the owner public key from secret key material.
    pub fn from_secret(
        owner_secret: SecretKey,
        namespace: pallas::Base,
        payload_hash: pallas::Base,
        expiry: u64,
        nonce: pallas::Base,
        blind: pallas::Base,
    ) -> Self {
        Self::new(
            PublicKey::from_secret(owner_secret),
            namespace,
            payload_hash,
            expiry,
            nonce,
            blind,
        )
    }

    /// Compute the commitment for this intent.
    ///
    /// Uses domain separator `9001` to prevent collision with other hashes.
    pub fn commitment(&self) -> IntentCommitment {
        // PublicKey constructor rejects identity, so xy() is always Some
        let (owner_x, owner_y) = self.owner.xy().expect("pk not identity");
        let commitment = poseidon_hash([
            pallas::Base::from(INTENT_COMMITMENT_DOMAIN),
            owner_x,
            owner_y,
            self.namespace,
            self.payload_hash,
            pallas::Base::from(self.expiry),
            self.nonce,
            self.blind,
        ]);
        IntentCommitment(commitment)
    }

    /// Derive a nullifier bound to this intent and owner secret key.
    ///
    /// Uses domain separator `9002` to prevent collision with other hashes.
    pub fn derive_nullifier(
        &self,
        owner_secret: SecretKey,
    ) -> Result<IntentNullifier, ContractError> {
        if PublicKey::from_secret(owner_secret.clone()) != self.owner {
            return Err(ContractError::IoError(
                "Intent nullifier derivation failed: owner secret mismatch".to_string(),
            ))
        }

        let nullifier = poseidon_hash([
            pallas::Base::from(INTENT_NULLIFIER_DOMAIN),
            *owner_secret.inner(),
            self.namespace,
            self.nonce,
            self.commitment().inner(),
        ]);

        Ok(IntentNullifier(nullifier))
    }

    /// Returns true if the intent is expired at the given block height.
    pub fn is_expired_at(&self, height: u64) -> bool {
        height >= self.expiry
    }
}

/// Commitment to a `PrivateIntent`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct IntentCommitment(pallas::Base);

impl IntentCommitment {
    /// Construct an IntentCommitment from a pallas::Base field element.
    /// Named constructor per §8.5 — no From<pallas::Base> impl.
    pub fn from_base(x: pallas::Base) -> Self {
        Self(x)
    }

    /// Get the inner pallas::Base value.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create from bytes.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError(
                "Failed to instantiate IntentCommitment from bytes".to_string(),
            )),
        }
    }

    /// Convert to bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

fp_from_bs58!(IntentCommitment);
fp_to_bs58!(IntentCommitment);

/// Nullifier for a `PrivateIntent`.
#[derive(Copy, Clone, Debug, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct IntentNullifier(pallas::Base);

impl IntentNullifier {
    /// Construct an IntentNullifier from a pallas::Base field element.
    /// Named constructor per §8.5 — no From<pallas::Base> impl.
    pub fn from_base(x: pallas::Base) -> Self {
        Self(x)
    }

    /// Get the inner pallas::Base value.
    pub fn inner(&self) -> pallas::Base {
        self.0
    }

    /// Create from bytes.
    pub fn from_bytes(x: [u8; 32]) -> Result<Self, ContractError> {
        match pallas::Base::from_repr(x).into() {
            Some(v) => Ok(Self(v)),
            None => Err(ContractError::IoError(
                "Failed to instantiate IntentNullifier from bytes".to_string(),
            )),
        }
    }

    /// Convert to bytes.
    pub fn to_bytes(&self) -> [u8; 32] {
        self.0.to_repr()
    }
}

fp_from_bs58!(IntentNullifier);
fp_to_bs58!(IntentNullifier);

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;

    use crate::crypto::pasta_prelude::Field;
    use crate::crypto::Keypair;

    #[test]
    fn intent_commitment_is_deterministic() {
        let keypair = Keypair::random(&mut OsRng);
        let intent = PrivateIntent::new(
            keypair.public,
            pallas::Base::from(11),
            pallas::Base::from(22),
            12345,
            pallas::Base::from(33),
            pallas::Base::from(44),
        );

        assert_eq!(intent.commitment(), intent.commitment());
    }

    #[test]
    fn intent_nullifier_changes_with_nonce() {
        let keypair = Keypair::random(&mut OsRng);
        let intent1 = PrivateIntent::new(
            keypair.public,
            pallas::Base::from(11),
            pallas::Base::from(22),
            12345,
            pallas::Base::from(33),
            pallas::Base::from(44),
        );
        let intent2 = PrivateIntent::new(
            keypair.public,
            pallas::Base::from(11),
            pallas::Base::from(22),
            12345,
            pallas::Base::from(34), // Different nonce
            pallas::Base::from(44),
        );

        let nullifier1 = intent1.derive_nullifier(keypair.secret.clone()).unwrap();
        let nullifier2 = intent2.derive_nullifier(keypair.secret).unwrap();

        assert_ne!(nullifier1, nullifier2);
    }

    #[test]
    fn intent_expires_at_correct_height() {
        let keypair = Keypair::random(&mut OsRng);
        let intent = PrivateIntent::new(
            keypair.public,
            pallas::Base::from(11),
            pallas::Base::from(22),
            100, // expires at block 100
            pallas::Base::from(33),
            pallas::Base::from(44),
        );

        assert!(!intent.is_expired_at(99));
        assert!(intent.is_expired_at(100));
        assert!(intent.is_expired_at(101));
    }
}
