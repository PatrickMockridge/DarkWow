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

//! Identity verify_capability_v1 ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, PublicKey},
    pasta::pallas,
};
use rand::{rngs::OsRng, SeedableRng};

/// VerifyCapabilityV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct VerifyCapabilityPublicInputs {
    /// The credential's nullifier — the revocation handle the host checks unspent.
    pub nullifier: pallas::Base,
    /// The credential's schema, compared by the host against the capability's requirement.
    pub schema_hash: pallas::Base,
    /// The credential's issuer, compared the same way.
    pub issuer_pub_x: pallas::Base,
    pub issuer_pub_y: pallas::Base,
    /// The threshold the predicate was evaluated at; the host requires it to be at least the
    /// capability's `min_threshold`.
    pub threshold: pallas::Base,
    pub predicate_result: pallas::Base,
    /// The name of the attribute the predicate is over — the host requires it to be the capability's
    /// `attribute_name`.
    pub attribute_1_name: pallas::Base,
    /// The credential commitment the proof reconstructs in-circuit — the host requires it to be the
    /// one its stored `Credential` carries.
    pub commitment: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyCapabilityPublicInputs {
    /// `VerifyCapability_V2`'s instance order. This returned three values — the nullifier and the tx
    /// pair — while the struct carried eight fields, so `capability_id` was the only thing the proof
    /// said anything about and it was not even an instance. See OBL-Z17.
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.nullifier,
            self.schema_hash,
            self.issuer_pub_x,
            self.issuer_pub_y,
            self.attribute_1_name,
            self.threshold,
            self.predicate_result,
            self.commitment,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for verify_capability proof generation
#[derive(Debug, Clone)]
pub struct VerifyCapabilityCallData {
    /// The credential's preimage — the same fields `issue_credential` hashes. The holder knows them;
    /// the proof reconstructs the commitment from them so that everything below is bound.
    pub credential_secret: pallas::Base,
    pub issuer_public: PublicKey,
    pub holder_public: PublicKey,
    pub schema_hash: pallas::Base,
    /// The attribute slots' *names*, as `attribute_name_field` maps them; the commitment covers the
    /// name and the value together.
    pub attribute_1_name: pallas::Base,
    pub attribute_2_name: pallas::Base,
    pub attribute_1: pallas::Base,
    pub attribute_2: pallas::Base,
    pub attribute_blind: pallas::Base,
    pub issued_at: u64,
    pub expires_at: u64,
    /// The credential commitment, computed from the preimage above by `new` — never supplied, so a
    /// caller cannot state a commitment that disagrees with the credential it is proving about.
    pub commitment: pallas::Base,
    /// The threshold the committed attribute must meet.
    pub threshold: pallas::Base,
    pub predicate_result: bool,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl VerifyCapabilityCallData {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        credential_secret: pallas::Base,
        attribute_1_name: pallas::Base,
        attribute_1: pallas::Base,
        threshold: pallas::Base,
        attribute_2_name: pallas::Base,
        attribute_2: pallas::Base,
        attribute_blind: pallas::Base,
        issuer_public: PublicKey,
        holder_public: PublicKey,
        schema_hash: pallas::Base,
        issued_at: u64,
        expires_at: u64,
        predicate_result: bool,
    ) -> Self {
        let mut call = Self {
            credential_secret,
            attribute_1_name,
            attribute_2_name,
            issuer_public,
            holder_public,
            schema_hash,
            attribute_1,
            attribute_2,
            attribute_blind,
            issued_at,
            expires_at,
            commitment: pallas::Base::zero(),
            threshold,
            predicate_result,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        };
        call.commitment = call.compute_commitment();
        call
    }

    /// The credential commitment — the same two hashes `issue_credential.zk:41-61` and
    /// `IssueCredentialCallData::compute_commitment` compute. The verify circuit reconstructs it
    /// in-circuit, so the client must agree with it exactly.
    pub fn compute_commitment(&self) -> pallas::Base {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (hx, hy) = self.holder_public.xy().expect("pk not identity");
        let attribute_1_hash = poseidon_hash([
            pallas::Base::from(10u64),
            self.attribute_1_name,
            self.attribute_1,
        ]);
        let attribute_2_hash = poseidon_hash([
            pallas::Base::from(10u64),
            self.attribute_2_name,
            self.attribute_2,
        ]);
        let credential_data = poseidon_hash([
            pallas::Base::from(4u64),
            ix,
            iy,
            hx,
            hy,
            self.schema_hash,
            attribute_1_hash,
            attribute_2_hash,
            self.attribute_blind,
        ]);
        poseidon_hash([
            pallas::Base::from(4u64),
            credential_data,
            self.credential_secret,
            pallas::Base::from(self.issued_at),
            pallas::Base::from(self.expires_at),
        ])
    }

    /// Compute nullifier from credential_secret and commitment (domain-separated, V2)
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(1u64), self.credential_secret, self.commitment])
    }

    pub fn compute_public_inputs(&self) -> VerifyCapabilityPublicInputs {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        VerifyCapabilityPublicInputs {
            nullifier: self.compute_nullifier(),
            schema_hash: self.schema_hash,
            issuer_pub_x: ix,
            issuer_pub_y: iy,
            attribute_1_name: self.attribute_1_name,
            threshold: self.threshold,
            predicate_result: if self.predicate_result {
                pallas::Base::one()
            } else {
                pallas::Base::zero()
            },
            commitment: self.commitment,
            tx_binding,
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        #[expect(clippy::expect_used, reason = "PublicKey constructor rejects identity, so xy()/x()/y() is always Some")]
        let (hx, hy) = self.holder_public.xy().expect("pk not identity");
        vec![
            // The credential preimage, in `issue_credential.zk`'s order.
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(hx)),
            Witness::Base(Value::known(hy)),
            Witness::Base(Value::known(self.schema_hash)),
            Witness::Base(Value::known(self.attribute_1_name)),
            Witness::Base(Value::known(self.attribute_1)),
            Witness::Base(Value::known(self.attribute_2_name)),
            Witness::Base(Value::known(self.attribute_2)),
            Witness::Base(Value::known(self.attribute_blind)),
            Witness::Base(Value::known(self.credential_secret)),
            Witness::Base(Value::known(pallas::Base::from(self.issued_at))),
            Witness::Base(Value::known(pallas::Base::from(self.expires_at))),
            Witness::Base(Value::known(self.commitment)),
            Witness::Base(Value::known(self.threshold)),
            Witness::Base(Value::known(if self.predicate_result { pallas::Base::one() } else { pallas::Base::zero() })),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(tx_binding)), // tx_binding
        ]
    }
}

/// Create a VerifyCapability ZK proof
pub fn create_verify_capability_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &VerifyCapabilityCallData,
) -> Result<(Proof, VerifyCapabilityPublicInputs)> {
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