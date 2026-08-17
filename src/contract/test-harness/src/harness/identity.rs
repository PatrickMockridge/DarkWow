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

//! Identity Test Harness
//!
//! Provides isolated testing for Identity contract (3 circuits, 9 functions).

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, poseidon_hash, PublicKey, SecretKey, schnorr::SchnorrSecret},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_identity_contract::client::{
    issue_credential::{IssueCredentialCallData, create_issue_credential_proof, IssueCredentialPublicInputs},
    verify_capability::{VerifyCapabilityCallData, create_verify_capability_proof, VerifyCapabilityPublicInputs},
};
use dwow_identity_contract::model::{
    CapabilityId, CapabilitySecret, InitializeParams, IssueCredentialParams,
    RegisterCapabilityParams, IssueCapabilityParams, VerifyCapabilityParams,
    RevokeCapabilityParams, RevokeCredentialParams,
};

/// Identity Harness for isolated testing (2 circuits)
pub struct IdentityHarness {
    issue_credential_zkbin: ZkBinary,
    issue_credential_pk: ProvingKey,
    verify_capability_zkbin: ZkBinary,
    verify_capability_pk: ProvingKey,
}

impl IdentityHarness {
    /// Spawn a new Identity harness with 2 pre-loaded circuits
    pub fn spawn() -> Self {
        let issue_bin = include_bytes!("../../../identity/proof/issue_credential.zk.bin");
        let verify_bin = include_bytes!("../../../identity/proof/verify_capability.zk.bin");

        let issue_credential_zkbin = ZkBinary::decode(issue_bin, false).unwrap();
        let verify_capability_zkbin = ZkBinary::decode(verify_bin, false).unwrap();

        let issue_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&issue_credential_zkbin).unwrap(),
            &issue_credential_zkbin,
        );
        let verify_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&verify_capability_zkbin).unwrap(),
            &verify_capability_zkbin,
        );

        let issue_credential_pk = ProvingKey::build(issue_credential_zkbin.k, &issue_circuit).expect("ProvingKey::build failed");
        let verify_capability_pk = ProvingKey::build(verify_capability_zkbin.k, &verify_circuit).expect("ProvingKey::build failed");

        Self {
            issue_credential_zkbin, issue_credential_pk,
            verify_capability_zkbin, verify_capability_pk,
        }
    }

    /// Initialize the identity contract
    pub fn initialize(&self) -> Result<InitializeResult> {
        let params = InitializeParams { version: 1 };
        let mut call_data = vec![0x00]; // InitializeV1
        call_data.extend_from_slice(&params.encode());
        Ok(InitializeResult { call_data })
    }

    /// Issue a credential to a holder
    pub fn issue_credential(
        &self,
        issuer_secret: pallas::Base,
        credential_secret: pallas::Base,
        attribute_1: pallas::Base,
        attribute_2: pallas::Base,
        attribute_blind: pallas::Base,
        schema_hash: pallas::Base,
        issued_at: u64,
        expires_at: u64,
    ) -> Result<IssueCredentialResult> {
        let issuer_public = PublicKey::from_secret(SecretKey::from_bytes(issuer_secret.to_repr()).unwrap());
        let holder_public = PublicKey::from_secret(SecretKey::from_bytes(credential_secret.to_repr()).unwrap());

        let input = IssueCredentialCallData::new(
            issuer_secret, credential_secret, attribute_1, attribute_2, attribute_blind,
            issuer_public, holder_public, schema_hash, issued_at, expires_at,
        );

        let (proof, public_inputs) = create_issue_credential_proof(
            &self.issue_credential_zkbin, &self.issue_credential_pk, &input,
        )?;

        let credential_nullifier = input.compute_nullifier();

        let params = IssueCredentialParams {
            issuer_pub: issuer_public,
            holder_pub: holder_public,
            schema_hash: schema_hash.to_repr(),
            encrypted_attributes: vec![],
            commitment: dwow_sdk::crypto::IntentCommitment::from_bytes(public_inputs.commitment.to_repr()).unwrap(),
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(credential_nullifier.to_repr()).unwrap(),
            issued_at, expires_at,
            proof: vec![], fee: 0,
        };

        let mut call_data = vec![0x01]; // IssueCredentialV1
        call_data.extend_from_slice(&params.encode());
        Ok(IssueCredentialResult { call_data, public_inputs, proof })
    }

    /// Verify a capability with ZK proof
    #[allow(clippy::too_many_arguments)]
    pub fn verify_capability(
        &self,
        credential_secret: pallas::Base,
        commitment: pallas::Base,
        attribute_value: pallas::Base,
        threshold: pallas::Base,
        capability_secret: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        capability_id: pallas::Base,
        predicate_result: bool,
    ) -> Result<VerifyCapabilityResult> {
        let input = VerifyCapabilityCallData::new(
            credential_secret, commitment, attribute_value, threshold,
            capability_secret, issuer_public, schema_hash, capability_id, predicate_result,
        );

        let (proof, public_inputs) = create_verify_capability_proof(
            &self.verify_capability_zkbin, &self.verify_capability_pk, &input,
        )?;

        let params = VerifyCapabilityParams {
            capability_proof: dwow_identity_contract::model::CapabilityProof {
                capability_id: CapabilityId(capability_id),
                capability_secret: CapabilitySecret(capability_secret),
                nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(public_inputs.nullifier.to_repr()).unwrap(),
                issuer_pub: issuer_public,
                schema_hash: schema_hash.to_repr(),
                predicate_result: if predicate_result { 1 } else { 0 },
                proof: vec![],
                created_at: 0,
            },
            verifier_pub: issuer_public,
            fee: 0,
        };

        let mut call_data = vec![0x06]; // VerifyCapabilityV1
        call_data.extend_from_slice(&params.encode());
        Ok(VerifyCapabilityResult { call_data, public_inputs, proof })
    }

    // ========================================================================
    // NON-ZK OCAP METHODS
    // ========================================================================

    /// Register a new capability type (0x04)
    pub fn register_capability(
        &self,
        name: Vec<u8>,
        credential_requirement: dwow_identity_contract::model::CredentialRequirement,
        max_holders: Option<u64>,
    ) -> Result<RegisterCapabilityHarnessResult> {
        // Compute capability_id matching contract's compute_capability_id
        let mut data = credential_requirement.encode();
        data.extend_from_slice(&name);
        let mut b = [0u8; 8];
        let len = data.len().min(8);
        b[..len].copy_from_slice(&data[..len]);
        let value = u64::from_le_bytes(b);
        let capability_id = CapabilityId(poseidon_hash([pallas::Base::from(value)]));
        let params = RegisterCapabilityParams { name, credential_requirement, max_holders, fee: 0 };
        let mut call_data = vec![0x04]; // RegisterCapabilityV1
        call_data.extend_from_slice(&params.encode());
        Ok(RegisterCapabilityHarnessResult { call_data, capability_id })
    }

    /// Issue a capability to a holder (0x05)
    pub fn issue_capability(
        &self,
        capability_id: CapabilityId,
        holder_pub: PublicKey,
        credential_nullifier: dwow_sdk::crypto::IntentNullifier,
    ) -> Result<IssueCapabilityHarnessResult> {
        let params = IssueCapabilityParams {
            capability_id, holder_pub, credential_nullifier,
            proof: vec![], issuer_sig: vec![], fee: 0,
        };
        let mut call_data = vec![0x05]; // IssueCapabilityV1
        call_data.extend_from_slice(&params.encode());
        Ok(IssueCapabilityHarnessResult { call_data })
    }

    /// Revoke a capability (0x07)
    pub fn revoke_capability(
        &self,
        capability_id: CapabilityId,
        holder_pub: PublicKey,
        capability_secret: CapabilitySecret,
        reason: Vec<u8>,
    ) -> Result<RevokeCapabilityHarnessResult> {
        let params = RevokeCapabilityParams {
            capability_id, holder_pub, capability_secret,
            signature: vec![], reason, fee: 0,
        };
        let mut call_data = vec![0x07]; // RevokeCapabilityV1
        call_data.extend_from_slice(&params.encode());
        Ok(RevokeCapabilityHarnessResult { call_data })
    }

    /// Register an issuer (0x08)
    pub fn register_issuer(
        &self,
        issuer_pub: PublicKey,
        name: Vec<u8>,
        authorized_schemas: Vec<[u8; 32]>,
    ) -> Result<RegisterIssuerHarnessResult> {
        let params = dwow_identity_contract::model::RegisterIssuerParams { issuer_pub, name, authorized_schemas };
        let mut call_data = vec![0x08]; // RegisterIssuerV1
        call_data.extend_from_slice(&params.encode());
        Ok(RegisterIssuerHarnessResult { call_data })
    }

    /// Revoke a credential (function code 0x02, non-ZK).
    /// Requires a valid Schnorr signature from the credential issuer over the nullifier.
    pub fn revoke_credential(
        &self,
        issuer_secret: pallas::Base,
        nullifier: dwow_sdk::crypto::IntentNullifier,
        reason: Vec<u8>,
    ) -> Result<RevokeCredentialHarnessResult> {
        let sk = SecretKey::from_bytes(issuer_secret.to_repr()).map_err(|e| {
            dwow_core::Error::Custom(format!("invalid issuer secret: {e}"))
        })?;
        let sig = sk.sign(&nullifier.to_bytes());
        let params = RevokeCredentialParams {
            nullifier,
            issuer_sig: sig.encode(),
            reason,
            fee: 0,
        };
        let mut call_data = vec![0x02]; // RevokeCredentialV1
        call_data.extend_from_slice(&params.encode());
        Ok(RevokeCredentialHarnessResult { call_data })
    }
}

impl super::ContractHarness for IdentityHarness {
    fn name(&self) -> &str { "identity" }

    fn circuits(&self) -> Vec<&'static str> {
        vec!["IssueCredentialV2", "VerifyCapabilityV2"]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "IssueCredentialV2" => Some(&self.issue_credential_zkbin),
            "VerifyCapabilityV2" => Some(&self.verify_capability_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "IssueCredentialV2" => Some(&self.issue_credential_pk),
            "VerifyCapabilityV2" => Some(&self.verify_capability_pk),
            _ => None,
        }
    }
}

/// Result structs
pub struct InitializeResult { pub call_data: Vec<u8> }
pub struct IssueCredentialResult { pub call_data: Vec<u8>, pub public_inputs: IssueCredentialPublicInputs, pub proof: dwow_core::zk::Proof }
pub struct VerifyCapabilityResult { pub call_data: Vec<u8>, pub public_inputs: VerifyCapabilityPublicInputs, pub proof: dwow_core::zk::Proof }
pub struct RegisterCapabilityHarnessResult { pub call_data: Vec<u8>, pub capability_id: CapabilityId }
pub struct IssueCapabilityHarnessResult { pub call_data: Vec<u8> }
pub struct RevokeCapabilityHarnessResult { pub call_data: Vec<u8> }
pub struct RegisterIssuerHarnessResult { pub call_data: Vec<u8> }
pub struct RevokeCredentialHarnessResult { pub call_data: Vec<u8> }
