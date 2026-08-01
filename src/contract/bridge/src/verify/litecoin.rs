//! Litecoin SHA-256d Merkle Proof verification.
//!
//! Litecoin uses SHA-256d (double-SHA256) for block hashing and Bitcoin-style
//! binary Merkle trees for transaction inclusion proofs.
//!
//! Layer 1 (structural): amount minimum, confirmations, non-zero values.
//! Layer 2 (cryptographic): SHA-256d Merkle proof against block header.

use sha2::{Sha256, Digest};
use dwow_sdk::error::ContractResult;
use crate::error::BridgeError;
use crate::model::LitecoinDepositProof;

/// Minimum Litecoin deposit in satoshis (0.001 LTC).
const MIN_LTC_DEPOSIT: u64 = 100_000;
/// Required block confirmations for Litecoin deposits.
const LTC_CONFIRMATIONS: u64 = 6;

/// Compute SHA-256d (double SHA-256) of data — Bitcoin/Litecoin standard.
fn sha256d(data: &[u8]) -> [u8; 32] {
    let first = Sha256::digest(data);
    let second = Sha256::digest(&first);
    let mut result = [0u8; 32];
    result.copy_from_slice(&second);
    result
}

/// Verify a Litecoin deposit proof (Merkle path against block header).
///
/// Layer 1: structural checks (amount, confirmations, non-zero).
/// Layer 2: cryptographic Merkle proof verification via SHA-256d.
pub fn verify_merkle_proof(proof: &LitecoinDepositProof) -> ContractResult {
    // --- Layer 1: Structural checks ---

    if proof.tx_hash.iter().all(|&b| b == 0) {
        return Err(BridgeError::InvalidDeposit("Litecoin tx_hash is zero".into()).into());
    }
    if proof.block_merkle_root.iter().all(|&b| b == 0) {
        return Err(BridgeError::InvalidMerkleProof.into());
    }
    if proof.block_height == 0 {
        return Err(BridgeError::InvalidDeposit("Litecoin block_height is zero".into()).into());
    }
    if proof.amount < MIN_LTC_DEPOSIT {
        return Err(BridgeError::InvalidDeposit(
            format!("Litecoin amount {} below minimum {}", proof.amount, MIN_LTC_DEPOSIT).into()
        ).into());
    }
    if proof.confirmations < LTC_CONFIRMATIONS {
        return Err(BridgeError::InsufficientConfirmations.into());
    }
    if proof.merkle_proof.is_empty() {
        return Err(BridgeError::InvalidMerkleProof.into());
    }

    // --- Layer 2: SHA-256d Merkle proof verification ---

    let mut current: [u8; 32] = proof.tx_hash;

    for sibling in &proof.merkle_proof {
        // Lexicographic ordering: the smaller hash goes first (Bitcoin convention).
        // Actually, Bitcoin's Merkle tree concatenates in the order: current then sibling,
        // regardless of which is smaller. The proof provides the sibling at each level.
        let mut concat = Vec::with_capacity(64);
        concat.extend_from_slice(&current);
        concat.extend_from_slice(sibling);
        current = sha256d(&concat);
    }

    // Verify computed root matches block's merkle root
    if current != proof.block_merkle_root {
        return Err(BridgeError::InvalidMerkleProof.into());
    }

    // MWEB Bulletproof verification deferred (requires bulletproof crate).
    // For non-confidential transactions, Merkle proof alone is sufficient.
    if proof.is_confidential {
        if proof.range_proof.is_none() {
            return Err(BridgeError::InvalidDeposit(
                "MWEB confidential transaction requires range proof".into()
            ).into());
        }
        // FIXME(litecoin-mweb): verify Bulletproof range proof for confidential amount
    }

    Ok(())
}
