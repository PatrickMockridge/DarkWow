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

//! Attestation revoke_attestation_v1 ZK proof generation (V2 circuit)
//!
//! Issue #3 (github PatrickMockridge/DarkWow#3): this module is new. `revoke_attestation`
//! had no circuit, and its host compared the stored `attestor_pub` against the wire's copy
//! of itself — so anyone who could read the record could revoke the attestation.
//! `revoke_attestation.zk` derive-and-exposes the attestor's key from `attestor_secret`
//! (`consume_claim.zk`'s form, the one `check-pubkey-binding.sh` calls SOUND), and
//! `revoke_attestation_v1` compares the published coordinates against the stored key.

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::rngs::OsRng;
use rand::SeedableRng;

/// RevokeAttestationV1 circuit public inputs
/// (V2: tx_binding, tx_nonce, attestor_pub_x, attestor_pub_y)
#[derive(Debug, Clone)]
pub struct RevokeAttestationV1PublicInputs {
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
    pub attestor_pub_x: pallas::Base,
    pub attestor_pub_y: pallas::Base,
}

impl RevokeAttestationV1PublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        // `OBL-C198`: the tx pair is the last two instances (matching the reordered circuit).
        vec![
            self.attestor_pub_x,
            self.attestor_pub_y,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for revoke_attestation proof generation
#[derive(Debug, Clone)]
pub struct RevokeAttestationV1CallData {
    pub attestor_secret: pallas::Base,
    pub attestor_public: PublicKey,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl RevokeAttestationV1CallData {
    pub fn new(attestor_secret: pallas::Base, attestor_public: PublicKey) -> Self {
        Self {
            attestor_secret,
            attestor_public,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    pub fn compute_public_inputs(&self) -> RevokeAttestationV1PublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ax, ay) = self.attestor_public.xy().expect("pk not identity");
        // Circuit: DOMAIN_TX_BINDING = witness_base(3) = 3
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        RevokeAttestationV1PublicInputs {
            tx_binding,
            tx_nonce: self.tx_nonce,
            attestor_pub_x: ax,
            attestor_pub_y: ay,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        // Circuit witness order: attestor_secret, attestor_pub_x, attestor_pub_y,
        // tx_commitment, tx_nonce, tx_binding
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ax, ay) = self.attestor_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        vec![
            Witness::Base(Value::known(self.attestor_secret)),
            Witness::Base(Value::known(ax)),
            Witness::Base(Value::known(ay)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)),
        ]
    }
}

/// Create a RevokeAttestation ZK proof
pub fn revoke_attestation_v1_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &RevokeAttestationV1CallData,
) -> Result<(Proof, RevokeAttestationV1PublicInputs)> {
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
