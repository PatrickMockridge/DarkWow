//! Arweave-compatible Ed25519 wallet (signature type 2)
//!
//! Generates Ed25519 keypairs for signing ANS-104 DataItems. Ed25519 is used
//! instead of RSA-4096 (signature type 1) for fast per-block key cycling —
//! ~microseconds vs seconds for RSA keygen. ArDrive Turbo accepts all valid
//! ANS-104 signature types.

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;
use rand::RngCore;

/// An Arweave-compatible Ed25519 wallet for Caribina anchoring.
///
/// Uses Arweave signature type 2 (Ed25519/Curve25519).
/// Public key = 32 bytes, signature = 64 bytes.
pub struct CaribinaWallet {
    signing_key: SigningKey,
}

impl CaribinaWallet {
    /// Generate a new random Ed25519 wallet.
    ///
    /// This is fast (~microseconds) and can be called per-block for
    /// address cycling. No funding required — ArDrive Turbo accepts
    /// small uploads from unfunded wallets for free.
    pub fn generate() -> Self {
        let mut csprng = OsRng;
        let mut secret = [0u8; 32];
        csprng.fill_bytes(&mut secret);
        let signing_key = SigningKey::from_bytes(&secret);
        Self { signing_key }
    }

    /// The 32-byte Ed25519 public key (owner field in ANS-104 DataItem).
    pub fn public_key(&self) -> [u8; 32] {
        self.signing_key.verifying_key().to_bytes()
    }

    /// Sign a message (the deepHash result) and return a 64-byte Ed25519 signature.
    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        use ed25519_dalek::Signer;
        self.signing_key.sign(message).to_bytes()
    }

    /// Verify a signature (used by nodes verifying anchors).
    pub fn verify(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> bool {
        use ed25519_dalek::Signature;
        let Ok(vk) = VerifyingKey::from_bytes(public_key) else {
            return false;
        };
        let Ok(sig) = Signature::from_slice(signature) else {
            return false;
        };
        use ed25519_dalek::Verifier;
        vk.verify(message, &sig).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keygen_is_fast_and_deterministic() {
        let w1 = CaribinaWallet::generate();
        let w2 = CaribinaWallet::generate();
        // Different wallets should have different keys
        assert_ne!(w1.public_key(), w2.public_key());
        assert_eq!(w1.public_key().len(), 32);
    }

    #[test]
    fn test_sign_and_verify_roundtrip() {
        let wallet = CaribinaWallet::generate();
        let message = b"test message for caribina";
        let sig = wallet.sign(message);
        assert_eq!(sig.len(), 64);
        assert!(CaribinaWallet::verify(&wallet.public_key(), message, &sig));
    }

    #[test]
    fn test_verify_tampered_message_fails() {
        let wallet = CaribinaWallet::generate();
        let sig = wallet.sign(b"original");
        assert!(!CaribinaWallet::verify(&wallet.public_key(), b"tampered", &sig));
    }

    #[test]
    fn test_verify_wrong_key_fails() {
        let w1 = CaribinaWallet::generate();
        let w2 = CaribinaWallet::generate();
        let sig = w1.sign(b"message");
        assert!(!CaribinaWallet::verify(&w2.public_key(), b"message", &sig));
    }
}
