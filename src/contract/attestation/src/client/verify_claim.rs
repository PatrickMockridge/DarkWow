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

//! Attestation verify_claim_v1 ZK proof generation (V2 circuit)

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::poseidon_hash,
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// VerifyClaimV1 circuit public inputs (V2: only tx_binding, tx_nonce)
#[derive(Debug, Clone)]
pub struct VerifyClaimV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyClaimV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.tx_binding, self.tx_nonce]
    }
}

/// Input data for verify_claim proof generation
///
/// Issue #3: this carried `evidence`, `attestation_data` and `nonce`, which the circuit
/// witnessed and then hashed into three values it discarded — `evidence_hash`,
/// `attestation_hash` and `leaf` reached no `constrain_instance` and no `constrain_equal_*`,
/// and the host read none of them. The witnesses and the dead derivations are **removed**
/// (`AGENTS.md` R2) rather than exposed: the verdict is now computed by the host from
/// `claim.evidence_commitment` — already checked against the stored claim — and the
/// attestation's stored `claim_data`, so a proof-bound copy of the same values would be a
/// second source for a value that has one home.
#[derive(Debug, Clone)]
pub struct VerifyClaimV1CallData {
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyClaimV1CallData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        _claim_id: pallas::Base,
        _revealed_result: pallas::Base,
        _evidence: pallas::Base,
        _attestation_data: pallas::Base,
        _nonce: pallas::Base,
        _pos: pallas::Base,
        _path: [pallas::Base; 255],
        _revocation_root: pallas::Base,
    ) -> Self {
        Self { tx_commitment: pallas::Base::zero(), tx_nonce: pallas::Base::zero() }
    }

    pub fn compute_public_inputs(&self) -> VerifyClaimV1PublicInputs {
        // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        VerifyClaimV1PublicInputs { tx_binding, tx_nonce: self.tx_nonce }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        // Circuit witness order: tx_commitment, tx_nonce, tx_binding
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
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
    let proof = if crate::deterministic_zk_enabled() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut rng)?
    } else {
        Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?
    };

    Ok((proof, public_inputs))
}
