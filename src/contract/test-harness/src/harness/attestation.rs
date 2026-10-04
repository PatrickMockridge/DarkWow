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
    crypto::{pasta_prelude::PrimeField, poseidon_hash, ContractId, PublicKey},
    pasta::pallas,
};
use dwow_serial::Encodable;
use rand::SeedableRng;

/// The commitment over an ordered call set (`OBL-C198`). The order is DFS post-order; attestation's
/// endpoints are childless, so each builder passes a one-element set. One commitment per
/// transaction, so an arm's proof and the arm's published binding are the same value.
fn commitment_of(calls: &[dwow_sdk::tx::ContractCall]) -> pallas::Base {
    dwow_sdk::crypto::util::tx_commitment(calls.iter())
}

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
    revoke_attestation::{
        RevokeAttestationV1CallData, revoke_attestation_v1_proof, RevokeAttestationV1PublicInputs,
    },
    expire_attestation::{
        ExpireAttestationV1CallData, expire_attestation_v1_proof, ExpireAttestationV1PublicInputs,
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
    revoke_attestation_zkbin: ZkBinary,
    revoke_attestation_pk: ProvingKey,
    expire_attestation_zkbin: ZkBinary,
    expire_attestation_pk: ProvingKey,
    /// The contract's deployed id (`OBL-C198`): the commitment is over the call set, and a call
    /// carries the contract it addresses, so a prover must know this id.
    contract_id: ContractId,
}

impl AttestationHarness {
    /// The commitment for a single-call endpoint of this harness — the call, and the node's own
    /// derivation over it (`OBL-C198`). One helper, so a second derivation cannot drift.
    fn commitment(&self, call_data: &[u8]) -> pallas::Base {
        commitment_of(&[dwow_sdk::tx::ContractCall { contract_id: self.contract_id, data: call_data.to_vec() }])
    }

    pub fn spawn(contract_id: ContractId) -> Self {
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
        // Issue #3: `revoke_attestation` gained a circuit — it had none, so its authority
        // check compared the stored `attestor_pub` against the wire's copy of itself.
        let revoke_attestation_bin =
            include_bytes!("../../../attestation/proof/revoke_attestation.zk.bin");
        // OBL-C196(i): `expire_attestation` gained a circuit — its arm checked no caller.
        let expire_attestation_bin =
            include_bytes!("../../../attestation/proof/expire_attestation.zk.bin");

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
        let revoke_attestation_zkbin = ZkBinary::decode(revoke_attestation_bin, false).unwrap();
        let expire_attestation_zkbin = ZkBinary::decode(expire_attestation_bin, false).unwrap();

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
        let revoke_attestation_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&revoke_attestation_zkbin).unwrap(),
            &revoke_attestation_zkbin,
        );
        let expire_attestation_circuit = ZkCircuit::new(
            dwow_core::zk::empty_witnesses(&expire_attestation_zkbin).unwrap(),
            &expire_attestation_zkbin,
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
        let revoke_attestation_pk =
            ProvingKey::build(revoke_attestation_zkbin.k, &revoke_attestation_circuit)
                .expect("ProvingKey::build failed for revoke_attestation");
        let expire_attestation_pk =
            ProvingKey::build(expire_attestation_zkbin.k, &expire_attestation_circuit)
                .expect("ProvingKey::build failed for expire_attestation");

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
            revoke_attestation_zkbin,
            revoke_attestation_pk,
            expire_attestation_zkbin,
            expire_attestation_pk,
            contract_id,
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
        let mut input = CreateAttestationV1CallData::new(attestor_secret, attestor_public);

        let params = CreateAttestationParamsV1 {
            // `OBL-C198`: the params carry an **empty** proof — the real proof rides the tx proof
            // vector, which the commitment excludes, else the commitment would cover the proof.
            proof: vec![],
            attestation_id: AttestationId(attestation_id),
            attestor_pub: attestor_public,
            claim_type,
            claim_data,
            metadata,
            expires_at,
        };

        let mut call_data = vec![0x00];
        call_data.extend_from_slice(&params.encode()?);

        // `OBL-C198`: prove LAST, over the finished call data — the commitment is a derivation over
        // these bytes and excludes proofs, which is what makes the order solvable.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = create_attestation_v1_proof(
            &self.create_attestation_zkbin,
            &self.create_attestation_pk,
            &input,
        )?;

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
        let mut input = CreateClaimV1CallData::new(attestation_id, claimant_secret, claimant_public);

        let params = CreateClaimParamsV1 {
            // `OBL-C198`: the params carry an **empty** proof — the real proof rides the tx proof
            // vector, which the commitment excludes, else the commitment would cover the proof.
            proof: vec![],
            claim_id: ClaimId(claim_id),
            attestation_id: AttestationId(attestation_id),
            claimant_pub: claimant_public,
            predicate,
            evidence_commitment,
            revealed_result,
        };

        let mut call_data = vec![0x03];
        call_data.extend_from_slice(&params.encode()?);

        // `OBL-C198`: prove LAST, over the finished call data.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = create_claim_v1_proof(
            &self.create_claim_zkbin,
            &self.create_claim_pk,
            &input,
        )?;

        Ok(CreateClaimResult { call_data, claim_id, proof, public_inputs })
    }

    /// Verify a claim (function code 0x04)
    ///
    /// Issue #3: `revealed_result` and `attestation_data` are no longer part of
    /// `VerifyClaimParamsV1` and no longer inputs to the verdict — the contract derives it
    /// from the stored claim's predicate and evidence and the attestation's stored
    /// `claim_data`. The parameters are kept, underscored, so call sites that still pass
    /// them read as what they now are: values with nowhere to go.
    #[allow(clippy::too_many_arguments)]
    pub fn verify_claim(
        &self,
        claim_id: pallas::Base,
        attestation_id: pallas::Base,
        _revealed_result: pallas::Base,
        evidence: pallas::Base,
        _attestation_data: pallas::Base,
        nonce: pallas::Base,
        pos: pallas::Base,
        path: [pallas::Base; 255],
        revocation_root: pallas::Base,
    ) -> Result<VerifyClaimResult, Box<dyn std::error::Error>> {
        let mut input = VerifyClaimV1CallData::new(
            claim_id,
            _revealed_result,
            evidence,
            _attestation_data,
            nonce,
            pos,
            path,
            revocation_root,
        );

        let params = VerifyClaimParamsV1 {
            claim_id: ClaimId(claim_id),
            attestation_id: AttestationId(attestation_id),
            evidence_commitment: evidence,
        };

        let mut call_data = vec![0x04];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: prove LAST, over the finished call data.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = verify_claim_v1_proof(
            &self.verify_claim_zkbin,
            &self.verify_claim_pk,
            &input,
        )?;

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
        let mut input = ConsumeClaimV1CallData::new(claim_id, nullifier, claimant_secret, claimant_public);
        // `OBL-C198`: the nullifier the params carry is a pure function of the claim and the
        // secret — not of the transaction — so it is known before the call is assembled. The call
        // data then exists, the commitment is derived over it, and only then is the proof made.
        let nullifier_out = input.compute_public_inputs().nullifier;

        let params = ConsumeClaimParamsV1 {
            claim_id: ClaimId(claim_id),
            attestation_id: AttestationId(attestation_id),
            claimant_pub: claimant_public,
            nullifier: nullifier_out,
        };

        let mut call_data = vec![0x05];
        call_data.extend_from_slice(&params.encode());

        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = consume_claim_v1_proof(
            &self.consume_claim_zkbin,
            &self.consume_claim_pk,
            &input,
        )?;

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
        let mut input = DelegateAttestationV1CallData::new(
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

        let params = DelegateAttestationParamsV1 {
            // `OBL-C198`: the params carry an **empty** proof — the real proof rides the tx proof
            // vector, which the commitment excludes, else the commitment would cover the proof.
            proof: vec![],
            delegation_id,
            parent_id,
            delegator_pub: delegator_public,
            delegatee_pub: delegatee_public,
            delegation_type: delegation_type.to_repr()[0],
            max_ratio: u64::from_le_bytes(max_ratio.to_repr()[0..8].try_into().unwrap()),
        };

        let mut call_data = vec![0x08];
        call_data.extend_from_slice(&params.encode()?);

        // `OBL-C198`: prove LAST, over the finished call data.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = delegate_attestation_v1_proof(
            &self.delegate_attestation_zkbin,
            &self.delegate_attestation_pk,
            &input,
        )?;

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
        let mut input = CheckNotRevokedV1CallData::new(revocation_root, nonce, pos, path);

        let params = CheckNotRevokedParamsV1 {
            // `OBL-C198`: the params carry an **empty** proof — the real proof rides the tx proof
            // vector, which the commitment excludes, else the commitment would cover the proof.
            proof: vec![],
            revocation_root,
            nonce,
        };

        let mut call_data = vec![0x07];
        call_data.extend_from_slice(&params.encode()?);

        // `OBL-C198`: prove LAST, over the finished call data.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = check_not_revoked_v1_proof(
            &self.check_not_revoked_zkbin, &self.check_not_revoked_pk, &input,
        )?;

        Ok(CheckNotRevokedResult { call_data, proof, public_inputs })
    }

    /// Update delegation parameters (function code 0x0a)
    ///
    /// OBL-C196(ii): `delegator_secret`/`delegator_pub` are new. The arm authorized no one, and
    /// its params carried no key; the circuit now derives the delegator's coordinates from the
    /// secret and exposes them, and the handler requires them to be the original attestation's
    /// `attestor_pub`.
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
        delegator_secret: pallas::Base,
        delegator_pub: PublicKey,
    ) -> Result<UpdateDelegationResult, Box<dyn std::error::Error>> {
        let _ = (
            delegation_type, current_depth, max_depth,
            delegator_stake, delegatee_stake, max_ratio,
        );
        let mut input = UpdateDelegationV1CallData {
            delegator_secret,
            delegator_public: delegator_pub,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };

        let params = UpdateDelegationParamsV1 {
            // `OBL-C198`: the params carry an **empty** proof — the real proof rides the tx proof
            // vector, which the commitment excludes, else the commitment would cover the proof.
            proof: vec![],
            original_attestation_id,
            delegation_type: delegation_type_u8,
            max_ratio: max_ratio_u64,
            delegator_pub,
        };

        let mut call_data = vec![0x0a];
        call_data.extend_from_slice(&params.encode()?);

        // `OBL-C198`: prove LAST, over the finished call data.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = update_delegation_v1_proof(
            &self.update_delegation_zkbin, &self.update_delegation_pk, &input,
        )?;

        Ok(UpdateDelegationResult { call_data, proof, public_inputs })
    }

    /// Slash an attestation (function code 0x0b)
    /// Issue #3: `attester_secret` is a parameter now, and the instance vector carries the
    /// coordinates. The circuit no longer leaves the secret and the coordinates unconstrained:
    /// it derives the coordinates from the secret and instances them, so a fixture that
    /// hardcoded a secret of `1` beside a public key derived from another value, under a
    /// two-element instance vector, described a proof the circuit can no longer be satisfied
    /// by — which is the binding working. The old fixture would have failed at exactly this
    /// point once the circuits changed, and the value it hardcoded was never related to the
    /// key it named.
    pub fn attest_slash(
        &self,
        attester_secret: pallas::Base,
        relayer_pub: PublicKey,
        slash_amount: u64,
        withdrawal_id: pallas::Base,
        block_height: u64,
    ) -> Result<AttestSlashResult, Box<dyn std::error::Error>> {
        let (ax, ay) = relayer_pub.xy().expect("pk not identity");
        let params = AttestSlashParamsV1 {
            relayer_pub,
            slash_amount,
            withdrawal_id,
            block_height,
        };

        let mut call_data = vec![0x0b];
        call_data.extend_from_slice(&params.encode());

        // `OBL-C198`: the binding derives from the commitment over the finished call data. It was
        // the constant `poseidon_hash(3, 0, 0)`, which bound the proof to nothing at all.
        let tc = self.commitment(&call_data);
        let txb = dwow_sdk::crypto::poseidon_hash([pallas::Base::from(3u64), tc, pallas::Base::zero()]);
        // Circuit witness order: attester_secret, attester_pub_x, attester_pub_y, tx_commitment,
        // tx_nonce, tx_binding — and `tx_commitment` is now the real commitment.
        let witnesses = vec![
            Witness::Base(Value::known(attester_secret)),
            Witness::Base(Value::known(ax)),
            Witness::Base(Value::known(ay)),
            Witness::Base(Value::known(tc)),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(txb)),
        ];
        // Circuit constrain_instance order: attester_pub_x, attester_pub_y, tx_binding, tx_nonce
        let publics = [ax, ay, txb, pallas::Base::zero()];
        let circuit = ZkCircuit::new(witnesses, &self.attest_slash_zkbin);
        let proof = if dwow_attestation_contract::deterministic_zk_enabled() {
            Proof::create(&self.attest_slash_pk, &[circuit], &publics, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.attest_slash_pk, &[circuit], &publics, rand::rngs::OsRng)
        }.map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        Ok(AttestSlashResult { call_data, proof })
    }

    /// Commit a fee schedule (function code 0x0c)
    /// Issue #3: as `attest_slash` above — the attester's secret is a parameter and the
    /// instance vector carries the coordinates, because the circuit derives and exposes them.
    pub fn commit_fee_schedule(
        &self,
        attester_secret: pallas::Base,
        attestor_pub: PublicKey,
        base_fee_bp: u64,
        guaranteed_premium_bp: u64,
        max_amount: u64,
        min_amount: u64,
        metadata: Vec<u8>,
    ) -> Result<CommitFeeScheduleResult, Box<dyn std::error::Error>> {
        let (ax, ay) = attestor_pub.xy().expect("pk not identity");
        let params = CommitFeeScheduleParamsV1 {
            attestor_pub,
            base_fee_bp,
            guaranteed_premium_bp,
            max_amount,
            min_amount,
            metadata,
        };

        let mut call_data = vec![0x0c];
        call_data.extend_from_slice(&params.encode()?);

        // `OBL-C198`: the binding derives from the commitment over the finished call data.
        let tc = self.commitment(&call_data);
        let txb = dwow_sdk::crypto::poseidon_hash([pallas::Base::from(3u64), tc, pallas::Base::zero()]);
        let witnesses = vec![
            Witness::Base(Value::known(attester_secret)),
            Witness::Base(Value::known(ax)),
            Witness::Base(Value::known(ay)),
            Witness::Base(Value::known(tc)),
            Witness::Base(Value::known(pallas::Base::zero())),
            Witness::Base(Value::known(txb)),
        ];
        // Circuit constrain_instance order: attester_pub_x, attester_pub_y, tx_binding, tx_nonce
        let publics = [ax, ay, txb, pallas::Base::zero()];
        let circuit = ZkCircuit::new(witnesses, &self.commit_fee_schedule_zkbin);
        let proof = if dwow_attestation_contract::deterministic_zk_enabled() {
            Proof::create(&self.commit_fee_schedule_pk, &[circuit], &publics, rand::rngs::StdRng::seed_from_u64(0))
        } else {
            Proof::create(&self.commit_fee_schedule_pk, &[circuit], &publics, rand::rngs::OsRng)
        }.map_err(|_| dwow_core::Error::Custom("Proof::create failed".to_string()))?;

        Ok(CommitFeeScheduleResult { call_data, proof })
    }

    /// Revoke an attestation (function code 0x01, ZK — the circuit is new in this change).
    ///
    /// Issue #3: the instruction had no circuit, so its only authority check was the stored
    /// `attestor_pub` against the wire's copy of itself. It now requires a proof that the
    /// caller can open the attestor's key, which is why this builder takes the secret.
    pub fn revoke_attestation(
        &self,
        attestor_secret: pallas::Base,
        attestor_pub: PublicKey,
        attestation_id: pallas::Base,
    ) -> Result<RevokeAttestationResult, Box<dyn std::error::Error>> {
        let mut input = RevokeAttestationV1CallData::new(attestor_secret, attestor_pub);
        let params = dwow_attestation_contract::model::RevokeAttestationParamsV1 {
            attestor_pub,
            attestation_id: dwow_attestation_contract::model::AttestationId(attestation_id),
        };
        let mut call_data = vec![0x01];
        call_data.extend_from_slice(&params.encode());
        // `OBL-C198`: prove LAST, over the finished call data.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = revoke_attestation_v1_proof(
            &self.revoke_attestation_zkbin,
            &self.revoke_attestation_pk,
            &input,
        )?;
        Ok(RevokeAttestationResult { call_data, proof, public_inputs })
    }

    /// Expire an attestation (function code 0x02, ZK — the circuit is new, OBL-C196(i)).
    ///
    /// The arm checked no caller, so any party could expire any attestation. It now requires a
    /// proof that the caller can open the attestor's key, which is why this builder takes the
    /// secret — `revoke_attestation`'s form.
    pub fn expire_attestation(
        &self,
        attestor_secret: pallas::Base,
        attestor_pub: PublicKey,
        attestation_id: pallas::Base,
    ) -> Result<ExpireAttestationResult, Box<dyn std::error::Error>> {
        let mut input = ExpireAttestationV1CallData::new(attestor_secret, attestor_pub);
        let params = dwow_attestation_contract::model::ExpireAttestationParamsV1 {
            attestation_id: dwow_attestation_contract::model::AttestationId(attestation_id),
            attestor_pub,
        };
        let mut call_data = vec![0x02];
        call_data.extend_from_slice(&params.encode());
        // `OBL-C198`: prove LAST, over the finished call data.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, public_inputs) = expire_attestation_v1_proof(
            &self.expire_attestation_zkbin,
            &self.expire_attestation_pk,
            &input,
        )?;
        Ok(ExpireAttestationResult { call_data, proof, public_inputs })
    }

    /// Verify a delegation chain (function code 0x09, ZK).
    pub fn verify_chain(
        &self,
        delegation_id: pallas::Base,
    ) -> Result<VerifyChainResult, Box<dyn std::error::Error>> {
        use dwow_attestation_contract::client::verify_chain::{VerifyChainV1CallData, verify_chain_v1_proof};
        let mut input = VerifyChainV1CallData::new(
            pallas::Base::zero(), pallas::Base::zero(), pallas::Base::zero(),
            pallas::Base::zero(), pallas::Base::zero(),
            pallas::Base::zero(), [pallas::Base::from(0u64); 255],
        );
        let params = dwow_attestation_contract::model::VerifyChainParamsV1 {
            // `OBL-C198`: the params carry an **empty** proof — the real proof rides the tx proof
            // vector, which the commitment excludes, else the commitment would cover the proof.
            proof: vec![],
            delegation_id,
            parent_id: pallas::Base::zero(),
        };
        let mut call_data = vec![0x09];
        call_data.extend_from_slice(&params.encode()?);
        // `OBL-C198`: prove LAST, over the finished call data.
        input.tx_commitment = self.commitment(&call_data);
        let (proof, _public_inputs) = verify_chain_v1_proof(
            &self.verify_chain_zkbin, &self.verify_chain_pk, &input,
        )?;
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
        call_data.extend_from_slice(&params.encode()?);
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
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: RevokeAttestationV1PublicInputs,
}

pub struct ExpireAttestationResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
    pub public_inputs: ExpireAttestationV1PublicInputs,
}

pub struct ValidateClaimResult {
    pub call_data: Vec<u8>,
}

pub struct VerifyChainResult {
    pub call_data: Vec<u8>,
    pub proof: dwow_core::zk::Proof,
}
