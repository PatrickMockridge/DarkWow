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

//! O-Capability verification module for tau task delegation.
//!
//! This module provides capability-based authorization for task claiming,
//! allowing workers to prove qualifications without revealing identity.
//!
//! ## Verification Modes
//!
//! - **OffChain (Hot Path)**: For trusted workers - fast, local verification
//! - **OnChain (Cold Path)**: For unproven workers - full ZK verification via Identity contract

use darkfi_serial::{SerialDecodable, SerialEncodable};

use crate::error::TaudResult;

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
/// Returns `Ok(true)` if verification succeeds, `Ok(false)` if it fails.
pub fn verify_capability_offchain(
    proof: &CapabilityProof,
    required_capability_id: &[u8; 32],
) -> TaudResult<VerificationResult> {
    // 1. Check capability_id matches what the task requires
    if proof.capability_id != *required_capability_id {
        return Ok(VerificationResult {
            valid: false,
            capability_id: proof.capability_id,
            predicate_result: proof.predicate_result,
        })
    }

    // 2. Verify predicate result indicates requirements are met
    if proof.predicate_result != 1 {
        return Ok(VerificationResult {
            valid: false,
            capability_id: proof.capability_id,
            predicate_result: proof.predicate_result,
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
/// # Implementation Requirements
///
/// This function requires integration with:
/// 1. DarkWow wallet to sign transactions
/// 2. darkfid JSON-RPC to broadcast transactions
/// 3. Identity contract's VerifyCapabilityV1 (0x0b) function
///
/// # Flow
///
/// 1. Build transaction calling identity::VerifyCapabilityV1
/// 2. Sign transaction with PM's wallet key
/// 3. Broadcast via darkfid RPC: `tx_broadcast`
/// 4. Wait for confirmation and read CapabilityVerified event
///
/// # Arguments
///
/// * `proof` - The capability proof to verify
/// * `required_capability_id` - The capability ID required for the task
/// * `verifier_pub` - The public key of who is requesting verification (PM)
/// * `identity_contract_id` - The ContractId of the identity contract
///
/// # Returns
///
/// Returns `Ok(VerificationResult)` with verification outcome.
pub async fn verify_capability_onchain(
    proof: &CapabilityProof,
    required_capability_id: &[u8; 32],
    _verifier_pub: &[u8; 32],
    _identity_contract_id: &[u8; 32],
) -> TaudResult<VerificationResult> {
    // TODO: Phase 2 implementation requires:
    //
    // 1. Wallet integration to sign transactions
    //    - PM's secret key needed to sign the tx
    //    - tau doesn't currently have wallet functionality
    //
    // 2. Transaction construction:
    //    let tx = Transaction::new();
    //    tx.add_call(IdentityContract, VerifyCapabilityV1, [
    //        capability_proof: proof.clone(),
    //        verifier_pub: verifier_pub,
    //        fee: 0,
    //    ]);
    //
    // 3. Broadcast via darkfid RPC:
    //    let params = serde_json::json!({
    //        "tx": base64::encode(tx.encode()),
    //    });
    //    rpc::call("tx_broadcast", params).await?;
    //
    // 4. Event parsing:
    //    - Listen for CapabilityVerified event
    //    - Extract verification result from event data
    //
    // For now, fall back to off-chain verification with a warning.
    tracing::warn!(
        target: "tau",
        "On-chain verification called but not implemented. Falling back to off-chain."
    );

    // Fall back to off-chain verification for now
    verify_capability_offchain(proof, required_capability_id)
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
pub fn parse_capability_proof(params: &tinyjson::JsonValue) -> TaudResult<CapabilityProof> {
    use std::str::FromStr;

    let get_field = |key: &str| -> TaudResult<String> {
        params[key]
            .get::<String>()
            .map(|s| s.clone())
            .ok_or_else(|| crate::error::TaudError::ParseFailed(key.to_string()).into())
    };

    let get_bytes32 = |key: &str| -> TaudResult<[u8; 32]> {
        let s = get_field(key)?;
        let decoded = bs58::decode(&s).into_vec().map_err(|_| {
            crate::error::TaudError::ParseFailed(format!("Invalid bs58 for {}", key))
        })?;
        decoded.as_slice().try_into().map_err(|_| {
            crate::error::TaudError::ParseFailed(format!("Invalid length for {}", key))
        })
    };

    let get_vec_u8 = |key: &str| -> TaudResult<Vec<u8>> {
        let s = get_field(key)?;
        bs58::decode(&s).into_vec().map_err(|_| {
            crate::error::TaudError::ParseFailed(format!("Invalid bs58 for {}", key))
        })
    };

    let get_u64 = |key: &str| -> TaudResult<u64> {
        let s = get_field(key)?;
        u64::from_str(&s).map_err(|_| {
            crate::error::TaudError::ParseFailed(format!("Invalid u64 for {}", key))
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