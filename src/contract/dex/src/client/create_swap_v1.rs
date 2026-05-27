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

//! CreateSwap ZK proof generation

use dwow_core::{
    zk::{halo2::Value, Proof, ProvingKey, Witness, ZkCircuit},
    zkas::ZkBinary,
    Result,
};
use dwow_sdk::{
    crypto::{poseidon_hash, pasta_prelude::Field, PublicKey, SecretKey},
    pasta::pallas,
};
use rand::rngs::OsRng;

/// CreateSwap circuit public inputs (in order of constrain_instance)
#[derive(Debug, Clone)]
pub struct CreateSwapPublicInputs {
    /// Lock commitment = poseidon_hash([secret, offer_token, offer_amount, token_blind, amount_blind])
    pub lock_commitment: pallas::Base,
    /// Swap ID = poseidon_hash([lock_commitment, request_token, request_amount])
    pub swap_id: pallas::Base,
    /// Nullifier = poseidon_hash([secret, lock_commitment])
    pub nullifier: pallas::Base,
    /// Signature public key X coordinate
    pub signature_public_x: pallas::Base,
    /// Signature public key Y coordinate
    pub signature_public_y: pallas::Base,
}

impl CreateSwapPublicInputs {
    /// Convert to vector for ZK proof creation
    /// Order must match constrain_instance calls in create_swap_v1.zk
    pub fn to_vec(&self) -> Vec<pallas::Base> {
        vec![
            self.lock_commitment,
            self.swap_id,
            self.nullifier,
            self.signature_public_x,
            self.signature_public_y,
        ]
    }
}

/// Input data for CreateSwap proof generation
#[derive(Debug, Clone)]
pub struct CreateSwapCallData {
    /// Secret for the lock
    pub secret: pallas::Base,
    /// Token being offered
    pub offer_token: pallas::Base,
    /// Amount being offered
    pub offer_amount: pallas::Base,
    /// Token being requested
    pub request_token: pallas::Base,
    /// Amount being requested
    pub request_amount: pallas::Base,
    /// Blinding factor for token
    pub token_blind: pallas::Base,
    /// Blinding factor for amount
    pub amount_blind: pallas::Base,
    /// Secret key for signature
    pub ephemeral_signature_secret: SecretKey,
    /// Signature public key (derived from ephemeral_signature_secret)
    pub signature_public: PublicKey,
}

impl CreateSwapCallData {
    /// Create new call data with random blinds
    pub fn new(
        secret: pallas::Base,
        offer_token: pallas::Base,
        offer_amount: u64,
        request_token: pallas::Base,
        request_amount: u64,
        ephemeral_signature_secret: SecretKey,
    ) -> Self {
        let signature_public = PublicKey::from_secret(ephemeral_signature_secret);
        let token_blind = pallas::Base::random(&mut OsRng);
        let amount_blind = pallas::Base::random(&mut OsRng);

        Self {
            secret,
            offer_token,
            offer_amount: pallas::Base::from(offer_amount),
            request_token,
            request_amount: pallas::Base::from(request_amount),
            token_blind,
            amount_blind,
            ephemeral_signature_secret,
            signature_public,
        }
    }

    /// Compute public inputs for this call
    pub fn compute_public_inputs(&self) -> CreateSwapPublicInputs {
        // Compute lock commitment
        // lock = poseidon_hash([secret, offer_token, offer_amount, token_blind, amount_blind])
        let lock_commitment = poseidon_hash([
            self.secret,
            self.offer_token,
            self.offer_amount,
            self.token_blind,
            self.amount_blind,
        ]);

        // Compute swap ID
        // swap_id = poseidon_hash([lock_commitment, request_token, request_amount])
        let swap_id = poseidon_hash([
            lock_commitment,
            self.request_token,
            self.request_amount,
        ]);

        // Compute nullifier
        // nullifier = poseidon_hash([secret, lock_commitment])
        let nullifier = poseidon_hash([self.secret, lock_commitment]);

        // Get signature public key coordinates
        let (sig_x, sig_y) = self.signature_public.xy();

        CreateSwapPublicInputs {
            lock_commitment,
            swap_id,
            nullifier,
            signature_public_x: sig_x,
            signature_public_y: sig_y,
        }
    }

    /// Generate prover witnesses for the circuit
    pub fn to_witnesses(&self) -> Vec<Witness> {
        vec![
            // Base secret
            Witness::Base(Value::known(self.secret)),
            // Base offer_token
            Witness::Base(Value::known(self.offer_token)),
            // Base offer_amount
            Witness::Base(Value::known(self.offer_amount)),
            // Base request_token
            Witness::Base(Value::known(self.request_token)),
            // Base request_amount
            Witness::Base(Value::known(self.request_amount)),
            // Base token_blind
            Witness::Base(Value::known(self.token_blind)),
            // Base amount_blind
            Witness::Base(Value::known(self.amount_blind)),
            // Base ephemeral_signature_secret
            Witness::Base(Value::known(self.ephemeral_signature_secret.inner())),
            // Base signature_public_x
            Witness::Base(Value::known(self.signature_public.x())),
            // Base signature_public_y
            Witness::Base(Value::known(self.signature_public.y())),
        ]
    }
}

/// Create a CreateSwap ZK proof
///
/// # Arguments
///
/// * `zkbin` - The compiled ZK binary for CreateSwap circuit
/// * `pk` - The proving key for the circuit
/// * `input` - The call data containing secrets and parameters
///
/// # Returns
///
/// * `(Proof, CreateSwapPublicInputs)` - The ZK proof and public inputs
pub fn create_create_swap_proof(
    zkbin: &ZkBinary,
    pk: &ProvingKey,
    input: &CreateSwapCallData,
) -> Result<(Proof, CreateSwapPublicInputs)> {
    let public_inputs = input.compute_public_inputs();
    let witnesses = input.to_witnesses();

    //dwow_core::zk::export_witness_json("proof/witness/create_swap_v1.json", &witnesses, &public_inputs.to_vec());
    let circuit = ZkCircuit::new(witnesses, zkbin);
    let proof = Proof::create(pk, &[circuit], &public_inputs.to_vec(), &mut OsRng)?;

    Ok((proof, public_inputs))
}
