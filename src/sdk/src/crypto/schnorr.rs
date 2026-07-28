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

use dwow_serial::{SerialDecodable, SerialEncodable};
use halo2_gadgets::ecc::chip::FixedPoint;
use pasta_curves::{
    group::{ff::PrimeField, Group, GroupEncoding},
    pallas,
};

use super::{
    constants::{NullifierK, DRK_SCHNORR_CHALLENGE_DOMAIN, DRK_SCHNORR_NONCE_DOMAIN},
    util::{fp_mod_fv, hash_to_scalar},
    PublicKey, SecretKey,
};

/// Schnorr signature with a commit and response
#[derive(Debug, Clone, Copy, Eq, PartialEq, SerialEncodable, SerialDecodable)]
pub struct Signature {
    commit: pallas::Point,
    response: pallas::Scalar,
}

impl Signature {
    /// Return a dummy identity `Signature`
    pub fn dummy() -> Self {
        Self { commit: pallas::Point::identity(), response: pallas::Scalar::zero() }
    }

    /// Encode to canonical fixed-width bytes (64 bytes: commit || response).
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(64);
        buf.extend_from_slice(&self.commit.to_bytes());
        buf.extend_from_slice(&self.response.to_repr());
        buf
    }

    /// Decode from canonical fixed-width bytes (64 bytes: commit || response).
    /// Returns None if the bytes are invalid.
    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() != 64 {
            return None;
        }
        let bytes32: &[u8; 32] = data[0..32].try_into().ok()?;
        let commit = pallas::Point::from_bytes(bytes32).into_option()?;
        let bytes32: &[u8; 32] = data[32..64].try_into().ok()?;
        let response = pallas::Scalar::from_repr(*bytes32).into_option()?;
        Some(Self { commit, response })
    }
}

/// Trait for secret keys that implements a signature creation
pub trait SchnorrSecret {
    /// Sign a given message
    fn sign(&self, message: &[u8]) -> Signature;
}

/// Trait for public keys that implements a signature verification
pub trait SchnorrPublic {
    /// Verify a given message is valid given a signature.
    fn verify(&self, message: &[u8], signature: &Signature) -> bool;
}

/// Schnorr signature trait implementations for the stuff in `keypair.rs`
impl SchnorrSecret for SecretKey {
    fn sign(&self, message: &[u8]) -> Signature {
        // Derive a deterministic nonce (RFC 6979 pattern).
        // Uses a distinct domain separator from the challenge per type-system.md §2.1.
        let mask = hash_to_scalar(DRK_SCHNORR_NONCE_DOMAIN, &[&self.inner().to_repr(), message]);

        let commit = NullifierK.generator() * mask;

        let commit_bytes = commit.to_bytes();
        let pubkey_bytes = PublicKey::from_secret(self.clone()).to_bytes();
        let transcript = &[&commit_bytes, &pubkey_bytes, message];

        let challenge = hash_to_scalar(DRK_SCHNORR_CHALLENGE_DOMAIN, transcript);
        let response = mask + challenge * fp_mod_fv(*self.inner());

        Signature { commit, response }
    }
}

impl SchnorrPublic for PublicKey {
    fn verify(&self, message: &[u8], signature: &Signature) -> bool {
        let commit_bytes = signature.commit.to_bytes();
        let pubkey_bytes = self.to_bytes();
        let transcript = &[&commit_bytes, &pubkey_bytes, message];

        let challenge = hash_to_scalar(DRK_SCHNORR_CHALLENGE_DOMAIN, transcript);
        NullifierK.generator() * signature.response - self.inner() * challenge == signature.commit
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dwow_serial::{deserialize, serialize};
    use rand::rngs::OsRng;

    #[test]
    fn test_schnorr_signature() {
        let secret = SecretKey::random(&mut OsRng);
        let message: &[u8] = b"aaaahhhh i'm signiiinngg";
        let signature = secret.sign(message);
        let public = PublicKey::from_secret(secret);
        assert!(public.verify(message, &signature));

        // Check out if it's also fine with serialization
        let ser = serialize(&signature);
        let de = deserialize(&ser).unwrap();
        assert!(public.verify(message, &de));
    }
}
