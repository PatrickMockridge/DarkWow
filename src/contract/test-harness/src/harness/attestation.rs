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

//! Attestation Test Harness
//!
//! Provides isolated testing for Attestation contract.

use dwow_core::{
    zk::{ProvingKey, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{crypto::PublicKey, pasta::pallas};
use dwow_serial::Encodable;

use dwow_attestation_contract::client::{
    consume_claim_v1::{
        ConsumeClaimV1CallData, consume_claim_v1_proof, ConsumeClaimV1PublicInputs,
    },
    create_attestation_v1::{
        CreateAttestationV1CallData, create_attestation_v1_proof, CreateAttestationV1PublicInputs,
    },
    create_claim_v1::{
        CreateClaimV1CallData, create_claim_v1_proof, CreateClaimV1PublicInputs,
    },
    delegate_attestation_v1::{
        DelegateAttestationV1CallData, delegate_attestation_v1_proof,
        DelegateAttestationV1PublicInputs,
    },
    verify_claim_v1::{
        VerifyClaimV1CallData, verify_claim_v1_proof, VerifyClaimV1PublicInputs,
    },
};
use dwow_attestation_contract::model::{
    ConsumeClaimParamsV1, CreateAttestationParamsV1, CreateClaimParamsV1,
    DelegateAttestationParamsV1, VerifyClaimParamsV1, Predicate,
};

/// Attestation Harness for isolated testing
pub struct AttestationHarness {
    create_attestation_zkbin: ZkBinary,
    create_attestation_pk: ProvingKey,
    create_claim_zkbin: ZkBinary,
    create_claim_pk: ProvingKey,
    verify_claim_zkbin: ZkBinary,
    verify_claim_pk: ProvingKey,
    consume_claim_zkbin: ZkBinary,
    consume_claim_pk: ProvingKey,
    delegate_attestation_zkbin: ZkBinary,
    delegate_attestation_pk: ProvingKey,
}

impl AttestationHarness {
    pub fn spawn() -> Self {
        let create_att_bin =
            include_bytes!("../../../attestation/proof/create_attestation_v1.zk.bin");
        let create_claim_bin =
            include_bytes!("../../../attestation/proof/create_claim_v1.zk.bin");
        let verify_claim_bin =
            include_bytes!("../../../attestation/proof/verify_claim_v1.zk.bin");
        eprintln!("DEBUG: raw verify_claim_bin len={} first_10_bytes={:02x?}", verify_claim_bin.len(), &verify_claim_bin[..10]);
        let consume_claim_bin =
            include_bytes!("../../../attestation/proof/consume_claim_v1.zk.bin");
        let delegate_bin =
            include_bytes!("../../../attestation/proof/delegate_attestation_v1.zk.bin");

        let create_attestation_zkbin = ZkBinary::decode(create_att_bin, false).unwrap();
        let create_claim_zkbin = ZkBinary::decode(create_claim_bin, false).unwrap();
        let verify_claim_zkbin = ZkBinary::decode(verify_claim_bin, false).unwrap();
        let consume_claim_zkbin = ZkBinary::decode(consume_claim_bin, false).unwrap();
        let delegate_attestation_zkbin = ZkBinary::decode(delegate_bin, false).unwrap();

        let create_att_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_attestation_zkbin).unwrap(),
            &create_attestation_zkbin,
        );
        let create_claim_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&create_claim_zkbin).unwrap(),
            &create_claim_zkbin,
        );
        let verify_claim_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&verify_claim_zkbin).unwrap(),
            &verify_claim_zkbin,
        );
        let consume_claim_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&consume_claim_zkbin).unwrap(),
            &consume_claim_zkbin,
        );
        let delegate_attestation_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&delegate_attestation_zkbin).unwrap(),
            &delegate_attestation_zkbin,
        );

        // Build verify_claim first to isolate which circuit fails
        eprintln!("DEBUG: verify_claim k={}", verify_claim_zkbin.k);
        eprintln!("DEBUG: verify_claim namespace={}", verify_claim_zkbin.namespace);
        eprintln!("DEBUG: verify_claim constants={:?}", verify_claim_zkbin.constants);
        eprintln!("DEBUG: verify_claim witnesses={:?}", verify_claim_zkbin.witnesses);
        eprintln!("DEBUG: verify_claim num_opcodes={}", verify_claim_zkbin.opcodes.len());
        for (i, (op, args)) in verify_claim_zkbin.opcodes.iter().enumerate() {
            eprintln!("DEBUG:   opcode[{}]: {:?} args={:?}", i, op, args);
        }
        eprintln!("DEBUG: Building verify_claim ProvingKey with k={}", verify_claim_zkbin.k);
        let verify_claim_pk = ProvingKey::build(verify_claim_zkbin.k, &verify_claim_circuit);
        eprintln!("DEBUG: verify_claim PK built successfully!");
        eprintln!("DEBUG: Building create_attestation PK with k={}", create_attestation_zkbin.k);
        let create_attestation_pk =
            ProvingKey::build(create_attestation_zkbin.k, &create_att_circuit);
        eprintln!("DEBUG: create_attestation PK built successfully!");
        eprintln!("DEBUG: Building create_claim PK with k={}", create_claim_zkbin.k);
        let create_claim_pk = ProvingKey::build(create_claim_zkbin.k, &create_claim_circuit);
        eprintln!("DEBUG: create_claim PK built successfully!");
        eprintln!("DEBUG: Building consume_claim PK with k={}", consume_claim_zkbin.k);
        let consume_claim_pk = ProvingKey::build(consume_claim_zkbin.k, &consume_claim_circuit);
        eprintln!("DEBUG: consume_claim PK built successfully!");
        eprintln!("DEBUG: Building delegate_attestation PK with k={}", delegate_attestation_zkbin.k);
        let delegate_attestation_pk =
            ProvingKey::build(delegate_attestation_zkbin.k, &delegate_attestation_circuit);
        eprintln!("DEBUG: delegate_attestation PK built successfully!");

        Self {
            create_attestation_zkbin,
            create_attestation_pk,
            create_claim_zkbin,
            create_claim_pk,
            verify_claim_zkbin,
            verify_claim_pk,
            consume_claim_zkbin,
            consume_claim_pk,
            delegate_attestation_zkbin,
            delegate_attestation_pk,
        }
    }

    /// Create an attestation (function code 0x00)
    pub fn create_attestation(
        &self,
        attestor_secret: pallas::Base,
        attestor_public: PublicKey,
        claim_type: Predicate,
        claim_data: Vec<pallas::Base>,
        metadata: Vec<u8>,
        expires_at: Option<u64>,
        attestation_id: pallas::Base,
    ) -> Result<CreateAttestationResult, Box<dyn std::error::Error>> {
        let input = CreateAttestationV1CallData::new(attestor_secret, attestor_public);
        let (proof, public_inputs) = create_attestation_v1_proof(
            &self.create_attestation_zkbin,
            &self.create_attestation_pk,
            &input,
        )?;

        let params = CreateAttestationParamsV1 {
            proof: proof.as_ref().to_vec(),
            attestation_id,
            attestor_pub_x: public_inputs.attestor_pub_x,
            attestor_pub_y: public_inputs.attestor_pub_y,
            claim_type,
            claim_data,
            metadata,
            expires_at,
        };

        let mut call_data = vec![0x00];
        params.encode(&mut call_data)?;

        Ok(CreateAttestationResult { call_data, attestation_id, proof, public_inputs })
    }

    /// Create a claim (function code 0x02)
    pub fn create_claim(
        &self,
        attestation_id: pallas::Base,
        claimant_secret: pallas::Base,
        claimant_public: PublicKey,
        predicate: Predicate,
        evidence_commitment: Vec<u8>,
        revealed_result: Vec<u8>,
        claim_id: pallas::Base,
    ) -> Result<CreateClaimResult, Box<dyn std::error::Error>> {
        let input = CreateClaimV1CallData::new(attestation_id, claimant_secret, claimant_public);
        let (proof, public_inputs) = create_claim_v1_proof(
            &self.create_claim_zkbin,
            &self.create_claim_pk,
            &input,
        )?;

        let params = CreateClaimParamsV1 {
            proof: proof.as_ref().to_vec(),
            claim_id,
            attestation_id,
            claimant_pub_x: public_inputs.claimant_pub_x,
            claimant_pub_y: public_inputs.claimant_pub_y,
            predicate,
            evidence_commitment,
            revealed_result,
        };

        let mut call_data = vec![0x02];
        params.encode(&mut call_data)?;

        Ok(CreateClaimResult { call_data, claim_id, proof, public_inputs })
    }

    /// Verify a claim (function code 0x03)
    pub fn verify_claim(
        &self,
        claim_id: pallas::Base,
        attestation_id: pallas::Base,
        revealed_result: pallas::Base,
        evidence: pallas::Base,
        attestation_data: pallas::Base,
        nonce: pallas::Base,
        pos: pallas::Base,
        path: [pallas::Base; 255],
        revocation_root: pallas::Base,
    ) -> Result<VerifyClaimResult, Box<dyn std::error::Error>> {
        let input = VerifyClaimV1CallData::new(
            claim_id,
            revealed_result,
            evidence,
            attestation_data,
            nonce,
            pos,
            path,
            revocation_root,
        );
        let (proof, public_inputs) = verify_claim_v1_proof(
            &self.verify_claim_zkbin,
            &self.verify_claim_pk,
            &input,
        )?;

        let params = VerifyClaimParamsV1 {
            claim_id,
            attestation_id,
            evidence_commitment: evidence,
            revealed_result: public_inputs.revealed_result,
            attestation_data,
            revocation_root,
        };

        let mut call_data = vec![0x03];
        params.encode(&mut call_data)?;

        Ok(VerifyClaimResult { call_data, proof, public_inputs })
    }

    /// Consume a claim (function code 0x04)
    pub fn consume_claim(
        &self,
        claim_id: pallas::Base,
        attestation_id: pallas::Base,
        nullifier: pallas::Base,
        claimant_secret: pallas::Base,
        claimant_public: PublicKey,
    ) -> Result<ConsumeClaimResult, Box<dyn std::error::Error>> {
        let input = ConsumeClaimV1CallData::new(claim_id, nullifier, claimant_secret, claimant_public);
        let (proof, public_inputs) = consume_claim_v1_proof(
            &self.consume_claim_zkbin,
            &self.consume_claim_pk,
            &input,
        )?;

        let params = ConsumeClaimParamsV1 {
            claim_id,
            attestation_id,
            claimant_pub_x: public_inputs.claimant_pub_x,
            claimant_pub_y: public_inputs.claimant_pub_y,
            nullifier: public_inputs.nullifier,
        };

        let mut call_data = vec![0x04];
        params.encode(&mut call_data)?;

        Ok(ConsumeClaimResult { call_data, proof, public_inputs })
    }

    /// Delegate an attestation (function code 0x06)
    #[allow(clippy::too_many_arguments)]
    pub fn delegate_attestation(
        &self,
        delegation_id: pallas::Base,
        parent_id: pallas::Base,
        delegator_secret: pallas::Base,
        delegation_type: pallas::Base,
        max_ratio: pallas::Base,
        revocation_root: pallas::Base,
        chain_root: pallas::Base,
        current_depth: pallas::Base,
        max_depth: pallas::Base,
        delegator_stake: pallas::Base,
        delegatee_stake: pallas::Base,
        nonce: pallas::Base,
        pos: pallas::Base,
        path: [pallas::Base; 255],
        chain_pos: pallas::Base,
        chain_path: [pallas::Base; 255],
        delegator_public: PublicKey,
        delegatee_public: PublicKey,
    ) -> Result<DelegateAttestationResult, Box<dyn std::error::Error>> {
        let input = DelegateAttestationV1CallData::new(
            delegation_id,
            parent_id,
            delegator_secret,
            delegation_type,
            max_ratio,
            revocation_root,
            chain_root,
            current_depth,
            max_depth,
            delegator_stake,
            delegatee_stake,
            nonce,
            pos,
            path,
            chain_pos,
            chain_path,
            delegator_public,
            delegatee_public,
        );
        let (proof, public_inputs) = delegate_attestation_v1_proof(
            &self.delegate_attestation_zkbin,
            &self.delegate_attestation_pk,
            &input,
        )?;

        let params = DelegateAttestationParamsV1 {
            proof: proof.as_ref().to_vec(),
            delegation_id: public_inputs.delegation_id,
            parent_id,
            delegator_pub_x: public_inputs.delegator_pub_x,
            delegator_pub_y: public_inputs.delegator_pub_y,
            delegatee_pub_x: public_inputs.delegatee_pub_x,
            delegatee_pub_y: public_inputs.delegatee_pub_y,
            delegation_type,
            max_ratio: public_inputs.max_ratio,
            revocation_root: public_inputs.revocation_root,
            chain_root,
            chain_depth: public_inputs.current_depth,
            max_depth: public_inputs.max_depth,
            delegator_stake: public_inputs.delegator_stake,
            delegatee_stake: public_inputs.delegatee_stake,
        };

        let mut call_data = vec![0x06];
        params.encode(&mut call_data)?;

        Ok(DelegateAttestationResult { call_data, proof, public_inputs })
    }
}

impl super::ContractHarness for AttestationHarness {
    fn name(&self) -> &str {
        "attestation"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateAttestationV1",
            "CreateClaimV1",
            "VerifyClaimV1",
            "ConsumeClaimV1",
            "DelegateAttestationV1",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateAttestationV1" => Some(&self.create_attestation_zkbin),
            "CreateClaimV1" => Some(&self.create_claim_zkbin),
            "VerifyClaimV1" => Some(&self.verify_claim_zkbin),
            "ConsumeClaimV1" => Some(&self.consume_claim_zkbin),
            "DelegateAttestationV1" => Some(&self.delegate_attestation_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateAttestationV1" => Some(&self.create_attestation_pk),
            "CreateClaimV1" => Some(&self.create_claim_pk),
            "VerifyClaimV1" => Some(&self.verify_claim_pk),
            "ConsumeClaimV1" => Some(&self.consume_claim_pk),
            "DelegateAttestationV1" => Some(&self.delegate_attestation_pk),
            _ => None,
        }
    }
}

pub struct CreateAttestationResult {
    pub call_data: Vec<u8>,
    pub attestation_id: pallas::Base,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CreateAttestationV1PublicInputs,
}

pub struct CreateClaimResult {
    pub call_data: Vec<u8>,
    pub claim_id: pallas::Base,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CreateClaimV1PublicInputs,
}

pub struct VerifyClaimResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: VerifyClaimV1PublicInputs,
}

pub struct ConsumeClaimResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: ConsumeClaimV1PublicInputs,
}

pub struct DelegateAttestationResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: DelegateAttestationV1PublicInputs,
}
