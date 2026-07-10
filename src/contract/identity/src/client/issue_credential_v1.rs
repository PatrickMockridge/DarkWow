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

//! Identity issue_credential ZK proof generation

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

/// IssueCredentialV1 circuit public inputs
#[derive(Debug, Clone)]
pub struct IssueCredentialPublicInputs {
    pub commitment: pallas::Base,
    pub issuer_pub_x: pallas::Base,
    pub issuer_pub_y: pallas::Base,
    pub holder_pub_x: pallas::Base,
    pub holder_pub_y: pallas::Base,
    pub schema_hash: pallas::Base,
    pub issued_at: pallas::Base,
    pub expires_at: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl IssueCredentialPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.commitment,
            self.issuer_pub_x,
            self.issuer_pub_y,
            self.holder_pub_x,
            self.holder_pub_y,
            self.schema_hash,
            self.issued_at,
            self.expires_at,
            self.tx_binding,
            self.tx_nonce,
        ]
    }
}

/// Input data for issue_credential proof generation
#[derive(Debug, Clone)]
pub struct IssueCredentialCallData {
    pub issuer_secret: pallas::Base,
    pub credential_secret: pallas::Base,
    pub attribute_1: pallas::Base,
    pub attribute_2: pallas::Base,
    pub attribute_blind: pallas::Base,
    // Public inputs
    pub issuer_public: PublicKey,
    pub holder_public: PublicKey,
    pub schema_hash: pallas::Base,
    pub issued_at: u64,
    pub expires_at: u64,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl IssueCredentialCallData {
    pub fn new(
        issuer_secret: pallas::Base,
        credential_secret: pallas::Base,
        attribute_1: pallas::Base,
        attribute_2: pallas::Base,
        attribute_blind: pallas::Base,
        issuer_public: PublicKey,
        holder_public: PublicKey,
        schema_hash: pallas::Base,
        issued_at: u64,
        expires_at: u64,
    ) -> Self {
        Self {
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
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute credential commitment
    pub fn compute_commitment(&self) -> pallas::Base {
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        let (hx, hy) = self.holder_public.xy().expect("pk not identity");
        let credential_data = poseidon_hash([
            ix,
            iy,
            hx,
            hy,
            self.schema_hash,
            self.attribute_1,
            self.attribute_2,
            self.attribute_blind,
        ]);
        poseidon_hash([
            credential_data,
            self.credential_secret,
            pallas::Base::from(self.issued_at),
            pallas::Base::from(self.expires_at),
        ])
    }

    pub fn compute_public_inputs(&self) -> IssueCredentialPublicInputs {
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        let (hx, hy) = self.holder_public.xy().expect("pk not identity");
        IssueCredentialPublicInputs {
            commitment: self.compute_commitment(),
            issuer_pub_x: ix,
            issuer_pub_y: iy,
            holder_pub_x: hx,
            holder_pub_y: hy,
            schema_hash: self.schema_hash,
            issued_at: pallas::Base::from(self.issued_at),
            expires_at: pallas::Base::from(self.expires_at),
            tx_binding: pallas::Base::zero(),
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        let (hx, hy) = self.holder_public.xy().expect("pk not identity");
        vec![
            // Public inputs as witnesses
            Witness::Base(Value::known(self.compute_commitment())),
            Witness::Base(Value::known(ix)),
            Witness::Base(Value::known(iy)),
            Witness::Base(Value::known(hx)),
            Witness::Base(Value::known(hy)),
            Witness::Base(Value::known(self.schema_hash)),
            Witness::Base(Value::known(pallas::Base::from(self.issued_at))),
            Witness::Base(Value::known(pallas::Base::from(self.expires_at))),
            // Private inputs
            Witness::Base(Value::known(self.issuer_secret)),
            Witness::Base(Value::known(self.credential_secret)),
            Witness::Base(Value::known(self.attribute_1)),
            Witness::Base(Value::known(self.attribute_2)),
            Witness::Base(Value::known(self.attribute_blind)),
            Witness::Base(Value::known(self.tx_commitment)),
            Witness::Base(Value::known(self.tx_nonce)),
            Witness::Base(Value::known(pallas::Base::zero())), // tx_binding
        ]
    }
}

/// Create an IssueCredential ZK proof
pub fn create_issue_credential_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &IssueCredentialCallData,
) -> Result<(Proof, IssueCredentialPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}