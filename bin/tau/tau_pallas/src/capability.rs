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

//! O-Capability verification module for tau task delegation.
//!
//! This module provides capability-based authorization for task claiming,
//! allowing workers to prove qualifications without revealing identity.
//!
//! ## Verification Modes
//!
//! - **OffChain (Hot Path)**: For trusted workers - fast, local verification
//! - **OnChain (Cold Path)**: For unproven workers - full ZK verification via Identity contract

use std::sync::Arc;

use darkfi::tx::{ContractCallLeaf, TransactionBuilder};
use darkfi_sdk::{
    crypto::{ContractId, Keypair},
    tx::{ContractCall, TransactionHash},
};
use darkfi_serial::{SerialDecodable, SerialEncodable};
use smol::Executor;

use crate::error::{TauPallasError, TauPallasResult};
use crate::rpc_client::DarkfidClient;
use crate::identity_client::ClientCapabilityProof;

/// A proof of capability presented to verify task claim authorization.
///
/// This structure contains all data needed to verify a capability claim
/// without revealing the holder's identity.
#[derive(Clone, Debug, SerialEncodable, SerialDecodable, PartialEq)]
pub struct CapabilityProof {
    /// Hash of the capability definition (what's being proven)
    pub capability_id: [u8; 32],
    /// Nullifier from the underlying credential (proves credential exists)
    pub nullifier: [u8; 32],
    /// Public predicate result (1 if satisfied, 0 if not)
    pub predicate_result: u8,
    /// Issuer's public key (who issued the capability)
    pub issuer_pub: [u8; 32],
    /// Schema hash of the underlying credential
    pub schema_hash: [u8; 32],
    /// ZK proof of capability satisfaction
    pub proof: Vec<u8>,
    /// Capability secret (proves holder owns this capability)
    pub capability_secret: [u8; 32],
    /// Timestamp when proof was created
    pub created_at: u64,
}

/// Result of capability verification
#[derive(Debug)]
pub struct VerificationResult {
    /// Whether the proof is valid
    pub valid: bool,
    /// The capability_id that was verified
    pub capability_id: [u8; 32],
    /// The predicate result (1 = requirements met)
    pub predicate_result: u8,
    /// Transaction hash if on-chain verification was used
    pub tx_hash: Option<TransactionHash>,
}

/// Default expiry time for off-chain verified proofs (7 days in seconds)
const DEFAULT_EXPIRY_SECS: u64 = 7 * 24 * 60 * 60;

/// Off-chain capability verification.
///
/// For trusted workers - performs local verification of capability proof.
/// This is the "hot path" for fast verification of known/trusted workers.
///
/// # Arguments
///
/// * `proof` - The capability proof to verify
/// * `required_capability_id` - The capability ID required for the task
///
/// # Returns
///
/// Returns `Ok(VerificationResult)` if verification succeeds.
pub fn verify_capability_offchain(
    proof: &CapabilityProof,
    required_capability_id: &[u8; 32],
) -> TauPallasResult<VerificationResult> {
    // 1. Check capability_id matches what the task requires
    if proof.capability_id != *required_capability_id {
        return Ok(VerificationResult {
            valid: false,
            capability_id: proof.capability_id,
            predicate_result: proof.predicate_result,
            tx_hash: None,
        })
    }

    // 2. Verify predicate result indicates requirements are met
    if proof.predicate_result != 1 {
        return Ok(VerificationResult {
            valid: false,
            capability_id: proof.capability_id,
            predicate_result: proof.predicate_result,
            tx_hash: None,
        })
    }

    // 3. Check not expired (basic timestamp check)
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if proof.created_at + DEFAULT_EXPIRY_SECS < current_time {
        return Ok(VerificationResult {
            valid: false,
            capability_id: proof.capability_id,
            predicate_result: proof.predicate_result,
            tx_hash: None,
        })
    }

    // Note: In a full implementation, we would also verify:
    // - The issuer's signature over the proof data
    // - The nullifier is valid for the credential
    // - The capability_secret is bound to the capability_id

    Ok(VerificationResult {
        valid: true,
        capability_id: proof.capability_id,
        predicate_result: proof.predicate_result,
        tx_hash: None,
    })
}

/// On-chain capability verification via Identity contract.
///
/// For unproven workers - performs full ZK proof verification via the
/// Identity contract's VerifyCapabilityV1 (0x0b).
///
/// This is the "cold path" for maximum security when dealing with
/// workers who haven't established trust.
///
/// # Flow
///
/// 1. Build transaction calling identity::VerifyCapabilityV1
/// 2. Sign transaction with verifier's (PM's) Pallas secret key
/// 3. Broadcast via darkfid RPC: `tx.broadcast`
/// 4. The transaction is verified on-chain and emits CapabilityVerified event
///
/// # Arguments
///
/// * `proof` - The capability proof to verify
/// * `required_capability_id` - The capability ID required for the task
/// * `verifier_keypair` - The keypair of who is requesting verification (PM)
/// * `identity_contract_id` - The ContractId of the identity contract
/// * `darkfid_client` - RPC client for darkfid
/// * `executor` - Async executor for RPC calls
///
/// # Returns
///
/// Returns `Ok(VerificationResult)` with verification outcome.
/// Note: This sends the transaction to the network for verification.
/// The result's `valid` field indicates if the tx was broadcast successfully.
/// Actual on-chain verification happens asynchronously.
pub async fn verify_capability_onchain(
    proof: &CapabilityProof,
    required_capability_id: &[u8; 32],
    verifier_keypair: &Keypair,
    identity_contract_id: ContractId,
    darkfid_client: &DarkfidClient,
    _executor: Arc<Executor<'static>>,
) -> TauPallasResult<VerificationResult> {
    // First do basic validation
    if proof.capability_id != *required_capability_id {
        return Ok(VerificationResult {
            valid: false,
            capability_id: proof.capability_id,
            predicate_result: proof.predicate_result,
            tx_hash: None,
        })
    }

    if proof.predicate_result != 1 {
        return Ok(VerificationResult {
            valid: false,
            capability_id: proof.capability_id,
            predicate_result: proof.predicate_result,
            tx_hash: None,
        })
    }

    // Convert to client capability proof for calldata building
    let client_proof = ClientCapabilityProof {
        capability_id: proof.capability_id,
        nullifier: proof.nullifier,
        predicate_result: proof.predicate_result,
        issuer_pub: proof.issuer_pub,
        schema_hash: proof.schema_hash,
        proof: proof.proof.clone(),
        capability_secret: proof.capability_secret,
        created_at: proof.created_at,
    };

    // Build the calldata for VerifyCapabilityV1 (0x0b)
    let verifier_pub = verifier_keypair.public.to_bytes();
    let calldata =
        crate::identity_client::build_verify_capability_calldata(&client_proof, verifier_pub, 0)?;

    // Create the contract call
    let call = ContractCall { contract_id: identity_contract_id, data: calldata };

    // Build the transaction using DarkTree
    let call_leaf = ContractCallLeaf { call, proofs: vec![] };
    let mut tx_builder = TransactionBuilder::new(call_leaf, vec![])
        .map_err(|e| TauPallasError::TransactionError(format!("Failed to build tx: {}", e)))?;

    let mut tx = tx_builder
        .build()
        .map_err(|e| TauPallasError::TransactionError(format!("Failed to build tx: {}", e)))?;

    // Sign the transaction with the verifier's secret key
    let sigs = tx
        .create_sigs(&[verifier_keypair.secret])
        .map_err(|e| TauPallasError::TransactionError(format!("Failed to sign tx: {}", e)))?;

    tx.signatures.push(sigs);

    // Broadcast the transaction via darkfid RPC
    let tx_hash = darkfid_client
        .broadcast_tx(&tx)
        .await
        .map_err(|e| TauPallasError::RpcError(format!("Failed to broadcast tx: {}", e)))?;

    tracing::info!(
        target: "tau_pallas",
        "Broadcasted capability verification tx: {:?}",
        tx_hash
    );

    Ok(VerificationResult {
        valid: true,
        capability_id: proof.capability_id,
        predicate_result: proof.predicate_result,
        tx_hash: Some(tx_hash),
    })
}

/// Parse a capability proof from JSON parameters.
///
/// # Arguments
///
/// * `params` - JSON value containing proof fields
///
/// # Returns
///
/// Returns `Ok(CapabilityProof)` if parsing succeeds.
pub fn parse_capability_proof(params: &tinyjson::JsonValue) -> TauPallasResult<CapabilityProof> {
    use std::str::FromStr;

    let get_field = |key: &str| -> TauPallasResult<String> {
        params[key]
            .get::<String>()
            .map(|s| s.clone())
            .ok_or_else(|| TauPallasError::ParseFailed(key.to_string()).into())
    };

    let get_bytes32 = |key: &str| -> TauPallasResult<[u8; 32]> {
        let s = get_field(key)?;
        let decoded = bs58::decode(&s).into_vec().map_err(|_| {
            TauPallasError::ParseFailed(format!("Invalid bs58 for {}", key))
        })?;
        decoded.as_slice().try_into().map_err(|_| {
            TauPallasError::ParseFailed(format!("Invalid length for {}", key))
        })
    };

    let get_vec_u8 = |key: &str| -> TauPallasResult<Vec<u8>> {
        let s = get_field(key)?;
        bs58::decode(&s).into_vec().map_err(|_| {
            TauPallasError::ParseFailed(format!("Invalid bs58 for {}", key))
        })
    };

    let get_u64 = |key: &str| -> TauPallasResult<u64> {
        let s = get_field(key)?;
        u64::from_str(&s).map_err(|_| {
            TauPallasError::ParseFailed(format!("Invalid u64 for {}", key))
        })
    };

    Ok(CapabilityProof {
        capability_id: get_bytes32("capability_id")?,
        nullifier: get_bytes32("nullifier")?,
        predicate_result: get_u64("predicate_result")? as u8,
        issuer_pub: get_bytes32("issuer_pub")?,
        schema_hash: get_bytes32("schema_hash")?,
        proof: get_vec_u8("proof")?,
        capability_secret: get_bytes32("capability_secret")?,
        created_at: get_u64("created_at")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verify_capability_offchain_valid() {
        let proof = CapabilityProof {
            capability_id: [1u8; 32],
            nullifier: [2u8; 32],
            predicate_result: 1,
            issuer_pub: [3u8; 32],
            schema_hash: [4u8; 32],
            proof: vec![],
            capability_secret: [5u8; 32],
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let result = verify_capability_offchain(&proof, &[1u8; 32]).unwrap();
        assert!(result.valid);
        assert_eq!(result.capability_id, [1u8; 32]);
        assert!(result.tx_hash.is_none());
    }

    #[test]
    fn test_verify_capability_offchain_capability_mismatch() {
        let proof = CapabilityProof {
            capability_id: [1u8; 32],
            nullifier: [2u8; 32],
            predicate_result: 1,
            issuer_pub: [3u8; 32],
            schema_hash: [4u8; 32],
            proof: vec![],
            capability_secret: [5u8; 32],
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let result = verify_capability_offchain(&proof, &[2u8; 32]).unwrap();
        assert!(!result.valid);
    }

    #[test]
    fn test_verify_capability_offchain_predicate_failed() {
        let proof = CapabilityProof {
            capability_id: [1u8; 32],
            nullifier: [2u8; 32],
            predicate_result: 0, // Failed
            issuer_pub: [3u8; 32],
            schema_hash: [4u8; 32],
            proof: vec![],
            capability_secret: [5u8; 32],
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        };

        let result = verify_capability_offchain(&proof, &[1u8; 32]).unwrap();
        assert!(!result.valid);
    }
}
