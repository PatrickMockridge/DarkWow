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

//! Identity contract client for tau_pallas
//!
//! This module provides utilities for building Identity contract calls,
//! specifically for on-chain capability verification.
//!
//! The Identity contract enables selective disclosure of attributes without
//! revealing more than necessary. For tau's use case, we use it to verify
//! capability proofs on-chain.

use crate::error::TauPallasResult;

/// Function code for VerifyCapabilityV1 in the Identity contract
pub const IDENTITY_FUNCTION_VERIFY_CAPABILITY_V1: u8 = 0x0b;

/// A client-side representation of a capability proof
///
/// This is a simplified version of the contract's CapabilityProof that uses
/// plain [u8; 32] types instead of type-safe wrappers like IntentNullifier.
/// The conversion to the contract type happens during transaction construction.
#[derive(Debug, Clone)]
pub struct ClientCapabilityProof {
    /// Hash of the capability definition
    pub capability_id: [u8; 32],
    /// Nullifier from the underlying credential (proves credential exists)
    pub nullifier: [u8; 32],
    /// Public predicate result (1 if satisfied, 0 if not)
    pub predicate_result: u8,
    /// Issuer's public key
    pub issuer_pub: [u8; 32],
    /// Schema hash
    pub schema_hash: [u8; 32],
    /// ZK proof of capability satisfaction
    pub proof: Vec<u8>,
    /// Capability secret (proves holder owns this capability)
    pub capability_secret: [u8; 32],
    /// Timestamp when proof was created
    pub created_at: u64,
}

/// Build calldata for the Identity contract's VerifyCapabilityV1 function
///
/// This constructs the binary format expected by the Identity contract's
/// VerifyCapabilityV1 (0x0b) function.
///
/// # Arguments
///
/// * `capability_proof` - The capability proof to verify
/// * `verifier_pub` - The public key of who is requesting verification
/// * `fee` - The fee to pay for verification
///
/// # Returns
///
/// Returns the calldata bytes suitable for adding to a Transaction
pub fn build_verify_capability_calldata(
    capability_proof: &ClientCapabilityProof,
    verifier_pub: [u8; 32],
    fee: u64,
) -> TauPallasResult<Vec<u8>> {
    let mut call_data = Vec::new();

    // Function code: VerifyCapabilityV1 = 0x0b
    call_data.push(IDENTITY_FUNCTION_VERIFY_CAPABILITY_V1);

    // Serialize capability_proof
    call_data.extend_from_slice(&capability_proof.capability_id);
    call_data.extend_from_slice(&capability_proof.nullifier);
    call_data.push(capability_proof.predicate_result);
    call_data.extend_from_slice(&capability_proof.issuer_pub);
    call_data.extend_from_slice(&capability_proof.schema_hash);

    // proof: Vec<u8> - length prefix + data
    call_data.extend_from_slice(&(capability_proof.proof.len() as u32).to_le_bytes());
    call_data.extend_from_slice(&capability_proof.proof);

    call_data.extend_from_slice(&capability_proof.capability_secret);
    call_data.extend_from_slice(&capability_proof.created_at.to_le_bytes());

    // verifier_pub
    call_data.extend_from_slice(&verifier_pub);

    // fee
    call_data.extend_from_slice(&fee.to_le_bytes());

    Ok(call_data)
}
