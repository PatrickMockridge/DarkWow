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
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
};
use dwow_sdk::{
    crypto::{pasta_prelude::PrimeField, poseidon_hash, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;
use rand::SeedableRng;

use dwow_attestation_contract::client::{
    check_not_revoked::{
        CheckNotRevokedV1CallData, check_not_revoked_v1_proof, CheckNotRevokedV1PublicInputs,
    },
    consume_claim::{
        ConsumeClaimV1CallData, consume_claim_v1_proof, ConsumeClaimV1PublicInputs,
    },
    create_attestation::{
        CreateAttestationV1CallData, create_attestation_v1_proof, CreateAttestationV1PublicInputs,
    },
    create_claim::{
        CreateClaimV1CallData, create_claim_v1_proof, CreateClaimV1PublicInputs,
    },
    delegate_attestation::{
        DelegateAttestationV1CallData, delegate_attestation_v1_proof,
        DelegateAttestationV1PublicInputs,
    },
    update_delegation::{
        UpdateDelegationV1CallData, update_delegation_v1_proof, UpdateDelegationV1PublicInputs,
    },
    verify_claim::{
        VerifyClaimV1CallData, verify_claim_v1_proof, VerifyClaimV1PublicInputs,
    },
};
use dwow_attestation_contract::model::{
    AttestSlashParamsV1, AttestationId, CheckNotRevokedParamsV1, ClaimId,
    CommitFeeScheduleParamsV1, ConsumeClaimParamsV1, CreateAttestationParamsV1,
    CreateClaimParamsV1, DelegateAttestationParamsV1, Predicate,
    UpdateDelegationParamsV1, VerifyClaimParamsV1,
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
    attest_slash_zkbin: ZkBinary,
    attest_slash_pk: ProvingKey,
    check_not_revoked_zkbin: ZkBinary,
    check_not_revoked_pk: ProvingKey,
    commit_fee_schedule_zkbin: ZkBinary,
    commit_fee_schedule_pk: ProvingKey,
    verify_chain_zkbin: ZkBinary,
    verify_chain_pk: ProvingKey,
    update_delegation_zkbin: ZkBinary,
    update_delegation_pk: ProvingKey,
}

impl AttestationHarness {
    pub fn spawn() -> Self {
        dwow_attestation_contract::enable_deterministic_zk();
        let create_att_bin =
            include_bytes!("../../../attestation/proof/create_attestation.zk.bin");
        let create_claim_bin =
            include_bytes!("../../../attestation/proof/create_claim.zk.bin");
        let verify_claim_bin =
            include_bytes!("../../../attestation/proof/verify_claim.zk.bin");
        eprintln!("DEBUG: raw verify_claim_bin len={} first_10_bytes={:02x?}", verify_claim_bin.len(), &verify_claim_bin[..10]);
        let consume_claim_bin =
            include_bytes!("../../../attestation/proof/consume_claim.zk.bin");
        let delegate_bin =
            include_bytes!("../../../attestation/proof/delegate_attestation.zk.bin");
        let attest_slash_bin =
            include_bytes!("../../../attestation/proof/attest_slash.zk.bin");
        let check_not_revoked_bin =
            include_bytes!("../../../attestation/proof/check_not_revoked.zk.bin");
        let commit_fee_schedule_bin =
            include_bytes!("../../../attestation/proof/commit_fee_schedule.zk.bin");
        let update_delegation_bin =
            include_bytes!("../../../attestation/proof/update_delegation.zk.bin");
        let verify_chain_bin =
            include_bytes!("../../../attestation/proof/verify_chain.zk.bin");

        let create_attestation_zkbin = ZkBinary::decode(create_att_bin, false).unwrap();
        let create_claim_zkbin = ZkBinary::decode(create_claim_bin, false).unwrap();
        let verify_claim_zkbin = ZkBinary::decode(verify_claim_bin, false).unwrap();
        let consume_claim_zkbin = ZkBinary::decode(consume_claim_bin, false).unwrap();
        let delegate_attestation_zkbin = ZkBinary::decode(delegate_bin, false).unwrap();
        let attest_slash_zkbin = ZkBinary::decode(attest_slash_bin, false).unwrap();
        let check_not_revoked_zkbin = ZkBinary::decode(check_not_revoked_bin, false).unwrap();
        let commit_fee_schedule_zkbin = ZkBinary::decode(commit_fee_schedule_bin, false).unwrap();
        let update_delegation_zkbin = ZkBinary::decode(update_delegation_bin, false).unwrap();
        let verify_chain_zkbin = ZkBinary::decode(verify_chain_bin, false).unwrap();

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
        let attest_slash_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&attest_slash_zkbin).unwrap(),
            &attest_slash_zkbin,
        );
        let check_not_revoked_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&check_not_revoked_zkbin).unwrap(),
            &check_not_revoked_zkbin,
        );
        let commit_fee_schedule_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&commit_fee_schedule_zkbin).unwrap(),
            &commit_fee_schedule_zkbin,
        );
        let update_delegation_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&update_delegation_zkbin).unwrap(),
            &update_delegation_zkbin,
        );
        let verify_chain_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&verify_chain_zkbin).unwrap(),
            &verify_chain_zkbin,
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
        let verify_claim_pk = ProvingKey::build(verify_claim_zkbin.k, &verify_claim_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: verify_claim PK built successfully!");
        eprintln!("DEBUG: Building create_attestation PK with k={}", create_attestation_zkbin.k);
        let create_attestation_pk =
            ProvingKey::build(create_attestation_zkbin.k, &create_att_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: create_attestation PK built successfully!");
        eprintln!("DEBUG: Building create_claim PK with k={}", create_claim_zkbin.k);
        let create_claim_pk = ProvingKey::build(create_claim_zkbin.k, &create_claim_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: create_claim PK built successfully!");
        eprintln!("DEBUG: Building consume_claim PK with k={}", consume_claim_zkbin.k);
        let consume_claim_pk = ProvingKey::build(consume_claim_zkbin.k, &consume_claim_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: consume_claim PK built successfully!");
        eprintln!("DEBUG: Building delegate_attestation PK with k={}", delegate_attestation_zkbin.k);
        let delegate_attestation_pk =
            ProvingKey::build(delegate_attestation_zkbin.k, &delegate_attestation_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: delegate_attestation PK built successfully!");
        eprintln!("DEBUG: Building attest_slash PK with k={}", attest_slash_zkbin.k);
        let attest_slash_pk =
            ProvingKey::build(attest_slash_zkbin.k, &attest_slash_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: attest_slash PK built successfully!");
        eprintln!("DEBUG: Building check_not_revoked PK with k={}", check_not_revoked_zkbin.k);
        let check_not_revoked_pk =
            ProvingKey::build(check_not_revoked_zkbin.k, &check_not_revoked_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: check_not_revoked PK built successfully!");
        eprintln!("DEBUG: Building commit_fee_schedule PK with k={}", commit_fee_schedule_zkbin.k);
        let commit_fee_schedule_pk =
            ProvingKey::build(commit_fee_schedule_zkbin.k, &commit_fee_schedule_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: commit_fee_schedule PK built successfully!");
        eprintln!("DEBUG: Building update_delegation PK with k={}", update_delegation_zkbin.k);
        let update_delegation_pk =
            ProvingKey::build(update_delegation_zkbin.k, &update_delegation_circuit).expect("ProvingKey::build failed");
        eprintln!("DEBUG: update_delegation PK built successfully!");
        let verify_chain_pk =
            ProvingKey::build(verify_chain_zkbin.k, &verify_chain_circuit)
                .expect("ProvingKey::build failed for verify_chain");

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
            attest_slash_zkbin,
            attest_slash_pk,
            check_not_revoked_zkbin,
            check_not_revoked_pk,
            commit_fee_schedule_zkbin,
            commit_fee_schedule_pk,
            update_delegation_zkbin,
            update_delegation_pk,
            verify_chain_zkbin,
            verify_chain_pk,
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
            attestation_id: AttestationId(attestation_id),
            attestor_pub: attestor_public,
            claim_type,
            claim_data,
            metadata,
            expires_at,
        };

        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode());

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
            claim_id: ClaimId(claim_id),
            attestation_id: AttestationId(attestation_id),
            claimant_pub: claimant_public,
            predicate,
            evidence_commitment,
            revealed_result,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode());

        Ok(CreateClaimResult { call_data, claim_id, proof, public_inputs })
    }

    /// Verify a claim (function code 0x04)
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
            claim_id: ClaimId(claim_id),
            attestation_id: AttestationId(attestation_id),
            evidence_commitment: evidence,
            revealed_result,
            attestation_data,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        Ok(VerifyClaimResult { call_data, proof, public_inputs })
    }

    /// Consume a claim (function code 0x05)
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
            claim_id: ClaimId(claim_id),
            attestation_id: AttestationId(attestation_id),
            claimant_pub: claimant_public,
            nullifier: public_inputs.nullifier,
        };

        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode());

        Ok(ConsumeClaimResult { call_data, proof, public_inputs })
    }

    /// Delegate an attestation (function code 0x08)
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
            delegation_id,
            parent_id,
            delegator_pub: delegator_public,
            delegatee_pub: delegatee_public,
            delegation_type: delegation_type.to_repr()[0],
            max_ratio: u64::from_le_bytes(max_ratio.to_repr()[0..8].try_into().unwrap()),
        };

        let mut call_data = vec![0x08];
        call_data.extend_from_slice(&params.encode());

        Ok(DelegateAttestationResult { call_data, proof, public_inputs })
    }

    /// Check non-revocation status (function code 0x07)
    pub fn check_not_revoked(
        &self,
        revocation_root: pallas::Base,
        nonce: pallas::Base,
        pos: u64,
        path: Vec<dwow_sdk::crypto::MerkleNode>,
    ) -> Result<CheckNotRevokedResult, Box<dyn std::error::Error>> {
        let input = CheckNotRevokedV1CallData::new(revocation_root, nonce, pos, path);
        let (proof, public_inputs) = check_not_revoked_v1_proof(
            &self.check_not_revoked_zkbin, &self.check_not_revoked_pk, &input,
        )?;

        let params = CheckNotRevokedParamsV1 {
            proof: proof.as_ref().to_vec(),
            revocation_root,
            nonce,
        };

        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&params.encode());

        Ok(CheckNotRevokedResult { call_data, proof, public_inputs })
    }

    /// Update delegation parameters (function code 0x0a)
    #[allow(clippy::too_many_arguments)]
    pub fn update_delegation(
        &self,
        original_attestation_id: pallas::Base,
        delegation_type: pallas::Base,
        current_depth: pallas::Base,
        max_depth: pallas::Base,
        delegator_stake: pallas::Base,
        delegatee_stake: pallas::Base,
        max_ratio: pallas::Base,
        max_ratio_u64: u64,
        delegation_type_u8: u8,
    ) -> Result<UpdateDelegationResult, Box<dyn std::error::Error>> {
        let input = UpdateDelegationV1CallData::new(
            original_attestation_id, delegation_type, current_depth,
            max_depth, delegator_stake, delegatee_stake, max_ratio,
        );
        let (proof, public_inputs) = update_delegation_v1_proof(
            &self.update_delegation_zkbin, &self.update_delegation_pk, &input,
        )?;

        let params = UpdateDelegationParamsV1 {
            proof: proof.as_ref().to_vec(),
            original_attestation_id,
            delegation_type: delegation_type_u8,
            max_ratio: max_ratio_u64,
        };

        let mut call_data = vec![0x0a];
        call_data.extend_from_slice(&params.encode());

        Ok(UpdateDelegationResult { call_data, proof, public_inputs })
    }

    /// Slash an attestation (function code 0x0b)
    pub fn attest_slash(
        &self,
        relayer_pub: PublicKey,
        slash_amount: u64,
        withdrawal_id: pallas::Base,
        block_height: u64,
    ) -> Result<AttestSlashResult, Box<dyn std::error::Error>> {
        let txb = dwow_sdk::crypto::poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]);
        let (ax, ay) = relayer_pub.xy().expect("pk not identity");
        let witnesses = vec![
            Witness::Base(Value::known(pallas::Base::from(1u64))),
            Witness::Base(Value::known(ax)),
            Witness::Base(Value::known(ay)),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(txb)),
        ];
        let circuit = ZkCircuit::new(witnesses, &self.attest_slash_zkbin);
        let proof = if dwow_attestation_contract::deterministic_zk_enabled() {
            Proof::create(&self.attest_slash_pk, &[circuit], &[txb, pallas::Base::zero()], rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.attest_slash_pk, &[circuit], &[txb, pallas::Base::zero()], rand::rngs::OsRng)
        }.map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        let params = AttestSlashParamsV1 {
            relayer_pub,
            slash_amount,
            withdrawal_id,
            block_height,
        };

        let mut call_data = vec![0x0b];
        call_data.extend_from_slice(&params.encode());

        Ok(AttestSlashResult { call_data, proof })
    }

    /// Commit a fee schedule (function code 0x0c)
    pub fn commit_fee_schedule(
        &self,
        attestor_pub: PublicKey,
        base_fee_bp: u64,
        guaranteed_premium_bp: u64,
        max_amount: u64,
        min_amount: u64,
        metadata: Vec<u8>,
    ) -> Result<CommitFeeScheduleResult, Box<dyn std::error::Error>> {
        let txb = dwow_sdk::crypto::poseidon_hash([pallas::Base::from(3u64), pallas::Base::zero(), pallas::Base::zero()]);
        let (ax, ay) = attestor_pub.xy().expect("pk not identity");
        let witnesses = vec![
            Witness::Base(Value::known(pallas::Base::from(1u64))),
            Witness::Base(Value::known(ax)),
            Witness::Base(Value::known(ay)),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(txb)),
        ];
        let circuit = ZkCircuit::new(witnesses, &self.commit_fee_schedule_zkbin);
        let proof = if dwow_attestation_contract::deterministic_zk_enabled() {
            Proof::create(&self.commit_fee_schedule_pk, &[circuit], &[txb, pallas::Base::zero()], rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.commit_fee_schedule_pk, &[circuit], &[txb, pallas::Base::zero()], rand::rngs::OsRng)
        }.map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        let params = CommitFeeScheduleParamsV1 {
            attestor_pub,
            base_fee_bp,
            guaranteed_premium_bp,
            max_amount,
            min_amount,
            metadata,
        };

        let mut call_data = vec![0x0c];
        call_data.extend_from_slice(&params.encode());

        Ok(CommitFeeScheduleResult { call_data, proof })
    }

    /// Revoke an attestation (function code 0x01, non-ZK).
    pub fn revoke_attestation(
        &self,
        attestor_pub: PublicKey,
        attestation_id: pallas::Base,
    ) -> Result<RevokeAttestationResult, Box<dyn std::error::Error>> {
        let params = dwow_attestation_contract::model::RevokeAttestationParamsV1 {
            attestor_pub,
            attestation_id: dwow_attestation_contract::model::AttestationId(attestation_id),
        };
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());
        Ok(RevokeAttestationResult { call_data })
    }

    /// Expire an attestation (function code 0x02, non-ZK).
    pub fn expire_attestation(
        &self,
        attestation_id: pallas::Base,
    ) -> Result<ExpireAttestationResult, Box<dyn std::error::Error>> {
        let params = dwow_attestation_contract::model::ExpireAttestationParamsV1 {
            attestation_id: dwow_attestation_contract::model::AttestationId(attestation_id),
        };
        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());
        Ok(ExpireAttestationResult { call_data })
    }

    /// Verify a delegation chain (function code 0x09, ZK).
    pub fn verify_chain(
        &self,
        delegation_id: pallas::Base,
    ) -> Result<VerifyChainResult, Box<dyn std::error::Error>> {
        use dwow_attestation_contract::client::verify_chain::{VerifyChainV1CallData, verify_chain_v1_proof};
        let input = VerifyChainV1CallData::new(
            pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            pallas::Base::zero(), pallas::Base::zero(),
            pallas::Base::zero(), [pallas::Base::from(0u64); 255],
        );
        let (proof, _public_inputs) = verify_chain_v1_proof(
            &self.verify_chain_zkbin, &self.verify_chain_pk, &input,
        )?;
        let params = dwow_attestation_contract::model::VerifyChainParamsV1 {
            proof: proof.as_ref().to_vec(),
            delegation_id,
            parent_id: pallas::Base::zero(),
        };
        let mut call_data = vec![0x09];
        call_data.extend_from_slice(&params.encode());
        Ok(VerifyChainResult { call_data, proof })
    }

    /// Validate a claim (function code 0x06, non-ZK).
    pub fn validate_claim(
        &self,
        claim_id: pallas::Base,
        attestation_id: pallas::Base,
        evidence: Vec<pallas::Base>,
    ) -> Result<ValidateClaimResult, Box<dyn std::error::Error>> {
        let params = dwow_attestation_contract::model::ValidateClaimParamsV1 {
            claim_id: dwow_attestation_contract::model::ClaimId(claim_id),
            attestation_id: dwow_attestation_contract::model::AttestationId(attestation_id),
            evidence,
        };
        let mut call_data = vec![0x06];
        call_data.extend_from_slice(&params.encode());
        Ok(ValidateClaimResult { call_data })
    }
}

impl super::ContractHarness for AttestationHarness {
    fn name(&self) -> &str {
        "attestation"
    }

    fn circuits(&self) -> Vec<&'static str> {
        vec![
            "CreateAttestationV2",
            "CreateClaimV2",
            "VerifyClaimV2",
            "ConsumeClaimV2",
            "DelegateAttestationV2",
            "AttestSlashV2",
            "CheckNotRevokedV2",
            "CommitFeeScheduleV2",
            "UpdateDelegationV2",
            "VerifyChainV2",
        ]
    }

    fn get_zkbin(&self, ns: &str) -> Option<&ZkBinary> {
        match ns {
            "CreateAttestationV2" => Some(&self.create_attestation_zkbin),
            "CreateClaimV2" => Some(&self.create_claim_zkbin),
            "VerifyClaimV2" => Some(&self.verify_claim_zkbin),
            "ConsumeClaimV2" => Some(&self.consume_claim_zkbin),
            "DelegateAttestationV2" => Some(&self.delegate_attestation_zkbin),
            "AttestSlashV2" => Some(&self.attest_slash_zkbin),
            "CheckNotRevokedV2" => Some(&self.check_not_revoked_zkbin),
            "CommitFeeScheduleV2" => Some(&self.commit_fee_schedule_zkbin),
            "UpdateDelegationV2" => Some(&self.update_delegation_zkbin),
            "VerifyChainV2" => Some(&self.verify_chain_zkbin),
            _ => None,
        }
    }

    fn get_pk(&self, ns: &str) -> Option<&ProvingKey> {
        match ns {
            "CreateAttestationV2" => Some(&self.create_attestation_pk),
            "CreateClaimV2" => Some(&self.create_claim_pk),
            "VerifyClaimV2" => Some(&self.verify_claim_pk),
            "ConsumeClaimV2" => Some(&self.consume_claim_pk),
            "DelegateAttestationV2" => Some(&self.delegate_attestation_pk),
            "AttestSlashV2" => Some(&self.attest_slash_pk),
            "CheckNotRevokedV2" => Some(&self.check_not_revoked_pk),
            "CommitFeeScheduleV2" => Some(&self.commit_fee_schedule_pk),
            "UpdateDelegationV2" => Some(&self.update_delegation_pk),
            "VerifyChainV2" => Some(&self.verify_chain_pk),
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

/// Result of check_not_revoked
pub struct CheckNotRevokedResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: CheckNotRevokedV1PublicInputs,
}

/// Result of update_delegation
pub struct UpdateDelegationResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: UpdateDelegationV1PublicInputs,
}

/// Result of attest_slash
pub struct AttestSlashResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}

/// Result of commit_fee_schedule
pub struct CommitFeeScheduleResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}

pub struct RevokeAttestationResult {
    pub call_data: Vec<u8>,
}

pub struct ExpireAttestationResult {
    pub call_data: Vec<u8>,
}

pub struct ValidateClaimResult {
    pub call_data: Vec<u8>,
}

pub struct VerifyChainResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}
