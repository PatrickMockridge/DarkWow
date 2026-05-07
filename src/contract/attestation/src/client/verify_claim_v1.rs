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

//! Attestation verify_claim_v1 ZK proof generation

use darkfi::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use darkfi_sdk::pasta::pallas;
use rand::rngs::OsRng;

/// VerifyClaimV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct VerifyClaimV1PublicInputs {
    pub claim_id: pallas::Base,
    pub revealed_result: pallas::Base,
    pub revocation_root: pallas::Base,
    pub attestation_data: pallas::Base,
}

impl VerifyClaimV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // Must match constrain_instance execution order:
        // 1. revocation_root (from SetMembership opcode)
        // 2. claim_id (explicit)
        // 3. revealed_result (explicit)
        // 4. revocation_root (explicit, duplicate)
        // 5. attestation_data (explicit)
        vec![
            self.revocation_root,
            self.claim_id,
            self.revealed_result,
            self.revocation_root,
            self.attestation_data,
        ]
    }
}

/// Input data for verify_claim proof generation
#[derive(Debug, Clone)]
pub struct VerifyClaimV1CallData {
    pub claim_id: pallas::Base,
    pub revealed_result: pallas::Base,
    pub evidence: pallas::Base,
    pub attestation_data: pallas::Base,
    pub nonce: pallas::Base,
    pub pos: pallas::Base,
    pub path: [pallas::Base; 255],
    pub revocation_root: pallas::Base,
}

impl VerifyClaimV1CallData {
    pub fn new(
        claim_id: pallas::Base,
        revealed_result: pallas::Base,
        evidence: pallas::Base,
        attestation_data: pallas::Base,
        nonce: pallas::Base,
        pos: pallas::Base,
        path: [pallas::Base; 255],
        revocation_root: pallas::Base,
    ) -> Self {
        Self {
            claim_id,
            revealed_result,
            evidence,
            attestation_data,
            nonce,
            pos,
            path,
            revocation_root,
        }
    }

    pub fn compute_public_inputs(&self) -> VerifyClaimV1PublicInputs {
        VerifyClaimV1PublicInputs {
            claim_id: self.claim_id,
            revealed_result: self.revealed_result,
            revocation_root: self.revocation_root,
            attestation_data: self.attestation_data,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            Witness::Base(Value::known(self.claim_id)),
            Witness::Base(Value::known(self.revealed_result)),
            Witness::Base(Value::known(self.evidence)),
            Witness::Base(Value::known(self.attestation_data)),
            Witness::Base(Value::known(self.nonce)),
            Witness::Base(Value::known(self.pos)),
            Witness::SparseMerklePath(Value::known(self.path)),
            Witness::Base(Value::known(self.revocation_root)),
        ]
    }
}

/// Create a VerifyClaim ZK proof
pub fn verify_claim_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &VerifyClaimV1CallData,
) -> Result<(Proof, VerifyClaimV1PublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}