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

use std::io::Cursor;

use blake3;
use chacha20poly1305::{AeadInPlace, ChaCha20Poly1305, KeyInit};
use dwow_serial::{Decodable, Encodable, SerialDecodable, SerialEncodable};
use pasta_curves::{group::ff::Field, pallas};
use rand_core::{CryptoRng, RngCore};

use super::{diffie_hellman, poseidon_hash, util::fp_mod_fv, PublicKey, SecretKey};
use crate::error::ContractError;

/// AEAD tag length in bytes
pub const AEAD_TAG_SIZE: usize = 16;

/// An encrypted note using Diffie-Hellman and ChaCha20Poly1305
#[derive(Debug, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct AeadEncryptedNote {
    pub ciphertext: Vec<u8>,
    pub ephem_public: PublicKey,
}

impl AeadEncryptedNote {
    /// Derive a 12-byte AEAD nonce from the ephemeral public key.
    /// Deterministic — same ephem_public always produces the same nonce.
    /// Closes: M7 (AEAD nonce is zero — fragile, safe only by caller
    /// invariant). Enforces: defense-in-depth against nonce-reuse.
    fn derive_nonce(ephem_public: &PublicKey) -> [u8; 12] {
        let hash = blake3::hash(ephem_public.to_bytes().as_ref());
        let mut nonce = [0u8; 12];
        nonce.copy_from_slice(&hash.as_bytes()[..12]);
        nonce
    }

    pub fn encrypt(
        note: &impl Encodable,
        public: &PublicKey,
        rng: &mut (impl CryptoRng + RngCore),
    ) -> Result<Self, ContractError> {
        let ephem_secret = SecretKey::random(rng);
        Self::encrypt_with_ephem(note, public, ephem_secret)
    }

    /// Deterministic encryption — takes an explicitly provided ephemeral secret
    /// instead of generating a random one. Used at genesis (consensus-coinbase.md
    /// §2.7: "no random keys") to ensure the genesis block hash is reproducible.
    /// The ephemeral secret MUST be derived deterministically from sk_H with a
    /// unique domain separator to prevent key reuse.
    pub fn encrypt_deterministic(
        note: &impl Encodable,
        public: &PublicKey,
        ephem_secret: SecretKey,
    ) -> Result<Self, ContractError> {
        Self::encrypt_with_ephem(note, public, ephem_secret)
    }

    /// Shared encryption implementation — accepts any ephemeral secret.
    fn encrypt_with_ephem(
        note: &impl Encodable,
        public: &PublicKey,
        ephem_secret: SecretKey,
    ) -> Result<Self, ContractError> {
        let shared_secret = diffie_hellman::sapling_ka_agree(&ephem_secret, public)?;
        let ephem_public = PublicKey::from_secret(ephem_secret);
        let key = diffie_hellman::kdf_sapling(&shared_secret, &ephem_public);

        let mut input = Vec::new();
        note.encode(&mut input)?;
        let input_len = input.len();

        let mut ciphertext = vec![0_u8; input_len + AEAD_TAG_SIZE];
        ciphertext[..input_len].copy_from_slice(&input);

        // Nonce derived from ephem_public: defense-in-depth against nonce
        // reuse. If key uniqueness is ever broken (RNG failure, deterministic
        // key derivation bug), a random nonce prevents catastrophic loss of
        // ChaCha20Poly1305 confidentiality/authenticity. Closes: M7.
        let nonce = Self::derive_nonce(&ephem_public);
        ChaCha20Poly1305::new(key.as_ref().into())
            .encrypt_in_place(nonce[..].into(), &[], &mut ciphertext)
            .unwrap();

        Ok(Self { ciphertext, ephem_public })
    }

    pub fn decrypt<D: Decodable>(&self, secret: &SecretKey) -> Result<D, ContractError> {
        let shared_secret = diffie_hellman::sapling_ka_agree(secret, &self.ephem_public)?;
        let key = diffie_hellman::kdf_sapling(&shared_secret, &self.ephem_public);

        let ct_len = self.ciphertext.len();
        let mut plaintext = vec![0_u8; ct_len];
        plaintext.copy_from_slice(&self.ciphertext);

        // Try derived nonce first, fall back to zero nonce for legacy notes
        // encrypted before the M7 fix. Closes: M7. Enforces: defense-in-depth.
        let nonce = Self::derive_nonce(&self.ephem_public);
        let result = ChaCha20Poly1305::new(key.as_ref().into())
            .decrypt_in_place(nonce[..].into(), &[], &mut plaintext)
            .or_else(|_| {
                plaintext.copy_from_slice(&self.ciphertext);
                ChaCha20Poly1305::new(key.as_ref().into())
                    .decrypt_in_place([0u8; 12][..].into(), &[], &mut plaintext)
            });

        match result {
            Ok(()) => {
                let mut cursor = Cursor::new(&plaintext[..ct_len - AEAD_TAG_SIZE]);
                Ok(D::decode(&mut cursor)?)
            }
            Err(e) => Err(ContractError::IoError(format!("Note decrypt failed: {e}"))),
        }
    }

    /// Decrypt to the raw serialized note bytes — exactly `encode(note)`, with no
    /// `Decodable` interpretation.
    ///
    /// The encrypt path pre-sizes the buffer with a tag-sized zero pad, so the
    /// decrypted plaintext is `encode(note) || [0u8; AEAD_TAG_SIZE]` and the
    /// ciphertext is `encode(note).len() + 2*AEAD_TAG_SIZE` (one tag pad that
    /// becomes trailing plaintext, one appended authentication tag). `decrypt::<D>`
    /// tolerates the trailing pad because `D::decode` reads from the front; a
    /// schema-driven generic decode needs the exact note bytes, so we strip both.
    pub fn decrypt_raw(&self, secret: &SecretKey) -> Result<Vec<u8>, ContractError> {
        let shared_secret = diffie_hellman::sapling_ka_agree(secret, &self.ephem_public)?;
        let key = diffie_hellman::kdf_sapling(&shared_secret, &self.ephem_public);

        let ct_len = self.ciphertext.len();
        let mut plaintext = vec![0_u8; ct_len];
        plaintext.copy_from_slice(&self.ciphertext);

        let nonce = Self::derive_nonce(&self.ephem_public);
        let result = ChaCha20Poly1305::new(key.as_ref().into())
            .decrypt_in_place(nonce[..].into(), &[], &mut plaintext)
            .or_else(|_| {
                plaintext.copy_from_slice(&self.ciphertext);
                ChaCha20Poly1305::new(key.as_ref().into())
                    .decrypt_in_place([0u8; 12][..].into(), &[], &mut plaintext)
            });

        match result {
            Ok(()) => {
                let note_len = ct_len.checked_sub(2 * AEAD_TAG_SIZE).ok_or_else(|| {
                    ContractError::IoError("Note ciphertext shorter than padding".to_string())
                })?;
                plaintext.truncate(note_len);
                Ok(plaintext)
            }
            Err(e) => Err(ContractError::IoError(format!("Note decrypt failed: {e}"))),
        }
    }
}

/// An encrypted note using an ElGamal scheme verifiable in ZK.
///
/// **WARNING:**
/// Without ZK, there is no authentication of the ciphertexts so these should
/// not be used without a corresponding ZK proof.
#[derive(Debug, Copy, Clone, PartialEq, Eq, SerialEncodable, SerialDecodable)]
pub struct ElGamalEncryptedNote<const N: usize> {
    /// The values encrypted with the derived shared secret using Diffie-Hellman
    pub encrypted_values: [pallas::Base; N],
    /// The ephemeral public key used for Diffie-Hellman key derivation
    pub ephem_public: PublicKey,
}

impl<const N: usize> ElGamalEncryptedNote<N> {
    /// Encrypt given values to the given `PublicKey` using a `SecretKey` for Diffie-Hellman
    ///
    /// Note that this does not do any message authentication.
    /// This means that alterations of the ciphertexts lead to the same alterations
    /// on the plaintexts.
    pub fn encrypt_unsafe(
        values: [pallas::Base; N],
        ephem_secret: &SecretKey,
        public: &PublicKey,
    ) -> Result<Self, ContractError> {
        // Derive shared secret using DH
        let ephem_public = PublicKey::from_secret(ephem_secret.clone());
        let (ss_x, ss_y) =
            PublicKey::try_from(public.inner() * fp_mod_fv(ephem_secret.inner()))?
                .xy().ok_or_else(|| ContractError::IoError(
                    "ElGamal encrypt: derived point is identity".to_string()))?;
        let shared_secret = poseidon_hash([ss_x, ss_y]);

        // Derive the blinds using the shared secret and incremental nonces
        let mut blinds = [pallas::Base::ZERO; N];
        for (i, item) in blinds.iter_mut().enumerate().take(N) {
            *item = poseidon_hash([shared_secret, pallas::Base::from(i as u64 + 1)]);
        }

        // Encrypt the values
        let mut encrypted_values = [pallas::Base::ZERO; N];
        for i in 0..N {
            encrypted_values[i] = values[i] + blinds[i];
        }

        Ok(Self { encrypted_values, ephem_public })
    }

    /// Decrypt the `ElGamalEncryptedNote` using a `SecretKey` for shared secret derivation
    /// using Diffie-Hellman
    ///
    /// Note that this does not do any message authentication.
    /// This means that alterations of the ciphertexts lead to the same alterations
    /// on the plaintexts.
    pub fn decrypt_unsafe(&self, secret: &SecretKey) -> Result<[pallas::Base; N], ContractError> {
        // Derive shared secret using DH
        let (ss_x, ss_y) =
            PublicKey::try_from(self.ephem_public.inner() * fp_mod_fv(secret.inner()))?
                .xy().ok_or_else(|| ContractError::IoError(
                    "ElGamal decrypt: derived point is identity".to_string()))?;
        let shared_secret = poseidon_hash([ss_x, ss_y]);

        let mut blinds = [pallas::Base::ZERO; N];
        for (i, item) in blinds.iter_mut().enumerate().take(N) {
            *item = poseidon_hash([shared_secret, pallas::Base::from(i as u64 + 1)]);
        }

        let mut decrypted_values = [pallas::Base::ZERO; N];
        for i in 0..N {
            decrypted_values[i] = self.encrypted_values[i] - blinds[i];
        }

        Ok(decrypted_values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::Keypair;

    use rand::rngs::OsRng;

    #[test]
    fn test_aead_note() {
        let plaintext = "gm world";
        let keypair = Keypair::random(&mut OsRng);

        let encrypted_note =
            AeadEncryptedNote::encrypt(&plaintext, &keypair.public, &mut OsRng).unwrap();

        let plaintext2: String = encrypted_note.decrypt(&keypair.secret).unwrap();

        assert_eq!(plaintext, plaintext2);
    }

    #[test]
    fn test_decrypt_raw() {
        // decrypt_raw returns the exact serialized plaintext (no VarInt prefix
        // reinterpretation, unlike decrypt::<Vec<u8>>).
        let keypair = Keypair::random(&mut OsRng);
        let value = 12345u64;
        let note = AeadEncryptedNote::encrypt(&value, &keypair.public, &mut OsRng).unwrap();
        let raw = note.decrypt_raw(&keypair.secret).unwrap();
        assert_eq!(raw, dwow_serial::serialize(&value));

        // Wrong key must fail cleanly.
        let wrong = SecretKey::random(&mut OsRng);
        assert!(note.decrypt_raw(&wrong).is_err());
    }

    /// Full pipeline test: encrypt → encode → decode → decrypt.
    /// This verifies the EXACT path that a coinbase encrypted_note takes:
    /// encrypt with public key → encode to bytes (coinbase.encrypted_note) →
    /// decode from bytes (scan reads from sled) → decrypt with secret key.
    /// Uses the known test key (hex 0x00...01) from keys.toml.
    #[test]
    fn test_aead_full_pipeline_with_test_key() {
        use crate::crypto::keypair::{PublicKey, SecretKey};

        let test_secret_bytes: [u8; 32] = [
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let sk = SecretKey::from_bytes(test_secret_bytes)
            .expect("test key 0x01 must be valid");
        let pk = PublicKey::from_secret(sk);

        // Known plaintext
        let plaintext: Vec<u8> = (0..200u8).collect();

        // Encrypt → encode (miner path: build_linear_coinbase)
        let note = AeadEncryptedNote::encrypt(&plaintext, &pk, &mut OsRng).unwrap();
        let mut encoded = vec![];
        note.encode(&mut encoded).unwrap();

        // Decode → decrypt (wallet scan path)
        let decoded = AeadEncryptedNote::decode(&mut std::io::Cursor::new(&encoded))
            .expect("decode must succeed");
        let decrypted: Vec<u8> = decoded.decrypt(&sk)
            .expect("decrypt with correct key must succeed");
        assert_eq!(decrypted, plaintext,
            "full AEAD pipeline: encrypt→encode→decode→decrypt must be lossless");

        // Wrong key must fail
        let wrong_sk = SecretKey::random(&mut OsRng);
        let result: Result<Vec<u8>, _> = decoded.decrypt(&wrong_sk);
        assert!(result.is_err(), "decrypt with wrong key must fail");
    }

    #[test]
    fn test_elgamal_note() {
        const N_MSGS: usize = 10;

        let plain_values = [pallas::Base::random(&mut OsRng); N_MSGS];
        let keypair = Keypair::random(&mut OsRng);
        let ephem_secret = SecretKey::random(&mut OsRng);

        let encrypted_note =
            ElGamalEncryptedNote::encrypt_unsafe(plain_values, &ephem_secret, &keypair.public)
                .unwrap();

        let decrypted_values = encrypted_note.decrypt_unsafe(&keypair.secret).unwrap();

        assert_eq!(plain_values, decrypted_values);
    }
}
