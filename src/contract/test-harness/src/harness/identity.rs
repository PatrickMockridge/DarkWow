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
//! Provides isolated testing for Identity contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{pasta_prelude::*, PublicKey, SecretKey},
    pasta::pallas,
};
use dwow_serial::Encodable;

use dwow_identity_contract::client::{
    create_claim_v1::{CreateClaimCallData, create_claim_proof, CreateClaimPublicInputs},
    create_claim_v1_dag::{CreateClaimDagCallData, create_claim_dag_proof, CreateClaimDagPublicInputs},
    create_claim_v1_l1::{CreateClaimL1CallData, create_claim_l1_proof, CreateClaimL1PublicInputs},
    create_claim_v1_l1_v2::{CreateClaimL1V2CallData, create_claim_l1_v2_proof, CreateClaimL1V2PublicInputs},
    create_claim_v1_multi::{CreateClaimMultiCallData, create_claim_multi_proof, CreateClaimMultiPublicInputs},
    create_claim_v1_ratio::{CreateClaimRatioCallData, create_claim_ratio_proof, CreateClaimRatioPublicInputs},
    issue_credential_v1::{IssueCredentialCallData, create_issue_credential_proof, IssueCredentialPublicInputs},
    verify_capability_v1::{VerifyCapabilityCallData, create_verify_capability_proof, VerifyCapabilityPublicInputs},
};
use dwow_identity_contract::model::{
    CapabilityId, CapabilitySecret, CreateClaimParams, CreateClaimParamsL1, InitializeParams, IssueCredentialParams,
    RegisterCapabilityParams, IssueCapabilityParams, VerifyCapabilityParams,
    RevokeCapabilityParams,
};

/// Identity Harness for isolated testing
pub struct IdentityHarness {
    /// CreateClaim_V1 ZkBinary
    create_claim_zkbin: ZkBinary,
    /// CreateClaim_V1 ProvingKey
    create_claim_pk: ProvingKey,
    /// CreateClaimDag_V1 ZkBinary
    create_claim_dag_zkbin: ZkBinary,
    /// CreateClaimDag_V1 ProvingKey
    create_claim_dag_pk: ProvingKey,
    /// CreateClaimL1_V1 ZkBinary
    create_claim_l1_zkbin: ZkBinary,
    /// CreateClaimL1_V1 ProvingKey
    create_claim_l1_pk: ProvingKey,
    /// CreateClaimL1V2_V1 ZkBinary
    create_claim_l1_v2_zkbin: ZkBinary,
    /// CreateClaimL1V2_V1 ProvingKey
    create_claim_l1_v2_pk: ProvingKey,
    /// CreateClaimMulti_V1 ZkBinary
    create_claim_multi_zkbin: ZkBinary,
    /// CreateClaimMulti_V1 ProvingKey
    create_claim_multi_pk: ProvingKey,
    /// CreateClaimRatio_V1 ZkBinary
    create_claim_ratio_zkbin: ZkBinary,
    /// CreateClaimRatio_V1 ProvingKey
    create_claim_ratio_pk: ProvingKey,
    /// IssueCredential_V1 ZkBinary
    issue_credential_zkbin: ZkBinary,
    /// IssueCredential_V1 ProvingKey
    issue_credential_pk: ProvingKey,
    /// VerifyCapability_V1 ZkBinary
    verify_capability_zkbin: ZkBinary,
    /// VerifyCapability_V1 ProvingKey
    verify_capability_pk: ProvingKey,
}

impl IdentityHarness {
    /// Spawn a new Identity harness with pre-loaded circuits
    pub fn spawn() -> Self {
        let claim_bin = include_bytes!("../../../identity/proof/create_claim_v2.zk.bin");
        let claim_dag_bin = include_bytes!("../../../identity/proof/create_claim_v1_dag.zk.bin");
        let claim_l1_bin = include_bytes!("../../../identity/proof/create_claim_v1_l1.zk.bin");
        let claim_l1_v2_bin = include_bytes!("../../../identity/proof/create_claim_v1_l1_v2.zk.bin");
        let claim_multi_bin = include_bytes!("../../../identity/proof/create_claim_v1_multi.zk.bin");
        let claim_ratio_bin = include_bytes!("../../../identity/proof/create_claim_v1_ratio.zk.bin");
        let issue_bin = include_bytes!("../../../identity/proof/issue_credential_v2.zk.bin");
        let verify_bin = include_bytes!("../../../identity/proof/verify_capability_v2.zk.bin");

        let create_claim_zkbin = ZkBinary::decode(claim_bin, false).unwrap();
        let create_claim_dag_zkbin = ZkBinary::decode(claim_dag_bin, false).unwrap();
        let create_claim_l1_zkbin = ZkBinary::decode(claim_l1_bin, false).unwrap();
        let create_claim_l1_v2_zkbin = ZkBinary::decode(claim_l1_v2_bin, false).unwrap();
        let create_claim_multi_zkbin = ZkBinary::decode(claim_multi_bin, false).unwrap();
        let create_claim_ratio_zkbin = ZkBinary::decode(claim_ratio_bin, false).unwrap();
        let issue_credential_zkbin = ZkBinary::decode(issue_bin, false).unwrap();
        let verify_capability_zkbin = ZkBinary::decode(verify_bin, false).unwrap();

        let claim_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_claim_zkbin).unwrap(),
            &create_claim_zkbin,
        );
        let claim_dag_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_claim_dag_zkbin).unwrap(),
            &create_claim_dag_zkbin,
        );
        let claim_l1_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_claim_l1_zkbin).unwrap(),
            &create_claim_l1_zkbin,
        );
        let claim_l1_v2_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_claim_l1_v2_zkbin).unwrap(),
            &create_claim_l1_v2_zkbin,
        );
        let claim_multi_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_claim_multi_zkbin).unwrap(),
            &create_claim_multi_zkbin,
        );
        let claim_ratio_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_claim_ratio_zkbin).unwrap(),
            &create_claim_ratio_zkbin,
        );
        let issue_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&issue_credential_zkbin).unwrap(),
            &issue_credential_zkbin,
        );
        let verify_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&verify_capability_zkbin).unwrap(),
            &verify_capability_zkbin,
        );

        let create_claim_pk = ProvingKey::build(create_claim_zkbin.k, &claim_circuit).expect("ProvingKey::build failed");
        let create_claim_dag_pk = ProvingKey::build(create_claim_dag_zkbin.k, &claim_dag_circuit).expect("ProvingKey::build failed");
        let create_claim_l1_pk = ProvingKey::build(create_claim_l1_zkbin.k, &claim_l1_circuit).expect("ProvingKey::build failed");
        let create_claim_l1_v2_pk = ProvingKey::build(create_claim_l1_v2_zkbin.k, &claim_l1_v2_circuit).expect("ProvingKey::build failed");
        let create_claim_multi_pk = ProvingKey::build(create_claim_multi_zkbin.k, &claim_multi_circuit).expect("ProvingKey::build failed");
        let create_claim_ratio_pk = ProvingKey::build(create_claim_ratio_zkbin.k, &claim_ratio_circuit).expect("ProvingKey::build failed");
        let issue_credential_pk = ProvingKey::build(issue_credential_zkbin.k, &issue_circuit).expect("ProvingKey::build failed");
        let verify_capability_pk = ProvingKey::build(verify_capability_zkbin.k, &verify_circuit).expect("ProvingKey::build failed");

        Self {
            create_claim_zkbin,
            create_claim_pk,
            create_claim_dag_zkbin,
            create_claim_dag_pk,
            create_claim_l1_zkbin,
            create_claim_l1_pk,
            create_claim_l1_v2_zkbin,
            create_claim_l1_v2_pk,
            create_claim_multi_zkbin,
            create_claim_multi_pk,
            create_claim_ratio_zkbin,
            create_claim_ratio_pk,
            issue_credential_zkbin,
            issue_credential_pk,
            verify_capability_zkbin,
            verify_capability_pk,
        }
    }

    /// Initialize the identity contract
    pub fn initialize(&self) -> Result<InitializeResult> {
        // Build InitializeParams
        let params = InitializeParams { version: 1 };
        let mut call_data = vec![];
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
        // Derive public keys from secrets
        let issuer_public = PublicKey::from_secret(SecretKey::from_bytes(issuer_secret.to_repr()).unwrap());
        let holder_public = PublicKey::from_secret(SecretKey::from_bytes(credential_secret.to_repr()).unwrap());

        let input = IssueCredentialCallData::new(
            issuer_secret,
            credential_secret,
            attribute_1,
            attribute_2,
            attribute_blind,
            issuer_public,
            holder_public,
            schema_hash,
            issued_at,
            expires_at,
        );

        let (proof, public_inputs) = create_issue_credential_proof(
            &self.issue_credential_zkbin,
            &self.issue_credential_pk,
            &input,
        )?;

        // Build IssueCredentialParams

        let params = IssueCredentialParams {
            issuer_pub: issuer_public,
            holder_pub: holder_public,
            schema_hash: schema_hash.to_repr(),
            encrypted_attributes: vec![],
            commitment: dwow_sdk::crypto::IntentCommitment::from_bytes(public_inputs.commitment.to_repr()).unwrap(),
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(public_inputs.commitment.to_repr()).unwrap(),
            issued_at,
            expires_at,
            proof: vec![],
            fee: 0,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(IssueCredentialResult { call_data, public_inputs, proof })
    }

    /// Create a claim from a credential
    pub fn create_claim(
        &self,
        credential_secret: pallas::Base,
        attribute_value: pallas::Base,
        threshold: pallas::Base,
        commitment: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
    ) -> Result<CreateClaimResult> {
        let input = CreateClaimCallData::new(
            credential_secret,
            attribute_value,
            threshold,
            commitment,
            issuer_public,
            schema_hash,
            claim_type,
        );

        let (proof, public_inputs) = create_claim_proof(
            &self.create_claim_zkbin,
            &self.create_claim_pk,
            &input,
        )?;

        // Build CreateClaimParams
        let params = CreateClaimParams {
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(public_inputs.nullifier.to_repr()).unwrap(),
            claim_type: claim_type.to_repr().to_vec(),
            predicate: vec![],
            revealed_attributes: vec![],
            proof: vec![],
            fee: 0,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateClaimResult { call_data, public_inputs, proof })
    }

    /// Create a Level 1 claim (selective disclosure with predicate_result)
    pub fn create_claim_l1(
        &self,
        credential_secret: pallas::Base,
        attribute_value: pallas::Base,
        threshold: pallas::Base,
        commitment: pallas::Base,
        delta: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
        predicate_result: bool,
    ) -> Result<CreateClaimL1Result> {
        let input = CreateClaimL1CallData::new(
            credential_secret,
            attribute_value,
            threshold,
            commitment,
            delta,
            issuer_public,
            schema_hash,
            claim_type,
            predicate_result,
        );

        let (proof, public_inputs) = create_claim_l1_proof(
            &self.create_claim_l1_zkbin,
            &self.create_claim_l1_pk,
            &input,
        )?;

        let params = CreateClaimParamsL1 {
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(public_inputs.nullifier.to_repr()).unwrap(),
            claim_type: claim_type.to_repr().to_vec(),
            predicate: vec![],
            revealed_attributes: vec![],
            proof: vec![],
            predicate_result: if predicate_result { 1 } else { 0 },
            fee: 0,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateClaimL1Result { call_data, public_inputs, proof })
    }

    /// Create a Level 1 V2 claim
    pub fn create_claim_l1_v2(
        &self,
        credential_secret: pallas::Base,
        attribute_value: pallas::Base,
        threshold: pallas::Base,
        commitment: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
        predicate_result: bool,
    ) -> Result<CreateClaimL1V2Result> {
        let input = CreateClaimL1V2CallData::new(
            credential_secret,
            attribute_value,
            threshold,
            commitment,
            issuer_public,
            schema_hash,
            claim_type,
            predicate_result,
        );

        let (proof, public_inputs) = create_claim_l1_v2_proof(
            &self.create_claim_l1_v2_zkbin,
            &self.create_claim_l1_v2_pk,
            &input,
        )?;

        let params = CreateClaimParamsL1 {
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(public_inputs.nullifier.to_repr()).unwrap(),
            claim_type: claim_type.to_repr().to_vec(),
            predicate: vec![],
            revealed_attributes: vec![],
            proof: vec![],
            predicate_result: if predicate_result { 1 } else { 0 },
            fee: 0,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateClaimL1V2Result { call_data, public_inputs, proof })
    }

    /// Create a multi-credential claim
    #[allow(clippy::too_many_arguments)]
    pub fn create_claim_multi(
        &self,
        secret_1: pallas::Base,
        commitment_1: pallas::Base,
        attribute_1: pallas::Base,
        threshold_1: pallas::Base,
        secret_2: pallas::Base,
        commitment_2: pallas::Base,
        attribute_2: pallas::Base,
        threshold_2: pallas::Base,
        secret_3: pallas::Base,
        commitment_3: pallas::Base,
        attribute_3: pallas::Base,
        threshold_3: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
    ) -> Result<CreateClaimMultiResult> {
        let input = CreateClaimMultiCallData::new(
            secret_1, commitment_1, attribute_1, threshold_1,
            secret_2, commitment_2, attribute_2, threshold_2,
            secret_3, commitment_3, attribute_3, threshold_3,
            issuer_public, schema_hash, claim_type, 3,
        );

        let (proof, public_inputs) = create_claim_multi_proof(
            &self.create_claim_multi_zkbin,
            &self.create_claim_multi_pk,
            &input,
        )?;

        let params = CreateClaimParams {
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(public_inputs.nullifier.to_repr()).unwrap(),
            claim_type: claim_type.to_repr().to_vec(),
            predicate: vec![],
            revealed_attributes: vec![],
            proof: vec![],
            fee: 0,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateClaimMultiResult { call_data, public_inputs, proof })
    }

    /// Create a ratio-based claim (e.g., voting power proportion)
    pub fn create_claim_ratio(
        &self,
        credential_secret: pallas::Base,
        commitment: pallas::Base,
        my_value: pallas::Base,
        total_supply: pallas::Base,
        threshold_ratio: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
        predicate_result: bool,
    ) -> Result<CreateClaimRatioResult> {
        let input = CreateClaimRatioCallData::new(
            credential_secret,
            commitment,
            my_value,
            total_supply,
            threshold_ratio,
            issuer_public,
            schema_hash,
            claim_type,
            predicate_result,
        );

        let (proof, public_inputs) = create_claim_ratio_proof(
            &self.create_claim_ratio_zkbin,
            &self.create_claim_ratio_pk,
            &input,
        )?;

        let params = CreateClaimParams {
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(public_inputs.nullifier.to_repr()).unwrap(),
            claim_type: claim_type.to_repr().to_vec(),
            predicate: vec![],
            revealed_attributes: vec![],
            proof: vec![],
            fee: 0,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateClaimRatioResult { call_data, public_inputs, proof })
    }

    /// Create a DAG-based claim
    pub fn create_claim_dag(
        &self,
        credential_secret: pallas::Base,
        commitment: pallas::Base,
        attribute_value: pallas::Base,
        threshold: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
    ) -> Result<CreateClaimDagHarnessResult> {
        let input = CreateClaimDagCallData::new(
            credential_secret,
            commitment,
            attribute_value,
            threshold,
            issuer_public,
            schema_hash,
            claim_type,
        );

        let (proof, public_inputs) = create_claim_dag_proof(
            &self.create_claim_dag_zkbin,
            &self.create_claim_dag_pk,
            &input,
        )?;

        let params = CreateClaimParams {
            nullifier: dwow_sdk::crypto::IntentNullifier::from_bytes(public_inputs.nullifier.to_repr()).unwrap(),
            claim_type: claim_type.to_repr().to_vec(),
            predicate: vec![],
            revealed_attributes: vec![],
            proof: vec![],
            fee: 0,
        };

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateClaimDagHarnessResult { call_data, public_inputs, proof })
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
            credential_secret,
            commitment,
            attribute_value,
            threshold,
            capability_secret,
            issuer_public,
            schema_hash,
            capability_id,
            predicate_result,
        );

        let (proof, public_inputs) = create_verify_capability_proof(
            &self.verify_capability_zkbin,
            &self.verify_capability_pk,
            &input,
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

        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());

        Ok(VerifyCapabilityResult { call_data, public_inputs, proof })
    }

    // ========================================================================
    // NON-ZK OCAP METHODS
    // ========================================================================

    /// Register a new capability type
    pub fn register_capability(
        &self,
        name: Vec<u8>,
        credential_requirement: dwow_identity_contract::model::CredentialRequirement,
        max_holders: Option<u64>,
    ) -> Result<RegisterCapabilityHarnessResult> {
        let params = RegisterCapabilityParams {
            name,
            credential_requirement,
            max_holders,
            fee: 0,
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(RegisterCapabilityHarnessResult { call_data })
    }

    /// Issue a capability to a holder
    pub fn issue_capability(
        &self,
        capability_id: CapabilityId,
        holder_pub: PublicKey,
        credential_nullifier: dwow_sdk::crypto::IntentNullifier,
    ) -> Result<IssueCapabilityHarnessResult> {
        let params = IssueCapabilityParams {
            capability_id,
            holder_pub,
            credential_nullifier,
            proof: vec![],
            issuer_sig: vec![],
            fee: 0,
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(IssueCapabilityHarnessResult { call_data })
    }

    /// Revoke a capability
    pub fn revoke_capability(
        &self,
        capability_id: CapabilityId,
        holder_pub: PublicKey,
        capability_secret: CapabilitySecret,
        reason: Vec<u8>,
    ) -> Result<RevokeCapabilityHarnessResult> {
        let params = RevokeCapabilityParams {
            capability_id,
            holder_pub,
            capability_secret,
            signature: vec![],
            reason,
            fee: 0,
        };
        let mut call_data = vec![];
        call_data.extend_from_slice(&params.encode());
        Ok(RevokeCapabilityHarnessResult { call_data })
    }
}

impl super::ContractHarness for IdentityHarness {
    fn name(&self) -> &str {
        "identity"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateClaimV1",
            "CreateClaimV1DAG",
            "CreateClaimV1L1",
            "CreateClaimV1L1V2",
            "CreateClaimV1Multi",
            "CreateClaimV1Ratio",
            "IssueCredentialV1",
            "VerifyCapabilityV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateClaimV1" => Some(&self.create_claim_zkbin),
            "CreateClaimV1DAG" => Some(&self.create_claim_dag_zkbin),
            "CreateClaimV1L1" => Some(&self.create_claim_l1_zkbin),
            "CreateClaimV1L1V2" => Some(&self.create_claim_l1_v2_zkbin),
            "CreateClaimV1Multi" => Some(&self.create_claim_multi_zkbin),
            "CreateClaimV1Ratio" => Some(&self.create_claim_ratio_zkbin),
            "IssueCredentialV1" => Some(&self.issue_credential_zkbin),
            "VerifyCapabilityV1" => Some(&self.verify_capability_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateClaimV1" => Some(&self.create_claim_pk),
            "CreateClaimV1DAG" => Some(&self.create_claim_dag_pk),
            "CreateClaimV1L1" => Some(&self.create_claim_l1_pk),
            "CreateClaimV1L1V2" => Some(&self.create_claim_l1_v2_pk),
            "CreateClaimV1Multi" => Some(&self.create_claim_multi_pk),
            "CreateClaimV1Ratio" => Some(&self.create_claim_ratio_pk),
            "IssueCredentialV1" => Some(&self.issue_credential_pk),
            "VerifyCapabilityV1" => Some(&self.verify_capability_pk),
            _ => None,
        }
    }
}

/// Result of initialize
pub struct InitializeResult {
    pub call_data: Vec<u8>,
}

/// Result of issue_credential
pub struct IssueCredentialResult {
    pub call_data: Vec<u8>,
    pub public_inputs: IssueCredentialPublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of create_claim
pub struct CreateClaimResult {
    pub call_data: Vec<u8>,
    pub public_inputs: CreateClaimPublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of create_claim_l1
pub struct CreateClaimL1Result {
    pub call_data: Vec<u8>,
    pub public_inputs: CreateClaimL1PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of create_claim_l1_v2
pub struct CreateClaimL1V2Result {
    pub call_data: Vec<u8>,
    pub public_inputs: CreateClaimL1V2PublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of create_claim_multi
pub struct CreateClaimMultiResult {
    pub call_data: Vec<u8>,
    pub public_inputs: CreateClaimMultiPublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of create_claim_ratio
pub struct CreateClaimRatioResult {
    pub call_data: Vec<u8>,
    pub public_inputs: CreateClaimRatioPublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of create_claim_dag
pub struct CreateClaimDagHarnessResult {
    pub call_data: Vec<u8>,
    pub public_inputs: CreateClaimDagPublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of verify_capability
pub struct VerifyCapabilityResult {
    pub call_data: Vec<u8>,
    pub public_inputs: VerifyCapabilityPublicInputs,
    pub proof: dwow_core::zk::Proof,
}

/// Result of register_capability
pub struct RegisterCapabilityHarnessResult {
    pub call_data: Vec<u8>,
}

/// Result of issue_capability
pub struct IssueCapabilityHarnessResult {
    pub call_data: Vec<u8>,
}

/// Result of revoke_capability
pub struct RevokeCapabilityHarnessResult {
    pub call_data: Vec<u8>,
}