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

//! Identity create_claim_v1 ZK proof generation (unified V2 circuit, 30 witnesses)

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

/// CreateClaimV1 circuit public inputs (V2: nullifier, tx_binding, tx_nonce)
#[derive(Debug, Clone)]
pub struct CreateClaimPublicInputs {
    pub nullifier: pallas::Base,
    pub tx_binding: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateClaimPublicInputs {
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![self.nullifier, self.tx_binding, self.tx_nonce]
    }
}

/// Input data for create_claim proof generation (unified, 30-witness circuit)
#[derive(Debug, Clone)]
pub struct CreateClaimCallData {
    pub claim_mode: u64,
    pub credential_secret: pallas::Base,
    pub attribute_value: pallas::Base,
    pub threshold: pallas::Base,
    pub commitment: pallas::Base,
    pub predicate_result: pallas::Base,
    pub my_value: pallas::Base,
    pub total_supply: pallas::Base,
    pub threshold_ratio: pallas::Base,
    pub secret_2: pallas::Base,
    pub commitment_2: pallas::Base,
    pub attribute_value_2: pallas::Base,
    pub threshold_2: pallas::Base,
    pub secret_3: pallas::Base,
    pub commitment_3: pallas::Base,
    pub attribute_value_3: pallas::Base,
    pub threshold_3: pallas::Base,
    pub path_index: pallas::Base,
    pub num_credentials: pallas::Base,
    pub is_lte_1: pallas::Base,
    pub is_lte_2: pallas::Base,
    pub is_lte_3: pallas::Base,
    pub issuer_public: PublicKey,
    pub schema_hash: pallas::Base,
    pub claim_type: pallas::Base,
    pub tx_commitment: pallas::Base,
    pub tx_nonce: pallas::Base,
}

impl CreateClaimCallData {
    /// Create a basic claim (mode 0)
    pub fn new_basic(
        credential_secret: pallas::Base,
        attribute_value: pallas::Base,
        threshold: pallas::Base,
        commitment: pallas::Base,
        issuer_public: PublicKey,
        schema_hash: pallas::Base,
        claim_type: pallas::Base,
    ) -> Self {
        Self {
            claim_mode: 0,
            credential_secret, attribute_value, threshold, commitment,
            predicate_result: pallas::Base::zero(),
            my_value: pallas::Base::zero(),
            total_supply: pallas::Base::zero(),
            threshold_ratio: pallas::Base::zero(),
            secret_2: pallas::Base::zero(),
            commitment_2: pallas::Base::zero(),
            attribute_value_2: pallas::Base::zero(),
            threshold_2: pallas::Base::zero(),
            secret_3: pallas::Base::zero(),
            commitment_3: pallas::Base::zero(),
            attribute_value_3: pallas::Base::zero(),
            threshold_3: pallas::Base::zero(),
            path_index: pallas::Base::zero(),
            num_credentials: pallas::Base::zero(),
            is_lte_1: pallas::Base::zero(),
            is_lte_2: pallas::Base::zero(),
            is_lte_3: pallas::Base::zero(),
            issuer_public, schema_hash, claim_type,
            tx_commitment: pallas::Base::zero(),
            tx_nonce: pallas::Base::zero(),
        }
    }

    /// Compute nullifier (domain-separated, V2)
    pub fn compute_nullifier(&self) -> pallas::Base {
        poseidon_hash([pallas::Base::from(1u64), self.credential_secret, self.commitment])
    }

    pub fn compute_public_inputs(&self) -> CreateClaimPublicInputs {
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        CreateClaimPublicInputs {
            nullifier: self.compute_nullifier(),
            tx_binding,
            tx_nonce: self.tx_nonce,
        }
    }

    pub fn to_witnesses(&self) -> Vec<Witness> {
        let (ix, iy) = self.issuer_public.xy().expect("pk not identity");
        let tx_binding = poseidon_hash([pallas::Base::from(3u64), self.tx_commitment, self.tx_nonce]);
        // Must match circuit witness order exactly (30 elements):
        vec![
            Witness::Base(Value::known(pallas::Base::from(self.claim_mode))), // 0: mode
            Witness::Base(Value::known(self.compute_nullifier())),              // 1: nullifier
            Witness::Base(Value::known(self.claim_type)),                       // 2: claim_type
            Witness::Base(Value::known(ix)),                                    // 3: issuer_pub_x
            Witness::Base(Value::known(iy)),                                    // 4: issuer_pub_y
            Witness::Base(Value::known(self.schema_hash)),                      // 5: schema_hash
            Witness::Base(Value::known(self.credential_secret)),                // 6: credential_secret
            Witness::Base(Value::known(self.commitment)),                       // 7: commitment
            Witness::Base(Value::known(self.attribute_value)),                  // 8: attribute_value
            Witness::Base(Value::known(self.threshold)),                        // 9: threshold
            Witness::Base(Value::known(self.predicate_result)),                 // 10: predicate_result
            Witness::Base(Value::known(self.my_value)),                         // 11: my_value
            Witness::Base(Value::known(self.total_supply)),                     // 12: total_supply
            Witness::Base(Value::known(self.threshold_ratio)),                  // 13: threshold_ratio
            Witness::Base(Value::known(self.secret_2)),                         // 14: secret_2
            Witness::Base(Value::known(self.commitment_2)),                     // 15: commitment_2
            Witness::Base(Value::known(self.attribute_value_2)),                // 16: attribute_value_2
            Witness::Base(Value::known(self.threshold_2)),                      // 17: threshold_2
            Witness::Base(Value::known(self.secret_3)),                         // 18: secret_3
            Witness::Base(Value::known(self.commitment_3)),                     // 19: commitment_3
            Witness::Base(Value::known(self.attribute_value_3)),                // 20: attribute_value_3
            Witness::Base(Value::known(self.threshold_3)),                      // 21: threshold_3
            Witness::Base(Value::known(self.path_index)),                       // 22: path_index
            Witness::Base(Value::known(self.num_credentials)),                  // 23: num_credentials
            Witness::Base(Value::known(self.is_lte_1)),                         // 24: is_lte_1
            Witness::Base(Value::known(self.is_lte_2)),                         // 25: is_lte_2
            Witness::Base(Value::known(self.is_lte_3)),                         // 26: is_lte_3
            Witness::Base(Value::known(self.tx_commitment)),                    // 27: tx_commitment
            Witness::Base(Value::known(self.tx_nonce)),                         // 28: tx_nonce
            Witness::Base(Value::known(tx_binding)),                            // 29: tx_binding
        ]
    }
}

/// Create a CreateClaim ZK proof
pub fn create_claim_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateClaimCallData,
) -> Result<(Proof, CreateClaimPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
